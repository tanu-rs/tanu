//! # Test Runner Module
//!
//! The core test execution engine for tanu. This module provides the `Runner` struct
//! that orchestrates test discovery, execution, filtering, reporting, and event publishing.
//! It supports concurrent test execution with retry capabilities and comprehensive
//! event-driven reporting.
//!
//! ## Key Components
//!
//! - **`Runner`**: Main test execution engine
//! - **Event System**: Real-time test execution events via channels
//! - **Filtering**: Project, module, and test name filtering
//! - **Reporting**: Pluggable reporter system for test output
//! - **Retry Logic**: Configurable retry with exponential backoff
//!
//! ## Execution Flow (block diagram)
//!
//! ```text
//! +-------------------+     +-------------------+     +---------------------+
//! | Test registry     | --> | Filter chain      | --> | Semaphore           |
//! | add_test()        |     | project/module    |     | (concurrency ctrl)  |
//! +-------------------+     | test name/ignore  |     +---------------------+
//!                           +-------------------+               |
//!                                                               v
//!                                                     +---------------------+
//!                                                     | Tokio task spawn    |
//!                                                     | + task-local ctx    |
//!                                                     +---------------------+
//!                                                               |
//!                                                               v
//!                                                     +---------------------+
//!                                                     | Test execution      |
//!                                                     | + panic recovery    |
//!                                                     | + retry/backoff     |
//!                                                     +---------------------+
//!                                                               |
//!          +----------------------------------------------------+
//!          v
//! +-------------------+     +-------------------+     +-------------------+
//! | Event channel     | --> | Broadcast to all  | --> | Reporter(s)       |
//! | Start/Check/HTTP  |     | subscribers       |     | List/Table/Null   |
//! | Retry/End/Summary |     |                   |     | (format output)   |
//! +-------------------+     +-------------------+     +-------------------+
//! ```
//!
//! ## Basic Usage
//!
//! ```rust,ignore
//! use tanu_core::Runner;
//!
//! let mut runner = Runner::new();
//! runner.add_test("my_test", "my_module", None, test_factory);
//! runner.run(&[], &[], &[]).await?;
//! ```
use backon::Retryable;
use eyre::WrapErr;
use futures::{stream::FuturesUnordered, FutureExt, StreamExt};
use itertools::Itertools;
use once_cell::sync::Lazy;
use std::{
    collections::HashMap,
    ops::Deref,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime},
};
use tokio::sync::broadcast;
use tracing::*;

use crate::{
    config::{self, get_tanu_config, ProjectConfig},
    http,
    reporter::Reporter,
    Config, ModuleName, ProjectName,
};

tokio::task_local! {
    pub(crate) static TEST_INFO: Arc<TestInfo>;
}

pub(crate) fn get_test_info() -> Arc<TestInfo> {
    TEST_INFO.with(Arc::clone)
}

/// Runs a future in the current tanu test context (project + test info), if any.
///
/// This is useful when spawning additional Tokio tasks (e.g. via `tokio::spawn`/`JoinSet`)
/// from inside a `#[tanu::test]`, because Tokio task-locals are not propagated
/// automatically.
pub fn scope_current<F>(fut: F) -> impl std::future::Future<Output = F::Output> + Send
where
    F: std::future::Future + Send,
    F::Output: Send,
{
    let project = crate::config::PROJECT.try_with(Arc::clone).ok();
    let test_info = TEST_INFO.try_with(Arc::clone).ok();

    async move {
        match (project, test_info) {
            (Some(project), Some(test_info)) => {
                crate::config::PROJECT
                    .scope(project, TEST_INFO.scope(test_info, fut))
                    .await
            }
            (Some(project), None) => crate::config::PROJECT.scope(project, fut).await,
            (None, Some(test_info)) => TEST_INFO.scope(test_info, fut).await,
            (None, None) => fut.await,
        }
    }
}

// NOTE: Keep the runner receiver alive here so that sender never fails to send.
#[allow(clippy::type_complexity)]
pub(crate) static CHANNEL: Lazy<
    Mutex<Option<(broadcast::Sender<Event>, broadcast::Receiver<Event>)>>,
> = Lazy::new(|| Mutex::new(Some(broadcast::channel(1000))));

/// Barrier to synchronize reporter subscription before test execution starts.
/// This prevents the race condition where tests publish events before reporters subscribe.
pub(crate) static REPORTER_BARRIER: Lazy<Mutex<Option<Arc<tokio::sync::Barrier>>>> =
    Lazy::new(|| Mutex::new(None));

/// Publishes an event to the runner's event channel.
///
/// This function is used throughout the test execution pipeline to broadcast
/// real-time events including test starts, check results, HTTP logs, retries,
/// and test completions. All events are timestamped and include test context.
///
/// # Examples
///
/// ```rust,ignore
/// use tanu_core::runner::{publish, EventBody, Check};
///
/// // Publish a successful check
/// let check = Check::success("response.status() == 200");
/// publish(EventBody::Check(Box::new(check)))?;
///
/// // Publish test start
/// publish(EventBody::Start)?;
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - The channel lock cannot be acquired
/// - The channel has been closed
/// - The send operation fails
pub fn publish(e: impl Into<Event>) -> eyre::Result<()> {
    let Ok(guard) = CHANNEL.lock() else {
        eyre::bail!("failed to acquire runner channel lock");
    };
    let Some((tx, _)) = guard.deref() else {
        eyre::bail!("runner channel has been already closed");
    };

    tx.send(e.into())
        .wrap_err("failed to publish message to the runner channel")?;

    Ok(())
}

