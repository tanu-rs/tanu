use console::{style, Term};
use eyre::WrapErr;
use std::collections::HashMap;
use tokio::sync::broadcast;
use tracing::*;

use crate::{
    http,
    runner::{self, Test},
};

#[async_trait::async_trait]
pub trait Reporter {
    async fn run(&mut self) -> eyre::Result<()>;
}

pub struct NullReporter;

#[async_trait::async_trait]
impl Reporter for NullReporter {
    async fn run(&mut self) -> eyre::Result<()> {
        let mut rx = runner::subscribe()?;

        loop {
            trace!("NullReporter polling");
            match rx.recv().await {
                Ok(_test) => {}
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

        debug!("NullReporter stopped");

        Ok(())
    }
}

#[allow(clippy::vec_box)]
pub struct ListReporter {
    buffer: HashMap<(String, String), Vec<Box<http::Log>>>,
    capture_http: bool,
}

impl ListReporter {
    pub fn new(capture_http: bool) -> ListReporter {
        ListReporter {
            buffer: HashMap::new(),
            capture_http,
        }
    }
}

#[async_trait::async_trait]
impl Reporter for ListReporter {
    async fn run(&mut self) -> eyre::Result<()> {
        let mut rx = runner::subscribe()?;

        let term = Term::stdout();
        loop {
            trace!("ListReporter polling");
            match rx.recv().await {
                Ok(runner::Message::Start(project_name, test_name)) => {
                    self.buffer.insert((project_name, test_name), Vec::new());
                }
                Ok(runner::Message::HttpLog(project_name, test_name, log)) => {
                    if self.capture_http {
                        self.buffer
                            .get_mut(&(project_name, test_name.clone()))
                            .ok_or_else(|| {
                                eyre::eyre!("test case \"{test_name}\" not found in the buffer")
                            })?
                            .push(log);
                    }
                }
                Ok(runner::Message::End(project_name, test_name, test)) => {
                    let http_logs = self
                        .buffer
                        .remove(&(project_name.clone(), test_name.clone()))
                        .ok_or_else(|| {
                            eyre::eyre!("test case \"{test_name}\" not found in the buffer")
                        })?;

                    for log in http_logs {
                        write(
                            &term,
                            format!(" => {} {}", log.request.method, log.request.url),
                        )?;
                        write(&term, "  > request:")?;
                        write(&term, "    > headers:")?;
                        for key in log.request.headers.keys() {
                            write(
                                &term,
                                format!(
                                    "       > {key}: {}",
                                    log.request.headers.get(key).unwrap().to_str().unwrap()
                                ),
                            )?;
                        }
                        write(&term, "  < response")?;
                        write(&term, "    < headers:")?;
                        for key in log.response.headers.keys() {
                            write(
                                &term,
                                format!(
                                    "       < {key}: {}",
                                    log.response.headers.get(key).unwrap().to_str().unwrap()
                                ),
                            )?;
                        }
                        write(&term, format!("    < body: {}", log.response.body))?;
                    }

                    let Test { result, metadata } = test;
                    match result {
                        Ok(_res) => {
                            let status = style("✓").green();
                            term.write_line(&format!(
                                "{status} [{project_name}] {}::{}",
                                metadata.module, metadata.name
                            ))?;
                        }
                        Err(e) => {
                            let status = style("✘").red();
                            term.write_line(&format!(
                                "{status} [{project_name}] {}::{}: {e:#}",
                                metadata.module, metadata.name
                            ))?;
                        }
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

        debug!("ListReporter stopped");

        Ok(())
    }
}

fn write(term: &Term, s: impl AsRef<str>) -> eyre::Result<()> {
    let colored = style(s.as_ref()).dim();
    term.write_line(&format!("{colored}"))
        .wrap_err("failed to write character on terminal")
}
