use itertools::Itertools;
use ratatui::{
    prelude::*,
    widgets::{Block, HighlightSpacing, List, ListState},
};
use std::{sync::Arc, time::SystemTime};
use tanu_core::{self, Filter, TestIgnoreFilter, TestInfo, TestOnlyFilter};
use throbber_widgets_tui::ThrobberState;

use crate::{
    widget::{
        tabbed_block::CustomTabs,
        theme::{self, fmt_duration, muted, SELECTED_STYLE},
    },
    TestResult,
};

const EXPANDED: &str = "▾";

const COLLAPSED: &str = "▸";

const HIGHLIGHT_SYMBOL: &str = "▌";

/// Display width of `HIGHLIGHT_SYMBOL`.
const HIGHLIGHT_SYMBOL_WIDTH: u16 = 1;

pub struct TestListWidget<'a> {
    block: Block<'a>,
    tabs: Option<CustomTabs<'a>>,
}

impl<'a> TestListWidget<'a> {
    pub fn new(focused: bool, maximized: bool, state: &TestListState) -> Self {
        let mut title = vec![Span::raw("Tests")];
        title.push(Span::styled(format!(" ({})", state.len()), muted()));
        if state.status_filter != StatusFilter::All {
            title.push(Span::styled(
                format!(" [{}]", state.status_filter.label()),
                Style::new().fg(theme::RUNNING),
            ));
        }
        if !state.search.is_empty() || state.searching {
            title.push(Span::styled(
                format!(" /{}", state.search),
                Style::new().fg(theme::RUNNING),
            ));
        }
        if maximized {
            title.push(Span::styled(" [maximized]", muted()));
        }
        let mut block = theme::block(title, focused);
        if state.visible_rows().is_empty() {
            block = block.title_bottom(Line::styled(" no tests match the filter ", muted()));
        }

        // One tab per project, shown only when there are several projects.
        let tabs = (state.projects.len() > 1).then(|| {
            let tabs = state
                .projects
                .iter()
                .map(|project| {
                    let counts = project.counts();
                    let badge = if counts.failed > 0 {
                        Span::styled(
                            format!(" ✘{}", counts.failed),
                            Style::new().fg(theme::FAIL).bold(),
                        )
                    } else {
                        Span::styled(format!(" {}/{}", counts.passed, counts.total), muted())
                    };
                    (project.name.clone(), badge)
                })
                .collect();
            let failed = state
                .projects
                .iter()
                .enumerate()
                .filter(|(_, project)| project.counts().failed > 0)
                .map(|(p, _)| p)
                .collect();
            CustomTabs::new(tabs)
                .select(state.current_project)
                .alerts(failed)
        });

        TestListWidget { block, tabs }
    }
}

impl StatefulWidget for TestListWidget<'_> {
    type State = TestListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let inner = self.block.inner(area);
        self.block.render(area, buf);

        state.tab_hits.clear();
        let list_area = match self.tabs {
            Some(tabs) => {
                let [layout_tabs, layout_list] =
                    Layout::vertical([Constraint::Length(2), Constraint::Fill(1)]).areas(inner);
                for (p, start, end) in tabs.hit_areas(layout_tabs) {
                    state.tab_hits.push((p, start, end, layout_tabs.y));
                }
                tabs.render(layout_tabs, buf);
                layout_list
            }
            None => inner,
        };
        state.list_area = list_area;

        // Build lines only for the rows on screen; the list can have thousands of rows.
        let rows = state.visible_rows();
        let height = list_area.height as usize;
        let selected = state.list_state.selected().filter(|s| *s < rows.len());
        let mut offset = state.list_state.offset();
        if let Some(selected) = selected {
            if selected < offset {
                offset = selected;
            } else if height > 0 && selected >= offset + height {
                offset = selected + 1 - height;
            }
        }
        offset = offset.min(rows.len().saturating_sub(height));
        let end = (offset + height).min(rows.len());

        // Room left of the highlight symbol column.
        let content_width = list_area.width.saturating_sub(HIGHLIGHT_SYMBOL_WIDTH) as usize;
        let lines = rows[offset..end]
            .iter()
            .map(|row| state.row_line(*row, content_width))
            .collect::<Vec<_>>();
        let list_widget = List::new(lines)
            .highlight_style(SELECTED_STYLE)
            .highlight_symbol(Line::styled(
                HIGHLIGHT_SYMBOL,
                Style::new().fg(theme::ACCENT),
            ))
            .highlight_spacing(HighlightSpacing::Always);
        let mut window = ListState::default().with_selected(selected.map(|s| s - offset));
        StatefulWidget::render(list_widget, list_area, buf, &mut window);
        *state.list_state.offset_mut() = offset;
    }
}

/// Helper function to create a symbol for test result.
fn symbol_test_result(execution_state: &ExecutionState) -> Span<'static> {
    match execution_state {
        ExecutionState::Initialized => Span::styled("○ ", muted()),
        ExecutionState::Executing(throbber_state) => {
            let throbber = throbber_widgets_tui::Throbber::default()
                .throbber_style(Style::new().fg(theme::RUNNING));
            throbber.to_symbol_span(throbber_state)
        }
        ExecutionState::Executed(test_result) => {
            if test_result.is_ok() {
                Span::styled("✓ ", Style::default().fg(theme::OK).bold())
            } else {
                Span::styled("✘ ", Style::default().fg(theme::FAIL).bold())
            }
        }
    }
}