/// Subscribe to the channel to see the real-time test execution events.
pub fn subscribe() -> eyre::Result<broadcast::Receiver<Event>> {
    let Ok(guard) = CHANNEL.lock() else {
        eyre::bail!("failed to acquire runner channel lock");
    };
    let Some((tx, _)) = guard.deref() else {
        eyre::bail!("runner channel has been already closed");
    };

    Ok(tx.subscribe())
}

/// Set up barrier for N reporters (called before spawning reporters).
///
/// This ensures all reporters subscribe before tests start executing,
/// preventing the race condition where Start events are published before
/// reporters are ready to receive them.
pub(crate) fn setup_reporter_barrier(count: usize) -> eyre::Result<()> {
    let Ok(mut barrier) = REPORTER_BARRIER.lock() else {
        eyre::bail!("failed to acquire reporter barrier lock");
    };
    *barrier = Some(Arc::new(tokio::sync::Barrier::new(count + 1)));
    Ok(())
}

/// Wait on barrier (called by reporters after subscribing, and by runner before tests).
///
/// If no barrier is set (standalone reporter use), this is a no-op.
pub(crate) async fn wait_reporter_barrier() {
    let barrier = match REPORTER_BARRIER.lock() {
        Ok(guard) => guard.clone(),
        Err(e) => {
            error!("failed to acquire reporter barrier lock (poisoned): {e}");
            return;
        }
    };

    if let Some(b) = barrier {
        b.wait().await;
    }
}

/// Clear barrier after use.
pub(crate) fn clear_reporter_barrier() {
    match REPORTER_BARRIER.lock() {
        Ok(mut barrier) => {
            *barrier = None;
        }
        Err(e) => {
            error!("failed to clear reporter barrier (poisoned lock): {e}");
        }
    }
}

/// Test execution errors.
///
/// Represents the different ways a test can fail during execution.
/// These errors are captured and reported by the runner system.
#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("panic: {0}")]
    Panicked(String),
    #[error("error: {0}")]
    ErrorReturned(String),
}

/// Represents the result of a check/assertion within a test.
///
/// Checks are created by assertion macros (`check!`, `check_eq!`, etc.) and
/// track both the success/failure status and the original expression that
/// was evaluated. This information is used for detailed test reporting.
///
/// # Examples
///
/// ```rust,ignore
/// use tanu_core::runner::Check;
///
/// // Create a successful check
/// let check = Check::success("response.status() == 200");
/// assert!(check.result);
///
/// // Create a failed check
/// let check = Check::error("user_count != 0");
/// assert!(!check.result);
/// ```
#[derive(Debug, Clone)]
pub struct Check {
    pub result: bool,
    pub expr: String,
}

impl Check {
    pub fn success(expr: impl Into<String>) -> Check {
        Check {
            result: true,
            expr: expr.into(),
        }
    }

    pub fn error(expr: impl Into<String>) -> Check {
        Check {
            result: false,
            expr: expr.into(),
        }
    }
}

/// A test execution event with full context.
///
/// Events are published throughout test execution and include the project,
/// module, and test name for complete traceability. The event body contains
/// the specific event data (start, check, HTTP, retry, or end).
///
/// # Event Flow
///
/// 1. `Start` - Test begins execution
/// 2. `Check` - Assertion results (can be multiple per test)
/// 3. `Http` - HTTP request/response logs (can be multiple per test)
/// 4. `Retry` - Test retry attempts (if configured)
/// 5. `End` - Test completion with final result
#[derive(Debug, Clone)]
pub struct Event {
    pub project: ProjectName,
    pub module: ModuleName,
    pub test: ModuleName,
    pub body: EventBody,
}

/// The specific event data published during test execution.
///
/// Each event type carries different information:
/// - `Start`: Signals test execution beginning
/// - `Check`: Contains assertion results with expression details
/// - `Http`: HTTP request/response logs for debugging
/// - `Retry`: Indicates a test retry attempt
/// - `End`: Final test result with timing and outcome
/// - `Summary`: Overall test execution summary with counts and timing
#[derive(Debug, Clone)]
pub enum EventBody {
    Start,
    Check(Box<Check>),
    Http(Box<http::Log>),
    Retry(Test),
    End(Test),
    Summary(TestSummary),
}

impl From<EventBody> for Event {
    fn from(body: EventBody) -> Self {
        let project = crate::config::get_config();
        let test_info = crate::runner::get_test_info();
        Event {
            project: project.name.clone(),
            module: test_info.module.clone(),
            test: test_info.name.clone(),
            body,
        }
    }
}

