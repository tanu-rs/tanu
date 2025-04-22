/// tanu's test runner
use backon::Retryable;
use eyre::WrapErr;
use futures::{stream::FuturesUnordered, FutureExt, StreamExt};
use once_cell::sync::Lazy;
use std::{
    collections::HashMap,
    ops::Deref,
    pin::Pin,
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast;
use tracing::*;

use crate::{
    config::{self, get_config, get_tanu_config, ProjectConfig},
    http,
    reporter::Reporter,
    Config, ModuleName, ProjectName, TestName,
};

pub static CHANNEL: Lazy<Mutex<Option<broadcast::Sender<Message>>>> =
    Lazy::new(|| Mutex::new(Some(broadcast::channel(1000).0)));

pub fn publish(msg: Message) -> eyre::Result<()> {
    let Ok(guard) = CHANNEL.lock() else {
        eyre::bail!("failed to acquire runner channel lock");
    };
    let Some(tx) = guard.deref() else {
        eyre::bail!("runner channel has been already closed");
    };

    tx.send(msg)
        .wrap_err("failed to publish message to the runner channel")?;

    Ok(())
}

/// Subscribe to the channel to see the real-time test execution events.
pub fn subscribe() -> eyre::Result<broadcast::Receiver<Message>> {
    let Ok(guard) = CHANNEL.lock() else {
        eyre::bail!("failed to acquire runner channel lock");
    };
    let Some(tx) = guard.deref() else {
        eyre::bail!("runner channel has been already closed");
    };

    Ok(tx.subscribe())
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("panic: {0}")]
    Panicked(String),
    #[error("error: {0}")]
    ErrorReturned(String),
}

#[derive(Debug, Clone)]
pub enum Message {
    Start(ProjectName, ModuleName, TestName),
    HttpLog(ProjectName, ModuleName, TestName, Box<http::Log>),
    End(ProjectName, ModuleName, TestName, Test),
}

#[derive(Debug, Clone)]
pub struct Test {
    pub info: TestInfo,
    pub result: Result<(), Error>,
}

#[derive(Debug, Clone)]
pub struct TestInfo {
    pub module: String,
    pub name: String,
}

impl TestInfo {
    /// Full test name including module
    pub fn full_name(&self) -> String {
        format!("{}::{}", self.module, self.name)
    }

    /// Unique test name including project and module names
    pub fn unique_name(&self, project: &str) -> String {
        format!("{project}::{}::{}", self.module, self.name)
    }
}

type TestCaseFactory = Arc<
    dyn Fn() -> Pin<Box<dyn futures::Future<Output = eyre::Result<()>> + Send + 'static>>
        + Sync
        + Send
        + 'static,
>;

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub debug: bool,
    pub capture_http: bool,
    pub capture_rust: bool,
    pub terminate_channel: bool,
}

/// Test case filter trait.
pub trait Filter {
    fn filter(&self, project: &ProjectConfig, info: &TestInfo) -> bool;
}

/// Filter test cases by project name.
pub struct ProjectFilter<'a> {
    project_names: &'a [String],
}

impl Filter for ProjectFilter<'_> {
    fn filter(&self, project: &ProjectConfig, _info: &TestInfo) -> bool {
        if self.project_names.is_empty() {
            return true;
        }

        self.project_names
            .iter()
            .any(|project_name| &project.name == project_name)
    }
}

/// Filter test cases by module name.
pub struct ModuleFilter<'a> {
    module_names: &'a [String],
}

impl Filter for ModuleFilter<'_> {
    fn filter(&self, _project: &ProjectConfig, info: &TestInfo) -> bool {
        if self.module_names.is_empty() {
            return true;
        }

        self.module_names
            .iter()
            .any(|module_name| &info.module == module_name)
    }
}

/// Filter test cases by test name.
pub struct TestNameFilter<'a> {
    test_names: &'a [String],
}

impl Filter for TestNameFilter<'_> {
    fn filter(&self, _project: &ProjectConfig, info: &TestInfo) -> bool {
        if self.test_names.is_empty() {
            return true;
        }

        self.test_names
            .iter()
            .any(|test_name| &info.full_name() == test_name)
    }
}

/// Filter test cases by test ignore config.
pub struct TestIgnoreFilter {
    test_ignores: HashMap<String, Vec<String>>,
}

impl Default for TestIgnoreFilter {
    fn default() -> TestIgnoreFilter {
        TestIgnoreFilter {
            test_ignores: get_tanu_config()
                .projects
                .iter()
                .map(|proj| (proj.name.clone(), proj.test_ignore.clone()))
                .collect(),
        }
    }
}

