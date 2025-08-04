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
//! ## Basic Usage
//!
//! ```rust,ignore
//! use tanu_core::Runner;
//!
//! let mut runner = Runner::new();
//! runner.add_test("my_test", "my_module", test_factory);
//! runner.run(&[], &[], &[]).await?;
//! ```
use backon::Retryable;
use eyre::WrapErr;
use futures::{stream::FuturesUnordered, FutureExt, StreamExt};
use once_cell::sync::Lazy;
use std::{
    collections::HashMap,
    ops::Deref,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::{broadcast, Semaphore};
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
    TEST_INFO.with(|info| info.clone())
}

// NOTE: Keep the runner receiver alive here so that sender never fails to send.
#[allow(clippy::type_complexity)]
pub(crate) static CHANNEL: Lazy<
    Mutex<Option<(broadcast::Sender<Event>, broadcast::Receiver<Event>)>>,
> = Lazy::new(|| Mutex::new(Some(broadcast::channel(1000))));

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
#[derive(Debug, Clone)]
pub enum EventBody {
    Start,
    Check(Box<Check>),
    Http(Box<http::Log>),
    Retry(Test),
    End(Test),
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
    pub request_time: Duration,
    pub result: Result<(), Error>,
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
#[derive(Debug, Clone, Default)]
pub struct Options {
    pub debug: bool,
    pub capture_http: bool,
    pub capture_rust: bool,
    pub terminate_channel: bool,
    pub concurrency: Option<usize>,
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
/// runner.add_test("health_check", "api", test_factory);
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
    pub fn add_test(&mut self, name: &str, module: &str, factory: TestCaseFactory) {
        self.test_cases.push((
            Arc::new(TestInfo {
                name: name.into(),
                module: module.into(),
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
        if self.options.capture_rust {
            tracing_subscriber::fmt::init();
        }

        let reporters = std::mem::take(&mut self.reporters);
        let reporter_handles: Vec<_> = reporters
            .into_iter()
            .map(|mut reporter| tokio::spawn(async move { reporter.run().await }))
            .collect();

        let project_filter = ProjectFilter { project_names };
        let module_filter = ModuleFilter { module_names };
        let test_name_filter = TestNameFilter { test_names };
        let test_ignore_filter = TestIgnoreFilter::default();

        let start = std::time::Instant::now();
        let handles: FuturesUnordered<_> = {
            // Create a semaphore to limit concurrency if specified
            let semaphore = Arc::new(tokio::sync::Semaphore::new(
                self.options.concurrency.unwrap_or(Semaphore::MAX_PERMITS),
            ));

            self.test_cases
                .iter()
                .flat_map(|(info, factory)| {
                    let projects = self.cfg.projects.clone();
                    let projects = if projects.is_empty() {
                        vec![Arc::new(ProjectConfig {
                            name: "default".into(),
                            ..Default::default()
                        })]
                    } else {
                        projects
                    };
                    projects
                        .into_iter()
                        .map(move |project| (project, info.clone(), factory.clone()))
                })
                .filter(move |(project, info, _)| test_name_filter.filter(project, info))
                .filter(move |(project, info, _)| module_filter.filter(project, info))
                .filter(move |(project, info, _)| project_filter.filter(project, info))
                .filter(move |(project, info, _)| test_ignore_filter.filter(project, info))
                .map(|(project, info, factory)| {
                    let semaphore = semaphore.clone();
                    tokio::spawn(async move {
                        let _permit = semaphore.acquire().await.unwrap();
                        let project_for_scope = project.clone();
                        let info_for_scope = info.clone();
                        config::PROJECT
                            .scope(project_for_scope, async {
                                TEST_INFO
                                    .scope(info_for_scope, async {
                                        let test_name = info.name.clone();
                                        publish(EventBody::Start)?;

                                        let retry_count =
                                            AtomicUsize::new(project.retry.count.unwrap_or(0));
                                        let f = || async {
                                            let request_started = std::time::Instant::now();
                                            let res = factory().await;

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
                                                    info: info.clone(),
                                                    request_time: request_started.elapsed(),
                                                };
                                                publish(EventBody::Retry(test))?;
                                                retry_count.fetch_sub(1, Ordering::SeqCst);
                                            };
                                            res
                                        };
                                        let started = std::time::Instant::now();
                                        let fut = f.retry(project.retry.backoff());
                                        let fut = std::panic::AssertUnwindSafe(fut).catch_unwind();
                                        let res = fut.await;
                                        let request_time = started.elapsed();

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
                                            request_time,
                                            result,
                                        }))?;

                                        eyre::ensure!(!is_err);
                                        eyre::Ok(())
                                    })
                                    .await
                            })
                            .await
                    })
                })
                .collect()
        };
        debug!(
            "created handles for {} test cases; took {}s",
            handles.len(),
            start.elapsed().as_secs_f32()
        );

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
                        if let Err(e) = res {
                            debug!("test case failed: {e:#}");
                            has_any_error = true;
                        }
                    }
                    Err(e) => {
                        if e.is_panic() {
                            // Resume the panic on the main task
                            error!("{e}");
                            has_any_error = true;
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

        let runner_result = runner.await;

        for handle in reporter_handles {
            match handle.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => error!("reporter failed: {e:#}"),
                Err(e) => error!("reporter task panicked: {e:#}"),
            }
        }

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
