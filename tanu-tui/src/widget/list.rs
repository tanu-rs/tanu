use http::StatusCode;
use itertools::Itertools;
use ratatui::{
    prelude::*,
    widgets::{Block, BorderType, HighlightSpacing, List, ListState},
};
use std::{collections::HashMap, sync::Arc, time::SystemTime};
use tanu_core::{self, Filter, TestIgnoreFilter, TestInfo};
use throbber_widgets_tui::ThrobberState;

use crate::{TestResult, SELECTED_STYLE};

const EXPANDED: &str = "▸";

const UNEXPANDED: &str = "▾";

pub struct TestListWidget<'a> {
    list_widget: List<'a>,
}

impl<'a> TestListWidget<'a> {
    pub fn new(focused: bool, projects: &[ProjectState]) -> TestListWidget<'a> {
        let list_widget = List::new(projects.iter().flat_map(|p| p.to_lines_recursively()))
            .block(
                Block::bordered()
                    .title("Tests".bold())
                    .border_type(if focused {
                        BorderType::Thick
                    } else {
                        BorderType::Plain
                    })
                    .border_style(if focused {
                        Style::default().fg(Color::Blue).bold()
                    } else {
                        Style::default().fg(Color::Blue)
                    }),
            )
            .highlight_style(SELECTED_STYLE)
            .highlight_symbol(">")
            .highlight_spacing(HighlightSpacing::Always);

        TestListWidget { list_widget }
    }
}

impl StatefulWidget for TestListWidget<'_> {
    type State = TestListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        StatefulWidget::render(self.list_widget, area, buf, &mut state.list_state);
    }
}

/// Helper function to create a symbol for test result.
fn symbol_test_result(execution_state: &ExecutionState) -> Span<'static> {
    match execution_state {
        ExecutionState::Initialized => Span::styled("○ ", Style::default().bold()),
        ExecutionState::Executing(throbber_state) => {
            let throbber = throbber_widgets_tui::Throbber::default();
            throbber.to_symbol_span(throbber_state)
        }
        ExecutionState::Executed(test_result) => {
            let test_result = test_result
                .test
                .as_ref()
                .map(|test| test.result.is_ok())
                .unwrap_or_default();
            if test_result {
                Span::styled("✓ ", Style::default().fg(Color::Green).bold())
            } else {
                Span::styled("✘ ", Style::default().fg(Color::Red).bold())
            }
        }
    }
}

fn symbol_http_result(status: StatusCode) -> Span<'static> {
    Span::styled(
        "▪",
        Style::default()
            .fg(if status.is_success() {
                Color::Green
            } else {
                Color::Red
            })
            .bold(),
    )
}

