use console::{style, StyledObject, Term};
use eyre::WrapErr;
use indexmap::IndexMap;
use itertools::Itertools;
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
};
use tokio::sync::broadcast;
use tracing::*;

use crate::{
    get_tanu_config, http,
    runner::{self, Event, EventBody, Test},
    ModuleName, ProjectName, TestName,
};

#[derive(Debug, Clone, Default, strum::EnumString, strum::Display)]
#[strum(serialize_all = "snake_case")]
pub enum ReporterType {
    Null,
    #[default]
    List,
    Table,
}

async fn run<R: Reporter + Send + ?Sized>(reporter: &mut R) -> eyre::Result<()> {
    let mut rx = runner::subscribe()?;

    loop {
        let res = match rx.recv().await {
            Ok(Event {
                project,
                module,
                test,
                body: EventBody::Start,
            }) => reporter.on_start(project, module, test).await,
            Ok(Event {
                project,
                module,
                test,
                body: EventBody::Check(check),
            }) => reporter.on_check(project, module, test, check).await,
            Ok(Event {
                project,
                module,
                test,
                body: EventBody::Http(log),
            }) => reporter.on_http_call(project, module, test, log).await,
            Ok(Event {
                project,
                module,
                test,
                body: EventBody::Retry,
            }) => reporter.on_retry(project, module, test).await,
            Ok(Event {
                project,
                module,
                test: test_name,
                body: EventBody::End(test),
            }) => reporter.on_end(project, module, test_name, test).await,
            Err(broadcast::error::RecvError::Closed) => {
                debug!("runner channel has been closed");
                break;
            }
            Err(broadcast::error::RecvError::Lagged(_)) => {
                debug!("runner channel recv error");
                continue;
            }
        };

        if let Err(e) = res {
            warn!("reporter error: {e:#}");
        }
    }

    Ok(())
}

/// Reporter trait. The trait is based on the "template method" pattern.
/// You can implement on_xxx methods to hook into the test runner. This way is enough for most usecases.
/// If you need more control, you can override the "run" method.
#[async_trait::async_trait]
pub trait Reporter {
    async fn run(&mut self) -> eyre::Result<()> {
        run(self).await
    }

    /// Called when a test case starts.
    async fn on_start(
        &mut self,
        _project: String,
        _module: String,
        _test_name: String,
    ) -> eyre::Result<()> {
        Ok(())
    }

    /// Called when a check macro is used.
    async fn on_check(
        &mut self,
        _project: String,
        _module: String,
        _test_name: String,
        _check: Box<runner::Check>,
    ) -> eyre::Result<()> {
        Ok(())
    }

    /// Called when an HTTP call is made.
    async fn on_http_call(
        &mut self,
        _project: String,
        _module: String,
        _test_name: String,
        _log: Box<http::Log>,
    ) -> eyre::Result<()> {
        Ok(())
    }

    /// Called when a test case fails but to be retried.
    async fn on_retry(
        &mut self,
        _project: String,
        _module: String,
        _test_name: String,
    ) -> eyre::Result<()> {
        Ok(())
    }

    /// Called when a test case ends.
    async fn on_end(
        &mut self,
        _project: String,
        _module: String,
        _test_name: String,
        _test: Test,
    ) -> eyre::Result<()> {
        Ok(())
    }
}

pub struct NullReporter;

#[async_trait::async_trait]
impl Reporter for NullReporter {}

/// Capture current states of the stdout for the test case.
#[allow(clippy::vec_box)]
#[derive(Default, Debug)]
struct Buffer {
    test_number: Option<usize>,
    http_logs: Vec<Box<http::Log>>,
}

fn generate_test_number() -> usize {
    static TEST_NUMBER: LazyLock<Mutex<usize>> = LazyLock::new(|| Mutex::new(0));
    let mut test_number = TEST_NUMBER.lock().unwrap();
    *test_number += 1;
    *test_number
}
pub struct ListReporter {
    terminal: Term,
    buffer: IndexMap<(ProjectName, ModuleName, TestName), Buffer>,
    capture_http: bool,
}

impl ListReporter {
    pub fn new(capture_http: bool) -> ListReporter {
        ListReporter {
            terminal: Term::stdout(),
            buffer: IndexMap::new(),
            capture_http,
        }
    }
}

#[async_trait::async_trait]
impl Reporter for ListReporter {
    async fn on_start(
        &mut self,
        project_name: String,
        module_name: String,
        test_name: String,
    ) -> eyre::Result<()> {
        self.buffer
            .insert((project_name, module_name, test_name), Buffer::default());
        Ok(())
    }

    async fn on_http_call(
        &mut self,
        project_name: String,
        module_name: String,
        test_name: String,
        log: Box<http::Log>,
    ) -> eyre::Result<()> {
        if self.capture_http {
            self.buffer
                .get_mut(&(project_name, module_name, test_name.clone()))
                .ok_or_else(|| eyre::eyre!("test case \"{test_name}\" not found in the buffer"))?
                .http_logs
                .push(log);
        }
        Ok(())
    }

