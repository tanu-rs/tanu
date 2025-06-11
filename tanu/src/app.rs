use clap::Parser;
use console::Term;
use eyre::OptionExt;
use itertools::Itertools;
use std::{collections::HashMap, str::FromStr};
use tanu_core::Filter;

use crate::{get_tanu_config, ListReporter, ReporterType, TableReporter};

/// tanu CLI.
#[derive(Default)]
pub struct App {
    third_party_reporters: HashMap<String, Box<dyn tanu_core::reporter::Reporter + 'static + Send>>,
}

impl App {
    pub fn new() -> App {
        App {
            third_party_reporters: HashMap::new(),
        }
    }

    /// Install a third-party reporter.
    pub fn install_reporter(
        &mut self,
        name: impl Into<String>,
        reporter: impl tanu_core::reporter::Reporter + 'static + Send,
    ) {
        self.third_party_reporters
            .insert(name.into(), Box::new(reporter));
    }

    /// Parse command-line args and run tanu CLI sub command.
    pub async fn run(mut self, mut runner: crate::Runner) -> eyre::Result<()> {
        let args = Args::parse();
        color_eyre::install().unwrap();

        let cfg = get_tanu_config();
        let term = Term::stdout();
        match args.command {
            Command::Test {
                capture_rust,
                capture_http,
                projects,
                modules,
                tests,
                reporters: reporters_arg,
                concurrency,
                color: color_command,
            } => {
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

                for reporter in reporters_arg.into_iter().flatten() {
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
            Command::Tui {
                log_level,
                tanu_log_level,
                concurrency,
            } => {
                if let Some(concurrency) = concurrency {
                    runner.set_concurrency(concurrency);
                } else {
                    runner.set_concurrency(num_cpus::get());
                }

                tanu_tui::run(runner, log_level, tanu_log_level).await
            }
            Command::Ls {} => {
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
        }
    }
}

/// tanu CLI offers various commands, including listing and executing test cases.
#[derive(clap::Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, Default, strum::EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum Color {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Run tests in CLI mode
    Test {
        /// Capture http debug logs.
        #[arg(long)]
        capture_http: bool,
        /// Capture Rust "log" crate based logs. This is usefull in the following two cases
        /// 1) tanu failed unexpectedly and you would want to see the tanu's internal logs.
        /// 2) you would want to see logs produced from your tests that uses "log" crate.
        #[arg(long)]
        capture_rust: bool,
        /// Specify projects to run in comma-separated string.
        /// --projects dev --projects staging
        #[arg(short, long, value_delimiter = ',')]
        projects: Vec<String>,
        /// Specify modules to run in comma-separated string.
        /// --modules foo,bar
        #[arg(short, long, value_delimiter = ',')]
        modules: Vec<String>,
        /// Specify test cases to run in comma-separated string.
        /// e.g. --tests a,b
        #[arg(short, long, value_delimiter = ',')]
        tests: Vec<String>,
        /// Specify the reporters to use in comma-separated string. Default is "list". Possible values are "table", "list" and "null".
        #[arg(long, value_delimiter = ',')]
        reporters: Option<Vec<String>>,
        /// Specify the maximum number of tests to run in parallel. When unspecified, all tests run in parallel.
        #[arg(short, long)]
        concurrency: Option<usize>,
        /// Produce color output: "auto", "always" and "never". Default is "auto" [env: CARGO_TERM_COLOR]
        #[arg(long)]
        color: Option<Color>,
    },
    /// Run tests in TUI mode
    Tui {
        #[arg(long, default_value = "Info")]
        log_level: log::LevelFilter,
        #[arg(long, default_value = "Info")]
        tanu_log_level: log::LevelFilter,
        /// Specify the maximum number of tests to run in parallel. Default is the number of logical CPU cores.
        #[arg(short, long)]
        concurrency: Option<usize>,
    },
    /// List test cases
    Ls {},
}