#[cfg(feature = "grpc")]
fn symbol_grpc_result(status_code: tonic::Code) -> Span<'static> {
    Span::styled(
        "▪",
        Style::default()
            .fg(if status_code == tonic::Code::Ok {
                Color::Green
            } else {
                Color::Red
            })
            .bold(),
    )
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
                            .filter(|t| &t.info.name == test)
                        {
                            // Test is selected in the list.
                            Self::execute_test(test_state);
                        }
                    }
                }
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
        // Determine the module's execution state based on the execution state of its modules.
        let mut still_executing = false;
        let ok = module_state.tests.iter().all(|test| {
            let ExecutionState::Executed(ref test_result) = test.execution_state else {
                still_executing = true;
                return false;
            };
            test_result
                .test
                .as_ref()
                .map(|test| test.result.is_ok())
                .unwrap_or_default()
        });

        if !still_executing {
            module_state.execution_state.executed(TestResult {
                project_name: module_state.project_name.clone(),
                module_name: module_state.name.clone(),
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
            });
        }
    }

    /// Handler for when a project is updated.
    pub fn on_project_updated(project_state: &mut ProjectState) {
        // Determine project execution state based on module execution states.
        let mut still_executing = false;
        let ok = project_state.modules.iter().all(|module| {
            let ExecutionState::Executed(ref test_result) = module.execution_state else {
                still_executing = true;
                return false;
            };
            test_result
                .test
                .as_ref()
                .map(|test| test.result.is_ok())
                .unwrap_or_default()
        });

        if !still_executing {
            project_state.execution_state.executed(TestResult {
                project_name: project_state.name.clone(),
                test: Some(tanu_core::runner::Test {
                    info: Arc::new(TestInfo {
                        module: "".into(),
                        name: "".into(),
                        serial_group: None,
                        line: 0,
                        ordered: false,
                    }),
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
            });
        }
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
    fn to_line_summary(&self) -> Line<'static> {
        let icon = if self.expanded { EXPANDED } else { UNEXPANDED };
        let icon = Span::raw(format!("{icon} "));
        let symbol = symbol_test_result(&self.execution_state);
        let name = Span::raw(self.name.clone());
        Line::from(vec![icon, symbol, name])
    }

    fn to_lines_recursively(&self) -> Vec<Line<'static>> {
        let mut lines = vec![self.to_line_summary()];
        if self.expanded {
            lines.extend(
                self.modules
                    .iter()
                    .flat_map(|module| module.to_lines_recursively()),
            );
        }
        lines
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
    fn to_line_summary(&self) -> Line<'static> {
        let icon = if self.expanded { EXPANDED } else { UNEXPANDED };
        let icon = Span::raw(format!("  {icon} "));
        let symbol = symbol_test_result(&self.execution_state);
        let name = Span::raw(self.name.clone());
        Line::from(vec![icon, symbol, name])
    }

    fn to_lines_recursively(&self) -> Vec<Line<'static>> {
        let mut lines = vec![self.to_line_summary()];
        if self.expanded {
            lines.extend(
                self.tests
                    .iter()
                    .flat_map(|test| test.to_lines_recursively()),
            );
        }
        lines
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
    fn to_lines_recursively(&self) -> Vec<Line<'static>> {
        let more_than_one_call =
            if let ExecutionState::Executed(test_result) = &self.execution_state {
                let http_count = test_result.logs.len();
                #[cfg(feature = "grpc")]
                let grpc_count = test_result.grpc_logs.len();
                #[cfg(not(feature = "grpc"))]
                let grpc_count = 0;
                http_count + grpc_count > 1
            } else {
                false
            };

        let mut lines = {
            let icon = if !more_than_one_call {
                " "
            } else if self.expanded {
                EXPANDED
            } else {
                UNEXPANDED
            };
            let icon = Span::raw(format!("    {icon} "));
            let symbol = symbol_test_result(&self.execution_state);
            let name = Span::raw(self.info.name.clone());
            vec![Line::from(vec![icon, symbol, name])]
        };

        if self.expanded {
            if let ExecutionState::Executed(test_result) = &self.execution_state {
                for http_call in &test_result.logs {
                    let indent = Span::raw("        ");
                    let symbol = symbol_http_result(http_call.response.status);
                    let name = Span::raw(format!(" {}", http_call.request.url));
                    lines.push(Line::from(vec![indent, symbol, name]));
                }
                #[cfg(feature = "grpc")]
                for grpc_call in &test_result.grpc_logs {
                    let indent = Span::raw("        ");
                    let symbol = symbol_grpc_result(grpc_call.response.status_code);
                    let name = Span::raw(format!(" {}", grpc_call.request.method));
                    lines.push(Line::from(vec![indent, symbol, name]));
                }
            }
        }

        lines
    }
}

/// Represents the currently selected item in the `TestListWidget`.
///
/// This struct is used to track which test case, module, or project is currently
/// selected in the UI. It contains hierarchical information about the selection:
/// - A project is always selected
/// - A module may be selected if viewing inside a project
/// - A test case may be selected if viewing inside a module
#[derive(Debug, Default)]
pub struct TestCaseSelector {
    /// The name of the selected project.
    pub project: String,
    /// The name of the selected module, if any.
    pub module: Option<String>,
    /// The full name of the selected test case, if any.
    pub test: Option<String>,
    /// The index for HTTP call logs, if any.
    pub http_call_index: Option<usize>,
    /// The index for gRPC call logs, if any.
    #[cfg(feature = "grpc")]
    pub grpc_call_index: Option<usize>,
}

