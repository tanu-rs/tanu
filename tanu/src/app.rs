use clap::{
    builder::styling::{AnsiColor, Effects, Styles},
    error::ErrorKind,
    value_parser, Arg, ArgAction, ArgMatches, Command as ClapCommand,
};
use console::Term;
use itertools::Itertools;
use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    num::NonZeroUsize,
    str::FromStr,
};
use tanu_core::runner::{module_matches, test_name_matches, TestInfo};
use tanu_core::Filter;
use tanu_core::{config::parse_byte_size, CaptureHttpMode, MaxBodySize, ProjectConfig};

use crate::{get_tanu_config, ListReporter, ReporterType};

/// Define CLI color styles
fn cli_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Green.on_default() | Effects::BOLD)
        .usage(AnsiColor::Green.on_default() | Effects::BOLD)
        .literal(AnsiColor::Cyan.on_default() | Effects::BOLD)
        .placeholder(AnsiColor::Yellow.on_default())
        .error(AnsiColor::Red.on_default() | Effects::BOLD)
        .valid(AnsiColor::Green.on_default())
        .invalid(AnsiColor::Red.on_default())
}

const TEST_EXAMPLES: &str = "\
Examples:
  test                          Run all tests
  test users                    Run tests whose name contains \"users\"
  test -p staging -m api        Run the api module (and submodules) against staging
  test -t get_user --capture-http
                                Run one test and print all HTTP requests/responses
  test -c 4 --fail-fast         Limit parallelism and stop at the first failure";

/// Filter arguments shared by `test` and `ls`.
fn filter_args(verb: &str) -> [Arg; 4] {
    [
        Arg::new("patterns")
            .value_name("PATTERN")
            .help(format!("Only {verb} tests whose full name (module::test) contains one of these substrings"))
            .num_args(0..)
            .help_heading("Filtering"),
        Arg::new("projects")
            .short('p')
            .long("projects")
            .value_name("PROJECTS")
            .help(format!("Only {verb} these projects, comma-separated or repeated. e.g. -p dev,staging"))
            .value_delimiter(',')
            .action(ArgAction::Append)
            .help_heading("Filtering"),
        Arg::new("modules")
            .short('m')
            .long("modules")
            .value_name("MODULES")
            .help(format!("Only {verb} tests in these modules and their submodules, comma-separated or repeated. The crate prefix can be omitted. e.g. -m api,auth"))
            .value_delimiter(',')
            .action(ArgAction::Append)
            .help_heading("Filtering"),
        Arg::new("tests")
            .short('t')
            .long("tests")
            .value_name("TESTS")
            .help(format!("Only {verb} these tests, by full name or any trailing part of it, comma-separated or repeated. e.g. -t users::get,login"))
            .value_delimiter(',')
            .action(ArgAction::Append)
            .help_heading("Filtering"),
    ]
}