/// Final test execution result.
///
/// Contains the complete outcome of a test execution including metadata,
/// execution time, and the final result (success or specific error type).
/// This is published in the `End` event when a test completes.
#[derive(Debug, Clone)]
pub struct Test {
    pub info: Arc<TestInfo>,
    pub worker_id: isize,
    pub started_at: SystemTime,
    pub ended_at: SystemTime,
    pub request_time: Duration,
    pub result: Result<(), Error>,
}

/// Overall test execution summary.
///
/// Contains aggregate information about the entire test run including
/// total counts, timing, and success/failure statistics.
/// This is published in the `Summary` event when all tests complete.
#[derive(Debug, Clone)]
pub struct TestSummary {
    pub total_tests: usize,
    pub passed_tests: usize,
    pub failed_tests: usize,
    pub total_time: Duration,
    pub test_prep_time: Duration,
}

/// Test metadata and identification.
///
/// Contains the module and test name for a test case. This information
/// is used for test filtering, reporting, and event context throughout
/// the test execution pipeline.
#[derive(Debug, Clone, Default)]
pub struct TestInfo {
    pub module: String,
    pub name: String,
    pub serial_group: Option<String>,
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

/// Pool of reusable worker IDs for timeline visualization.
///
/// Worker IDs are assigned to tests when they start executing and returned
/// to the pool when they complete. This allows timeline visualization tools
/// to display tests in lanes based on which worker executed them.
#[derive(Debug)]
pub struct WorkerIds {
    enabled: bool,
    ids: Mutex<Vec<isize>>,
}

impl WorkerIds {
    /// Creates a new worker ID pool with IDs from 0 to concurrency-1.
    ///
    /// If `concurrency` is `None`, the pool is disabled and `acquire()` always returns -1.
    pub fn new(concurrency: Option<usize>) -> Self {
        match concurrency {
            Some(c) => Self {
                enabled: true,
                ids: Mutex::new((0..c as isize).collect()),
            },
            None => Self {
                enabled: false,
                ids: Mutex::new(Vec::new()),
            },
        }
    }

    /// Acquires a worker ID from the pool.
    ///
    /// Returns -1 if the pool is disabled, empty, or the mutex is poisoned.
    pub fn acquire(&self) -> isize {
        if !self.enabled {
            return -1;
        }
        self.ids
            .lock()
            .ok()
            .and_then(|mut guard| guard.pop())
            .unwrap_or(-1)
    }

    /// Returns a worker ID to the pool.
    ///
    /// Does nothing if the pool is disabled, the mutex is poisoned, or id is negative.
    pub fn release(&self, id: isize) {
        if !self.enabled || id < 0 {
            return;
        }
        if let Ok(mut guard) = self.ids.lock() {
            guard.push(id);
        }
    }
}

type TestCaseFactory = Arc<
    dyn Fn() -> Pin<Box<dyn futures::Future<Output = eyre::Result<()>> + Send + 'static>>
        + Sync
        + Send
        + 'static,
>;

/// Configuration options for test runner behavior.
///
/// Controls various aspects of test execution including logging,
/// concurrency, and channel management. These options can be set
/// via the builder pattern on the `Runner`.
///
/// # Examples
///
/// ```rust,ignore
/// use tanu_core::Runner;
///
/// let mut runner = Runner::new();
/// runner.capture_http(); // Enable HTTP logging
/// runner.set_concurrency(4); // Limit to 4 concurrent tests
/// ```
#[derive(Debug, Clone)]
pub struct Options {
    pub debug: bool,
    pub capture_http: bool,
    pub capture_rust: bool,
    pub terminate_channel: bool,
    pub concurrency: Option<usize>,
    /// Whether to mask sensitive data (API keys, tokens) in HTTP logs.
    /// Defaults to `true` (masked). Set to `false` with `--show-sensitive` flag.
    pub mask_sensitive: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            debug: false,
            capture_http: false,
            capture_rust: false,
            terminate_channel: false,
            concurrency: None,
            mask_sensitive: true, // Masked by default for security
        }
    }
}

/// Trait for filtering test cases during execution.
///
/// Filters allow selective test execution based on project configuration
/// and test metadata. Multiple filters can be applied simultaneously,
/// and a test must pass all filters to be executed.
///
/// # Examples
///
/// ```rust,ignore
/// use tanu_core::runner::{Filter, TestInfo, ProjectConfig};
///
/// struct CustomFilter;
///
/// impl Filter for CustomFilter {
///     fn filter(&self, project: &ProjectConfig, info: &TestInfo) -> bool {
///         // Only run tests with "integration" in the name
///         info.name.contains("integration")
///     }
/// }
/// ```
pub trait Filter {
    fn filter(&self, project: &ProjectConfig, info: &TestInfo) -> bool;
}

/// Filters tests to only run from specified projects.
///
/// When project names are provided, only tests from those projects
/// will be executed. If the list is empty, all projects are included.
///
/// # Examples
///
/// ```rust,ignore
/// use tanu_core::runner::ProjectFilter;
///
/// let filter = ProjectFilter { project_names: &["staging".to_string()] };
/// // Only tests from "staging" project will run
/// ```
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