/// Builds a line that fits into `width`: `prefix` + `name` on the left and `meta` right-aligned.
/// The name is truncated with an ellipsis if there is not enough room.
fn fit_line(
    prefix: Vec<Span<'static>>,
    name: String,
    name_style: Style,
    meta: Vec<Span<'static>>,
    width: usize,
) -> Line<'static> {
    let prefix_width: usize = prefix.iter().map(|s| s.width()).sum();
    let meta_width: usize = meta.iter().map(|s| s.width()).sum();
    let name_width = name.chars().count();

    let mut spans = prefix;
    let room_for_name = width.saturating_sub(prefix_width + meta_width + 1);
    if meta_width > 0 && room_for_name >= 8.min(name_width) {
        let name = if name_width > room_for_name {
            let truncated: String = name.chars().take(room_for_name.saturating_sub(1)).collect();
            format!("{truncated}…")
        } else {
            name
        };
        let used = prefix_width + name.chars().count();
        spans.push(Span::styled(name, name_style));
        spans.push(Span::raw(
            " ".repeat(width.saturating_sub(used + meta_width)),
        ));
        spans.extend(meta);
    } else {
        // Not enough room for metadata; show the name only.
        spans.push(Span::styled(name, name_style));
    }
    Line::from(spans)
}

/// `✘2 12/14` style counters for project and module rows.
fn counter_spans(counts: Counts) -> Vec<Span<'static>> {
    let mut spans = vec![];
    if counts.failed > 0 {
        spans.push(Span::styled(
            format!("✘{} ", counts.failed),
            Style::new().fg(theme::FAIL).bold(),
        ));
    }
    if counts.running > 0 {
        spans.push(Span::styled(
            format!("⋯{} ", counts.running),
            Style::new().fg(theme::RUNNING),
        ));
    }
    spans.push(Span::styled(
        format!("{}/{}", counts.passed, counts.total),
        muted(),
    ));
    spans
}

/// Aggregated counts of test states.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub running: usize,
}

impl Counts {
    pub fn pending(&self) -> usize {
        self.total - self.passed - self.failed - self.running
    }

    fn add(&mut self, state: &ExecutionState) {
        self.total += 1;
        match state {
            ExecutionState::Initialized => {}
            ExecutionState::Executing(_) => self.running += 1,
            ExecutionState::Executed(result) if result.is_ok() => self.passed += 1,
            ExecutionState::Executed(_) => self.failed += 1,
        }
    }
}

/// The main state controller for test cases.
pub struct ExecutionStateController;

impl ExecutionStateController {
    /// Executes all test cases in the list.
    pub fn execute_all(test_cases_list: &mut TestListState) {
        test_cases_list
            .projects
            .iter_mut()
            .for_each(|project_state| {
                Self::execute_project(project_state);
            });
    }

    /// Executes the specified test cases in the list.
    pub fn execute_specified(test_cases_list: &mut TestListState, selector: &TestCaseSelector) {
        for project_state in test_cases_list
            .projects
            .iter_mut()
            .filter(|p| p.name == selector.project)
        {
            // Project is selected in the list.
            if selector.module.is_none() && selector.test.is_none() {
                Self::execute_project(project_state)
            }

            if let Some(module) = &selector.module {
                for module_state in project_state
                    .modules
                    .iter_mut()
                    .filter(|m| &m.name == module)
                {
                    // Module is selected in the list.
                    if selector.test.is_none() {
                        Self::execute_module(module_state);
                    }

                    if let Some(ref test) = selector.test {
                        for test_state in module_state
                            .tests
                            .iter_mut()
                            .filter(|t| &t.info.full_name() == test)
                        {
                            // Test is selected in the list.
                            Self::execute_test(test_state);
                        }
                        module_state.execution_state.execute();
                    }
                }
                project_state.execution_state.execute();
            }
        }
    }

    /// Execute the specified project and its modules and tests.
    fn execute_project(project_state: &mut ProjectState) {
        project_state.execution_state.execute();

        // Propagate the execution state to all modules.
        project_state
            .modules
            .iter_mut()
            .for_each(Self::execute_module);
    }

    /// Execute the specified module and its tests.
    fn execute_module(module_state: &mut ModuleState) {
        module_state.execution_state.execute();

        // Propagate the execution state to all tests.
        module_state.tests.iter_mut().for_each(Self::execute_test);
    }

    /// Execute the specified test case.
    fn execute_test(test_state: &mut TestState) {
        test_state.expanded = false;
        test_state.execution_state.execute();
    }

    /// Handler for when a test case is updated.
    pub fn on_test_updated(
        test_cases_list: &mut TestListState,
        project_name: &str,
        module_name: &str,
        name: &str,
        test_result: TestResult,
    ) {
        test_cases_list
            .projects
            .iter_mut()
            .filter(|p| p.name == project_name)
            .for_each(|project_state| {
                let mut project_updated = false;
                project_state
                    .modules
                    .iter_mut()
                    .filter(|m| m.name == module_name)
                    .for_each(|module_state| {
                        let mut module_updated = false;
                        module_state
                            .tests
                            .iter_mut()
                            .filter(|t| t.info.name == name)
                            .for_each(|test_state| {
                                test_state.execution_state.executed(test_result.clone());
                                module_updated = true;
                                project_updated = true;
                            });
                        if module_updated {
                            Self::on_module_updated(module_state);
                        }
                    });
                if project_updated {
                    Self::on_project_updated(project_state);
                }
            });
    }

    /// Handler for when a module is updated.
    pub fn on_module_updated(module_state: &mut ModuleState) {
        // A module is done once none of its tests is executing.
        let still_executing = module_state
            .tests
            .iter()
            .any(|test| matches!(test.execution_state, ExecutionState::Executing(_)));
        if still_executing {
            return;
        }
        let ok = module_state
            .tests
            .iter()
            .all(|test| match &test.execution_state {
                ExecutionState::Executed(result) => result.is_ok(),
                _ => true,
            });
        module_state
            .execution_state
            .executed(aggregated_result(&module_state.project_name, ok));
    }

