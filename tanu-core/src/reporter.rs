use console::{style, StyledObject, Term};
use eyre::{OptionExt, WrapErr};
use indexmap::IndexMap;
use indicatif::{MultiProgress, ProgressBar};
use itertools::Itertools;
use std::{borrow::Cow, collections::HashMap, time::Duration};
use tokio::sync::broadcast;
use tracing::*;

use crate::{
    get_tanu_config, http,
    runner::{self, Message, Test},
    ModuleName, ProjectName, TestName,
};

#[derive(Debug, Clone, Default, strum::EnumString)]
#[strum(serialize_all = "snake_case")]
pub enum ReporterType {
    Null,
    #[default]
    List,
    Table,
}

async fn run<R: Reporter + Send + ?Sized>(reporter: &mut R) -> eyre::Result<()> {
    let term = Term::stdout();
    term.hide_cursor()?;
    let mut rx = runner::subscribe()?;

    let mut interval = tokio::time::interval(Duration::from_millis(100));
    loop {
        let res = tokio::select! {
            _ = interval.tick() => {
                reporter.on_update(None, None, None).await?;
                Ok(())
            }
            msg = rx.recv() => match msg {
                Ok(Message::Start(project_name, module_name, test_name)) => {
                    match reporter
                        .on_start(project_name.clone(), module_name.clone(), test_name.clone())
                        .await
                    {
                        Ok(_) => {
                            reporter
                                .on_update(Some(project_name), Some(module_name), Some(test_name))
                                .await
                        }
                        e @ Err(_) => e,
                    }
                }
                Ok(Message::HttpLog(project_name, module_name, test_name, log)) => {
                    match reporter
                        .on_http_call(
                            project_name.clone(),
                            module_name.clone(),
                            test_name.clone(),
                            log,
                        )
                        .await
                    {
                        Ok(_) => {
                            reporter
                                .on_update(Some(project_name), Some(module_name), Some(test_name))
                                .await
                        }
                        e @ Err(_) => e,
                    }
                }
                Ok(Message::End(project_name, module_name, test_name, test)) => {
                    match reporter
                        .on_end(
                            project_name.clone(),
                            module_name.clone(),
                            test_name.clone(),
                            test,
                        )
                        .await
                    {
                        Ok(_) => {
                            reporter
                                .on_update(Some(project_name), Some(module_name), Some(test_name))
                                .await
                        }
                        e @ Err(_) => e,
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
        };

        if let Err(e) = res {
            warn!("reporter error: {e:#}");
        }
    }

    term.show_cursor()?;
    Ok(())
}

/// Reporter trait. The trait is based on the "template method" pattern.
/// You can implement on_xxx methods to hook into the test runner. This way is enough for most usecases.
/// If you need more control, you can override the "run" method.
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

    /// Called every after on_start, on_http_call, and on_end.
    async fn on_update(
        &mut self,
        _project_name: Option<String>,
        _module_name: Option<String>,
        _test_name: Option<String>,
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

/// Represents the execution state of a test case.
#[derive(Debug, Clone, Default)]
pub enum ExecutionState {
    /// The test case, module, or project is initialized.
    #[default]
    Initialized,
    /// The test case, module, or project is executing.
    Executing(ProgressBar),
    /// The test case, module, or project has been executed.
    Executed(ProgressBar, Test),
}

impl ExecutionState {
    fn executed(&mut self, test: Test) {
        match std::mem::take(self) {
            ExecutionState::Initialized => {
                warn!("Expected to be in Executing state, but was in Initialized state");
                *self = ExecutionState::Initialized;
            }
            ExecutionState::Executing(pb) => {
                *self = ExecutionState::Executed(pb, test);
            }
            ExecutionState::Executed(pb, _test) => {
                *self = ExecutionState::Executed(pb, test);
            }
        }
    }

    fn update_message(&mut self, msg: impl Into<Cow<'static, str>>) {
        match self {
            ExecutionState::Initialized => {
                warn!("Expected to be in Executing state, but was in Initialized state");
            }
            ExecutionState::Executing(pb) => {
                pb.set_message(msg);
            }
            ExecutionState::Executed(pb, _test) => {
                pb.set_message(msg);
            }
        }
    }

    fn progreess_bar(&self) -> Option<&ProgressBar> {
        match self {
            ExecutionState::Initialized => None,
            ExecutionState::Executing(pb) => Some(pb),
            ExecutionState::Executed(pb, _test) => Some(pb),
        }
    }

    fn is_executing(&self) -> bool {
        matches!(self, ExecutionState::Executing(_))
    }

    fn is_executed(&self) -> bool {
        matches!(self, ExecutionState::Executed(_, _))
    }
}

/// Capture current states of the stdout for the test case.
#[allow(clippy::vec_box)]
struct TestState {
    test_number: usize,
    call_states: Vec<CallState>,
    execution_state: ExecutionState,
}

impl Default for TestState {
    fn default() -> Self {
        TestState {
            test_number: 0,
            call_states: Vec::new(),
            execution_state: ExecutionState::Initialized,
        }
    }
}

struct CallState {
    log: Box<http::Log>,
    execution_state: ExecutionState,
}

pub struct ListReporter {
    test_count: usize,
    states: IndexMap<(ProjectName, ModuleName, TestName), TestState>,
    capture_http: bool,
    multi_progress: MultiProgress,
}

impl ListReporter {
    pub fn new(capture_http: bool) -> ListReporter {
        let multi_progress = MultiProgress::new();
        multi_progress.set_move_cursor(true);
        ListReporter {
            test_count: 0,
            states: IndexMap::new(),
            capture_http,
            multi_progress,
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
        self.test_count += 1;
        let progress_bar = self.multi_progress.add(ProgressBar::new_spinner());
        self.states.insert(
            (project_name, module_name, test_name),
            TestState {
                test_number: self.test_count,
                execution_state: ExecutionState::Executing(progress_bar),
                ..Default::default()
            },
        );
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
            let state = self
                .states
                .get_mut(&(project_name, module_name, test_name.clone()))
                .ok_or_else(|| eyre::eyre!("test case \"{test_name}\" not found in the buffer"))?;

            let url = style(&log.request.url).dim();
            let test_number = state.test_number;
            let space = " ".repeat(test_number.to_string().len());
            let pb = state
                .execution_state
                .progreess_bar()
                .ok_or_eyre("missing progress bar")?;
            let http_pb = self
                .multi_progress
                .insert_after(pb, ProgressBar::new_spinner());
            http_pb.set_message(format!("  {space} {url}"));
            http_pb.finish();

            state.call_states.push(CallState {
                log,
                execution_state: ExecutionState::Executing(http_pb),
            });
        }
        Ok(())
    }

    async fn on_update(
        &mut self,
        project_name: Option<String>,
        module_name: Option<String>,
        test_name: Option<String>,
    ) -> eyre::Result<()> {
        match (project_name, module_name, test_name) {
            (Some(project_name), Some(module_name), Some(test_name)) => {
                let state = self
                    .states
                    .get_mut(&(project_name.clone(), module_name.clone(), test_name.clone()))
                    .ok_or_else(|| {
                        eyre::eyre!("test case \"{test_name}\" not found in the test state")
                    })?;

                let status = symbol_test_result(&state.execution_state);
                let test_number = style(state.test_number).dim();
                state.execution_state.update_message(format!(
                    "{status} {test_number} [{project_name}] {module_name}::{test_name}",
                ));
            }
            _ => {
                for state in self.states.values_mut() {
                    match &state.execution_state {
                        ExecutionState::Executing(pb) => {
                            if !pb.is_finished() {
                                pb.tick();
                                for call_state in &state.call_states {
                                    let pb = call_state
                                        .execution_state
                                        .progreess_bar()
                                        .ok_or_eyre("missing progress bar")?;
                                    if !pb.is_finished() {
                                        pb.tick();
                                    }
                                }
                            }
                        }
                        ExecutionState::Executed(pb, _test) => {
                            pb.finish_and_clear();
                            for call_state in &state.call_states {
                                call_state
                                    .execution_state
                                    .progreess_bar()
                                    .ok_or_eyre("missing progress bar")?
                                    .finish();
                            }
                            self.multi_progress.remove(pb);
                        }
                        _ => {}
                    }
                }
                //self.reorder_progress_bars();
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
        let state = self
            .states
            .get_mut(&(project_name.clone(), module_name, test_name.clone()))
            .ok_or_else(|| eyre::eyre!("test case \"{test_name}\" not found in the buffer"))?;
        state.execution_state.executed(test);

        Ok(())
    }
}

impl ListReporter {
    fn reorder_progress_bars(&mut self) {
        let states_vec: Vec<_> = self.states.values().collect();
        let mut i = 0;

        // Process each state
        while i < states_vec.len() {
            let es = &states_vec[i].execution_state;

            // Only reorder executing progress bars
            if !es.is_executing() {
                i += 1;
                continue;
            }

            // Look ahead up to 5 states to see if we need to reorder
            let mut needs_reorder = false;
            let mut executed_count = 0;

            for j in i + 1..std::cmp::min(i + 6, states_vec.len()) {
                let next_es = &states_vec[j].execution_state;
                if next_es.is_executed() {
                    executed_count += 1;
                }
            }

            // If there are executed states after this executing state, it needs reordering
            needs_reorder = executed_count > 0;

            if needs_reorder {
                if let Some(pb) = es.progreess_bar() {
                    self.multi_progress.remove(pb);
                    self.multi_progress.add(pb.clone());
                }
            }

            i += 1;
        }
    }
}

fn write(term: &Term, s: impl AsRef<str>) -> eyre::Result<()> {
    let colored = style(s.as_ref()).dim();
    term.write_line(&format!("{colored}"))
        .wrap_err("failed to write character on terminal")
}

fn symbol_test_result(execution_state: &ExecutionState) -> StyledObject<&'static str> {
    match execution_state {
        ExecutionState::Initialized => style(" "),
        ExecutionState::Executing(_pb) => style(" "),
        ExecutionState::Executed(_pb, test) => match test.result {
            Ok(_) => style("✓").green(),
            Err(_) => style("✘").red(),
        },
    }
}

fn emoji_symbol_test_result(test: &Test) -> char {
    match test.result {
        Ok(_) => '🟢',
        Err(_) => '🔴',
    }
}

#[allow(clippy::vec_box, dead_code)]
pub struct TableReporter {
    terminal: Term,
    buffer: HashMap<(ProjectName, ModuleName, TestName), Test>,
    capture_http: bool,
}

impl TableReporter {
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
