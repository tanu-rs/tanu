use clap::{value_parser, Arg, ArgAction, Command as ClapCommand};
use console::Term;
use eyre::OptionExt;
use itertools::Itertools;
use std::{
    collections::{HashMap, VecDeque},
    str::FromStr,
};
use tanu_core::Filter;

use crate::{get_tanu_config, ListReporter, ReporterType, TableReporter};

/// Build the CLI with clap's builder pattern
fn build_cli<'a>(third_party_reporters: impl Iterator<Item = &'a String>) -> ClapCommand {
    let mut reporter_choices: VecDeque<_> = third_party_reporters.map(|s| s.to_string()).collect();
    reporter_choices.push_front(ReporterType::Table.to_string());
    reporter_choices.push_front(ReporterType::List.to_string());
    ClapCommand::new("tanu")
        .about("tanu CLI offers various commands, including listing and executing test cases")
        .version(env!("CARGO_PKG_VERSION"))
        .subcommand_required(true)
        .subcommand(
            ClapCommand::new("test")
                .about("Run tests in CLI mode")
                .arg(Arg::new("capture-http")
                    .long("capture-http")
                    .help("Capture http debug logs")
                    .action(ArgAction::SetTrue))
                .arg(Arg::new("capture-rust")
                    .long("capture-rust")
                    .help("Capture Rust \"log\" crate based logs. This is usefull in the following two cases\n1) tanu failed unexpectedly and you would want to see the tanu's internal logs.\n2) you would want to see logs produced from your tests that uses \"log\" crate")
                    .action(ArgAction::SetTrue))
                .arg(Arg::new("projects")
                    .short('p')
                    .long("projects")
                    .help("Specify projects to run in comma-separated string. --projects dev --projects staging")
                    .value_delimiter(',')
                    .action(ArgAction::Append))
                .arg(Arg::new("modules")
                    .short('m')
                    .long("modules")
                    .help("Specify modules to run in comma-separated string. --modules foo,bar")
                    .value_delimiter(',')
                    .action(ArgAction::Append))
                .arg(Arg::new("tests")
                    .short('t')
                    .long("tests")
                    .help("Specify test cases to run in comma-separated string. e.g. --tests a,b")
                    .value_delimiter(',')
                    .action(ArgAction::Append))
                .arg(Arg::new("reporters")
                    .long("reporters")
                    .help(format!("Specify the reporters to use in comma-separated string. Default is \"list\". [possible values: {}]", reporter_choices.into_iter().join(", ")))
                    .value_delimiter(',')
                    .action(ArgAction::Append))
                .arg(Arg::new("concurrency")
                    .short('c')
                    .long("concurrency")
                    .help("Specify the maximum number of tests to run in parallel. When unspecified, all tests run in parallel")
                    .value_parser(value_parser!(usize)))
                .arg(Arg::new("color")
                    .long("color")
                    .help("Produce color output. Default is \"auto\" [env: CARGO_TERM_COLOR]")
                    .value_parser(["auto", "always", "never"]))
        )
        .subcommand(
            ClapCommand::new("tui")
                .about("Run tests in TUI mode")
                .arg(Arg::new("log-level")
                    .long("log-level")
                    .help("Log level filter")
                    .default_value("Info"))
                .arg(Arg::new("tanu-log-level")
                    .long("tanu-log-level")
                    .help("tanu log level filter")
                    .default_value("Info"))
                .arg(Arg::new("concurrency")
                    .short('c')
                    .long("concurrency")
                    .help("Specify the maximum number of tests to run in parallel. Default is the number of logical CPU cores")
                    .value_parser(value_parser!(usize)))
        )
        .subcommand(
            ClapCommand::new("ls")
                .about("List test cases")
        )
}

