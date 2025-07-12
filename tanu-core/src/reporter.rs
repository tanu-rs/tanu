//! # Test Reporter Module
//!
//! The reporter system provides pluggable output formatting for test results.
//! Reporters subscribe to test execution events and format them for different
//! output destinations (console, files, etc.). Multiple reporters can run
//! simultaneously to generate multiple output formats.
//!
//! ## Built-in Reporters
//!
//! - **`NullReporter`**: No output (useful for testing)
//! - **`ListReporter`**: Real-time streaming output with detailed logs
//! - **`TableReporter`**: Summary table output after all tests complete
//!
//! ## Custom Reporters
//!
//! Implement the `Reporter` trait to create custom output formats:
//!
//! ```rust,ignore
//! use tanu_core::reporter::Reporter;
//!
//! struct JsonReporter;
//!
//! #[async_trait::async_trait]
//! impl Reporter for JsonReporter {
//!     async fn on_end(
//!         &mut self,
//!         project: String,
//!         module: String,
//!         test_name: String,
//!         test: Test
//!     ) -> eyre::Result<()> {
//!         println!("{}", serde_json::to_string(&test)?);
//!         Ok(())
//!     }
//! }
//! ```

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

/// Available built-in reporter types.
///
/// Used for selecting which reporter to use via configuration or CLI arguments.
/// Each type corresponds to a different output format and behavior.
///
/// # Variants
///
/// - `Null`: No output, useful for testing or when output is not needed
/// - `List`: Real-time streaming output with detailed information
/// - `Table`: Summary table displayed after all tests complete
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

/// Trait for implementing custom test result reporting.
///
/// Reporters receive real-time events during test execution and can format
/// and output results in various ways. The trait uses the template method pattern:
/// implement the `on_*` methods to handle specific events, or override `run()`
/// for complete control.
///
/// # Event Flow
///
/// For each test, events are fired in this order:
/// 1. `on_start()` - Test begins
/// 2. `on_check()` - Each assertion (0 or more)
/// 3. `on_http_call()` - Each HTTP request (0 or more)
/// 4. `on_retry()` - If test fails and retry is configured
/// 5. `on_end()` - Test completes with final result
///
/// # Examples
///
/// ```rust,ignore
/// use tanu_core::reporter::Reporter;
/// use tanu_core::runner::Test;
///
/// struct SimpleReporter;
///
/// #[async_trait::async_trait]
/// impl Reporter for SimpleReporter {
///     async fn on_start(
///         &mut self,
///         project: String,
///         module: String,
///         test_name: String,
///     ) -> eyre::Result<()> {
///         println!("Starting {project}::{module}::{test_name}");
///         Ok(())
///     }
///
///     async fn on_end(
///         &mut self,
///         project: String,
///         module: String,
///         test_name: String,
///         test: Test,
///     ) -> eyre::Result<()> {
///         let status = if test.result.is_ok() { "PASS" } else { "FAIL" };
///         println!("{status}: {project}::{module}::{test_name}");
///         Ok(())
///     }
/// }
/// ```
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

/// A reporter that produces no output.
///
/// Useful for testing scenarios where you want to run tests without
/// any console output, or when implementing custom output handling
/// outside of the reporter system.
///
/// # Examples
///
/// ```rust,ignore
/// use tanu_core::{Runner, reporter::NullReporter};
///
/// let mut runner = Runner::new();
/// runner.add_reporter(NullReporter);
/// ```
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
/// A real-time streaming reporter that outputs test results as they happen.
///
/// This reporter provides immediate feedback during test execution, showing
/// test results, retry attempts, and optional HTTP request/response details.
/// Output is formatted with colors and symbols for easy readability.
///
/// # Features
///
/// - **Real-time output**: Results appear as tests complete
/// - **HTTP logging**: Optional detailed HTTP request/response logs
/// - **Retry indication**: Shows when tests are being retried
/// - **Colored output**: Success/failure indicators with colors
/// - **Test numbering**: Sequential numbering for easy reference
///
/// # Examples
///
/// ```rust,ignore
/// use tanu_core::{Runner, reporter::ListReporter};
///
/// let mut runner = Runner::new();
/// runner.add_reporter(ListReporter::new(true)); // Enable HTTP logging
/// ```
///
/// # Output Format
///
/// ```text
/// ✓ 1 [staging] api::health_check (45.2ms)
/// ✘ 2 [production] auth::login (123.4ms):
/// Error: Authentication failed
///   => POST https://api.example.com/auth/login
///   > request:
///     > headers:
///        > content-type: application/json
///   < response:
///     < headers:
///        < content-type: application/json
///     < body: {"error": "invalid credentials"}
/// ```
pub struct ListReporter {
    terminal: Term,
    buffer: IndexMap<(ProjectName, ModuleName, TestName), Buffer>,
    capture_http: bool,
}

impl ListReporter {
    /// Creates a new list reporter.
    ///
    /// # Parameters
    ///
    /// - `capture_http`: Whether to include HTTP request/response details in output
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use tanu_core::reporter::ListReporter;
    ///
    /// // With HTTP logging
    /// let reporter = ListReporter::new(true);
    ///
    /// // Without HTTP logging (faster, less verbose)
    /// let reporter = ListReporter::new(false);
    /// ```
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
/// A reporter that displays test results in a summary table after all tests complete.
///
/// This reporter buffers all test results and displays them in a formatted table
/// at the end of execution. Useful for getting an overview of all test results
/// without the noise of real-time output.
///
/// # Features
///
/// - **Summary table**: Clean tabular output after test completion
/// - **Project ordering**: Results ordered by project configuration
/// - **Emoji indicators**: Visual success/failure indicators
/// - **Modern styling**: Attractive table borders and formatting
///
/// # Examples
///
/// ```rust,ignore
/// use tanu_core::{Runner, reporter::TableReporter};
///
/// let mut runner = Runner::new();
/// runner.add_reporter(TableReporter::new(false)); // No HTTP details in table
/// ```
///
/// # Output Format
///
/// ```text
/// ┌─────────┬────────┬──────────────┬────────┐
/// │ Project │ Module │ Test         │ Result │
/// ├─────────┼────────┼──────────────┼────────┤
/// │ staging │ api    │ health_check │   🟢   │
/// │ staging │ auth   │ login        │   🔴   │
/// │ prod    │ api    │ status       │   🟢   │
/// └─────────┴────────┴──────────────┴────────┘
/// ```
pub struct TableReporter {
    terminal: Term,
    buffer: HashMap<(ProjectName, ModuleName, TestName), Test>,
    capture_http: bool,
}

impl TableReporter {
    /// Creates a new table reporter.
    ///
    /// # Parameters
    ///
    /// - `capture_http`: Whether to capture HTTP details (currently unused in table output)
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use tanu_core::reporter::TableReporter;
    ///
    /// let reporter = TableReporter::new(false);
    /// ```
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