/// Build the CLI with clap's builder pattern
fn build_cli<'a>(third_party_reporters: impl Iterator<Item = &'a String>) -> ClapCommand {
    let mut reporter_choices: VecDeque<_> = third_party_reporters.map(|s| s.to_string()).collect();
    reporter_choices.push_front(ReporterType::List.to_string());
    ClapCommand::new("tanu")
        .styles(cli_styles())
        .about("tanu CLI offers various commands, including listing and executing test cases")
        .version(env!("CARGO_PKG_VERSION"))
        .subcommand_required(true)
        .subcommand(
            ClapCommand::new("test")
                .about("Run tests in CLI mode")
                .after_help(TEST_EXAMPLES)
                .args(filter_args("run"))
                .arg(Arg::new("capture-http")
                    .long("capture-http")
                    .value_name("MODE")
                    .help("When to print captured HTTP requests and responses. A bare --capture-http means \"all\" [default: on-failure]")
                    .num_args(0..=1)
                    .default_missing_value("all")
                    .value_parser(["all", "off", "on-failure"])
                    .help_heading("HTTP capture"))
                .arg(Arg::new("max-body-size")
                    .long("max-body-size")
                    .value_name("SIZE")
                    .help("Max bytes of each HTTP request/response body to print, e.g. 65536, 64KB, 2MB. 0 or \"unlimited\" disables the cap [default: 16KB]")
                    .value_parser(parse_byte_size)
                    .help_heading("HTTP capture"))
                .arg(Arg::new("show-sensitive")
                    .long("show-sensitive")
                    .help("Show sensitive data (API keys, tokens) in HTTP logs instead of masking them with *****")
                    .action(ArgAction::SetTrue)
                    .help_heading("HTTP capture"))
                .arg(Arg::new("concurrency")
                    .short('c')
                    .long("concurrency")
                    .value_name("N")
                    .help("Maximum number of tests to run in parallel. When unspecified, all tests run in parallel")
                    .value_parser(value_parser!(NonZeroUsize))
                    .help_heading("Execution"))
                .arg(Arg::new("fail-fast")
                    .long("fail-fast")
                    .help("Abort test execution after the first failure")
                    .action(ArgAction::SetTrue)
                    .help_heading("Execution"))
                .arg(Arg::new("reporters")
                    .long("reporters")
                    .value_name("REPORTERS")
                    .help(format!("Reporters to use, comma-separated [default: list] [possible values: {}]", reporter_choices.into_iter().join(", ")))
                    .value_delimiter(',')
                    .action(ArgAction::Append)
                    .help_heading("Output"))
                .arg(Arg::new("color")
                    .long("color")
                    .value_name("WHEN")
                    .help("Produce color output [default: auto] [env: CARGO_TERM_COLOR]")
                    .value_parser(["auto", "always", "never"])
                    .help_heading("Output"))
                .arg(Arg::new("capture-rust")
                    .long("capture-rust")
                    .help("Print logs from the Rust \"log\" crate, both tanu's internal logs and logs emitted by your tests")
                    .action(ArgAction::SetTrue)
                    .help_heading("Output"))
        )
        .subcommand(
            ClapCommand::new("tui")
                .about("Run tests in TUI mode")
                .arg(Arg::new("log-level")
                    .long("log-level")
                    .help("Log level filter")
                    .default_value("Debug"))
                .arg(Arg::new("tanu-log-level")
                    .long("tanu-log-level")
                    .help("tanu log level filter")
                    .default_value("Debug"))
                .arg(Arg::new("concurrency")
                    .short('c')
                    .long("concurrency")
                    .value_name("N")
                    .help("Specify the maximum number of tests to run in parallel. Default is the number of logical CPU cores")
                    .value_parser(value_parser!(NonZeroUsize)))
        )
        .subcommand(
            ClapCommand::new("ls")
                .about("List test cases")
                .args(filter_args("list"))
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
    /// use tanu::{async_trait, App, reporter::Reporter};
    ///
    /// struct MyReporter;
    ///
    /// #[async_trait::async_trait]
    /// impl Reporter for MyReporter {
    ///     // All methods have default implementations
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
        let cfg = get_tanu_config();
        let reporter_names: Vec<String> = self.third_party_reporters.keys().cloned().collect();

        let matches = build_cli(reporter_names.iter()).get_matches();
        color_eyre::install().unwrap();
        let term = Term::stdout();

        match matches.subcommand() {
            Some(("test", test_matches)) => {
                // Merge config values with CLI flags (CLI takes precedence)
                let capture_http = test_matches
                    .get_one::<String>("capture-http")
                    .map(|s| match s.as_str() {
                        "on-failure" => CaptureHttpMode::OnFailure,
                        "off" => CaptureHttpMode::Off,
                        _ => CaptureHttpMode::All,
                    })
                    .or_else(|| cfg.runner.capture_http.clone())
                    .unwrap_or_default();
                let max_body_size = test_matches
                    .get_one::<MaxBodySize>("max-body-size")
                    .copied()
                    .or(cfg.runner.max_body_size)
                    .unwrap_or_default();
                let capture_rust = test_matches.get_flag("capture-rust")
                    || cfg.runner.capture_rust.unwrap_or(false);
                let show_sensitive = test_matches.get_flag("show-sensitive")
                    || cfg.runner.show_sensitive.unwrap_or(false);
                let filters = Filters::from_matches(test_matches);
                filters.validate_or_exit(cfg, &runner.list());
                let mut reporters_arg = test_matches
                    .get_many::<String>("reporters")
                    .into_iter()
                    .flat_map(|vals| vals.cloned())
                    .collect::<Vec<_>>();
                if reporters_arg.is_empty() {
                    reporters_arg.push(ReporterType::List.to_string());
                }
                // Merge config value with CLI flag (CLI takes precedence)
                let concurrency = test_matches
                    .get_one::<NonZeroUsize>("concurrency")
                    .map(|n| n.get())
                    .or(cfg.runner.concurrency);
                let color_command = test_matches
                    .get_one::<String>("color")
                    .and_then(|s| Color::from_str(s).ok());

                runner.set_capture_http_mode(capture_http.clone());
                if capture_rust {
                    runner.capture_rust();
                }
                if show_sensitive {
                    runner.show_sensitive();
                }
                let extra_keys = cfg.runner.extra_sensitive_keys.clone();
                let extra_headers = cfg.runner.extra_sensitive_headers.clone();
                if !extra_keys.is_empty() || !extra_headers.is_empty() {
                    runner.set_sensitive_overrides(extra_keys, extra_headers);
                }
                if let Some(concurrency) = concurrency {
                    runner.set_concurrency(concurrency);
                }
                let fail_fast =
                    test_matches.get_flag("fail-fast") || cfg.runner.fail_fast.unwrap_or(false);
                if fail_fast {
                    runner.set_fail_fast(true);
                }
                runner.set_name_patterns(filters.patterns);
                runner.terminate_channel();

                let mut reporters = std::mem::take(&mut self.third_party_reporters);
                reporters.extend([(
                    ReporterType::List.to_string(),
                    Box::new(ListReporter::new(capture_http, max_body_size)),
                )]
                    as [(
                        String,
                        Box<dyn tanu_core::reporter::Reporter + 'static + Send>,
                    ); 1]);

                for reporter in reporters_arg {
                    let available = reporters.keys().sorted().join(", ");
                    runner.add_boxed_reporter(reporters.remove(&reporter).ok_or_else(|| {
                        eyre::eyre!("unknown reporter \"{reporter}\" (available: {available})")
                    })?);
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
                    Some(Color::Auto) | None => {
                        console::set_colors_enabled(term.is_term());
                        console::set_colors_enabled_stderr(term.is_term());
                    }
                }

                runner
                    .run(&filters.projects, &filters.modules, &filters.tests)
                    .await
            }
            Some(("tui", tui_matches)) => {
                let log_level_str = tui_matches.get_one::<String>("log-level").unwrap();
                let tanu_log_level_str = tui_matches.get_one::<String>("tanu-log-level").unwrap();
                let log_level =
                    log::LevelFilter::from_str(log_level_str).unwrap_or(log::LevelFilter::Debug);
                let tanu_log_level = log::LevelFilter::from_str(tanu_log_level_str)
                    .unwrap_or(log::LevelFilter::Debug);
                // Merge config value with CLI flag (CLI takes precedence), default to CPU cores
                let concurrency = tui_matches
                    .get_one::<NonZeroUsize>("concurrency")
                    .map(|n| n.get())
                    .or(cfg.runner.concurrency)
                    .unwrap_or_else(num_cpus::get);

                runner.set_concurrency(concurrency);
                if cfg.runner.show_sensitive.unwrap_or(false) {
                    runner.show_sensitive();
                }
                let extra_keys = cfg.runner.extra_sensitive_keys.clone();
                let extra_headers = cfg.runner.extra_sensitive_headers.clone();
                if !extra_keys.is_empty() || !extra_headers.is_empty() {
                    runner.set_sensitive_overrides(extra_keys, extra_headers);
                }

                tanu_tui::run(runner, log_level, tanu_log_level).await
            }
            Some(("ls", ls_matches)) => {
                use console::style;

                let filters = Filters::from_matches(ls_matches);
                filters.validate_or_exit(cfg, &runner.list());

                let ignore_filter = tanu_core::runner::TestIgnoreFilter::default();
                let only_filter = tanu_core::runner::TestOnlyFilter::default();
                let projects = if cfg.projects.is_empty() {
                    vec![std::sync::Arc::new(ProjectConfig {
                        name: "default".into(),
                        ..Default::default()
                    })]
                } else {
                    cfg.projects.clone()
                };
                let list = runner.list();
                let test_case_by_module = list.iter().into_group_map_by(|test| test.module.clone());
                let mut listed_any = false;
                for (module, test_cases) in test_case_by_module
                    .iter()
                    .sorted_by(|(a, _), (b, _)| a.cmp(b))
                {
                    let lines: Vec<String> = projects
                        .iter()
                        .flat_map(|project| test_cases.iter().map(move |info| (project, info)))
                        .filter(|(project, info)| {
                            ignore_filter.filter(project, info)
                                && only_filter.filter(project, info)
                                && filters.matches(&project.name, info)
                        })
                        .map(|(project, info)| {
                            format!(
                                "  {} {} {}::{}",
                                style("-").dim(),
                                style(format!("[{}]", project.name)).magenta().bold(),
                                style(&info.module).cyan(),
                                style(&info.name).blue().bold()
                            )
                        })
                        .collect();
                    if lines.is_empty() {
                        continue;
                    }
                    listed_any = true;
                    term.write_line(&format!(
                        "{} {}",
                        style("*").green().bold(),
                        style(module).yellow().bold()
                    ))?;
                    for line in lines {
                        term.write_line(&line)?;
                    }
                }
                if !listed_any && !filters.is_empty() {
                    term.write_line("no tests matched the given filters")?;
                }

                Ok(())
            }
            _ => unreachable!("Subcommand required is set to true"),
        }
    }
}