/// Filters tests to only run from specified modules.
///
/// When module names are provided, only tests from those modules
/// will be executed. If the list is empty, all modules are included.
/// Module names correspond to Rust module paths.
///
/// # Examples
///
/// ```rust,ignore
/// use tanu_core::runner::ModuleFilter;
///
/// let filter = ModuleFilter { module_names: &["api".to_string(), "auth".to_string()] };
/// // Only tests from "api" and "auth" modules will run
/// ```
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

/// Filters tests to only run specific named tests.
///
/// When test names are provided, only those exact tests will be executed.
/// Test names should include the module (e.g., "api::health_check").
/// If the list is empty, all tests are included.
///
/// # Examples
///
/// ```rust,ignore
/// use tanu_core::runner::TestNameFilter;
///
/// let filter = TestNameFilter {
///     test_names: &["api::health_check".to_string(), "auth::login".to_string()]
/// };
/// // Only the specified tests will run
/// ```
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

/// Filters out tests that are configured to be ignored.
///
/// This filter reads the `test_ignore` configuration from each project
/// and excludes those tests from execution. Tests are matched by their
/// full name (module::test_name).
///
/// # Configuration
///
/// In `tanu.toml`:
/// ```toml
/// [[projects]]
/// name = "staging"
/// test_ignore = ["flaky_test", "slow_integration_test"]
/// ```
///
/// # Examples
///
/// ```rust,ignore
/// use tanu_core::runner::TestIgnoreFilter;
///
/// let filter = TestIgnoreFilter::default();
/// // Tests listed in test_ignore config will be skipped
/// ```
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

/// The main test execution engine for tanu.
///
/// `Runner` is responsible for orchestrating the entire test execution pipeline:
/// test discovery, filtering, concurrent execution, retry handling, event publishing,
/// and result reporting. It supports multiple projects, configurable concurrency,
/// and pluggable reporters.
///
/// # Features
///
/// - **Concurrent Execution**: Tests run in parallel with configurable limits
/// - **Retry Logic**: Automatic retry with exponential backoff for flaky tests
/// - **Event System**: Real-time event publishing for UI integration
/// - **Filtering**: Filter tests by project, module, or test name
/// - **Reporting**: Support for multiple output formats via reporters
/// - **HTTP Logging**: Capture and log all HTTP requests/responses
///
/// # Examples
///
/// ```rust,ignore
/// use tanu_core::{Runner, reporter::TableReporter};
///
/// let mut runner = Runner::new();
/// runner.capture_http();
/// runner.set_concurrency(8);
/// runner.add_reporter(TableReporter::new());
///
/// // Add tests (typically done by procedural macros)
/// runner.add_test("health_check", "api", None, test_factory);
///
/// // Run all tests
/// runner.run(&[], &[], &[]).await?;
/// ```
///
/// # Architecture
///
/// Tests are executed in separate tokio tasks with:
/// - Project-scoped configuration
/// - Test-scoped context for event publishing  
/// - Semaphore-based concurrency control
/// - Panic recovery and error handling
/// - Automatic retry with configurable backoff
#[derive(Default)]
pub struct Runner {
    cfg: Config,
    options: Options,
    test_cases: Vec<(Arc<TestInfo>, TestCaseFactory)>,
    reporters: Vec<Box<dyn Reporter + Send>>,
}

impl Runner {
    /// Creates a new runner with the global tanu configuration.
    ///
    /// This loads the configuration from `tanu.toml` and sets up
    /// default options. Use `with_config()` for custom configuration.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use tanu_core::Runner;
    ///
    /// let runner = Runner::new();
    /// ```
    pub fn new() -> Runner {
        Runner::with_config(get_tanu_config().clone())
    }

    /// Creates a new runner with the specified configuration.
    ///
    /// This allows for custom configuration beyond what's in `tanu.toml`,
    /// useful for testing or programmatic setup.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use tanu_core::{Runner, Config};
    ///
    /// let config = Config::default();
    /// let runner = Runner::with_config(config);
    /// ```
    pub fn with_config(cfg: Config) -> Runner {
        Runner {
            cfg,
            options: Options::default(),
            test_cases: Vec::new(),
            reporters: Vec::new(),
        }
    }

    /// Enables HTTP request/response logging.
    ///
    /// When enabled, all HTTP requests made via tanu's HTTP client
    /// will be logged and included in test reports. This is useful
    /// for debugging API tests and understanding request/response flow.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let mut runner = Runner::new();
    /// runner.capture_http();
    /// ```
    pub fn capture_http(&mut self) {
        self.options.capture_http = true;
    }

    /// Enables Rust logging output during test execution.
    ///
    /// This initializes the tracing subscriber to capture debug, info,
    /// warn, and error logs from tests and the framework itself.
    /// Useful for debugging test execution issues.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let mut runner = Runner::new();
    /// runner.capture_rust();
    /// ```
    pub fn capture_rust(&mut self) {
        self.options.capture_rust = true;
    }

    /// Configures the runner to close the event channel after test execution.
    ///
    /// By default, the event channel remains open for continued monitoring.
    /// This option closes the channel when all tests complete, signaling
    /// that no more events will be published.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let mut runner = Runner::new();
    /// runner.terminate_channel();
    /// ```
    pub fn terminate_channel(&mut self) {
        self.options.terminate_channel = true;
    }