    /// Handler for when a project is updated.
    pub fn on_project_updated(project_state: &mut ProjectState) {
        // A project is done once none of its modules is executing.
        let still_executing = project_state
            .modules
            .iter()
            .any(|module| matches!(module.execution_state, ExecutionState::Executing(_)));
        if still_executing {
            return;
        }
        let ok = project_state
            .modules
            .iter()
            .all(|module| match &module.execution_state {
                ExecutionState::Executed(result) => result.is_ok(),
                _ => true,
            });
        project_state
            .execution_state
            .executed(aggregated_result(&project_state.name, ok));
    }

    pub fn update_throbber(test_cases_list: &mut TestListState) {
        test_cases_list
            .projects
            .iter_mut()
            .for_each(|project_state| {
                project_state.execution_state.update_throbber();
                project_state.modules.iter_mut().for_each(|module_state| {
                    module_state.execution_state.update_throbber();
                    module_state.tests.iter_mut().for_each(|test_state| {
                        test_state.execution_state.update_throbber();
                    });
                })
            });
    }
}

/// Builds a synthetic result for projects and modules.
fn aggregated_result(project_name: &str, ok: bool) -> TestResult {
    TestResult {
        project_name: project_name.to_string(),
        test: Some(tanu_core::runner::Test {
            info: Arc::new(TestInfo::default()),
            worker_id: 0,
            result: if ok {
                Ok(())
            } else {
                Err(tanu_core::runner::Error::ErrorReturned(
                    "Execution failed".into(),
                ))
            },
            started_at: SystemTime::UNIX_EPOCH,
            ended_at: SystemTime::UNIX_EPOCH,
            request_time: std::time::Duration::from_secs(0),
        }),
        ..Default::default()
    }
}

/// Represents the execution state of a test case, module, or project.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Default)]
pub enum ExecutionState {
    /// The test case, module, or project is initialized.
    #[default]
    Initialized,
    /// The test case, module, or project is executing.
    Executing(ThrobberState),
    /// The test case, module, or project has been executed.
    Executed(TestResult),
}

impl ExecutionState {
    /// Transition to the executing state.
    fn execute(&mut self) {
        let _ = std::mem::replace(self, ExecutionState::Executing(ThrobberState::default()));
    }

    /// Transition to the executed state with the given test result.
    fn executed(&mut self, test_result: TestResult) {
        let _ = std::mem::replace(self, ExecutionState::Executed(test_result));
    }

    fn update_throbber(&mut self) {
        match self {
            ExecutionState::Executing(throbber_state) => {
                throbber_state.calc_next();
            }
            ExecutionState::Initialized => {}
            ExecutionState::Executed(_) => {}
        }
    }

    /// The test result if the test has been executed.
    pub fn result(&self) -> Option<&TestResult> {
        match self {
            ExecutionState::Executed(result) => Some(result),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct ProjectState {
    /// Project name
    pub name: String,
    /// true: the list item is expanded, false: not expanded
    pub expanded: bool,
    /// List of modules under this project
    pub modules: Vec<ModuleState>,
    /// The execution state of the project.
    pub execution_state: ExecutionState,
}

impl ProjectState {
    pub fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for module in &self.modules {
            for test in &module.tests {
                counts.add(&test.execution_state);
            }
        }
        counts
    }
}

#[derive(Debug)]
pub struct ModuleState {
    /// Project name
    pub project_name: String,
    /// Module name
    pub name: String,
    /// true: the list item is expanded, false: not expanded
    pub expanded: bool,
    /// List of test cases under this module
    pub tests: Vec<TestState>,
    /// The execution state of the module.
    pub execution_state: ExecutionState,
}

impl ModuleState {
    pub fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for test in &self.tests {
            counts.add(&test.execution_state);
        }
        counts
    }
}

#[derive(Debug, Clone)]
pub struct TestState {
    pub info: TestInfo,
    /// true: the list item is expanded, false: not expanded
    pub expanded: bool,
    /// The execution state of the test.
    pub execution_state: ExecutionState,
}

impl TestState {
    /// Number of HTTP and gRPC calls made by the test.
    pub fn call_count(&self) -> usize {
        self.execution_state
            .result()
            .map(TestResult::call_count)
            .unwrap_or_default()
    }

    fn matches(&self, filter: StatusFilter) -> bool {
        match filter {
            StatusFilter::All => true,
            StatusFilter::Failed => self.execution_state.result().is_some_and(|r| !r.is_ok()),
            StatusFilter::Passed => self.execution_state.result().is_some_and(|r| r.is_ok()),
            StatusFilter::NotRun => {
                matches!(self.execution_state, ExecutionState::Initialized)
            }
        }
    }
}

/// Filter the test list by execution status.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum StatusFilter {
    #[default]
    All,
    Failed,
    Passed,
    NotRun,
}

impl StatusFilter {
    pub fn next(self) -> StatusFilter {
        match self {
            StatusFilter::All => StatusFilter::Failed,
            StatusFilter::Failed => StatusFilter::Passed,
            StatusFilter::Passed => StatusFilter::NotRun,
            StatusFilter::NotRun => StatusFilter::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            StatusFilter::All => "all",
            StatusFilter::Failed => "failed",
            StatusFilter::Passed => "passed",
            StatusFilter::NotRun => "not run",
        }
    }
}

/// A reference to a visible row in the test list.
///
/// Indices point into `TestListState::projects`, its modules, tests and
/// the calls (HTTP calls first, then gRPC calls) of a test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowRef {
    Project(usize),
    Module(usize, usize),
    Test(usize, usize, usize),
    Call(usize, usize, usize, usize),
}

impl RowRef {
    /// Key to order rows in tree order.
    fn order_key(self) -> (usize, isize, isize, isize) {
        match self {
            RowRef::Project(p) => (p, -1, -1, -1),
            RowRef::Module(p, m) => (p, m as isize, -1, -1),
            RowRef::Test(p, m, t) => (p, m as isize, t as isize, -1),
            RowRef::Call(p, m, t, c) => (p, m as isize, t as isize, c as isize),
        }
    }