/// The main tanu CLI application.
///
/// `App` is the entry point for running tanu tests. It handles command-line argument parsing,
/// configuration management, and test execution coordination.
///
/// # Examples
///
/// Basic usage:
///
/// ```rust,no_run
/// use tanu::{App, eyre};
///
/// #[tanu::main]
/// #[tokio::main]
/// async fn main() -> eyre::Result<()> {
///     let runner = run();
///     let app = App::new();
///     app.run(runner).await?;
///     Ok(())
/// }
/// ```
///
/// With custom reporters:
///
/// ```rust,no_run
/// use tanu::{App, eyre};
///
/// #[tanu::main]
/// #[tokio::main]
/// async fn main() -> eyre::Result<()> {
///     let runner = run();
///     let mut app = App::new();
///     // app.install_reporter("custom", MyCustomReporter::new());
///     app.run(runner).await?;
///     Ok(())
/// }
/// ```
#[derive(Default)]
pub struct App {
    third_party_reporters: HashMap<String, Box<dyn tanu_core::reporter::Reporter + 'static + Send>>,
}

impl App {
    /// Creates a new tanu application instance.
    ///
    /// This initializes the application with default settings and no custom reporters.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use tanu::App;
    ///
    /// let app = App::new();
    /// ```
    pub fn new() -> App {
        App {
            third_party_reporters: HashMap::new(),
        }
    }

    /// Install a third-party reporter.
    ///
    /// Custom reporters allow you to extend tanu's output capabilities beyond the built-in
    /// `list` and `table` reporters. The reporter will be available via the `--reporters`
    /// command-line flag.
    ///
    /// # Arguments
    ///
    /// * `name` - The name that will be used to reference this reporter from the command line
    /// * `reporter` - A custom reporter implementation that implements the `Reporter` trait
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use tanu::{App, reporter::Reporter};
    ///
    /// struct MyReporter;
    /// impl Reporter for MyReporter {
    ///     // Implementation details...
    /// #   fn start(&mut self, _: &tanu::TestInfo) -> eyre::Result<()> { Ok(()) }
    /// #   fn success(&mut self, _: &tanu::TestInfo) -> eyre::Result<()> { Ok(()) }
    /// #   fn failure(&mut self, _: &tanu::TestInfo, _: &eyre::Report) -> eyre::Result<()> { Ok(()) }
    /// #   fn finish(&mut self) -> eyre::Result<()> { Ok(()) }
    /// }
    ///
    /// let mut app = App::new();
    /// app.install_reporter("custom", MyReporter);
    /// ```
    pub fn install_reporter(
        &mut self,
        name: impl Into<String>,
        reporter: impl tanu_core::reporter::Reporter + 'static + Send,
    ) {
        self.third_party_reporters
            .insert(name.into(), Box::new(reporter));
    }