impl Filter for TestIgnoreFilter {
    fn filter(&self, project: &ProjectConfig, info: &TestInfo) -> bool {
        let Some(test_ignore) = self.test_ignores.get(&project.name) else {
            return true;
        };

        test_ignore
            .iter()
            .all(|test_name| &info.full_name() != test_name)
    }
}

#[derive(Default)]
pub struct Runner {
    cfg: Config,
    options: Options,
    test_cases: Vec<(TestInfo, TestCaseFactory)>,
    reporters: Vec<Box<dyn Reporter + Send>>,
}

impl Runner {
    pub fn new() -> Runner {
        Runner::with_config(get_tanu_config().clone())
    }

    pub fn with_config(cfg: Config) -> Runner {
        Runner {
            cfg,
            options: Options::default(),
            test_cases: Vec::new(),
            reporters: Vec::new(),
        }
    }

    pub fn capture_http(&mut self) {
        self.options.capture_http = true;
    }

    pub fn capture_rust(&mut self) {
        self.options.capture_rust = true;
    }

    pub fn terminate_channel(&mut self) {
        self.options.terminate_channel = true;
    }

    pub fn add_reporter(&mut self, reporter: impl Reporter + 'static + Send) {
        self.reporters.push(Box::new(reporter));
    }

    /// Add a test case to the runner.
    pub fn add_test(&mut self, name: &str, module: &str, factory: TestCaseFactory) {
        self.test_cases.push((
            TestInfo {
                name: name.into(),
                module: module.into(),
            },
            factory,
        ));
    }

    /// Run tanu runner.
    #[allow(clippy::too_many_lines)]
    pub async fn run(
        &mut self,
        project_names: &[String],
        module_names: &[String],
        test_names: &[String],
    ) -> eyre::Result<()> {
        if self.options.capture_rust {
            tracing_subscriber::fmt::init();
        }

        let mut reporters = std::mem::take(&mut self.reporters);

        let project_filter = ProjectFilter { project_names };
        let module_filter = ModuleFilter { module_names };
        let test_name_filter = TestNameFilter { test_names };
        let test_ignore_filter = TestIgnoreFilter::default();

        let handles: FuturesUnordered<_> = self
                .test_cases
                .iter()
                .flat_map(|(info, factory)| {
                    let projects = self.cfg.projects.clone();
                    let projects = if projects.is_empty() {
                        vec![ProjectConfig {
                            name: "default".into(),
                            ..Default::default()
                        }]
                    } else {
                        projects
                    };
                    projects
                        .into_iter()
                        .map(move |project| {
                            (project.clone(), info.clone(), factory.clone())
                        })
                })
                .filter(move |(project, info, _)| {
                    test_name_filter.filter(project, info)
                })
                .filter(move |(project, info, _)| {
                    module_filter.filter(project, info)
                })
                .filter(move |(project, info, _)| {
                    project_filter.filter(project, info)
                })
                .filter(move |(project, info, _)| {
                    test_ignore_filter.filter(project, info)
                })
                .map(|(project, info, factory)| {
                    tokio::spawn(async move {
                        config::PROJECT.scope(project.clone(), async {
                            http::CHANNEL.scope(
                                Arc::new(Mutex::new(Some(broadcast::channel(1000).0))),
                                async {
                                    let test_name = &info.name;
                                    let mut http_rx = http::subscribe()?;

                                    let f= || async {factory().await};
                                    let fut = f.retry(project.retry.backoff());
                                    let fut =
                                        std::panic::AssertUnwindSafe(fut).catch_unwind();
                                    let res = fut.await;

                                    publish(Message::Start(project.name.clone(), info.module.clone(), test_name.to_string())).wrap_err("failed to send Message::Start to the channel")?;

                                    let result = match res {
                                        Ok(Ok(_)) => {
                                            debug!("{test_name} ok");
                                            Ok(())
                                        }
                                        Ok(Err(e)) => {
                                            debug!("{test_name} failed: {e:#}");
                                            Err(Error::ErrorReturned(format!("{e:?}")))
                                        }
                                        Err(e) => {
                                            let panic_message =
                                                if let Some(panic_message) = e.downcast_ref::<&str>() {
                                                    format!(
                                                    "{test_name} failed with message: {panic_message}"
                                                )
                                                } else if let Some(panic_message) =
                                                    e.downcast_ref::<String>()
                                                {
                                                    format!(
                                                    "{test_name} failed with message: {panic_message}"
                                                )
                                                } else {
                                                    format!("{test_name} failed with unknown message")
                                                };
                                            let e = eyre::eyre!(panic_message);
                                            Err(Error::Panicked(format!("{e:?}")))
                                        }
                                    };

                                    while let Ok(log) = http_rx.try_recv() {
                                        publish(Message::HttpLog(project.name.clone(), info.module.clone(), test_name.clone(), Box::new(log))).wrap_err("failed to send Message::HttpLog to the channel")?;
                                    }

                                    let project = get_config();
                                    let is_err = result.is_err();
                                    publish(Message::End(project.name, info.module.clone(), test_name.clone(), Test { info, result })).wrap_err("failed to send Message::End to the channel")?;

                                    eyre::ensure!(!is_err);
                                    eyre::Ok(())
                                }).await
                            })
                            .await
                    })
                })
                .collect();

        let reporters =
            futures::future::join_all(reporters.iter_mut().map(|reporter| reporter.run().boxed()));

        let mut has_any_error = false;
        let options = self.options.clone();
        let runner = async move {
            let results = handles.collect::<Vec<_>>().await;
            if results.is_empty() {
                console::Term::stdout().write_line("no test cases found")?;
            }
            for result in results {
                match result {
                    Ok(res) => {
                        if res.is_err() {
                            has_any_error = true;
                        }
                    }
                    Err(e) => {
                        if e.is_panic() {
                            // Resume the panic on the main task
                            error!("{e}");
                            has_any_error = true;
                            println!("e={e:?}");
                        }
                    }
                }
            }
            debug!("all test finished. sending stop signal to the background tasks.");

            if options.terminate_channel {
                let Ok(mut guard) = CHANNEL.lock() else {
                    eyre::bail!("failed to acquire runner channel lock");
                };
                guard.take(); // closing the runner channel.
            }

            if has_any_error {
                eyre::bail!("one or more tests failed");
            }

            eyre::Ok(())
        };

        let (handles, _reporters) = tokio::join!(runner, reporters);

        debug!("runner stopped");

        handles
    }