    fn parent(self) -> Option<RowRef> {
        match self {
            RowRef::Project(_) => None,
            RowRef::Module(p, _) => Some(RowRef::Project(p)),
            RowRef::Test(p, m, _) => Some(RowRef::Module(p, m)),
            RowRef::Call(p, m, t, _) => Some(RowRef::Test(p, m, t)),
        }
    }
}

/// Represents the item to execute in the `TestListWidget`.
///
/// - A project is always selected
/// - A module may be selected if viewing inside a project
/// - A test case may be selected if viewing inside a module
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TestCaseSelector {
    /// The name of the selected project.
    pub project: String,
    /// The name of the selected module, if any.
    pub module: Option<String>,
    /// The full name of the selected test case, if any.
    pub test: Option<String>,
}

#[derive(Debug)]
pub struct TestListState {
    pub projects: Vec<ProjectState>,
    pub list_state: ListState,
    /// Common crate prefix of all module names, hidden in the list.
    pub module_prefix: String,
    /// Case-insensitive search query.
    pub search: String,
    /// true while the user is typing a search query.
    pub searching: bool,
    /// Filter by execution status.
    pub status_filter: StatusFilter,
    /// Index of the project shown in the list; each project has its own tab.
    pub current_project: usize,
    /// Cursor position of each project, restored when switching back to it.
    saved_selection: Vec<Option<usize>>,
    /// Screen area of the list rows from the last render, for mouse clicks.
    pub list_area: Rect,
    /// Horizontal ranges `(project, x_start, x_end, y)` of the project tabs.
    tab_hits: Vec<(usize, u16, u16, u16)>,
}

impl TestListState {
    pub fn new(
        projects: &[Arc<tanu_core::ProjectConfig>],
        test_cases: &[TestInfo],
    ) -> TestListState {
        let test_ignore_filter = TestIgnoreFilter::default();
        let test_only_filter = TestOnlyFilter::default();
        let grouped_by_module: Vec<(String, Vec<TestState>)> = test_cases
            .iter()
            .cloned()
            .map(|info| TestState {
                info,
                expanded: false,
                execution_state: ExecutionState::default(),
            })
            .into_group_map_by(|test| test.info.module.clone())
            .into_iter()
            .map(|(module, mut tests)| {
                tests.sort_by(|a, b| (a.info.line, &a.info.name).cmp(&(b.info.line, &b.info.name)));
                (module, tests)
            })
            .sorted_by(|(a, _), (b, _)| a.cmp(b))
            .collect();

        let module_prefix = common_module_prefix(grouped_by_module.iter().map(|(m, _)| m.as_str()));

        let projects: Vec<_> = projects
            .iter()
            .map(|proj| ProjectState {
                name: proj.name.clone(),
                expanded: true,
                modules: grouped_by_module
                    .iter()
                    .cloned()
                    .map(|(module_name, tests)| ModuleState {
                        project_name: proj.name.clone(),
                        name: module_name,
                        expanded: true,
                        tests: tests
                            .into_iter()
                            .filter(|test| test_ignore_filter.filter(proj, &test.info))
                            .filter(|test| test_only_filter.filter(proj, &test.info))
                            .collect(),
                        execution_state: ExecutionState::default(),
                    })
                    .filter(|module|
                        // Filter out module that has no test cases
                        !module.tests.is_empty())
                    .collect(),
                execution_state: ExecutionState::default(),
            })
            .collect();

        let mut state = TestListState {
            projects,
            list_state: ListState::default().with_selected(Some(0)),
            module_prefix,
            search: String::new(),
            searching: false,
            status_filter: StatusFilter::default(),
            current_project: 0,
            saved_selection: vec![],
            list_area: Rect::default(),
            tab_hits: vec![],
        };
        state.clamp_selection();
        state
    }

    /// true if a search query or status filter is active.
    pub fn is_filtering(&self) -> bool {
        !self.search.is_empty() || self.status_filter != StatusFilter::All
    }

    fn test_visible(&self, module: &ModuleState, test: &TestState) -> bool {
        if !test.matches(self.status_filter) {
            return false;
        }
        if self.search.is_empty() {
            return true;
        }
        let query = self.search.to_lowercase();
        test.info.name.to_lowercase().contains(&query)
            || self
                .display_module_name(&module.name)
                .to_lowercase()
                .contains(&query)
    }

    /// Returns all rows currently visible in the list, in display order.
    pub fn visible_rows(&self) -> Vec<RowRef> {
        let filtering = self.is_filtering();
        let mut rows = vec![];
        let p = self.current_project;
        if let Some(project) = self.projects.get(p) {
            let modules: Vec<(usize, Vec<usize>)> = project
                .modules
                .iter()
                .enumerate()
                .map(|(m, module)| {
                    let tests = module
                        .tests
                        .iter()
                        .enumerate()
                        .filter(|(_, test)| self.test_visible(module, test))
                        .map(|(t, _)| t)
                        .collect::<Vec<_>>();
                    (m, tests)
                })
                .filter(|(_, tests)| !tests.is_empty())
                .collect();
            if filtering && modules.is_empty() {
                return rows;
            }

            // The project row stays at the top of its tab, so that it can be run or
            // inspected as a whole. It cannot be collapsed.
            rows.push(RowRef::Project(p));
            for (m, tests) in modules {
                rows.push(RowRef::Module(p, m));
                if !(project.modules[m].expanded || filtering) {
                    continue;
                }
                for t in tests {
                    rows.push(RowRef::Test(p, m, t));
                    let test = &project.modules[m].tests[t];
                    if test.expanded {
                        rows.extend((0..test.call_count()).map(|c| RowRef::Call(p, m, t, c)));
                    }
                }
            }
        }
        rows
    }