#[derive(Debug)]
pub struct TestListState {
    pub projects: Vec<ProjectState>,
    pub list_state: ListState,
}

impl TestListState {
    pub fn new(
        projects: &[Arc<tanu_core::ProjectConfig>],
        test_cases: &[TestInfo],
    ) -> TestListState {
        let test_ignore_filter = TestIgnoreFilter::default();
        let grouped_by_module = test_cases
            .iter()
            .cloned()
            .map(|info| TestState {
                info,
                expanded: false,
                execution_state: ExecutionState::default(),
            })
            .into_group_map_by(|test| test.info.module.clone());

        let projects: Vec<_> = projects
            .iter()
            .map(|proj| ProjectState {
                name: proj.name.clone(),
                expanded: true,
                modules: grouped_by_module
                    .clone()
                    .into_iter()
                    .map(|(module_name, tests)| ModuleState {
                        project_name: proj.name.clone(),
                        name: module_name,
                        expanded: true,
                        tests: tests
                            .into_iter()
                            .filter(|test| test_ignore_filter.filter(proj, &test.info))
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

        TestListState {
            projects,
            list_state: ListState::default().with_selected(Some(0)),
        }
    }

    /// Expands or collapses a selected item in the list.
    ///
    /// This function toggles the `expanded` state of a project or a module
    /// in the list. It first checks if an item is selected and then iterates
    /// through the `projects` and their `modules`, toggling the `expanded`
    /// state based on the selection index.
    ///
    /// The logic is structured in a way that it counts through both the projects
    /// and the modules, taking into account whether a project or a module is
    /// expanded to calculate the proper index.
    ///
    /// If a project or module is expanded, it reveals the items (modules or tests)
    /// underneath them in the list.
    pub fn expand(&mut self, test_results: &[TestResult]) {
        let Some(selected) = self.list_state.selected() else {
            return;
        };

        let test_results_map: HashMap<_, _> = test_results
            .iter()
            .map(|test| (test.unique_name(), test))
            .collect();

        let mut n = 0;
        for proj in &mut self.projects {
            if n == selected {
                proj.expanded = !proj.expanded;
                return;
            }
            n += 1;
            if proj.expanded {
                for module in &mut proj.modules {
                    if n == selected {
                        module.expanded = !module.expanded;
                        return;
                    }
                    n += 1;
                    if module.expanded {
                        for test in &mut module.tests {
                            if n == selected {
                                test.expanded = !test.expanded;
                                return;
                            }
                            n += 1;
                            if test.expanded {
                                let test_result = test_results_map.get(&test.info.unique_name(&proj.name));
                                for _http_call in test_result
                                    .into_iter()
                                    .flat_map(|test_result| test_result.logs.iter())
                                {
                                    n += 1;
                                }
                                #[cfg(feature = "grpc")]
                                for _grpc_call in test_result
                                    .into_iter()
                                    .flat_map(|test_result| test_result.grpc_logs.iter())
                                {
                                    n += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Find currently selected test case from the list widget.
    pub fn select_test_case(&self, test_results: &[TestResult]) -> Option<TestCaseSelector> {
        let selected = self.list_state.selected()?;
        let test_results_map: HashMap<_, _> = test_results
            .iter()
            .map(|test| (test.unique_name(), test))
            .collect();

        let mut n = 0;
        for proj in &self.projects {
            if n == selected {
                return Some(TestCaseSelector {
                    project: proj.name.clone(),
                    ..Default::default()
                });
            }

            n += 1;
            if proj.expanded {
                for module in &proj.modules {
                    if n == selected {
                        return Some(TestCaseSelector {
                            project: proj.name.clone(),
                            module: Some(module.name.clone()),
                            ..Default::default()
                        });
                    }
                    n += 1;
                    if module.expanded {
                        for test in &module.tests {
                            let test_result =
                                test_results_map.get(&test.info.unique_name(&proj.name));

                            if n == selected {
                                #[cfg(feature = "grpc")]
                                {
                                    let test_result_ref = test_result.into_iter().next();
                                    let http_count = test_result_ref.map(|tr| tr.logs.len()).unwrap_or(0);
                                    let grpc_count = test_result_ref.map(|tr| tr.grpc_logs.len()).unwrap_or(0);
                                    let total_count = http_count + grpc_count;

                                    return Some(TestCaseSelector {
                                        project: proj.name.clone(),
                                        module: Some(module.name.clone()),
                                        test: Some(test.info.full_name()),
                                        http_call_index: if total_count == 1 && http_count == 1 {
                                            Some(0)
                                        } else {
                                            None
                                        },
                                        grpc_call_index: if total_count == 1 && grpc_count == 1 {
                                            Some(0)
                                        } else {
                                            None
                                        },
                                    });
                                }
                                #[cfg(not(feature = "grpc"))]
                                {
                                    return Some(TestCaseSelector {
                                        project: proj.name.clone(),
                                        module: Some(module.name.clone()),
                                        test: Some(test.info.full_name()),
                                        http_call_index: test_result.into_iter().next().and_then(
                                            |test_result| {
                                                if test_result.logs.len() == 1 {
                                                    Some(0)
                                                } else {
                                                    None
                                                }
                                            },
                                        ),
                                    });
                                }
                            }
                            n += 1;

                            if test.expanded {
                                for (index, _test_result) in test_result
                                    .into_iter()
                                    .flat_map(|test| test.logs.iter())
                                    .enumerate()
                                {
                                    if n == selected {
                                        #[cfg(feature = "grpc")]
                                        {
                                            return Some(TestCaseSelector {
                                                project: proj.name.clone(),
                                                module: Some(module.name.clone()),
                                                test: Some(test.info.full_name()),
                                                http_call_index: Some(index),
                                                grpc_call_index: None,
                                            });
                                        }
                                        #[cfg(not(feature = "grpc"))]
                                        {
                                            return Some(TestCaseSelector {
                                                project: proj.name.clone(),
                                                module: Some(module.name.clone()),
                                                test: Some(test.info.full_name()),
                                                http_call_index: Some(index),
                                            });
                                        }
                                    }
                                    n += 1;
                                }
                                #[cfg(feature = "grpc")]
                                for (index, _grpc_call) in test_result
                                    .into_iter()
                                    .flat_map(|test| test.grpc_logs.iter())
                                    .enumerate()
                                {
                                    if n == selected {
                                        return Some(TestCaseSelector {
                                            project: proj.name.clone(),
                                            module: Some(module.name.clone()),
                                            test: Some(test.info.full_name()),
                                            http_call_index: None,
                                            grpc_call_index: Some(index),
                                        });
                                    }
                                    n += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        None
    }

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
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn expand_init() {
        let projects = vec![];
        let test_cases = vec![];
        let mut state = TestListState::new(&projects, &test_cases);
        state.expand(&[]);
    }

    #[test]
    fn expand() {
        let projects = vec![
            Arc::new(tanu_core::ProjectConfig {
                name: "dev".into(),
                ..Default::default()
            }),
            Arc::new(tanu_core::ProjectConfig {
                name: "staging".into(),
                ..Default::default()
            }),
        ];
        let test_cases = vec![
            TestInfo {
                module: "foo".into(),
                name: "test1".into(),
                serial_group: None,
                line: 0,
                ordered: false,
            },
            TestInfo {
                module: "bar".into(),
                name: "test2".into(),
                serial_group: None,
                line: 0,
                ordered: false,
            },
        ];

        // ▾ dev
        //   ▾ foo
        //     ○ test1
        //   ▾ bar
        //     ○ test2
        // ▾ staging
        //   ▾ foo
        //     ○ test1
        //   ▾ bar
        //     ○ test2
        let mut state = TestListState::new(projects.as_slice(), &test_cases);
        assert!(state.projects[0].expanded);
        assert!(state.projects[1].expanded);
        assert!(state.projects[0].modules[0].expanded);
        assert!(state.projects[0].modules[1].expanded);
        assert!(state.projects[1].modules[0].expanded);
        assert!(state.projects[1].modules[1].expanded);

        // ▸ dev
        // ▾ staging
        //   ▾ foo
        //     ○ test1
        //   ▾ bar
        //     ○ test2
        state.expand(&[]);
        assert!(!state.projects[0].expanded);
        assert!(state.projects[1].expanded);
        assert!(state.projects[0].modules[0].expanded);
        assert!(state.projects[0].modules[1].expanded);
        assert!(state.projects[1].modules[0].expanded);
        assert!(state.projects[1].modules[1].expanded);

        // ▸ dev
        // ▸ staging
        state.list_state.select_next();
        state.expand(&[]);
        assert!(!state.projects[0].expanded);
        assert!(!state.projects[1].expanded);

        // ▸ dev
        // ▾ staging
        //   ▸ foo
        //   ▾ bar
        //     ○ test2
        state.expand(&[]);
        state.list_state.select_next();
        state.expand(&[]);
        assert!(!state.projects[0].expanded);
        assert!(state.projects[1].expanded);
        assert!(state.projects[0].modules[0].expanded);
        assert!(state.projects[0].modules[1].expanded);
        assert!(!state.projects[1].modules[0].expanded);
        assert!(state.projects[1].modules[1].expanded);
    }

    #[test]
    fn expand_http_call() -> eyre::Result<()> {
        let projects = vec![
            Arc::new(tanu_core::ProjectConfig {
                name: "dev".into(),
                ..Default::default()
            }),
            Arc::new(tanu_core::ProjectConfig {
                name: "staging".into(),
                ..Default::default()
            }),
        ];
        let test_cases = vec![
            TestInfo {
                module: "foo".into(),
                name: "test1".into(),
                serial_group: None,
                line: 0,
                ordered: false,
            },
            TestInfo {
                module: "bar".into(),
                name: "test2".into(),
                serial_group: None,
                line: 0,
                ordered: false,
            },
        ];

        let test_results = vec![TestResult {
            project_name: "dev".into(),
            module_name: "foo".into(),
            name: "test1".into(),
            logs: vec![
                Box::new(tanu_core::http::Log {
                    request: tanu_core::http::LogRequest {
                        url: "https://example.com/1".parse()?,
                        method: http::Method::GET,
                        headers: http::header::HeaderMap::new(),
                    },
                    response: tanu_core::http::LogResponse {
                        status: StatusCode::OK,
                        ..Default::default()
                    },
                    started_at: std::time::SystemTime::UNIX_EPOCH,
                    ended_at: std::time::SystemTime::UNIX_EPOCH,
                }),
                Box::new(tanu_core::http::Log {
                    request: tanu_core::http::LogRequest {
                        url: "https://example.com/2".parse()?,
                        method: http::Method::GET,
                        headers: http::header::HeaderMap::new(),
                    },
                    response: tanu_core::http::LogResponse {
                        status: StatusCode::OK,
                        ..Default::default()
                    },
                    started_at: std::time::SystemTime::UNIX_EPOCH,
                    ended_at: std::time::SystemTime::UNIX_EPOCH,
                }),
            ],
            #[cfg(feature = "grpc")]
            grpc_logs: vec![],
            test: None,
        }];

        // ▾ dev
        //   ▾ foo
        //     ▸ ○ test1
        //   ▾ bar
        //     ▸ ○ test2
        // ▾ staging
        //   ▾ foo
        //     ▸ ○ test1
        //   ▾ bar
        //     ▸ ○ test2
        let mut state = TestListState::new(projects.as_slice(), &test_cases);
        assert!(state.projects[0].expanded);
        assert!(state.projects[1].expanded);
        assert!(state.projects[0].modules[0].expanded);
        assert!(state.projects[0].modules[1].expanded);
        assert!(state.projects[1].modules[0].expanded);
        assert!(state.projects[1].modules[1].expanded);
        assert!(!state.projects[0].modules[0].tests[0].expanded);
        assert!(!state.projects[0].modules[1].tests[0].expanded);
        assert!(!state.projects[1].modules[0].tests[0].expanded);
        assert!(!state.projects[1].modules[1].tests[0].expanded);

        state.list_state.select_next();
        state.list_state.select_next();
        state.expand(&test_results);
        // ▾ dev
        //   ▾ foo
        //     ▾ ○ test1
        //         ▪ https://example.com/1
        //         ▪ https://example.com/2
        //   ▾ bar
        //     ▸ ○ test2
        // ▾ staging
        //   ▾ foo
        //     ▸ ○ test1
        //   ▾ bar
        //     ▸ ○ test2
        assert!(state.projects[0].expanded);
        assert!(state.projects[1].expanded);
        assert!(state.projects[0].modules[0].expanded);
        assert!(state.projects[0].modules[1].expanded);
        assert!(state.projects[1].modules[0].expanded);
        assert!(state.projects[1].modules[1].expanded);
        assert!(state.projects[0].modules[0].tests[0].expanded);
        assert!(!state.projects[0].modules[1].tests[0].expanded);
        assert!(!state.projects[1].modules[0].tests[0].expanded);
        assert!(!state.projects[1].modules[1].tests[0].expanded);

        Ok(())
    }

    #[test]
    fn select_empty_contents() {
        let projects = vec![];
        let test_cases = vec![];
        let mut state = TestListState::new(&projects, &test_cases);
        state.list_state.select_next();
        state.list_state.select_next();
        assert_eq!(Some(2), state.list_state.selected());
        state.list_state.select_next();
        assert_eq!(Some(3), state.list_state.selected());
        state.expand(&[]);
        assert_eq!(Some(3), state.list_state.selected());
    }

    #[test]
    fn symbol_test_result() {
        // Test for ExecutionState::Initialized
        assert_eq!(
            super::symbol_test_result(&ExecutionState::Initialized),
            Span::styled("○ ", Style::default().bold())
        );

        // Test for ExecutionState::Executed with successful result
        let successful_result = TestResult {
            test: Some(tanu_core::runner::Test {
                info: Arc::new(TestInfo {
                    module: "".into(),
                    name: "".into(),
                    serial_group: None,
                    line: 0,
                    ordered: false,
                }),
                worker_id: 0,
                result: Ok(()),
                started_at: SystemTime::UNIX_EPOCH,
                ended_at: SystemTime::UNIX_EPOCH,
                request_time: std::time::Duration::from_secs(0),
            }),
            ..Default::default()
        };
        assert_eq!(
            super::symbol_test_result(&ExecutionState::Executed(successful_result)),
            Span::styled("✓ ", Style::default().fg(Color::Green).bold())
        );

        // Test for ExecutionState::Executed with failed result
        let failed_result = TestResult {
            test: Some(tanu_core::runner::Test {
                info: Arc::new(TestInfo {
                    module: "".into(),
                    name: "".into(),
                    serial_group: None,
                    line: 0,
                    ordered: false,
                }),
                worker_id: 0,
                result: Err(tanu_core::runner::Error::ErrorReturned("fail".into())),
                started_at: SystemTime::UNIX_EPOCH,
                ended_at: SystemTime::UNIX_EPOCH,
                request_time: std::time::Duration::from_secs(0),
            }),
            ..Default::default()
        };
        assert_eq!(
            super::symbol_test_result(&ExecutionState::Executed(failed_result)),
            Span::styled("✘ ", Style::default().fg(Color::Red).bold())
        );
    }
}
