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
//! - **`LiveReporter`**: A live view of the running tests, with failures printed above it
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
use indexmap::IndexMap;
use std::{
    collections::VecDeque,
    io::Write,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{broadcast, mpsc};
use tracing::*;

use crate::{
    http,
    runner::{self, Event, EventBody, Test},
    CaptureHttpMode, MaxBodySize, ModuleName, ProjectName, TestName,
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
/// - `Live`: A live view of the running tests, with failures printed above it
#[derive(Debug, Clone, Default, PartialEq, Eq, strum::EnumString, strum::Display)]
#[strum(serialize_all = "snake_case")]
pub enum ReporterType {
    Null,
    #[default]
    List,
    Live,
}

async fn run<R: Reporter + Send + ?Sized>(reporter: &mut R) -> eyre::Result<()> {
    // Attempt to subscribe first
    let rx_result = runner::subscribe();

    // Always participate in barrier, even if subscribe failed
    // This prevents deadlock if any reporter fails to subscribe
    runner::wait_reporter_barrier().await;

    // Now check if subscribe succeeded
    let mut rx = rx_result?;

    loop {
        match rx.recv().await {
            Ok(event) => {
                if let Err(e) = dispatch(reporter, event).await {
                    warn!("reporter error: {e:#}");
                }
            }
            Err(broadcast::error::RecvError::Closed) => {
                debug!("runner channel has been closed");
                break;
            }
            Err(broadcast::error::RecvError::Lagged(_)) => {
                debug!("runner channel recv error");
                continue;
            }
        }
    }

    Ok(())
}

/// Calls the reporter method matching the event.
async fn dispatch<R: Reporter + Send + ?Sized>(reporter: &mut R, event: Event) -> eyre::Result<()> {
    let Event {
        project,
        module,
        test,
        body,
    } = event;
    match body {
        EventBody::Plan(plan) => reporter.on_plan(plan).await,
        EventBody::Start => reporter.on_start(project, module, test).await,
        EventBody::Check(check) => reporter.on_check(project, module, test, check).await,
        EventBody::Call(log) => reporter.on_call(project, module, test, log).await,
        EventBody::Retry(result) => reporter.on_retry(project, module, test, result).await,
        EventBody::End(result) => reporter.on_end(project, module, test, result).await,
        EventBody::Summary(summary) => reporter.on_summary(summary).await,
    }
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
/// `on_plan()` is called once before any test starts.
///
/// For each test, events are fired in this order:
/// 1. `on_start()` - Test begins
/// 2. `on_check()` - Each assertion (0 or more)
/// 3. `on_call()` - Each protocol call (HTTP, gRPC, etc.) (0 or more)
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

    /// Called once before any test starts, with the number of tests to run.
    async fn on_plan(&mut self, _plan: runner::TestPlan) -> eyre::Result<()> {
        Ok(())
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

    /// Called when a protocol call (HTTP, gRPC, etc.) is made.
    async fn on_call(
        &mut self,
        _project: String,
        _module: String,
        _test_name: String,
        _log: runner::CallLog,
    ) -> eyre::Result<()> {
        Ok(())
    }

    /// Called when a test case fails but to be retried.
    async fn on_retry(
        &mut self,
        _project: String,
        _module: String,
        _test_name: String,
        _test: Test,
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

    /// Called when all tests complete with summary statistics.
    async fn on_summary(&mut self, _summary: runner::TestSummary) -> eyre::Result<()> {
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
    /// Number of failed attempts that were retried so far.
    retries: usize,
    http_logs: Vec<Box<http::Log>>,
    #[cfg(feature = "grpc")]
    grpc_logs: Vec<Box<crate::grpc::Log>>,
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
/// ✘ 2 [production] auth::login: retrying (attempt 2)...
///     Error: Authentication failed
/// ✘ 2 [production] auth::login (123.4ms) (after 1 retry):
///     Error: Authentication failed
///   => POST https://api.example.com/auth/login
///   > request:
///     > headers:
///        > content-type: application/json
///   < response:
///     < status: 401 Unauthorized
///     < headers:
///        < content-type: application/json
///     < body: {"error": "invalid credentials"}
///
/// Failures:
///   ✘ [production] auth::login
///     Error: Authentication failed
///
/// Tests: 1 passed, 1 failed, 2 total, 1 retried
/// Time: 168.6ms (prep: 1.2ms)
/// ```
pub struct ListReporter {
    terminal: Term,
    buffer: IndexMap<(ProjectName, ModuleName, TestName), Buffer>,
    capture_http: CaptureHttpMode,
    max_body_size: MaxBodySize,
    /// Failed tests, recapped at the end of the run.
    failures: Vec<Failure>,
    /// Number of tests that were retried at least once.
    retried_tests: usize,
}

struct Failure {
    project: ProjectName,
    module: ModuleName,
    test: TestName,
    error: String,
}

impl ListReporter {
    /// Creates a new list reporter.
    ///
    /// # Parameters
    ///
    /// - `capture_http`: Controls when HTTP request/response details are shown in output
    /// - `max_body_size`: Caps how many body bytes are printed (`MaxBodySize(0)` = unlimited)
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use tanu_core::{reporter::ListReporter, CaptureHttpMode, MaxBodySize};
    ///
    /// // Show HTTP logs for all tests, truncating bodies at the default 16KB
    /// let reporter = ListReporter::new(CaptureHttpMode::All, MaxBodySize::default());
    ///
    /// // Show HTTP logs only for failed tests, printing bodies in full
    /// let reporter = ListReporter::new(CaptureHttpMode::OnFailure, MaxBodySize(0));
    ///
    /// // No HTTP logging
    /// let reporter = ListReporter::new(CaptureHttpMode::Off, MaxBodySize::default());
    /// ```
    pub fn new(capture_http: CaptureHttpMode, max_body_size: MaxBodySize) -> ListReporter {
        ListReporter {
            terminal: Term::stdout(),
            buffer: IndexMap::new(),
            capture_http,
            max_body_size,
            failures: Vec::new(),
            retried_tests: 0,
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

    async fn on_call(
        &mut self,
        project_name: String,
        module_name: String,
        test_name: String,
        log: runner::CallLog,
    ) -> eyre::Result<()> {
        if !matches!(self.capture_http, CaptureHttpMode::Off) {
            let buffer = self
                .buffer
                .get_mut(&(project_name, module_name, test_name.clone()))
                .ok_or_else(|| eyre::eyre!("test case \"{test_name}\" not found in the buffer"))?;
            match log {
                runner::CallLog::Http(http_log) => buffer.http_logs.push(http_log),
                #[cfg(feature = "grpc")]
                runner::CallLog::Grpc(grpc_log) => buffer.grpc_logs.push(grpc_log),
            }
        }
        Ok(())
    }

    async fn on_retry(
        &mut self,
        project_name: String,
        module_name: String,
        test_name: String,
        test: Test,
    ) -> eyre::Result<()> {
        let should_print = match self.capture_http {
            CaptureHttpMode::All => true,
            CaptureHttpMode::OnFailure => test.result.is_err(),
            CaptureHttpMode::Off => false,
        };

        let buffer = self
            .buffer
            .get_mut(&(project_name.clone(), module_name.clone(), test_name.clone()))
            .ok_or_else(|| eyre::eyre!("test case \"{test_name}\" not found in the buffer",))?;

        let test_number = *buffer.test_number.get_or_insert_with(generate_test_number);
        buffer.retries += 1;
        if buffer.retries == 1 {
            self.retried_tests += 1;
        }
        let next_attempt = buffer.retries + 1;
        let http_logs = std::mem::take(&mut buffer.http_logs);
        #[cfg(feature = "grpc")]
        let grpc_logs = std::mem::take(&mut buffer.grpc_logs);

        if let Err(e) = test.result {
            self.terminal.write_line(&format!(
                "{status} {test_number} {project} {path}: {retry_message}\n{error}",
                status = symbol_error(),
                test_number = style(test_number).dim(),
                project = style_project(&project_name),
                path = style_module_path(&module_name, &test_name),
                retry_message = style(format!("retrying (attempt {next_attempt})...")).blue(),
                error = style(indent(&format!("{e:#}"), ERROR_INDENT)).dim(),
            ))?;
        }

        if should_print {
            for log in &http_logs {
                write_http_log(&self.terminal, log, self.max_body_size)?;
            }
            #[cfg(feature = "grpc")]
            for log in &grpc_logs {
                write_grpc_log(&self.terminal, log)?;
            }
        }

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

        let should_print = match self.capture_http {
            CaptureHttpMode::All => true,
            CaptureHttpMode::OnFailure => test.result.is_err(),
            CaptureHttpMode::Off => false,
        };

        let status = symbol_test_result(&test);
        let Test {
            result,
            info,
            request_time,
            started_at: _,
            ended_at: _,
            worker_id: _,
        } = test;
        let test_number = style(buffer.test_number.get_or_insert_with(generate_test_number)).dim();
        let request_time = style(format!("({request_time:.2?})")).dim();
        let project = style_project(&project_name);
        let path = style_module_path(&info.module, &info.name);
        let retries = match buffer.retries {
            0 => String::new(),
            1 => format!(" {}", style("(after 1 retry)").yellow()),
            n => format!(" {}", style(format!("(after {n} retries)")).yellow()),
        };
        match result {
            Ok(_res) => {
                self.terminal.write_line(&format!(
                    "{status} {test_number} {project} {path} {request_time}{retries}"
                ))?;
            }
            Err(e) => {
                let error = format!("{e:#}");
                self.terminal.write_line(&format!(
                    "{status} {test_number} {project} {path} {request_time}{retries}:\n{error}",
                    error = style(indent(&error, ERROR_INDENT)).red()
                ))?;
                self.failures.push(Failure {
                    project: project_name,
                    module: info.module.clone(),
                    test: info.name.clone(),
                    error,
                });
            }
        }

        if should_print {
            for log in &buffer.http_logs {
                write_http_log(&self.terminal, log, self.max_body_size)?;
            }
            #[cfg(feature = "grpc")]
            for log in &buffer.grpc_logs {
                write_grpc_log(&self.terminal, log)?;
            }
        }

        Ok(())
    }

    async fn on_summary(&mut self, summary: runner::TestSummary) -> eyre::Result<()> {
        write_summary(&self.terminal, &self.failures, self.retried_tests, summary)
    }
}

/// Prints the failure recap and the `Tests:` / `Time:` summary lines.
fn write_summary(
    terminal: &Term,
    failures: &[Failure],
    retried_tests: usize,
    summary: runner::TestSummary,
) -> eyre::Result<()> {
    let runner::TestSummary {
        total_tests,
        passed_tests,
        failed_tests,
        skipped_tests,
        total_time,
        test_prep_time,
    } = summary;

    if !failures.is_empty() {
        terminal.write_line("")?;
        terminal.write_line(&style("Failures:").red().bold().to_string())?;
        for failure in failures {
            terminal.write_line(&format!(
                "  {} {} {}",
                symbol_error(),
                style_project(&failure.project),
                style_module_path(&failure.module, &failure.test),
            ))?;
            terminal.write_line(
                &style(indent(&failure.error, ERROR_INDENT))
                    .red()
                    .to_string(),
            )?;
        }
    }

    terminal.write_line("")?;
    let mut parts = vec![format!(
        "{} {}",
        style(passed_tests).green().bold(),
        style("passed").green()
    )];
    if failed_tests > 0 {
        parts.push(format!(
            "{} {}",
            style(failed_tests).red().bold(),
            style("failed").red()
        ));
    }
    if skipped_tests > 0 {
        parts.push(format!(
            "{} {}",
            style(skipped_tests).yellow().bold(),
            style("skipped").yellow()
        ));
    }
    parts.push(format!(
        "{} {}",
        style(total_tests).bold(),
        style("total").dim()
    ));
    if retried_tests > 0 {
        parts.push(format!(
            "{} {}",
            style(retried_tests).yellow().bold(),
            style("retried").yellow()
        ));
    }
    terminal.write_line(&format!("{}: {}", style("Tests").bold(), parts.join(", ")))?;
    terminal.write_line(&format!(
        "{}: {} ({}: {})",
        style("Time").bold(),
        style(format!("{total_time:.2?}")).cyan(),
        style("prep").dim(),
        style(format!("{test_prep_time:.2?}")).dim()
    ))?;

    Ok(())
}

/// Where Rust logs (`--capture-rust`) go while a live reporter owns the terminal.
///
/// Logs printed straight to stdout would land in the middle of the live region
/// and break its redraw, so while a [`LiveReporter`] is running, [`LogWriter`]
/// sends each log line here and the reporter shows it in the live region.
static LOG_SINK: Mutex<LogSink> = Mutex::new(LogSink::Stdout);

enum LogSink {
    /// No live reporter: logs are written to stdout.
    Stdout,
    /// A live reporter is running and shows logs in its log window.
    Live(mpsc::UnboundedSender<Vec<u8>>),
    /// A live reporter has finished. Its log window is gone, so later logs are
    /// dropped rather than printed below the summary.
    Discard,
}

fn set_log_sink(sink: LogSink) {
    if let Ok(mut guard) = LOG_SINK.lock() {
        *guard = sink;
    }
}

/// A `tracing_subscriber` writer for Rust logs, which sends them wherever
/// [`LOG_SINK`] says.
///
/// `tracing_subscriber` makes one writer per log event, so the buffered bytes
/// are sent as a whole when the writer is dropped and lines are never split.
#[derive(Default)]
pub(crate) struct LogWriter {
    buf: Vec<u8>,
}

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for LogWriter {
    fn drop(&mut self) {
        if self.buf.is_empty() {
            return;
        }
        let buf = std::mem::take(&mut self.buf);
        let buf = match LOG_SINK.lock().as_deref() {
            Ok(LogSink::Live(sink)) => match sink.send(buf) {
                Ok(()) => return,
                Err(mpsc::error::SendError(buf)) => buf,
            },
            Ok(LogSink::Discard) => return,
            Ok(LogSink::Stdout) | Err(_) => buf,
        };
        let _ = std::io::stdout().write_all(&buf);
    }
}

/// A reporter with a live view of the running tests.
///
/// At the bottom of the terminal it keeps a live region, redrawn every
/// [`LIVE_REFRESH`], with one line per running test (spinner, elapsed time and
/// retry count) and a progress line. Failures are printed in full above it as
/// they happen, and passing tests, including ones that passed after a retry,
/// leave nothing behind. With `--capture-http all`, each HTTP/gRPC call is shown
/// as one line in a fixed-height window at the top of the live region, and with
/// `--capture-rust`, so are the latest Rust logs. When stdout is not a
/// terminal, the live region is never drawn, so only failures and the summary
/// (and Rust logs, as they come) are printed.
///
/// # Output Format
///
/// ```text
///  FAIL   [production] auth::login  123.40ms · after 1 retry
///     Error: Authentication failed
///
/// ────────────────────────────────────────────────────────────────
///   ⠋ [staging] api::users::create  1.2s
///   ⠋ [staging] api::orders::list   6.4s  ↻ retry 1
///  ━━━━━━━━━──────────────────── 12/40  ✓ 11  ✘ 1  ● 2 running  2.3s
/// ```
pub struct LiveReporter {
    terminal: Term,
    is_term: bool,
    /// Tests that have started but not ended, in start order.
    running: IndexMap<(ProjectName, ModuleName, TestName), Running>,
    capture_http: CaptureHttpMode,
    max_body_size: MaxBodySize,
    total: usize,
    done: usize,
    /// When the plan arrived; the live region is drawn only between the plan and the summary.
    started_at: Option<Instant>,
    finished: bool,
    /// Number of terminal lines the live region currently occupies.
    drawn: usize,
    /// Spinner animation frame.
    frame: usize,
    failures: Vec<Failure>,
    retried_tests: usize,
    /// The most recent Rust log lines (`--capture-rust`).
    logs: Window,
    /// One line per recent HTTP/gRPC call (`--capture-http all`).
    calls: Window,
}

/// The latest lines of a stream (logs, calls), shown in a fixed-height window
/// in the live region.
struct Window {
    title: &'static str,
    lines: VecDeque<String>,
}

impl Window {
    fn new(title: &'static str) -> Window {
        Window {
            title,
            lines: VecDeque::with_capacity(WINDOW_LINES),
        }
    }

    fn push(&mut self, line: String) {
        if self.lines.len() == WINDOW_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }
}

/// A test that is currently running.
struct Running {
    started_at: Instant,
    buffer: Buffer,
}

/// How often the live region is redrawn.
const LIVE_REFRESH: Duration = Duration::from_millis(100);

/// Maximum number of running tests listed in the live region.
const MAX_RUNNING_LINES: usize = 10;

/// Height of a window (logs, HTTP calls) in the live region.
const WINDOW_LINES: usize = 8;

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

impl LiveReporter {
    /// Creates a new live reporter.
    ///
    /// Takes the same parameters as [`ListReporter::new`]. With
    /// `CaptureHttpMode::All`, HTTP logs of passing tests are printed too.
    pub fn new(capture_http: CaptureHttpMode, max_body_size: MaxBodySize) -> LiveReporter {
        let terminal = Term::stdout();
        LiveReporter {
            is_term: terminal.is_term(),
            terminal,
            running: IndexMap::new(),
            capture_http,
            max_body_size,
            total: 0,
            done: 0,
            started_at: None,
            finished: false,
            drawn: 0,
            frame: 0,
            failures: Vec::new(),
            retried_tests: 0,
            logs: Window::new("logs"),
            calls: Window::new("http"),
        }
    }

    /// Erases the live region so that output can be printed in its place.
    /// The next tick draws it again below that output.
    fn clear_live(&mut self) -> eyre::Result<()> {
        if self.drawn > 0 {
            self.terminal
                .write_str(&format!("\x1b[{}A\r\x1b[J", self.drawn))?;
            self.drawn = 0;
        }
        Ok(())
    }

    /// Redraws the live region in place with a single write, so it doesn't flicker.
    fn draw_live(&mut self) -> eyre::Result<()> {
        let Some(started_at) = self.started_at else {
            return Ok(());
        };
        if !self.is_term || self.finished {
            return Ok(());
        }
        let (height, width) = self.terminal.size();
        let now = Instant::now();
        let running: Vec<RunningTest> = self
            .running
            .iter()
            .map(|((project, module, test), r)| RunningTest {
                project,
                module,
                test,
                elapsed: now.duration_since(r.started_at),
                retries: r.buffer.retries,
            })
            .collect();
        let failed = self.failures.len();
        let progress = Progress {
            done: self.done,
            total: self.total,
            passed: self.done - failed,
            failed,
            elapsed: now.duration_since(started_at),
        };
        // A window appears once it has a line, then keeps its height.
        let windows: Vec<WindowView> = [&self.calls, &self.logs]
            .into_iter()
            .filter(|w| !w.lines.is_empty())
            .map(|w| WindowView {
                title: w.title,
                lines: w.lines.iter().map(String::as_str).collect(),
            })
            .collect();
        // Keep one row free so the region never scrolls the screen. Windows share
        // the space with the running tests: one window gets up to a third, two get
        // up to a quarter each.
        let usable = (height as usize).saturating_sub(1);
        let window_height = WINDOW_LINES.min(usable / (windows.len() + 2));
        let window_rows = windows.len() * (window_height + 1);
        // Rows for the rule and the progress line.
        let max_running = MAX_RUNNING_LINES.min(usable.saturating_sub(2 + window_rows));
        // One column is kept free: a line that fills the whole row can wrap, which
        // would make the region taller than we think and break the redraw.
        let lines = live_region(
            &running,
            &progress,
            &windows,
            self.frame,
            Layout {
                max_running,
                window_height,
                width: (width as usize).saturating_sub(1),
            },
        );

        let mut out = String::new();
        if self.drawn > 0 {
            out.push_str(&format!("\x1b[{}A\r\x1b[J", self.drawn));
        }
        for line in &lines {
            out.push_str(line);
            out.push('\n');
        }
        self.terminal.write_str(&out)?;
        self.drawn = lines.len();
        Ok(())
    }

    /// Adds Rust log output to the log window, which keeps only the latest lines.
    fn push_log(&mut self, log: Vec<u8>) {
        for line in String::from_utf8_lossy(&log).lines() {
            if line.trim().is_empty() {
                continue;
            }
            // Tabs and carriage returns would throw off the width of the line.
            self.logs.push(line.replace('\t', "    ").replace('\r', ""));
        }
    }

    fn write_logs(&self, buffer: &Buffer) -> eyre::Result<()> {
        for log in &buffer.http_logs {
            write_http_log(&self.terminal, log, self.max_body_size)?;
        }
        #[cfg(feature = "grpc")]
        for log in &buffer.grpc_logs {
            write_grpc_log(&self.terminal, log)?;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Reporter for LiveReporter {
    /// Like the default event loop, but also redraws the live region on a timer,
    /// so the spinners and elapsed times move while tests are running.
    async fn run(&mut self) -> eyre::Result<()> {
        let rx_result = runner::subscribe();
        runner::wait_reporter_barrier().await;
        let mut rx = rx_result?;

        let mut ticker = tokio::time::interval(LIVE_REFRESH);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // Route Rust logs through this loop, so they are printed above the live region.
        let (log_tx, mut log_rx) = mpsc::unbounded_channel();
        if self.is_term {
            set_log_sink(LogSink::Live(log_tx));
        }

        loop {
            tokio::select! {
                Some(log) = log_rx.recv() => self.push_log(log),
                res = rx.recv() => match res {
                    Ok(event) => {
                        if let Err(e) = dispatch(self, event).await {
                            warn!("reporter error: {e:#}");
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        debug!("runner channel has been closed");
                        break;
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        debug!("runner channel recv error");
                        continue;
                    }
                },
                _ = ticker.tick() => {
                    self.frame = self.frame.wrapping_add(1);
                    if let Err(e) = self.draw_live() {
                        warn!("reporter error: {e:#}");
                    }
                }
            }
        }

        if self.is_term {
            set_log_sink(LogSink::Discard);
        }
        self.finished = true;
        self.clear_live()
    }

    async fn on_plan(&mut self, plan: runner::TestPlan) -> eyre::Result<()> {
        self.total = plan.total_tests;
        self.started_at = Some(Instant::now());
        Ok(())
    }

    async fn on_start(
        &mut self,
        project_name: String,
        module_name: String,
        test_name: String,
    ) -> eyre::Result<()> {
        self.running.insert(
            (project_name, module_name, test_name),
            Running {
                started_at: Instant::now(),
                buffer: Buffer::default(),
            },
        );
        Ok(())
    }

    async fn on_call(
        &mut self,
        project_name: String,
        module_name: String,
        test_name: String,
        log: runner::CallLog,
    ) -> eyre::Result<()> {
        if matches!(self.capture_http, CaptureHttpMode::All) {
            self.calls
                .push(call_line(&log, short_module(&module_name), &test_name));
        }
        if !matches!(self.capture_http, CaptureHttpMode::Off) {
            let buffer = &mut self
                .running
                .get_mut(&(project_name, module_name, test_name.clone()))
                .ok_or_else(|| eyre::eyre!("test case \"{test_name}\" not found in the buffer"))?
                .buffer;
            match log {
                runner::CallLog::Http(http_log) => buffer.http_logs.push(http_log),
                #[cfg(feature = "grpc")]
                runner::CallLog::Grpc(grpc_log) => buffer.grpc_logs.push(grpc_log),
            }
        }
        Ok(())
    }

    async fn on_retry(
        &mut self,
        project_name: String,
        module_name: String,
        test_name: String,
        _test: Test,
    ) -> eyre::Result<()> {
        let buffer = &mut self
            .running
            .get_mut(&(project_name, module_name, test_name.clone()))
            .ok_or_else(|| eyre::eyre!("test case \"{test_name}\" not found in the buffer"))?
            .buffer;
        buffer.retries += 1;
        if buffer.retries == 1 {
            self.retried_tests += 1;
        }
        // Only the final attempt's logs are printed, so drop the failed attempt's ones.
        buffer.http_logs.clear();
        #[cfg(feature = "grpc")]
        buffer.grpc_logs.clear();

        // Nothing is printed: the live region tags the test with its retry count while it
        // runs, and if it finally fails, its FAIL line says how many retries it took.
        Ok(())
    }

    async fn on_end(
        &mut self,
        project_name: String,
        module_name: String,
        test_name: String,
        test: Test,
    ) -> eyre::Result<()> {
        let Running { buffer, .. } = self
            .running
            .shift_remove(&(project_name.clone(), module_name, test_name.clone()))
            .ok_or_else(|| eyre::eyre!("test case \"{test_name}\" not found in the buffer"))?;
        self.done += 1;

        let Test {
            result,
            info,
            request_time,
            ..
        } = test;
        let mut details = format!("{request_time:.2?}");
        match buffer.retries {
            0 => {}
            1 => details.push_str(" · after 1 retry"),
            n => details.push_str(&format!(" · after {n} retries")),
        }
        let details = style(details).dim();
        let print_logs = matches!(
            (&self.capture_http, &result),
            (CaptureHttpMode::All, _) | (CaptureHttpMode::OnFailure, Err(_))
        );

        if let Err(e) = result {
            let error = format!("{e:#}");
            self.clear_live()?;
            self.terminal.write_line(&format!(
                "{badge} {project} {path}  {details}\n{error_text}",
                badge = badge("FAIL"),
                project = style_project(&project_name),
                path = style_module_path(&info.module, &info.name),
                error_text = style(indent(&error, ERROR_INDENT)).red(),
            ))?;
            if print_logs {
                self.write_logs(&buffer)?;
            }
            self.terminal.write_line("")?;
            self.failures.push(Failure {
                project: project_name,
                module: info.module.clone(),
                test: info.name.clone(),
                error,
            });
        } else if print_logs
            && !self.is_term
            && !(buffer.http_logs.is_empty() && grpc_logs_empty(&buffer))
        {
            // On a terminal, the calls of passing tests were shown in the http window.
            self.clear_live()?;
            self.terminal.write_line(&format!(
                "{} {} {}  {details}",
                badge("PASS"),
                style_project(&project_name),
                style_module_path(&info.module, &info.name),
            ))?;
            self.write_logs(&buffer)?;
        }

        Ok(())
    }

    async fn on_summary(&mut self, summary: runner::TestSummary) -> eyre::Result<()> {
        self.finished = true;
        self.clear_live()?;
        write_summary(&self.terminal, &self.failures, self.retried_tests, summary)
    }
}

#[cfg(feature = "grpc")]
fn grpc_logs_empty(buffer: &Buffer) -> bool {
    buffer.grpc_logs.is_empty()
}

#[cfg(not(feature = "grpc"))]
fn grpc_logs_empty(_buffer: &Buffer) -> bool {
    true
}

/// A running test as shown in the live region.
struct RunningTest<'a> {
    project: &'a str,
    module: &'a str,
    test: &'a str,
    elapsed: Duration,
    retries: usize,
}

/// Overall progress as shown on the last line of the live region.
struct Progress {
    done: usize,
    total: usize,
    passed: usize,
    failed: usize,
    elapsed: Duration,
}

/// A window's title and lines, as shown in the live region.
struct WindowView<'a> {
    title: &'a str,
    lines: Vec<&'a str>,
}

/// Size limits of the live region.
struct Layout {
    /// Maximum number of running tests listed.
    max_running: usize,
    /// Height of each window.
    window_height: usize,
    /// Terminal width every line is truncated to.
    width: usize,
}

/// Builds the lines of the live region: the windows, up to `max_running`
/// running tests, then the progress line.
fn live_region(
    running: &[RunningTest],
    progress: &Progress,
    windows: &[WindowView],
    frame: usize,
    layout: Layout,
) -> Vec<String> {
    let Layout {
        max_running,
        window_height,
        width,
    } = layout;
    let spinner = style(SPINNER[frame % SPINNER.len()]).cyan();
    // With too many tests to list, keep one row for the "… and N more" line.
    let shown = if running.len() > max_running {
        max_running.saturating_sub(1)
    } else {
        running.len()
    };

    let mut lines = Vec::new();

    // Pad the test names so the elapsed times line up in a column.
    let names: Vec<String> = running[..shown]
        .iter()
        .map(|r| {
            format!(
                "{} {}",
                style_project(r.project),
                style_module_path(short_module(r.module), r.test)
            )
        })
        .collect();
    let name_width = names
        .iter()
        .map(|n| console::measure_text_width(n))
        .max()
        .unwrap_or(0);
    for (r, name) in running[..shown].iter().zip(&names) {
        let mut line = format!(
            "  {spinner} {} {}",
            console::pad_str(name, name_width, console::Alignment::Left, None),
            style_elapsed(r.elapsed),
        );
        if r.retries > 0 {
            line.push_str(&format!(
                "  {}",
                style(format!("↻ retry {}", r.retries)).yellow().dim()
            ));
        }
        lines.push(line);
    }
    if shown < running.len() {
        lines.push(
            style(format!("  … and {} more", running.len() - shown))
                .dim()
                .to_string(),
        );
    }

    let bar_width = (width / 3).clamp(10, 30);
    let mut status = format!(
        " {}  {}{}",
        progress_bar(progress.done, progress.failed, progress.total, bar_width),
        style(progress.done).bold(),
        style(format!("/{}", progress.total)).dim(),
    );
    status.push_str(&format!(
        "  {}",
        style(format!("✓ {}", progress.passed)).green()
    ));
    if progress.failed > 0 {
        status.push_str(&format!(
            "  {}",
            style(format!("✘ {}", progress.failed)).red().bold()
        ));
    }
    status.push_str(&format!(
        "  {}  {}",
        style(format!("● {} running", running.len())).cyan(),
        style(format_elapsed(progress.elapsed)).dim(),
    ));
    lines.push(status);

    let rule = style("─".repeat(width)).dim().to_string();
    let mut region = Vec::new();
    for window in windows.iter().filter(|_| window_height > 0) {
        // Rules are built to fit, so they are not truncated.
        let label = format!("── {} ", window.title);
        region.push(
            style(format!(
                "{label}{}",
                "─".repeat(width.saturating_sub(label.chars().count()))
            ))
            .dim()
            .to_string(),
        );
        // Latest lines at the bottom; blank rows keep the window's height fixed.
        let shown = &window.lines[window.lines.len().saturating_sub(window_height)..];
        region.extend(std::iter::repeat_n(
            String::new(),
            window_height - shown.len(),
        ));
        region.extend(
            shown
                .iter()
                .map(|line| console::truncate_str(&format!("  {line}"), width, "…").into_owned()),
        );
    }
    region.push(rule);
    region.extend(
        lines
            .into_iter()
            .map(|line| console::truncate_str(&line, width, "…").into_owned()),
    );
    region
}

/// One line for the http window, e.g. `GET 200 OK http://host/path 12.00ms · api::create`.
fn call_line(log: &runner::CallLog, module: &str, test: &str) -> String {
    let (call, duration) = match log {
        runner::CallLog::Http(log) => (
            format!(
                "{} {} {}",
                style_http_method(log.request.method.as_ref()),
                style_status_code(log.response.status),
                log.request.url,
            ),
            log.response.duration_req,
        ),
        #[cfg(feature = "grpc")]
        runner::CallLog::Grpc(log) => (
            format!(
                "{} {} {}",
                style("gRPC").magenta(),
                style_grpc_status(log.response.status_code),
                log.request.method,
            ),
            log.response.duration,
        ),
    };
    format!(
        "{call} {}  {}",
        style(format!("{duration:.2?}")).dim(),
        style(format!("· {module}::{test}")).dim(),
    )
}

/// Draws a progress bar of `width` cells: green for passed tests, red for
/// failed ones, and dim for tests that have not finished yet. A failure always
/// takes at least one cell, so it is never hidden by rounding.
fn progress_bar(done: usize, failed: usize, total: usize, width: usize) -> String {
    let cells = |n: usize| (n * width).checked_div(total).unwrap_or(0);
    let failed_cells = if failed > 0 && total > 0 {
        cells(failed).max(1)
    } else {
        0
    };
    let done_cells = cells(done).max(failed_cells).min(width);
    let failed_cells = failed_cells.min(done_cells);
    format!(
        "{}{}{}",
        style("━".repeat(done_cells - failed_cells)).green(),
        style("━".repeat(failed_cells)).red(),
        style("─".repeat(width - done_cells)).dim(),
    )
}

/// Drops the crate name from a module path, since it is the same for every
/// test: `my_tests::api::users` becomes `api::users`.
fn short_module(module: &str) -> &str {
    module.split_once("::").map_or(module, |(_, rest)| rest)
}

/// Formats a duration as seconds, or minutes and seconds past a minute.
fn format_elapsed(elapsed: Duration) -> String {
    let secs = elapsed.as_secs_f64();
    if secs < 60.0 {
        format!("{secs:.1}s")
    } else {
        format!("{}m{:02}s", elapsed.as_secs() / 60, elapsed.as_secs() % 60)
    }
}

/// Elapsed time of a running test: dim while fast, yellow once slow, red when very slow.
fn style_elapsed(elapsed: Duration) -> StyledObject<String> {
    let text = format_elapsed(elapsed);
    match elapsed.as_secs() {
        0..5 => style(text).dim(),
        5..30 => style(text).yellow(),
        _ => style(text).red(),
    }
}

/// A colored label such as ` FAIL `, used to mark lines printed above the live region.
fn badge(label: &str) -> StyledObject<String> {
    // Same width for every label, so the paths after the badges line up.
    let text = format!(" {label:^5} ");
    match label {
        "FAIL" => style(text).on_red().black().bold(),
        _ => style(text).on_green().black().bold(),
    }
}

/// Indentation for error messages printed under a test line.
const ERROR_INDENT: usize = 4;

/// Indents every line of `text` by `width` spaces.
fn indent(text: &str, width: usize) -> String {
    let pad = " ".repeat(width);
    text.lines()
        .map(|line| format!("{pad}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
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

/// Color HTTP methods for visual distinction
fn style_http_method(method: &str) -> StyledObject<&str> {
    match method.to_uppercase().as_str() {
        "GET" => style(method).green(),
        "POST" => style(method).yellow(),
        "PUT" => style(method).blue(),
        "DELETE" => style(method).red(),
        "PATCH" => style(method).magenta(),
        "HEAD" => style(method).cyan(),
        "OPTIONS" => style(method).white(),
        _ => style(method),
    }
}

/// Color HTTP status codes by category
fn style_status_code(status: http::StatusCode) -> StyledObject<String> {
    let reason = status.canonical_reason().unwrap_or("");
    let s = format!("{} {}", status.as_u16(), reason);
    match status.as_u16() {
        100..=199 => style(s).cyan(),       // Informational
        200..=299 => style(s).green(),      // Success
        300..=399 => style(s).yellow(),     // Redirection
        400..=499 => style(s).red(),        // Client error
        500..=599 => style(s).red().bold(), // Server error
        _ => style(s),
    }
}

/// Color gRPC status codes
#[cfg(feature = "grpc")]
fn style_grpc_status(code: tonic::Code) -> StyledObject<String> {
    let s = format!("{:?}", code);
    match code {
        tonic::Code::Ok => style(s).green(),
        tonic::Code::Cancelled
        | tonic::Code::Unknown
        | tonic::Code::DeadlineExceeded
        | tonic::Code::ResourceExhausted
        | tonic::Code::Aborted
        | tonic::Code::Unavailable => style(s).yellow(),
        tonic::Code::InvalidArgument
        | tonic::Code::NotFound
        | tonic::Code::AlreadyExists
        | tonic::Code::PermissionDenied
        | tonic::Code::FailedPrecondition
        | tonic::Code::OutOfRange
        | tonic::Code::Unauthenticated => style(s).red(),
        tonic::Code::Unimplemented | tonic::Code::Internal | tonic::Code::DataLoss => {
            style(s).red().bold()
        }
    }
}

/// Style project name with bold magenta color
fn style_project(name: &str) -> StyledObject<String> {
    style(format!("[{name}]")).magenta().bold()
}

/// Style module path with cyan color and test name in bold blue
fn style_module_path(module: &str, test: &str) -> String {
    format!("{}::{}", style(module).cyan(), style(test).blue().bold())
}

/// Recursively formats a JSON value with ANSI terminal colors.
///
/// Keys are rendered in bold cyan, strings in green, numbers in yellow,
/// booleans in magenta, and null in dim. Structural characters are dim.
fn colorize_json(value: &serde_json::Value, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let inner = "  ".repeat(indent + 1);
    match value {
        serde_json::Value::Null => style("null").dim().to_string(),
        serde_json::Value::Bool(b) => style(b.to_string()).magenta().to_string(),
        serde_json::Value::Number(n) => style(n.to_string()).yellow().to_string(),
        serde_json::Value::String(s) => {
            let repr = serde_json::to_string(s).unwrap_or_else(|_| format!("\"{s}\""));
            style(repr).green().to_string()
        }
        serde_json::Value::Array(arr) => {
            if arr.is_empty() {
                return style("[]").dim().to_string();
            }
            let items: Vec<String> = arr
                .iter()
                .map(|v| format!("{}{}", inner, colorize_json(v, indent + 1)))
                .collect();
            let comma = style(",").dim().to_string();
            format!(
                "{}\n{}\n{}{}",
                style("[").dim(),
                items.join(&format!("{comma}\n")),
                pad,
                style("]").dim()
            )
        }
        serde_json::Value::Object(map) => {
            if map.is_empty() {
                return style("{}").dim().to_string();
            }
            let items: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    let key_repr = serde_json::to_string(k).unwrap_or_else(|_| format!("\"{k}\""));
                    let key = style(key_repr).cyan().bold().to_string();
                    let colon = style(":").dim().to_string();
                    format!("{}{}{} {}", inner, key, colon, colorize_json(v, indent + 1))
                })
                .collect();
            let comma = style(",").dim().to_string();
            let open = style("{").dim().to_string();
            let close = format!("{}{}", pad, style("}").dim());
            format!("{}\n{}\n{}", open, items.join(&format!("{comma}\n")), close)
        }
    }
}

/// Formats a body string for terminal display. Parses and colorizes JSON when
/// the content-type is `application/json`; otherwise returns the text as-is.
fn format_body_for_display(body: &str, content_type: &str, max_body_size: MaxBodySize) -> String {
    // Over the cap, skip pretty-printing entirely: truncating colorized output
    // would cut an ANSI escape in half, and colorizing a body we are about to
    // discard most of is wasted work.
    if let Some(truncated) = max_body_size.truncate(body) {
        return truncated;
    }
    if content_type.to_lowercase().starts_with("application/json") {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
            return colorize_json(&json, 0);
        }
    }
    body.to_string()
}

fn write_http_log(
    terminal: &Term,
    log: &http::Log,
    max_body_size: MaxBodySize,
) -> eyre::Result<()> {
    terminal.write_line(&format!(
        " {} {} {}",
        style("=>").cyan(),
        style_http_method(log.request.method.as_ref()),
        style(&log.request.url.to_string()).underlined()
    ))?;
    terminal.write_line(&format!(
        "  {} {}",
        style(">").cyan(),
        style("request:").cyan()
    ))?;
    terminal.write_line(&format!(
        "    {} {}",
        style(">").cyan(),
        style("headers:").dim()
    ))?;
    for key in log.request.headers.keys() {
        terminal.write_line(&format!(
            "       {} {}: {}",
            style(">").cyan(),
            style(key.as_str()).bold(),
            style(log.request.headers.get(key).unwrap().to_str().unwrap()).dim()
        ))?;
    }
    if let Some(ref body) = log.request.body {
        if !body.is_empty() {
            terminal.write_line(&format!(
                "    {} {}",
                style(">").cyan(),
                style("body:").dim()
            ))?;
            let req_ct = log
                .request
                .headers
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            for line in format_body_for_display(body, req_ct, max_body_size).lines() {
                terminal.write_line(&format!("       {}", line))?;
            }
        }
    }
    terminal.write_line(&format!(
        "  {} {}",
        style("<").yellow(),
        style("response:").yellow()
    ))?;
    terminal.write_line(&format!(
        "    {} {} {}",
        style("<").yellow(),
        style("status:").dim(),
        style_status_code(log.response.status)
    ))?;
    terminal.write_line(&format!(
        "    {} {}",
        style("<").yellow(),
        style("headers:").dim()
    ))?;
    for key in log.response.headers.keys() {
        terminal.write_line(&format!(
            "       {} {}: {}",
            style("<").yellow(),
            style(key.as_str()).bold(),
            style(log.response.headers.get(key).unwrap().to_str().unwrap()).dim()
        ))?;
    }
    terminal.write_line(&format!(
        "    {} {}",
        style("<").yellow(),
        style("body:").dim()
    ))?;
    if !log.response.body.is_empty() {
        let res_ct = log
            .response
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        for line in format_body_for_display(&log.response.body, res_ct, max_body_size).lines() {
            terminal.write_line(&format!("       {}", line))?;
        }
    }
    Ok(())
}

#[cfg(feature = "grpc")]
fn write_grpc_log(terminal: &Term, log: &crate::grpc::Log) -> eyre::Result<()> {
    terminal.write_line(&format!(
        " {} {} {}",
        style("=>").magenta(),
        style("gRPC").magenta().bold(),
        style(&log.request.method).underlined()
    ))?;
    terminal.write_line(&format!(
        "  {} {}",
        style(">").magenta(),
        style("request:").magenta()
    ))?;
    terminal.write_line(&format!(
        "    {} {}",
        style(">").magenta(),
        style("metadata:").dim()
    ))?;
    for key_value in log.request.metadata.iter() {
        let (key, value) = match key_value {
            tonic::metadata::KeyAndValueRef::Ascii(k, v) => (
                k.as_str().to_string(),
                v.to_str().unwrap_or("<binary>").to_string(),
            ),
            tonic::metadata::KeyAndValueRef::Binary(k, v) => (
                k.as_str().to_string(),
                format!("<binary: {} bytes>", v.as_encoded_bytes().len()),
            ),
        };
        terminal.write_line(&format!(
            "       {} {}: {}",
            style(">").magenta(),
            style(&key).bold(),
            style(&value).dim()
        ))?;
    }
    if !log.request.message.is_empty() {
        terminal.write_line(&format!(
            "    {} {} {}",
            style(">").magenta(),
            style("message:").dim(),
            style(format!("{} bytes", log.request.message.len())).dim()
        ))?;
    }
    terminal.write_line(&format!(
        "  {} {} {}",
        style("<").yellow(),
        style("response:").yellow(),
        style_grpc_status(log.response.status_code)
    ))?;
    terminal.write_line(&format!(
        "    {} {}",
        style("<").yellow(),
        style("metadata:").dim()
    ))?;
    for key_value in log.response.metadata.iter() {
        let (key, value) = match key_value {
            tonic::metadata::KeyAndValueRef::Ascii(k, v) => (
                k.as_str().to_string(),
                v.to_str().unwrap_or("<binary>").to_string(),
            ),
            tonic::metadata::KeyAndValueRef::Binary(k, v) => (
                k.as_str().to_string(),
                format!("<binary: {} bytes>", v.as_encoded_bytes().len()),
            ),
        };
        terminal.write_line(&format!(
            "       {} {}: {}",
            style("<").yellow(),
            style(&key).bold(),
            style(&value).dim()
        ))?;
    }
    if !log.response.message.is_empty() {
        terminal.write_line(&format!(
            "    {} {} {}",
            style("<").yellow(),
            style("message:").dim(),
            style(format!("{} bytes", log.response.message.len())).dim()
        ))?;
    }
    if !log.response.status_message.is_empty() {
        terminal.write_line(&format!(
            "    {} {} {}",
            style("<").yellow(),
            style("status_message:").dim(),
            style(&log.response.status_message).dim()
        ))?;
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    fn running(test: &str) -> RunningTest<'_> {
        RunningTest {
            project: "dev",
            module: "my_tests::users",
            test,
            elapsed: Duration::from_millis(1200),
            retries: 0,
        }
    }

    fn progress() -> Progress {
        Progress {
            done: 3,
            total: 10,
            passed: 2,
            failed: 1,
            elapsed: Duration::from_millis(2300),
        }
    }

    fn layout(max_running: usize, window_height: usize, width: usize) -> Layout {
        Layout {
            max_running,
            window_height,
            width,
        }
    }

    fn window<'a>(title: &'a str, lines: &[&'a str]) -> WindowView<'a> {
        WindowView {
            title,
            lines: lines.to_vec(),
        }
    }

    #[test]
    fn live_region_shows_a_fixed_height_log_window() {
        let lines = plain(live_region(
            &[],
            &progress(),
            &[window("logs", &["one", "two", "three"])],
            0,
            layout(10, 2, 30),
        ));
        assert_eq!(lines[0], format!("── logs {}", "─".repeat(22)));
        assert_eq!(lines[1..3], ["  two", "  three"]);
        assert_eq!(lines[3], "─".repeat(30));

        // Fewer logs than the window: blank rows keep the height.
        let lines = plain(live_region(
            &[],
            &progress(),
            &[window("logs", &["one"])],
            0,
            layout(10, 3, 30),
        ));
        assert_eq!(lines[1..4], ["", "", "  one"]);
    }

    #[test]
    fn live_region_stacks_windows() {
        let lines = plain(live_region(
            &[],
            &progress(),
            &[window("http", &["GET"]), window("logs", &["INFO"])],
            0,
            layout(10, 1, 30),
        ));
        assert_eq!(
            lines[..5],
            [
                format!("── http {}", "─".repeat(22)),
                "  GET".to_string(),
                format!("── logs {}", "─".repeat(22)),
                "  INFO".to_string(),
                "─".repeat(30),
            ]
        );
    }

    #[test]
    fn call_line_summarizes_an_http_call() {
        let log = runner::CallLog::Http(Box::new(http::Log {
            request: http::LogRequest {
                url: "http://localhost/users?id=1".parse().unwrap(),
                method: http::Method::POST,
                headers: Default::default(),
                body: None,
            },
            response: http::LogResponse {
                status: http::StatusCode::CREATED,
                duration_req: Duration::from_millis(12),
                ..Default::default()
            },
            started_at: std::time::SystemTime::now(),
            ended_at: std::time::SystemTime::now(),
        }));
        assert_eq!(
            console::strip_ansi_codes(&call_line(&log, "api", "create")),
            "POST 201 Created http://localhost/users?id=1 12.00ms  · api::create"
        );
    }

    fn plain(lines: Vec<String>) -> Vec<String> {
        lines
            .iter()
            .map(|l| console::strip_ansi_codes(l).into_owned())
            .collect()
    }

    #[test]
    fn live_region_lists_every_running_test() {
        let mut retried = running("bb");
        retried.retries = 1;
        retried.elapsed = Duration::from_secs(6);
        let lines = plain(live_region(
            &[running("a"), retried],
            &progress(),
            &[],
            0,
            layout(10, 0, 60),
        ));
        assert_eq!(
            lines,
            [
                "─".repeat(60),
                "  ⠋ [dev] users::a  1.2s".to_string(),
                "  ⠋ [dev] users::bb 6.0s  ↻ retry 1".to_string(),
                " ━━━━━━──────────────  3/10  ✓ 2  ✘ 1  ● 2 running  2.3s".to_string(),
            ]
        );
    }

    #[test]
    fn live_region_collapses_overflow() {
        let tests = ["a", "b", "c", "d"].map(running);
        let lines = plain(live_region(&tests, &progress(), &[], 0, layout(3, 0, 60)));
        assert_eq!(
            lines[1..4],
            [
                "  ⠋ [dev] users::a 1.2s",
                "  ⠋ [dev] users::b 1.2s",
                "  … and 2 more",
            ]
        );
    }

    #[test]
    fn live_region_omits_failed_when_none() {
        let mut p = progress();
        p.failed = 0;
        let lines = plain(live_region(&[], &p, &[], 0, layout(10, 0, 60)));
        assert_eq!(
            lines[1],
            " ━━━━━━──────────────  3/10  ✓ 2  ● 0 running  2.3s"
        );
    }

    #[test]
    fn live_region_fits_the_terminal_width() {
        let lines = live_region(
            &[running("some_long_test_name")],
            &progress(),
            &[],
            0,
            layout(10, 0, 16),
        );
        for line in &lines {
            assert!(console::measure_text_width(line) <= 16, "{line}");
        }
    }

    #[test]
    fn progress_bar_never_hides_a_failure() {
        let bar = console::strip_ansi_codes(&progress_bar(1, 1, 1000, 10)).into_owned();
        assert_eq!(bar, "━─────────");
        let bar = console::strip_ansi_codes(&progress_bar(0, 0, 0, 4)).into_owned();
        assert_eq!(bar, "────");
        let bar = console::strip_ansi_codes(&progress_bar(10, 0, 10, 4)).into_owned();
        assert_eq!(bar, "━━━━");
    }

    #[test]
    fn short_module_drops_the_crate_name() {
        assert_eq!(short_module("my_tests::api::users"), "api::users");
        assert_eq!(short_module("my_tests"), "my_tests");
    }

    #[test]
    fn format_elapsed_switches_to_minutes() {
        assert_eq!(format_elapsed(Duration::from_millis(1234)), "1.2s");
        assert_eq!(format_elapsed(Duration::from_secs(125)), "2m05s");
    }

    #[test]
    fn log_writer_sends_whole_lines_to_the_sink() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        set_log_sink(LogSink::Live(tx));
        {
            let mut w = LogWriter::default();
            write!(w, "INFO ").unwrap();
            writeln!(w, "hello").unwrap();
            assert!(
                rx.try_recv().is_err(),
                "nothing is sent before the event ends"
            );
        }
        assert_eq!(rx.try_recv().unwrap(), b"INFO hello\n");

        // After the live reporter is done, logs are dropped.
        set_log_sink(LogSink::Discard);
        writeln!(LogWriter::default(), "dropped").unwrap();
        assert!(rx.try_recv().is_err());

        // Without a live reporter, the log goes to stdout instead.
        set_log_sink(LogSink::Stdout);
        writeln!(LogWriter::default(), "to stdout").unwrap();
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn indent_prefixes_every_line() {
        assert_eq!(indent("a\nb", 4), "    a\n    b");
        assert_eq!(indent("", 4), "");
    }

    #[test]
    fn format_body_for_display_colorizes_json_within_the_cap() {
        let out = format_body_for_display(r#"{"a":1}"#, "application/json", MaxBodySize(64 * 1024));
        assert!(out.contains('\n'), "expected pretty-printed JSON: {out}");
    }

    #[test]
    fn format_body_for_display_truncates_without_colorizing() {
        let body = format!(r#"{{"a":"{}"}}"#, "x".repeat(2048));
        let out = format_body_for_display(&body, "application/json", MaxBodySize(64));
        assert!(out.starts_with(r#"{"a":"xxx"#));
        assert!(out.contains("truncated: showing 64B of 2.0KB"));
    }

    #[test]
    fn format_body_for_display_leaves_body_alone_when_unlimited() {
        let body = "x".repeat(4096);
        assert_eq!(
            format_body_for_display(&body, "text/plain", MaxBodySize(0)),
            body
        );
    }
}
