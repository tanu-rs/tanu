use clap::Parser;
use console::Term;
use eyre::OptionExt;
use itertools::Itertools;

use crate::{get_tanu_config, ListReporter};

/// tanu CLI.
pub struct App {}

impl App {
    pub fn new() -> App {
        App {}
    }

    /// Parse command-line args and run tanu CLI sub command.
    pub async fn run(self, mut runner: crate::Runner) -> eyre::Result<()> {
        let args = Args::parse();
        dotenv::dotenv().ok();
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
            } => {
                if capture_http {
                    runner.capture_http();
                }
                if capture_rust {
                    runner.capture_rust();
                }
                runner.terminate_channel();
                runner.add_reporter(ListReporter::new(capture_http));
                runner.run(&projects, &modules, &tests).await
            }
            Command::Tui {
                log_level,
                tanu_log_level,
            } => tanu_tui::run(runner, log_level, tanu_log_level).await,
            Command::Ls {} => {
                let list = runner.list();
                let test_case_by_module = list.iter().into_group_map_by(|test| test.module.clone());
                for module in test_case_by_module.keys() {
                    term.write_line(&format!("* {module}"))?;
                    for project in &cfg.projects {
                        for test_case in test_case_by_module
                            .get(module)
                            .ok_or_eyre("module not found")?
                        {
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

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Run tests with tanu
    Test {
        /// Capture http debug logs.
        #[arg(long)]
        capture_http: bool,
        /// Capture Rust "log" crate based logs. This is usefull in the following two cases
        /// 1) tanu failed unexpectedly and you would want to see the tanu's internal logs.
        /// 2) you would want to see logs produced from your tests that uses "log" crate.
        #[arg(long)]
        capture_rust: bool,
        /// Run only the specified projects. This option can be specified multiple times e.g.
        /// --projects dev --projects staging
        #[arg(short, long)]
        projects: Vec<String>,
        /// Run only the specified modules. This option can be specified multiple times e.g.
        /// --modules foo --modules bar
        #[arg(short, long)]
        modules: Vec<String>,
        /// Run only the specified test cases. This option can be specified multiple times e.g. --tests a
        /// ---tests b
        #[arg(short, long)]
        tests: Vec<String>,
    },
    Tui {
        #[arg(long, default_value = "Info")]
        log_level: log::LevelFilter,
        #[arg(long, default_value = "Info")]
        tanu_log_level: log::LevelFilter,
    },
    /// List test cases
    Ls {},
}