    /// Parse command-line arguments and run the tanu CLI.
    ///
    /// This method is the main entry point for executing tanu tests. It parses command-line
    /// arguments, configures the test runner based on the provided options, and executes
    /// the appropriate subcommand (test, tui, or ls).
    ///
    /// # Arguments
    ///
    /// * `runner` - A configured test runner containing all registered test functions
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful execution, or an error if something goes wrong
    /// during argument parsing, configuration loading, or test execution.
    ///
    /// # Supported Commands
    ///
    /// - `test` - Run tests in CLI mode with various filtering and reporting options
    /// - `tui` - Launch the interactive Terminal User Interface
    /// - `ls` - List all available test cases
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use tanu::{App, eyre};
    ///
    /// #[tanu::main]
    /// #[tokio::main]
    /// async fn main() -> eyre::Result<()> {
    ///     let runner = run();
    ///     let app = App::new();
    ///     app.run(runner).await
    /// }
    /// ```
    pub async fn run(mut self, mut runner: crate::Runner) -> eyre::Result<()> {
        let matches = build_cli(self.third_party_reporters.keys()).get_matches();
        color_eyre::install().unwrap();

        let cfg = get_tanu_config();
        let term = Term::stdout();

        match matches.subcommand() {
            Some(("test", test_matches)) => {
                let capture_http = test_matches.get_flag("capture-http");
                let capture_rust = test_matches.get_flag("capture-rust");
                let projects = test_matches
                    .get_many::<String>("projects")
                    .map(|vals| vals.cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                let modules = test_matches
                    .get_many::<String>("modules")
                    .map(|vals| vals.cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                let tests = test_matches
                    .get_many::<String>("tests")
                    .map(|vals| vals.cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                let mut reporters_arg = test_matches
                    .get_many::<String>("reporters")
                    .into_iter()
                    .flat_map(|vals| vals.cloned())
                    .collect::<Vec<_>>();
                if reporters_arg.is_empty() {
                    reporters_arg.push(ReporterType::List.to_string());
                }
                let concurrency = test_matches.get_one::<usize>("concurrency").cloned();
                let color_command = test_matches
                    .get_one::<String>("color")
                    .and_then(|s| Color::from_str(s).ok());

                if capture_http {
                    runner.capture_http();
                }
                if capture_rust {
                    runner.capture_rust();
                }
                if let Some(concurrency) = concurrency {
                    runner.set_concurrency(concurrency);
                }
                runner.terminate_channel();

                let mut reporters = std::mem::take(&mut self.third_party_reporters);
                reporters.extend([
                    (
                        ReporterType::Table.to_string(),
                        Box::new(TableReporter::new(capture_http)),
                    ),
                    (
                        ReporterType::List.to_string(),
                        Box::new(ListReporter::new(capture_http)),
                    ),
                ]
                    as [(
                        String,
                        Box<dyn tanu_core::reporter::Reporter + 'static + Send>,
                    ); 2]);

                for reporter in reporters_arg {
                    runner.add_boxed_reporter(
                        reporters
                            .remove(&reporter)
                            .ok_or_else(|| eyre::eyre!("Unknown reporter: {reporter}"))?,
                    );
                }

                let color_env = std::env::var("CARGO_TERM_COLOR");
                let color = match (color_command, color_env) {
                    (color @ Some(Color::Always), _) => color,
                    (color @ Some(Color::Never), _) => color,
                    (None, Ok(color)) => Color::from_str(&color).ok(),
                    _ => None,
                };
                match color {
                    Some(Color::Always) => {
                        console::set_colors_enabled(true);
                        console::set_colors_enabled_stderr(true);
                    }
                    Some(Color::Never) => {
                        console::set_colors_enabled(false);
                        console::set_colors_enabled_stderr(false);
                    }
                    _ => {}
                }

                runner.run(&projects, &modules, &tests).await
            }
            Some(("tui", tui_matches)) => {
                let log_level_str = tui_matches.get_one::<String>("log-level").unwrap();
                let tanu_log_level_str = tui_matches.get_one::<String>("tanu-log-level").unwrap();
                let log_level =
                    log::LevelFilter::from_str(log_level_str).unwrap_or(log::LevelFilter::Info);
                let tanu_log_level = log::LevelFilter::from_str(tanu_log_level_str)
                    .unwrap_or(log::LevelFilter::Info);
                let concurrency = tui_matches.get_one::<usize>("concurrency").cloned();

                if let Some(concurrency) = concurrency {
                    runner.set_concurrency(concurrency);
                } else {
                    runner.set_concurrency(num_cpus::get());
                }

                tanu_tui::run(runner, log_level, tanu_log_level).await
            }
            Some(("ls", _)) => {
                let filter = tanu_core::runner::TestIgnoreFilter::default();
                let list = runner.list();
                let test_case_by_module = list.iter().into_group_map_by(|test| test.module.clone());
                for module in test_case_by_module.keys() {
                    term.write_line(&format!("* {module}"))?;
                    for project in &cfg.projects {
                        for test_case in test_case_by_module
                            .get(module)
                            .ok_or_eyre("module not found")?
                        {
                            if !filter.filter(project, test_case) {
                                continue;
                            }
                            term.write_line(&format!(
                                "  - [{}] {}",
                                project.name,
                                test_case.full_name()
                            ))?;
                        }
                    }
                }

                Ok(())
            }
            _ => unreachable!("Subcommand required is set to true"),
        }
    }
}

#[derive(Debug, Clone, Default, strum::EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum Color {
    #[default]
    Auto,
    Always,
    Never,
}