    async fn on_retry(
        &mut self,
        project: String,
        module: String,
        test_name: String,
    ) -> eyre::Result<()> {
        let buffer = self
            .buffer
            .get_mut(&(project.clone(), module.clone(), test_name.clone()))
            .ok_or_else(|| eyre::eyre!("test case \"{test_name}\" not found in the buffer",))?;

        let test_number = style(buffer.test_number.get_or_insert_with(generate_test_number)).dim();
        self.terminal.write_line(&format!(
            "{status} {test_number} [{project}] {module}::{test_name}: {retry_message}",
            status = symbol_error(),
            retry_message = style("retrying...").blue(),
        ))?;
        Ok(())
    }

    async fn on_end(
        &mut self,
        project_name: String,
        module_name: String,
        test_name: String,
        test: Test,
    ) -> eyre::Result<()> {
        let mut buffer = self
            .buffer
            .swap_remove(&(project_name.clone(), module_name, test_name.clone()))
            .ok_or_else(|| eyre::eyre!("test case \"{test_name}\" not found in the buffer"))?;

        for log in buffer.http_logs {
            write(
                &self.terminal,
                format!(" => {} {}", log.request.method, log.request.url),
            )?;
            write(&self.terminal, "  > request:")?;
            write(&self.terminal, "    > headers:")?;
            for key in log.request.headers.keys() {
                write(
                    &self.terminal,
                    format!(
                        "       > {key}: {}",
                        log.request.headers.get(key).unwrap().to_str().unwrap()
                    ),
                )?;
            }
            write(&self.terminal, "  < response")?;
            write(&self.terminal, "    < headers:")?;
            for key in log.response.headers.keys() {
                write(
                    &self.terminal,
                    format!(
                        "       < {key}: {}",
                        log.response.headers.get(key).unwrap().to_str().unwrap()
                    ),
                )?;
            }
            write(&self.terminal, format!("    < body: {}", log.response.body))?;
        }

        let status = symbol_test_result(&test);
        let Test {
            result,
            info,
            request_time,
        } = test;
        let test_number = style(buffer.test_number.get_or_insert_with(generate_test_number)).dim();
        let request_time = style(format!("({request_time:.2?})")).dim();
        match result {
            Ok(_res) => {
                self.terminal.write_line(&format!(
                    "{status} {test_number} [{project_name}] {module_name}::{test_name} {request_time}",
                    module_name = info.module,
                    test_name = info.name
                ))?;
            }
            Err(e) => {
                self.terminal.write_line(&format!(
                    "{status} [{project_name}] {module_name}::{test_name} {request_time}:\n{e:#} ",
                    module_name = info.module,
                    test_name = info.name
                ))?;
            }
        }

        Ok(())
    }
}

fn write(term: &Term, s: impl AsRef<str>) -> eyre::Result<()> {
    let colored = style(s.as_ref()).dim();
    term.write_line(&format!("{colored}"))
        .wrap_err("failed to write character on terminal")
}

fn symbol_test_result(test: &Test) -> StyledObject<&'static str> {
    match test.result {
        Ok(_) => symbol_success(),
        Err(_) => symbol_error(),
    }
}

fn symbol_success() -> StyledObject<&'static str> {
    style("✓").green()
}

fn symbol_error() -> StyledObject<&'static str> {
    style("✘").red()
}

fn emoji_symbol_test_result(test: &Test) -> char {
    match test.result {
        Ok(_) => '🟢',
        Err(_) => '🔴',
    }
}

#[allow(clippy::vec_box, dead_code)]
pub struct TableReporter {
    terminal: Term,
    buffer: HashMap<(ProjectName, ModuleName, TestName), Test>,
    capture_http: bool,
}

impl TableReporter {
    pub fn new(capture_http: bool) -> TableReporter {
        TableReporter {
            terminal: Term::stdout(),
            buffer: HashMap::new(),
            capture_http,
        }
    }
}

#[async_trait::async_trait]
impl Reporter for TableReporter {
    async fn run(&mut self) -> eyre::Result<()> {
        run(self).await?;

        let project_order: Vec<_> = get_tanu_config().projects.iter().map(|p| &p.name).collect();

        let mut builder = tabled::builder::Builder::default();
        builder.push_record(["Project", "Module", "Test", "Result"]);
        self.buffer
            .drain()
            .sorted_by(|(a, _), (b, _)| {
                let project_order_a = project_order
                    .iter()
                    .position(|&p| *p == a.0)
                    .unwrap_or(usize::MAX);
                let project_order_b = project_order
                    .iter()
                    .position(|&p| *p == b.0)
                    .unwrap_or(usize::MAX);

                project_order_a
                    .cmp(&project_order_b)
                    .then(a.1.cmp(&b.1))
                    .then(a.2.cmp(&b.2))
            })
            .for_each(|((p, m, t), test)| {
                builder.push_record([p, m, t, emoji_symbol_test_result(&test).to_string()])
            });

        let mut table = builder.build();
        table.with(tabled::settings::Style::modern()).with(
            tabled::settings::Modify::new(tabled::settings::object::Columns::single(3))
                .with(tabled::settings::Alignment::center()),
        );

        write(&self.terminal, format!("{table}")).wrap_err("failed to write table on terminal")?;

        Ok(())
    }

    async fn on_end(
        &mut self,
        project_name: String,
        module_name: String,
        test_name: String,
        test: Test,
    ) -> eyre::Result<()> {
        self.buffer
            .insert((project_name, module_name, test_name), test);
        Ok(())
    }
}
