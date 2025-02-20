use console::{style, Term};
use eyre::WrapErr;
use std::collections::HashMap;
use tokio::sync::broadcast;
use tracing::*;

use crate::{
    http,
    runner::{self, Test},
};

/// Reporter trait. The trait is based on the "template method" pattern.
/// You can implement on_xxx methods to hook into the test runner. This way is enough for most usecases.
/// If you need more control, you can override the "run" method.
#[async_trait::async_trait]
pub trait Reporter {
    async fn run(&mut self) -> eyre::Result<()> {
        let mut rx = runner::subscribe()?;

        loop {
            match rx.recv().await {
                Ok(runner::Message::Start(project_name, module_name, test_name)) => {
                    self.on_start(project_name, module_name, test_name).await?;
                }
                Ok(runner::Message::HttpLog(project_name, module_name, test_name, log)) => {
                    self.on_http_call(project_name, module_name, test_name, log)
                        .await?;
                }
                Ok(runner::Message::End(project_name, module_name, test_name, test)) => {
                    self.on_end(project_name, module_name, test_name, test)
                        .await?;
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

    /// Called when a test case starts.
    async fn on_start(
        &mut self,
        _project: String,
        _module: String,
        _test_name: String,
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

#[allow(clippy::vec_box)]
pub struct ListReporter {
    terminal: Term,
    buffer: HashMap<(String, String), Vec<Box<http::Log>>>,
    capture_http: bool,
}

impl ListReporter {
    pub fn new(capture_http: bool) -> ListReporter {
        ListReporter {
            terminal: Term::stdout(),
            buffer: HashMap::new(),
            capture_http,
        }
    }
}

#[async_trait::async_trait]
impl Reporter for ListReporter {
    async fn on_start(
        &mut self,
        project_name: String,
        _module_name: String,
        test_name: String,
    ) -> eyre::Result<()> {
        self.buffer.insert((project_name, test_name), Vec::new());
        Ok(())
    }

    async fn on_http_call(
        &mut self,
        project_name: String,
        _module_name: String,
        test_name: String,
        log: Box<http::Log>,
    ) -> eyre::Result<()> {
        if self.capture_http {
            self.buffer
                .get_mut(&(project_name, test_name.clone()))
                .ok_or_else(|| eyre::eyre!("test case \"{test_name}\" not found in the buffer"))?
                .push(log);
        }
        Ok(())
    }

    async fn on_end(
        &mut self,
        project_name: String,
        _module_name: String,
        test_name: String,
        test: Test,
    ) -> eyre::Result<()> {
        let http_logs = self
            .buffer
            .remove(&(project_name.clone(), test_name.clone()))
            .ok_or_else(|| eyre::eyre!("test case \"{test_name}\" not found in the buffer"))?;

        for log in http_logs {
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

        let Test { result, metadata } = test;
        match result {
            Ok(_res) => {
                let status = style("✓").green();
                self.terminal.write_line(&format!(
                    "{status} [{project_name}] {}::{}",
                    metadata.module, metadata.name
                ))?;
            }
            Err(e) => {
                let status = style("✘").red();
                self.terminal.write_line(&format!(
                    "{status} [{project_name}] {}::{}: {e:#}",
                    metadata.module, metadata.name
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