/// Names of the configured projects; the runner falls back to "default" when none are configured.
fn project_names(cfg: &tanu_core::Config) -> Vec<&str> {
    if cfg.projects.is_empty() {
        vec!["default"]
    } else {
        cfg.projects.iter().map(|p| p.name.as_str()).collect()
    }
}

/// Test selection shared by `test` and `ls`.
#[derive(Debug, Default)]
struct Filters {
    patterns: Vec<String>,
    projects: Vec<String>,
    modules: Vec<String>,
    tests: Vec<String>,
}

impl Filters {
    fn from_matches(matches: &ArgMatches) -> Self {
        let values = |id: &str| {
            matches
                .get_many::<String>(id)
                .map(|vals| vals.cloned().collect::<Vec<_>>())
                .unwrap_or_default()
        };
        Filters {
            patterns: values("patterns"),
            projects: values("projects"),
            modules: values("modules"),
            tests: values("tests"),
        }
    }

    fn is_empty(&self) -> bool {
        self.patterns.is_empty()
            && self.projects.is_empty()
            && self.modules.is_empty()
            && self.tests.is_empty()
    }

    /// Whether a test in `project` is selected, using the same rules as the runner.
    fn matches(&self, project: &str, info: &TestInfo) -> bool {
        let full_name = info.full_name();
        (self.projects.is_empty() || self.projects.iter().any(|p| p == project))
            && (self.modules.is_empty()
                || self.modules.iter().any(|m| module_matches(&info.module, m)))
            && (self.tests.is_empty() || self.tests.iter().any(|t| test_name_matches(info, t)))
            && (self.patterns.is_empty() || self.patterns.iter().any(|p| full_name.contains(p)))
    }