    pub fn list(&self) -> Vec<&TestInfo> {
        self.test_cases
            .iter()
            .map(|(meta, _test)| meta)
            .collect::<Vec<_>>()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::config::RetryConfig;

    fn create_config() -> Config {
        Config {
            projects: vec![ProjectConfig {
                name: "default".into(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn create_config_with_retry() -> Config {
        Config {
            projects: vec![ProjectConfig {
                name: "default".into(),
                retry: RetryConfig {
                    count: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn runner_fail_because_no_retry_configured() -> eyre::Result<()> {
        let mut server = mockito::Server::new_async().await;
        let m1 = server
            .mock("GET", "/")
            .with_status(500)
            .expect(1)
            .create_async()
            .await;
        let m2 = server
            .mock("GET", "/")
            .with_status(200)
            .expect(0)
            .create_async()
            .await;

        let factory: TestCaseFactory = Arc::new(move || {
            let url = server.url();
            Box::pin(async move {
                let res = reqwest::get(url).await?;
                if res.status().is_success() {
                    Ok(())
                } else {
                    eyre::bail!("request failed")
                }
            })
        });

        let _runner_rx = subscribe()?;
        let mut runner = Runner::with_config(create_config());
        runner.add_test("retry_test", "module", factory);

        let result = runner.run(&[], &[], &[]).await;
        m1.assert_async().await;
        m2.assert_async().await;

        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn runner_retry_successful_after_failure() -> eyre::Result<()> {
        let mut server = mockito::Server::new_async().await;
        let m1 = server
            .mock("GET", "/")
            .with_status(500)
            .expect(1)
            .create_async()
            .await;
        let m2 = server
            .mock("GET", "/")
            .with_status(200)
            .expect(1)
            .create_async()
            .await;

        let factory: TestCaseFactory = Arc::new(move || {
            let url = server.url();
            Box::pin(async move {
                let res = reqwest::get(url).await?;
                if res.status().is_success() {
                    Ok(())
                } else {
                    eyre::bail!("request failed")
                }
            })
        });

        let _runner_rx = subscribe()?;
        let mut runner = Runner::with_config(create_config_with_retry());
        runner.add_test("retry_test", "module", factory);

        let result = runner.run(&[], &[], &[]).await;
        m1.assert_async().await;
        m2.assert_async().await;

        assert!(result.is_ok());

        Ok(())
    }
}