    /// Module name without the common crate prefix.
    pub fn display_module_name<'a>(&self, module: &'a str) -> &'a str {
        module.strip_prefix(&self.module_prefix).unwrap_or(module)
    }

    pub fn project(&self, p: usize) -> Option<&ProjectState> {
        self.projects.get(p)
    }

    pub fn module(&self, p: usize, m: usize) -> Option<&ModuleState> {
        self.project(p)?.modules.get(m)
    }

    pub fn test(&self, p: usize, m: usize, t: usize) -> Option<&TestState> {
        self.module(p, m)?.tests.get(t)
    }

    /// Builds the display line of a row.
    fn row_line(&self, row: RowRef, width: usize) -> Line<'static> {
        let icon = |expanded: bool| if expanded { EXPANDED } else { COLLAPSED };
        match row {
            RowRef::Project(p) => {
                let project = &self.projects[p];
                fit_line(
                    vec![symbol_test_result(&project.execution_state)],
                    project.name.clone(),
                    Style::new().bold(),
                    counter_spans(project.counts()),
                    width,
                )
            }
            RowRef::Module(p, m) => {
                let module = &self.projects[p].modules[m];
                fit_line(
                    vec![
                        Span::styled(format!("  {} ", icon(module.expanded)), muted()),
                        symbol_test_result(&module.execution_state),
                    ],
                    self.display_module_name(&module.name).to_string(),
                    Style::new().fg(theme::ACCENT),
                    counter_spans(module.counts()),
                    width,
                )
            }
            RowRef::Test(p, m, t) => {
                let test = &self.projects[p].modules[m].tests[t];
                let expander = if test.call_count() == 0 {
                    " "
                } else {
                    icon(test.expanded)
                };
                let mut meta = vec![];
                if let Some(result) = test.execution_state.result() {
                    if result.retries > 0 {
                        meta.push(Span::styled(
                            format!("↻{} ", result.retries),
                            Style::new().fg(theme::RUNNING),
                        ));
                    }
                    match result.call_count() {
                        0 => {}
                        1 => meta.push(Span::styled("1 call  ", muted())),
                        n => meta.push(Span::styled(format!("{n} calls  "), muted())),
                    }
                    if let Some(duration) = result.duration() {
                        meta.push(Span::styled(
                            format!("{:>7}", fmt_duration(duration)),
                            muted(),
                        ));
                    }
                }
                fit_line(
                    vec![
                        Span::styled(format!("    {expander} "), muted()),
                        symbol_test_result(&test.execution_state),
                    ],
                    test.info.name.clone(),
                    Style::new(),
                    meta,
                    width,
                )
            }
            RowRef::Call(p, m, t, c) => {
                let test = &self.projects[p].modules[m].tests[t];
                let Some(call) = test.execution_state.result().and_then(|r| r.call(c)) else {
                    return Line::default();
                };
                let status_style = Style::new().fg(call.status_color()).bold();
                fit_line(
                    vec![
                        Span::styled("        ", muted()),
                        Span::styled(format!("{:<6} ", call.method()), muted().bold()),
                        Span::styled(format!("{} ", call.status_label()), status_style),
                    ],
                    call.target(),
                    Style::new(),
                    vec![Span::styled(
                        format!("{:>7}", fmt_duration(call.duration())),
                        muted(),
                    )],
                    width,
                )
            }
        }
    }

    /// The row under the cursor.
    pub fn selected_row(&self) -> Option<RowRef> {
        let selected = self.list_state.selected()?;
        self.visible_rows().get(selected).copied()
    }

    /// Keep the selection within the visible rows.
    pub fn clamp_selection(&mut self) {
        let len = self.visible_rows().len();
        match self.list_state.selected() {
            _ if len == 0 => self.list_state.select(None),
            None => self.list_state.select(Some(0)),
            Some(selected) if selected >= len => self.list_state.select(Some(len - 1)),
            Some(_) => {}
        }
    }

    fn select_row(&mut self, row: RowRef) {
        if let Some(index) = self.visible_rows().iter().position(|r| *r == row) {
            self.list_state.select(Some(index));
        }
    }

    fn is_expanded(&self, row: RowRef) -> Option<bool> {
        match row {
            RowRef::Project(_) => None,
            RowRef::Module(p, m) => self.module(p, m).map(|m| m.expanded),
            RowRef::Test(p, m, t) => self
                .test(p, m, t)
                .filter(|t| t.call_count() > 0)
                .map(|t| t.expanded),
            RowRef::Call(..) => None,
        }
    }

    fn set_expanded(&mut self, row: RowRef, expanded: bool) {
        match row {
            RowRef::Project(p) => self.projects[p].expanded = expanded,
            RowRef::Module(p, m) => self.projects[p].modules[m].expanded = expanded,
            RowRef::Test(p, m, t) => self.projects[p].modules[m].tests[t].expanded = expanded,
            RowRef::Call(..) => {}
        }
    }

    /// Expands or collapses the selected item in the list.
    ///
    /// Expanding a test with calls moves the cursor to its first call so that
    /// the details of the call are shown right away.
    pub fn expand(&mut self) {
        let Some(row) = self.selected_row() else {
            return;
        };
        let Some(expanded) = self.is_expanded(row) else {
            return;
        };
        self.set_expanded(row, !expanded);
        if let (RowRef::Test(p, m, t), false) = (row, expanded) {
            self.select_row(RowRef::Call(p, m, t, 0));
        }
    }

    /// Collapses the selected item, or moves to its parent if it is already collapsed.
    pub fn collapse_or_parent(&mut self) {
        let Some(row) = self.selected_row() else {
            return;
        };
        if self.is_expanded(row) == Some(true) {
            self.set_expanded(row, false);
        } else if let Some(parent) = row.parent() {
            self.select_row(parent);
        }
    }

    /// Expands the selected item, or moves to its first child if it is already expanded.
    pub fn expand_or_child(&mut self) {
        let Some(row) = self.selected_row() else {
            return;
        };
        match self.is_expanded(row) {
            Some(false) => self.set_expanded(row, true),
            Some(true) => self.list_state.select_next(),
            None => {}
        }
        self.clamp_selection();
    }

    /// Moves the cursor to the next (or previous) failed test, expanding its parents.
    pub fn jump_to_failure(&mut self, forward: bool) {
        let failures: Vec<RowRef> = self
            .projects
            .iter()
            .enumerate()
            .flat_map(|(p, project)| {
                project
                    .modules
                    .iter()
                    .enumerate()
                    .flat_map(move |(m, module)| {
                        module
                            .tests
                            .iter()
                            .enumerate()
                            .filter_map(move |(t, test)| {
                                test.matches(StatusFilter::Failed)
                                    .then_some(RowRef::Test(p, m, t))
                            })
                    })
            })
            .filter(|row| {
                let RowRef::Test(p, m, t) = *row else {
                    return false;
                };
                let module = &self.projects[p].modules[m];
                self.test_visible(module, &module.tests[t])
            })
            .collect();
        if failures.is_empty() {
            return;
        }

        let current = self.selected_row().map(RowRef::order_key).unwrap_or((
            self.current_project,
            -1,
            -1,
            -1,
        ));
        let target = if forward {
            failures
                .iter()
                .find(|row| row.order_key() > current)
                .or(failures.first())
        } else {
            failures
                .iter()
                .rev()
                .find(|row| row.order_key() < current)
                .or(failures.last())
        };
        let Some(&target) = target else {
            return;
        };
        if let RowRef::Test(p, m, _) = target {
            self.show_project(p);
            self.projects[p].modules[m].expanded = true;
        }
        self.select_row(target);
    }

    /// Shows the tab of project `p`, saving the cursor of the current one.
    pub fn show_project(&mut self, p: usize) {
        if p >= self.projects.len() || p == self.current_project {
            return;
        }
        if self.saved_selection.len() < self.projects.len() {
            self.saved_selection.resize(self.projects.len(), None);
        }
        self.saved_selection[self.current_project] = self.list_state.selected();
        self.current_project = p;
        let selected = self.saved_selection[p].or(Some(0));
        self.list_state = ListState::default().with_selected(selected);
        self.clamp_selection();
    }

    /// Shows the next (`delta = 1`) or previous (`delta = -1`) project tab, wrapping around.
    pub fn cycle_project(&mut self, delta: isize) {
        let n = self.projects.len() as isize;
        if n > 1 {
            let p = (self.current_project as isize + delta).rem_euclid(n);
            self.show_project(p as usize);
        }
    }

    /// Returns the project whose tab is at the given screen position, if any.
    pub fn tab_at(&self, x: u16, y: u16) -> Option<usize> {
        self.tab_hits
            .iter()
            .find(|(_, start, end, row)| *row == y && (*start..*end).contains(&x))
            .map(|(p, ..)| *p)
    }

    /// Selects the given test, expanding its parents. Filters that hide it are cleared.
    pub fn select_test(&mut self, project: &str, module: &str, name: &str) {
        let found = self.projects.iter().enumerate().find_map(|(p, proj)| {
            (proj.name == project).then_some(())?;
            proj.modules
                .iter()
                .enumerate()
                .find_map(|(m, module_state)| {
                    (module_state.name == module).then_some(())?;
                    let t = module_state
                        .tests
                        .iter()
                        .position(|t| t.info.name == name)?;
                    Some((p, m, t))
                })
        });
        let Some((p, m, t)) = found else {
            return;
        };
        self.show_project(p);
        self.projects[p].modules[m].expanded = true;
        let row = RowRef::Test(p, m, t);
        if !self.visible_rows().contains(&row) {
            self.search.clear();
            self.status_filter = StatusFilter::All;
        }
        self.select_row(row);
    }

    /// Selects the row at the given index of visible rows.
    pub fn select_index(&mut self, index: usize) {
        self.list_state.select(Some(index));
        self.clamp_selection();
    }

    /// Find the item to execute from the currently selected row.
    pub fn select_test_case(&self) -> Option<TestCaseSelector> {
        match self.selected_row()? {
            RowRef::Project(p) => Some(TestCaseSelector {
                project: self.projects[p].name.clone(),
                ..Default::default()
            }),
            RowRef::Module(p, m) => Some(TestCaseSelector {
                project: self.projects[p].name.clone(),
                module: Some(self.projects[p].modules[m].name.clone()),
                test: None,
            }),
            RowRef::Test(p, m, t) | RowRef::Call(p, m, t, _) => {
                let module = &self.projects[p].modules[m];
                Some(TestCaseSelector {
                    project: self.projects[p].name.clone(),
                    module: Some(module.name.clone()),
                    test: Some(module.tests[t].info.full_name()),
                })
            }
        }
    }

    /// Total number of test cases.
    pub fn len(&self) -> usize {
        self.projects
            .iter()
            .map(|proj| {
                proj.modules
                    .iter()
                    .map(|module| module.tests.len())
                    .sum::<usize>()
            })
            .sum()
    }

    /// Aggregated counts of all test cases.
    pub fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for project in &self.projects {
            let c = project.counts();
            counts.total += c.total;
            counts.passed += c.passed;
            counts.failed += c.failed;
            counts.running += c.running;
        }
        counts
    }
}