    /// Adds a reporter for test output formatting.
    ///
    /// Reporters receive test events and format them for different output
    /// destinations (console, files, etc.). Multiple reporters can be added
    /// to generate multiple output formats simultaneously.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use tanu_core::{Runner, reporter::TableReporter};
    ///
    /// let mut runner = Runner::new();
    /// runner.add_reporter(TableReporter::new());
    /// ```
    pub fn add_reporter(&mut self, reporter: impl Reporter + 'static + Send) {
        self.reporters.push(Box::new(reporter));
    }

    /// Adds a boxed reporter for test output formatting.
    ///
    /// Similar to `add_reporter()` but accepts an already-boxed reporter.
    /// Useful when working with dynamic reporter selection.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use tanu_core::{Runner, reporter::ListReporter};
    ///
    /// let mut runner = Runner::new();
    /// let reporter: Box<dyn Reporter + Send> = Box::new(ListReporter::new());
    /// runner.add_boxed_reporter(reporter);
    /// ```
    pub fn add_boxed_reporter(&mut self, reporter: Box<dyn Reporter + 'static + Send>) {
        self.reporters.push(reporter);
    }

    /// Add a test case to the runner.
    pub fn add_test(
        &mut self,
        name: &str,
        module: &str,
        serial_group: Option<&str>,
        factory: TestCaseFactory,
    ) {
        self.test_cases.push((
            Arc::new(TestInfo {
                name: name.into(),
                module: module.into(),
                serial_group: serial_group.map(|s| s.to_string()),
            }),
            factory,
        ));
    }

    /// Sets the maximum number of tests to run concurrently.
    ///
    /// By default, tests run with unlimited concurrency. This setting
    /// allows you to limit concurrent execution to reduce resource usage
    /// or avoid overwhelming external services.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let mut runner = Runner::new();
    /// runner.set_concurrency(4); // Max 4 tests at once
    /// ```
    pub fn set_concurrency(&mut self, concurrency: usize) {
        self.options.concurrency = Some(concurrency);
    }

    /// Disables sensitive data masking in HTTP logs.
    ///
    /// By default, sensitive data (Authorization headers, API keys in URLs, etc.)
    /// is masked with `*****` when HTTP logging is enabled. Call this method
    /// to show the actual values instead.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let mut runner = Runner::new();
    /// runner.capture_http(); // Enable HTTP logging
    /// runner.show_sensitive(); // Show actual values instead of *****
    /// ```
    pub fn show_sensitive(&mut self) {
        self.options.mask_sensitive = false;
    }

    /// Executes all registered tests with optional filtering.
    ///
    /// Runs tests concurrently according to the configured options and filters.
    /// Tests can be filtered by project name, module name, or specific test names.
    /// Empty filter arrays mean "include all".
    ///
    /// # Parameters
    ///
    /// - `project_names`: Only run tests from these projects (empty = all projects)
    /// - `module_names`: Only run tests from these modules (empty = all modules)  
    /// - `test_names`: Only run these specific tests (empty = all tests)
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let mut runner = Runner::new();
    ///
    /// // Run all tests
    /// runner.run(&[], &[], &[]).await?;
    ///
    /// // Run only "staging" project tests
    /// runner.run(&["staging".to_string()], &[], &[]).await?;
    ///
    /// // Run specific test
    /// runner.run(&[], &[], &["api::health_check".to_string()]).await?;
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Any test fails (unless configured to continue on failure)
    /// - A test panics and cannot be recovered
    /// - Reporter setup or execution fails
    /// - Event channel operations fail
    #[allow(clippy::too_many_lines)]
    pub async fn run(
        &mut self,
        project_names: &[String],
        module_names: &[String],
        test_names: &[String],
    ) -> eyre::Result<()> {
        // Set masking configuration for HTTP logs
        crate::masking::set_mask_sensitive(self.options.mask_sensitive);

        if self.options.capture_rust {
            tracing_subscriber::fmt::init();
        }

        let reporters = std::mem::take(&mut self.reporters);

        // Set up barrier for all reporters + runner
        // This ensures all reporters subscribe before tests start
        setup_reporter_barrier(reporters.len())?;

        let reporter_handles: Vec<_> = reporters
            .into_iter()
            .map(|mut reporter| tokio::spawn(async move { reporter.run().await }))
            .collect();

        // Wait for all reporters to subscribe before starting tests
        wait_reporter_barrier().await;

        let project_filter = ProjectFilter { project_names };
        let module_filter = ModuleFilter { module_names };
        let test_name_filter = TestNameFilter { test_names };
        let test_ignore_filter = TestIgnoreFilter::default();

        let start = std::time::Instant::now();
        let handles: FuturesUnordered<_> = {
            // Create a semaphore to limit concurrency
            let concurrency = self.options.concurrency;
            let semaphore = Arc::new(tokio::sync::Semaphore::new(
                concurrency.unwrap_or(tokio::sync::Semaphore::MAX_PERMITS),
            ));

            // Worker ID pool for timeline visualization (only when concurrency is specified)
            let worker_ids = Arc::new(WorkerIds::new(concurrency));

            // Per-group mutexes for serial execution (project-scoped)
            // Key format: "project_name::group_name"
            let serial_groups: Arc<
                tokio::sync::RwLock<std::collections::HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
            > = Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));

