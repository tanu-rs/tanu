/// tanu's test runner
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
    ModuleName, ProjectName, TestName,
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
    pub metadata: TestMetadata,
    pub result: Result<(), Error>,
}

#[derive(Debug, Clone)]
pub struct TestMetadata {
    pub name: String,
    pub module: String,
}

impl TestMetadata {
    /// Full test name including module
    pub fn full_name(&self) -> String {
        format!("{}::{}", self.module, self.name)
    }
}

type TestCaseFactory = Box<
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

#[derive(Default)]
pub struct Runner {
    options: Options,
    test_cases: Vec<(TestMetadata, TestCaseFactory)>,
    reporters: Vec<Box<dyn Reporter + Send>>,
}

/// Test case filter trait.
pub trait Filter {
    fn filter(&self, project: &ProjectConfig, metadata: &TestMetadata) -> bool;
}

/// Filter test cases by project name.
pub struct ProjectFilter<'a> {
    project_names: &'a [String],
}

impl Filter for ProjectFilter<'_> {
    fn filter(&self, project: &ProjectConfig, _metadata: &TestMetadata) -> bool {
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
    fn filter(&self, _project: &ProjectConfig, metadata: &TestMetadata) -> bool {
        if self.module_names.is_empty() {
            return true;
        }

        self.module_names
            .iter()
            .any(|module_name| &metadata.module == module_name)
    }
}

/// Filter test cases by test name.
pub struct TestNameFilter<'a> {
    test_names: &'a [String],
}

impl Filter for TestNameFilter<'_> {
    fn filter(&self, _project: &ProjectConfig, metadata: &TestMetadata) -> bool {
        if self.test_names.is_empty() {
            return true;
        }

        self.test_names
            .iter()
            .any(|test_name| &metadata.full_name() == test_name)
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
    fn filter(&self, project: &ProjectConfig, metadata: &TestMetadata) -> bool {
        let Some(test_ignore) = self.test_ignores.get(&project.name) else {
            return true;
        };

        test_ignore
            .iter()
            .all(|test_name| &metadata.full_name() != test_name)
    }
}

impl Runner {
    pub fn new() -> Runner {
        Runner::default()
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
            TestMetadata {
                name: name.into(),
                module: module.into(),
            },
            factory,
        ));
    }

    /// Run tanu runner.
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
                .flat_map(|(metadata, factory)| {
                    let projects = get_tanu_config().projects.clone();
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
                        .map(move |project| (project.clone(), metadata.clone(), (factory)()))
                })
                .filter(move |(project, metadata, _)| {
                    test_name_filter.filter(project, metadata)
                })
                .filter(move |(project, metadata, _)| {
                    module_filter.filter(project, metadata)
                })
                .filter(move |(project, metadata, _)| {
                    project_filter.filter(project, metadata)
                })
                .filter(move |(project, metadata, _)| {
                    test_ignore_filter.filter(project, metadata)
                })
                .map(|(project, metadata, fut)| {
                    tokio::spawn(async move {
                        config::PROJECT
                            .scope(project, async {
                                http::CHANNEL
                                    .scope(
                                        Arc::new(Mutex::new(Some(broadcast::channel(1000).0))),
                                        async {
                                            let test_name = &metadata.name;
                                            let mut http_rx = http::subscribe()?;

                                            let fut =
                                                std::panic::AssertUnwindSafe(fut).catch_unwind();
                                            let res = fut.await;

                                            let project = get_config();
                                            publish(Message::Start(project.name.clone(), metadata.module.clone(), test_name.to_string()))?;

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
                                                publish(Message::HttpLog(
                                                    project.name.clone(),
                                                    metadata.module.clone(),
                                                    test_name.clone(),
                                                    Box::new(log),
                                                ))?;
                                            }

                                            let project = get_config();
                                            publish(Message::End(
                                                project.name,
                                                metadata.module.clone(),
                                                test_name.clone(),
                                                Test { metadata, result },
                                            ))?;

                                            eyre::Ok(())
                                        },
                                    )
                                    .await
                            })
                            .await
                    })
                })
                .collect();

        let reporters =
            futures::future::join_all(reporters.iter_mut().map(|reporter| reporter.run().boxed()));

        let options = self.options.clone();
        let runner = async move {
            let results = handles.collect::<Vec<_>>().await;
            for result in results {
                if let Err(e) = result {
                    if e.is_panic() {
                        // Resume the panic on the main task
                        error!("{e}");
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

            eyre::Ok(())
        };

        let (_handles, _reporters) = tokio::join!(runner, reporters);

        debug!("runner stopped");

        Ok(())
    }

    pub fn list(&self) -> Vec<&TestMetadata> {
        self.test_cases
            .iter()
            .map(|(meta, _test)| meta)
            .collect::<Vec<_>>()
    }
}