    /// Reports a `-p`, `-m`, or `-t` value that matches nothing as a usage error, so a
    /// typo gets a "did you mean" hint instead of silently selecting zero tests.
    fn validate_or_exit(&self, cfg: &tanu_core::Config, test_cases: &[&TestInfo]) {
        if let Err(message) = self.validate(cfg, test_cases) {
            clap::Error::raw(ErrorKind::InvalidValue, format!("{message}\n")).exit();
        }
    }

    fn validate(&self, cfg: &tanu_core::Config, test_cases: &[&TestInfo]) -> Result<(), String> {
        let project_names = project_names(cfg);
        for project in &self.projects {
            if !project_names.contains(&project.as_str()) {
                let hint = did_you_mean(project, project_names.iter().copied())
                    .unwrap_or_else(|| format!("available projects: {}", project_names.join(", ")));
                return Err(format!("unknown project \"{project}\"\n\n  tip: {hint}"));
            }
        }

        let modules: BTreeSet<String> = test_cases
            .iter()
            .flat_map(|info| module_and_ancestors(&info.module))
            .collect();
        for module in &self.modules {
            if !modules.iter().any(|m| module_matches(m, module)) {
                let hint = did_you_mean(module, modules.iter().map(String::as_str))
                    .unwrap_or_else(|| "run `ls` to list tests".into());
                return Err(format!("no module named \"{module}\"\n\n  tip: {hint}"));
            }
        }

        let tests: BTreeSet<String> = test_cases.iter().map(|info| info.full_name()).collect();
        for test in &self.tests {
            if !test_cases.iter().any(|info| test_name_matches(info, test)) {
                let hint = match modules.iter().find(|m| module_matches(m, test)) {
                    Some(_) => format!("\"{test}\" is a module; select it with -m {test}"),
                    None => did_you_mean(test, tests.iter().map(String::as_str))
                        .unwrap_or_else(|| "run `ls` to list tests".into()),
                };
                return Err(format!("no test named \"{test}\"\n\n  tip: {hint}"));
            }
        }
        Ok(())
    }
}