            let projects = self.cfg.projects.clone();
            let projects = if projects.is_empty() {
                vec![Arc::new(ProjectConfig {
                    name: "default".into(),
                    ..Default::default()
                })]
            } else {
                projects
            };
            self.test_cases
                .iter()
                .cartesian_product(projects.into_iter())
                .map(|((info, factory), project)| (project, Arc::clone(info), factory.clone()))
                .filter(move |(project, info, _)| test_name_filter.filter(project, info))
                .filter(move |(project, info, _)| module_filter.filter(project, info))
                .filter(move |(project, info, _)| project_filter.filter(project, info))
                .filter(move |(project, info, _)| test_ignore_filter.filter(project, info))
                .map(|(project, info, factory)| {
                    let semaphore = semaphore.clone();
                    let worker_ids = worker_ids.clone();
                    let serial_groups = serial_groups.clone();
                    tokio::spawn(async move {
                        // Step 1: Acquire serial group mutex FIRST (if needed) - project-scoped
                        // This ensures tests in the same group don't hold semaphore permits unnecessarily
                        let serial_mutex = match &info.serial_group {
                            Some(group_name) => {
                                // Create project-scoped key: "project_name::group_name"
                                let key = format!("{}::{}", project.name, group_name);

                                // Get or create mutex for this project+group
                                let read_lock = serial_groups.read().await;
                                if let Some(mutex) = read_lock.get(&key) {
                                    Some(Arc::clone(mutex))
                                } else {
                                    drop(read_lock);
                                    let mut write_lock = serial_groups.write().await;
                                    Some(
                                        write_lock
                                            .entry(key)
                                            .or_insert_with(|| {
                                                Arc::new(tokio::sync::Mutex::new(()))
                                            })
                                            .clone(),
                                    )
                                }
                            }
                            None => None,
                        };

                        // Step 2: Acquire global semaphore AFTER serial mutex
                        // This prevents blocking other tests while waiting for serial group
                        let _permit = semaphore
                            .acquire()
                            .await
                            .map_err(|e| eyre::eyre!("failed to acquire semaphore: {e}"))?;

                        // Acquire worker ID from pool
                        let worker_id = worker_ids.acquire();

                        let project_for_scope = Arc::clone(&project);
                        let info_for_scope = Arc::clone(&info);
                        let result = config::PROJECT
                            .scope(project_for_scope, async {
                                TEST_INFO
                                    .scope(info_for_scope, async {
                                        let test_name = info.name.clone();
                                        publish(EventBody::Start)?;

                                        let retry_count =
                                            AtomicUsize::new(project.retry.count.unwrap_or(0));
                                        let serial_mutex_clone = serial_mutex.clone();
                                        let f = || async {
                                            // Acquire serial guard just before test execution
                                            let _serial_guard =
                                                if let Some(ref mutex) = serial_mutex_clone {
                                                    Some(mutex.lock().await)
                                                } else {
                                                    None
                                                };

                                            let started_at = SystemTime::now();
                                            let request_started = std::time::Instant::now();
                                            let res = factory().await;
                                            let ended_at = SystemTime::now();

                                            if res.is_err()
                                                && retry_count.load(Ordering::SeqCst) > 0
                                            {
                                                let test_result = match &res {
                                                    Ok(_) => Ok(()),
                                                    Err(e) => {
                                                        Err(Error::ErrorReturned(format!("{e:?}")))
                                                    }
                                                };
                                                let test = Test {
                                                    result: test_result,
                                                    info: Arc::clone(&info),
                                                    worker_id,
                                                    started_at,
                                                    ended_at,
                                                    request_time: request_started.elapsed(),
                                                };
                                                publish(EventBody::Retry(test))?;
                                                retry_count.fetch_sub(1, Ordering::SeqCst);
                                            };
                                            res
                                        };
                                        let started_at = SystemTime::now();
                                        let started = std::time::Instant::now();
                                        let fut = f.retry(project.retry.backoff());
                                        let fut = std::panic::AssertUnwindSafe(fut).catch_unwind();
                                        let res = fut.await;
                                        let request_time = started.elapsed();
                                        let ended_at = SystemTime::now();

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
                                                let panic_message = if let Some(panic_message) =
                                                    e.downcast_ref::<&str>()
                                                {
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
                                                    format!(
                                                        "{test_name} failed with unknown message"
                                                    )
                                                };
                                                let e = eyre::eyre!(panic_message);
                                                Err(Error::Panicked(format!("{e:?}")))
                                            }
                                        };

                                        let is_err = result.is_err();
                                        publish(EventBody::End(Test {
                                            info,
                                            worker_id,
                                            started_at,
                                            ended_at,
                                            request_time,
                                            result,
                                        }))?;

                                        eyre::ensure!(!is_err);
                                        eyre::Ok(())
                                    })
                                    .await
                            })
                            .await;

                        // Return worker ID to pool
                        worker_ids.release(worker_id);

                        result
                    })
                })
                .collect()
        };
        let test_prep_time = start.elapsed();
        debug!(
            "created handles for {} test cases; took {}s",
            handles.len(),
            test_prep_time.as_secs_f32()
        );

        let mut has_any_error = false;
        let total_tests = handles.len();
        let options = self.options.clone();
        let runner = async move {
            let results = handles.collect::<Vec<_>>().await;
            if results.is_empty() {
                console::Term::stdout().write_line("no test cases found")?;
            }

            let mut failed_tests = 0;
            for result in results {
                match result {
                    Ok(res) => {
                        if let Err(e) = res {
                            debug!("test case failed: {e:#}");
                            has_any_error = true;
                            failed_tests += 1;
                        }
                    }
                    Err(e) => {
                        if e.is_panic() {
                            // Resume the panic on the main task
                            error!("{e}");
                            has_any_error = true;
                            failed_tests += 1;
                        }
                    }
                }
            }

            let passed_tests = total_tests - failed_tests;
            let total_time = start.elapsed();

            // Publish summary event
            let summary = TestSummary {
                total_tests,
                passed_tests,
                failed_tests,
                total_time,
                test_prep_time,
            };

            // Create a dummy event for summary (since it doesn't belong to a specific test)
            let summary_event = Event {
                project: "".to_string(),
                module: "".to_string(),
                test: "".to_string(),
                body: EventBody::Summary(summary),
            };

            if let Ok(guard) = CHANNEL.lock() {
                if let Some((tx, _)) = guard.as_ref() {
                    let _ = tx.send(summary_event);
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

        let runner_result = runner.await;

        for handle in reporter_handles {
            match handle.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => error!("reporter failed: {e:#}"),
                Err(e) => error!("reporter task panicked: {e:#}"),
            }
        }

        // Clean up barrier
        clear_reporter_barrier();

        debug!("runner stopped");

        runner_result
    }

    /// Returns a list of all registered test metadata.
    ///
    /// This provides access to test information without executing the tests.
    /// Useful for building test UIs, generating reports, or implementing
    /// custom filtering logic.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let runner = Runner::new();
    /// let tests = runner.list();
    ///
    /// for test in tests {
    ///     println!("Test: {}", test.full_name());
    /// }
    /// ```
    pub fn list(&self) -> Vec<&TestInfo> {
        self.test_cases
            .iter()
            .map(|(meta, _test)| meta.as_ref())
            .collect::<Vec<_>>()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::config::RetryConfig;
    use crate::ProjectConfig;
    use std::sync::Arc;

    fn create_config() -> Config {
        Config {
            projects: vec![Arc::new(ProjectConfig {
                name: "default".into(),
                ..Default::default()
            })],
            ..Default::default()
        }
    }

    fn create_config_with_retry() -> Config {
        Config {
            projects: vec![Arc::new(ProjectConfig {
                name: "default".into(),
                retry: RetryConfig {
                    count: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            })],
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
                let client = crate::http::Client::new();
                let res = client.get(&url).send().await?;
                if res.status().is_success() {
                    Ok(())
                } else {
                    eyre::bail!("request failed")
                }
            })
        });

        let _runner_rx = subscribe()?;
        let mut runner = Runner::with_config(create_config());
        runner.add_test("retry_test", "module", None, factory);

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
                let client = crate::http::Client::new();
                let res = client.get(&url).send().await?;
                if res.status().is_success() {
                    Ok(())
                } else {
                    eyre::bail!("request failed")
                }
            })
        });

        let _runner_rx = subscribe()?;
        let mut runner = Runner::with_config(create_config_with_retry());
        runner.add_test("retry_test", "module", None, factory);

        let result = runner.run(&[], &[], &[]).await;
        m1.assert_async().await;
        m2.assert_async().await;

        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn spawned_task_panics_without_task_local_context() {
        let project = Arc::new(ProjectConfig {
            name: "default".to_string(),
            ..Default::default()
        });
        let test_info = Arc::new(TestInfo {
            module: "mod".to_string(),
            name: "test".to_string(),
            serial_group: None,
        });

        crate::config::PROJECT
            .scope(
                project,
                TEST_INFO.scope(test_info, async move {
                    let handle = tokio::spawn(async move {
                        let _ = crate::config::get_config();
                    });

                    let join_err = handle.await.expect_err("spawned task should panic");
                    assert!(join_err.is_panic());
                }),
            )
            .await;
    }

    #[tokio::test]
    async fn scope_current_propagates_task_local_context_into_spawned_task() {
        let project = Arc::new(ProjectConfig {
            name: "default".to_string(),
            ..Default::default()
        });
        let test_info = Arc::new(TestInfo {
            module: "mod".to_string(),
            name: "test".to_string(),
            serial_group: None,
        });

        crate::config::PROJECT
            .scope(
                project,
                TEST_INFO.scope(test_info, async move {
                    let handle = tokio::spawn(super::scope_current(async move {
                        let _ = crate::config::get_config();
                        let _ = super::get_test_info();
                    }));

                    handle.await.expect("spawned task should not panic");
                }),
            )
            .await;
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn masking_masks_sensitive_query_params_in_http_logs() -> eyre::Result<()> {
        use crate::masking;

        // Ensure masking is enabled
        masking::set_mask_sensitive(true);

        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .create_async()
            .await;

        let factory: TestCaseFactory = Arc::new(move || {
            let url = server.url();
            Box::pin(async move {
                let client = crate::http::Client::new();
                // Make request with sensitive query param embedded in URL
                let _res = client
                    .get(format!("{url}?access_token=secret_token_123&user=john"))
                    .send()
                    .await?;
                Ok(())
            })
        });

        let mut rx = subscribe()?;
        let mut runner = Runner::with_config(create_config());
        runner.add_test("masking_query_test", "masking_module", None, factory);

        runner.run(&[], &[], &[]).await?;

        // Collect HTTP events for this specific test
        let mut found_http_event = false;
        while let Ok(event) = rx.try_recv() {
            // Filter to only our test's events
            if event.test != "masking_query_test" {
                continue;
            }
            if let EventBody::Http(log) = event.body {
                found_http_event = true;
                let url_str = log.request.url.to_string();

                // Verify sensitive param is masked
                assert!(
                    url_str.contains("access_token=*****"),
                    "access_token should be masked, got: {url_str}"
                );
                // Non-sensitive params should not be masked
                assert!(
                    url_str.contains("user=john"),
                    "user should not be masked, got: {url_str}"
                );
            }
        }

        assert!(found_http_event, "Should have received HTTP event");
        Ok(())
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn masking_masks_sensitive_headers_in_http_logs() -> eyre::Result<()> {
        use crate::masking;

        // Ensure masking is enabled
        masking::set_mask_sensitive(true);

        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/")
            .with_status(200)
            .create_async()
            .await;

        let factory: TestCaseFactory = Arc::new(move || {
            let url = server.url();
            Box::pin(async move {
                let client = crate::http::Client::new();
                // Make request with sensitive headers
                let _res = client
                    .get(&url)
                    .header("authorization", "Bearer secret_bearer_token")
                    .header("x-api-key", "my_secret_api_key")
                    .header("content-type", "application/json")
                    .send()
                    .await?;
                Ok(())
            })
        });

        let mut rx = subscribe()?;
        let mut runner = Runner::with_config(create_config());
        runner.add_test("masking_headers_test", "masking_module", None, factory);

        runner.run(&[], &[], &[]).await?;

        // Collect HTTP events for this specific test
        let mut found_http_event = false;
        while let Ok(event) = rx.try_recv() {
            // Filter to only our test's events
            if event.test != "masking_headers_test" {
                continue;
            }
            if let EventBody::Http(log) = event.body {
                found_http_event = true;

                // Verify sensitive headers are masked
                if let Some(auth) = log.request.headers.get("authorization") {
                    assert_eq!(
                        auth.to_str().unwrap(),
                        "*****",
                        "authorization header should be masked"
                    );
                }
                if let Some(api_key) = log.request.headers.get("x-api-key") {
                    assert_eq!(
                        api_key.to_str().unwrap(),
                        "*****",
                        "x-api-key header should be masked"
                    );
                }
                // Non-sensitive headers should not be masked
                if let Some(content_type) = log.request.headers.get("content-type") {
                    assert_eq!(
                        content_type.to_str().unwrap(),
                        "application/json",
                        "content-type header should not be masked"
                    );
                }
            }
        }

        assert!(found_http_event, "Should have received HTTP event");
        Ok(())
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn masking_show_sensitive_disables_masking_in_http_logs() -> eyre::Result<()> {
        use crate::masking;

        masking::set_mask_sensitive(true);

        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/")
            .with_status(200)
            .create_async()
            .await;

        let factory: TestCaseFactory = Arc::new(move || {
            let url = server.url();
            Box::pin(async move {
                let client = crate::http::Client::new();
                let _res = client
                    .get(format!("{url}?access_token=secret_token_123"))
                    .header("authorization", "Bearer secret_bearer_token")
                    .send()
                    .await?;
                Ok(())
            })
        });

        let mut rx = subscribe()?;
        let mut runner = Runner::with_config(create_config());
        runner.capture_http();
        runner.show_sensitive();
        runner.add_test("show_sensitive_test", "masking_module", None, factory);

        runner.run(&[], &[], &[]).await?;

        let mut found_http_event = false;
        while let Ok(event) = rx.try_recv() {
            if event.test != "show_sensitive_test" {
                continue;
            }
            if let EventBody::Http(log) = event.body {
                found_http_event = true;
                let url_str = log.request.url.to_string();
                assert!(
                    url_str.contains("access_token=secret_token_123"),
                    "access_token should not be masked when show_sensitive is enabled"
                );
                if let Some(auth) = log.request.headers.get("authorization") {
                    assert_eq!(
                        auth.to_str().unwrap(),
                        "Bearer secret_bearer_token",
                        "authorization header should not be masked when show_sensitive is enabled"
                    );
                }
            }
        }

        assert!(found_http_event, "Should have received HTTP event");
        Ok(())
    }
}