/// Returns `"<crate>::"` if every module starts with the same crate name, otherwise `""`.
fn common_module_prefix<'a>(mut modules: impl Iterator<Item = &'a str>) -> String {
    let Some(first) = modules.next() else {
        return String::new();
    };
    let Some((krate, _)) = first.split_once("::") else {
        return String::new();
    };
    let prefix = format!("{krate}::");
    let all_share_prefix = modules.all(|m| m.len() > prefix.len() && m.starts_with(&prefix));
    if all_share_prefix {
        prefix
    } else {
        String::new()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use http::StatusCode;
    use pretty_assertions::assert_eq;

    fn test_info(module: &str, name: &str, line: u32) -> TestInfo {
        TestInfo {
            module: module.into(),
            name: name.into(),
            serial_group: None,
            line,
            ordered: false,
        }
    }

    fn projects(names: &[&str]) -> Vec<Arc<tanu_core::ProjectConfig>> {
        names
            .iter()
            .map(|name| {
                Arc::new(tanu_core::ProjectConfig {
                    name: name.to_string(),
                    ..Default::default()
                })
            })
            .collect()
    }

    fn result(ok: bool, calls: usize) -> TestResult {
        let log = |n: usize| {
            Box::new(tanu_core::http::Log {
                request: tanu_core::http::LogRequest {
                    url: format!("https://example.com/{n}").parse().unwrap(),
                    method: http::Method::GET,
                    headers: http::header::HeaderMap::new(),
                    body: None,
                },
                response: tanu_core::http::LogResponse {
                    status: StatusCode::OK,
                    ..Default::default()
                },
                started_at: SystemTime::UNIX_EPOCH,
                ended_at: SystemTime::UNIX_EPOCH,
            })
        };
        TestResult {
            logs: (0..calls).map(log).collect(),
            test: Some(tanu_core::runner::Test {
                info: Arc::new(TestInfo::default()),
                worker_id: 0,
                result: if ok {
                    Ok(())
                } else {
                    Err(tanu_core::runner::Error::ErrorReturned("fail".into()))
                },
                started_at: SystemTime::UNIX_EPOCH,
                ended_at: SystemTime::UNIX_EPOCH,
                request_time: std::time::Duration::from_secs(0),
            }),
            ..Default::default()
        }
    }

    #[test]
    fn expand_init() {
        let mut state = TestListState::new(&[], &[]);
        state.expand();
        assert_eq!(None, state.list_state.selected());
    }

    #[test]
    fn sorted_modules_and_tests() {
        let state = TestListState::new(
            &projects(&["dev"]),
            &[
                test_info("krate::b", "second", 20),
                test_info("krate::a", "x", 1),
                test_info("krate::b", "first", 10),
            ],
        );
        let modules: Vec<_> = state.projects[0]
            .modules
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(vec!["krate::a", "krate::b"], modules);
        let tests: Vec<_> = state.projects[0].modules[1]
            .tests
            .iter()
            .map(|t| t.info.name.as_str())
            .collect();
        assert_eq!(vec!["first", "second"], tests);
        assert_eq!("krate::", state.module_prefix);
        assert_eq!("b", state.display_module_name("krate::b"));
    }

    #[test]
    fn module_prefix() {
        assert_eq!("", common_module_prefix(std::iter::empty()));
        assert_eq!("", common_module_prefix(["foo", "bar"].into_iter()));
        assert_eq!("a::", common_module_prefix(["a::x", "a::y::z"].into_iter()));
        assert_eq!("", common_module_prefix(["a::x", "b::y"].into_iter()));
        // A module equal to the crate root keeps the full names.
        assert_eq!("", common_module_prefix(["a::x", "a"].into_iter()));
    }

    #[test]
    fn expand() {
        // ✓ dev
        //   ▾ bar
        //       ○ test2
        //   ▾ foo
        //       ○ test1
        let mut state = TestListState::new(
            &projects(&["dev"]),
            &[test_info("foo", "test1", 0), test_info("bar", "test2", 0)],
        );
        assert_eq!(5, state.visible_rows().len());

        // The project row cannot be collapsed.
        state.expand();
        assert_eq!(5, state.visible_rows().len());

        //   ▸ bar
        state.list_state.select_next();
        state.expand();
        assert!(!state.projects[0].modules[0].expanded);
        assert_eq!(
            vec![
                RowRef::Project(0),
                RowRef::Module(0, 0),
                RowRef::Module(0, 1),
                RowRef::Test(0, 1, 0)
            ],
            state.visible_rows()
        );
    }

    #[test]
    fn project_tabs() {
        let mut state = TestListState::new(
            &projects(&["dev", "staging"]),
            &[test_info("foo", "test1", 0), test_info("bar", "test2", 0)],
        );
        // Only the current project is shown.
        assert_eq!(5, state.visible_rows().len());
        assert!(state
            .visible_rows()
            .iter()
            .all(|row| row.order_key().0 == 0));

        // Switching projects restores the cursor of each project.
        state.select_index(3);
        state.cycle_project(1);
        assert_eq!(1, state.current_project);
        assert_eq!(Some(RowRef::Project(1)), state.selected_row());
        state.cycle_project(1);
        assert_eq!(0, state.current_project);
        assert_eq!(Some(RowRef::Module(0, 1)), state.selected_row());
        state.cycle_project(-1);
        assert_eq!(1, state.current_project);

        // Jumping to a failure switches to its project.
        state.projects[0].modules[1].tests[0]
            .execution_state
            .executed(result(false, 0));
        state.jump_to_failure(true);
        assert_eq!(0, state.current_project);
        assert_eq!(Some(RowRef::Test(0, 1, 0)), state.selected_row());

        // Selecting a test (e.g. from the timeline) switches to its project.
        state.select_test("staging", "bar", "test2");
        assert_eq!(1, state.current_project);
        assert_eq!(Some(RowRef::Test(1, 0, 0)), state.selected_row());
    }

    #[test]
    fn expand_http_call() {
        let mut state = TestListState::new(
            &projects(&["dev"]),
            &[test_info("foo", "test1", 0), test_info("foo", "test2", 1)],
        );
        state.projects[0].modules[0].tests[0]
            .execution_state
            .executed(result(true, 2));

        // Select test1 and expand it; the cursor moves to its first call.
        state.select_index(2);
        state.expand();
        assert!(state.projects[0].modules[0].tests[0].expanded);
        assert_eq!(Some(RowRef::Call(0, 0, 0, 0)), state.selected_row());
        assert_eq!(
            vec![
                RowRef::Project(0),
                RowRef::Module(0, 0),
                RowRef::Test(0, 0, 0),
                RowRef::Call(0, 0, 0, 0),
                RowRef::Call(0, 0, 0, 1),
                RowRef::Test(0, 0, 1),
            ],
            state.visible_rows()
        );

        // A test without calls cannot be expanded.
        state.select_index(5);
        state.expand();
        assert!(!state.projects[0].modules[0].tests[1].expanded);

        // h on a call moves to its test, then collapses it.
        state.select_index(4);
        state.collapse_or_parent();
        assert_eq!(Some(RowRef::Test(0, 0, 0)), state.selected_row());
        state.collapse_or_parent();
        assert!(!state.projects[0].modules[0].tests[0].expanded);
    }

    #[test]
    fn select_empty_contents() {
        let mut state = TestListState::new(&[], &[]);
        state.list_state.select_next();
        state.clamp_selection();
        assert_eq!(None, state.list_state.selected());
        assert_eq!(None, state.select_test_case());
    }

    #[test]
    fn select_test_case() {
        let mut state = TestListState::new(&projects(&["dev"]), &[test_info("foo", "t", 0)]);
        state.select_index(2);
        assert_eq!(
            Some(TestCaseSelector {
                project: "dev".into(),
                module: Some("foo".into()),
                test: Some("foo::t".into()),
            }),
            state.select_test_case()
        );
        // Selection is clamped to the visible rows.
        state.select_index(100);
        assert_eq!(Some(2), state.list_state.selected());
    }

    #[test]
    fn filter_rows() {
        let mut state = TestListState::new(
            &projects(&["dev"]),
            &[
                test_info("foo", "alpha", 0),
                test_info("foo", "beta", 1),
                test_info("bar", "gamma", 0),
            ],
        );
        state.projects[0].modules[1].tests[0]
            .execution_state
            .executed(result(false, 0));
        state.projects[0].modules[1].tests[1]
            .execution_state
            .executed(result(true, 0));

        state.search = "ALP".into();
        assert_eq!(
            vec![
                RowRef::Project(0),
                RowRef::Module(0, 1),
                RowRef::Test(0, 1, 0)
            ],
            state.visible_rows()
        );

        // Searching by module name keeps all of its tests.
        state.search = "bar".into();
        assert_eq!(3, state.visible_rows().len());

        state.search.clear();
        state.status_filter = StatusFilter::Failed;
        assert_eq!(
            vec![
                RowRef::Project(0),
                RowRef::Module(0, 1),
                RowRef::Test(0, 1, 0)
            ],
            state.visible_rows()
        );
        state.status_filter = StatusFilter::NotRun;
        assert_eq!(
            vec![
                RowRef::Project(0),
                RowRef::Module(0, 0),
                RowRef::Test(0, 0, 0)
            ],
            state.visible_rows()
        );

        // Nothing matches.
        state.search = "zzz".into();
        assert!(state.visible_rows().is_empty());
        state.clamp_selection();
        assert_eq!(None, state.list_state.selected());
    }

    #[test]
    fn jump_to_failure() {
        let mut state = TestListState::new(
            &projects(&["dev"]),
            &[
                test_info("a", "t1", 0),
                test_info("a", "t2", 1),
                test_info("b", "t3", 0),
            ],
        );
        state.projects[0].modules[0].tests[0]
            .execution_state
            .executed(result(false, 0));
        state.projects[0].modules[1].tests[0]
            .execution_state
            .executed(result(false, 0));
        state.projects[0].modules[1].expanded = false;

        state.jump_to_failure(true);
        assert_eq!(Some(RowRef::Test(0, 0, 0)), state.selected_row());
        // Parent of the next failure is expanded automatically.
        state.jump_to_failure(true);
        assert_eq!(Some(RowRef::Test(0, 1, 0)), state.selected_row());
        // Wraps around.
        state.jump_to_failure(true);
        assert_eq!(Some(RowRef::Test(0, 0, 0)), state.selected_row());
        state.jump_to_failure(false);
        assert_eq!(Some(RowRef::Test(0, 1, 0)), state.selected_row());
    }

    #[test]
    fn counts() {
        let mut state = TestListState::new(
            &projects(&["dev"]),
            &[test_info("a", "t1", 0), test_info("a", "t2", 1)],
        );
        state.projects[0].modules[0].tests[0]
            .execution_state
            .executed(result(false, 0));
        state.projects[0].modules[0].tests[1]
            .execution_state
            .execute();
        assert_eq!(
            Counts {
                total: 2,
                passed: 0,
                failed: 1,
                running: 1
            },
            state.counts()
        );
    }

    #[test]
    fn symbol_test_result() {
        assert_eq!(
            super::symbol_test_result(&ExecutionState::Initialized),
            Span::styled("○ ", muted())
        );
        assert_eq!(
            super::symbol_test_result(&ExecutionState::Executed(result(true, 0))),
            Span::styled("✓ ", Style::default().fg(theme::OK).bold())
        );
        assert_eq!(
            super::symbol_test_result(&ExecutionState::Executed(result(false, 0))),
            Span::styled("✘ ", Style::default().fg(theme::FAIL).bold())
        );
    }

    #[test]
    fn fit_line_truncates_name() {
        let line = fit_line(
            vec![Span::raw("> ")],
            "a_very_long_test_name".into(),
            Style::new(),
            vec![Span::raw("12ms")],
            20,
        );
        assert_eq!(20, line.width());
        assert!(line.to_string().contains('…'));
        assert!(line.to_string().ends_with("12ms"));
    }
}