/// `api::users` -> `["api", "api::users"]`; `-m` accepts any of them.
fn module_and_ancestors(module: &str) -> Vec<String> {
    let segments: Vec<&str> = module.split("::").collect();
    (1..=segments.len())
        .map(|n| segments[..n].join("::"))
        .collect()
}

/// Formats a "did you mean" hint with the candidates closest to `query` (at most three).
fn did_you_mean<'a>(query: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let max_distance = (query.chars().count() / 3).max(1);
    let mut scored: Vec<(usize, &str)> = candidates
        .into_iter()
        .filter_map(|candidate| {
            let distance = name_distance(query, candidate);
            (distance <= max_distance).then_some((distance, candidate))
        })
        .collect();
    scored.sort();
    let best = scored.first().map(|(distance, _)| *distance);
    let suggestions: Vec<String> = scored
        .into_iter()
        .take_while(|(distance, _)| Some(*distance) == best)
        .take(3)
        .map(|(_, candidate)| format!("\"{candidate}\""))
        .collect();
    match suggestions.as_slice() {
        [] => None,
        [one] => Some(format!("did you mean {one}?")),
        many => Some(format!("did you mean one of {}?", many.join(", "))),
    }
}

/// Edit distance between `query` and a `::`-separated `name`, also comparing against the
/// name's trailing segments so partial paths like `users::get` match `api::users::get`.
fn name_distance(query: &str, name: &str) -> usize {
    std::iter::once(name)
        .chain(name.match_indices("::").map(|(i, _)| &name[i + 2..]))
        .map(|suffix| levenshtein(query, suffix))
        .min()
        .unwrap_or(usize::MAX)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev_diag = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let prev_up = row[j + 1];
            row[j + 1] = if ca == *cb {
                prev_diag
            } else {
                1 + prev_diag.min(prev_up).min(row[j])
            };
            prev_diag = prev_up;
        }
    }
    row[b.len()]
}

#[derive(Debug, Clone, Default, strum::EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum Color {
    #[default]
    Auto,
    Always,
    Never,
}

#[cfg(test)]
mod test {
    use super::*;

    fn parse(args: &[&str]) -> Result<clap::ArgMatches, clap::Error> {
        build_cli(std::iter::empty()).try_get_matches_from(args)
    }

    #[test]
    fn test_accepts_positional_patterns() {
        let matches = parse(&["tanu", "test", "users", "http::get", "-p", "dev"]).unwrap();
        let (_, test_matches) = matches.subcommand().unwrap();
        let patterns: Vec<_> = test_matches
            .get_many::<String>("patterns")
            .unwrap()
            .collect();
        assert_eq!(patterns, ["users", "http::get"]);
    }

    #[test]
    fn test_rejects_zero_concurrency() {
        assert!(parse(&["tanu", "test", "-c", "0"]).is_err());
        assert!(parse(&["tanu", "tui", "-c", "0"]).is_err());
        assert!(parse(&["tanu", "test", "-c", "1"]).is_ok());
    }

    #[test]
    fn test_accepts_patterns_before_bare_capture_http() {
        let matches = parse(&["tanu", "test", "users", "--capture-http"]).unwrap();
        let (_, test_matches) = matches.subcommand().unwrap();
        assert_eq!(
            test_matches.get_one::<String>("capture-http").unwrap(),
            "all"
        );
        assert_eq!(test_matches.get_one::<String>("patterns").unwrap(), "users");
    }

    #[test]
    fn validate_reports_unknown_names_with_suggestions() {
        let cfg = tanu_core::Config::default();
        let get = TestInfo {
            module: "api::users".into(),
            name: "get".into(),
            ..Default::default()
        };
        let set_cookie = TestInfo {
            module: "http::cookie".into(),
            name: "set_cookie".into(),
            ..Default::default()
        };
        let cases = [&get, &set_cookie];
        let validate = |p: &[&str], m: &[&str], t: &[&str]| {
            let v = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
            Filters {
                patterns: vec![],
                projects: v(p),
                modules: v(m),
                tests: v(t),
            }
            .validate(&cfg, &cases)
        };

        assert!(validate(&["default"], &["api"], &["get"]).is_ok());
        assert!(validate(&[], &["api::users"], &["api::users::get"]).is_ok());

        let err = validate(&["defualt"], &[], &[]).unwrap_err();
        assert!(err.contains("did you mean \"default\"?"), "{err}");
        let err = validate(&["staging"], &[], &[]).unwrap_err();
        assert!(err.contains("available projects: default"), "{err}");
        let err = validate(&[], &["http::cokie"], &[]).unwrap_err();
        assert!(err.contains("did you mean \"http::cookie\"?"), "{err}");
        let err = validate(&[], &[], &["set_cokie"]).unwrap_err();
        assert!(
            err.contains("did you mean \"http::cookie::set_cookie\"?"),
            "{err}"
        );
        assert!(validate(&[], &["users", "cookie"], &["users::get"]).is_ok());
        let err = validate(&[], &[], &["users::gte"]).unwrap_err();
        assert!(err.contains("did you mean \"api::users::get\"?"), "{err}");
        let err = validate(&[], &[], &["http::cookie"]).unwrap_err();
        assert!(
            err.contains("is a module; select it with -m http::cookie"),
            "{err}"
        );
        let err = validate(&[], &[], &["nothing_like_it"]).unwrap_err();
        assert!(err.contains("run `ls` to list tests"), "{err}");
    }

    #[test]
    fn filters_match_like_the_runner() {
        let info = TestInfo {
            module: "api::users".into(),
            name: "get".into(),
            ..Default::default()
        };
        let filters = Filters {
            patterns: vec!["users".into()],
            projects: vec!["dev".into()],
            modules: vec!["api".into()],
            tests: vec!["get".into()],
        };
        assert!(filters.matches("dev", &info));
        assert!(!filters.matches("staging", &info));
        assert!(Filters::default().matches("any", &info));
    }

    #[test]
    fn levenshtein_counts_edits() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("same", "same"), 0);
    }
}
