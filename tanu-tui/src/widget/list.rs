use itertools::Itertools;
use ratatui::{
    prelude::*,
    widgets::{block::BorderType, Block, HighlightSpacing, List, ListState},
};
use reqwest::StatusCode;
use std::collections::HashMap;
use tanu_core::{self, Filter, TestIgnoreFilter, TestInfo};

use crate::{TestResult, SELECTED_STYLE};

const EXPANDED: &str = "▸";

const UNEXPANDED: &str = "▾";

pub struct TestListWidget<'a> {
    list_widget: List<'a>,
}

impl<'a> TestListWidget<'a> {
    pub fn new(
        is_focused: bool,
        projects: &[Project],
        test_results: &[TestResult],
    ) -> TestListWidget<'a> {
        let grouped_by_project = test_results
            .iter()
            .into_group_map_by(|result| result.project_name.clone());
        let list_widget = List::new(
            projects
                .iter()
                .flat_map(|p| p.to_lines_recursively(&grouped_by_project)),
        )
        .block(
            Block::bordered()
                .title("Tests".bold())
                .border_type(if is_focused {
                    BorderType::Thick
                } else {
                    BorderType::Plain
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

#[derive(Debug)]
pub struct Project {
    /// Project name
    pub name: String,
    /// true: the list item is expanded, false: not expanded
    pub expanded: bool,
    /// List of modules under this project
    pub modules: Vec<Module>,
}

/// Helper function to create a symbol for test result.
fn symbol_test_result(maybe_ok: Option<bool>) -> Span<'static> {
    let (symbol, color) = match maybe_ok {
        Some(ok) => {
            if ok {
                ("✓", Some(Color::Green))
            } else {
                ("✘", Some(Color::Red))
            }
        }
        None => ("○", None),
    };
    let style = if let Some(color) = color {
        Style::default().fg(color).bold()
    } else {
        Style::default().bold()
    };
    Span::styled(symbol, style)
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

impl Project {
    fn to_line_summary(&self, ok: Option<bool>) -> Line<'static> {
        let icon = if self.expanded { EXPANDED } else { UNEXPANDED };
        let icon = Span::raw(format!("{icon} "));
        let symbol = symbol_test_result(ok);
        let name = Span::raw(format!(" {}", self.name));
        Line::from(vec![icon, symbol, name])
    }

    fn to_lines_recursively(
        &self,
        grouped_by_project: &HashMap<String, Vec<&TestResult>>,
    ) -> Vec<Line<'static>> {
        let test_results = grouped_by_project.get(&self.name);

        let test_length: usize = self.modules.iter().map(|module| module.tests.len()).sum();
        let test_results_length = test_results
            .map(|test_results| test_results.len())
            .unwrap_or(0);

        // If all of the tests are finished, check if all of the tests are successful to determine
        // test result symbol,
        let maybe_ok = if test_length == test_results_length {
            Some(test_results.into_iter().flatten().all(|res| {
                res.test
                    .as_ref()
                    .map(|test| test.result.is_ok())
                    .unwrap_or(false)
            }))
        } else {
            None
        };

        let mut lines = vec![self.to_line_summary(maybe_ok)];
        if self.expanded {
            let grouped_by_module = test_results
                .into_iter()
                .flatten()
                .copied()
                .into_group_map_by(|test| test.module_name.clone());
            lines.extend(
                self.modules
                    .iter()
                    .flat_map(|module| module.to_lines_recursively(&grouped_by_module)),
            );
        }
        lines
    }
}

#[derive(Debug)]
pub struct Module {
    /// Project name
    pub project_name: String,
    /// Module name
    pub name: String,
    /// true: the list item is expanded, false: not expanded
    pub expanded: bool,
    /// List of test cases under this module
    pub tests: Vec<Test>,
}

impl Module {
    fn to_line_summary(&self, maybe_ok: Option<bool>) -> Line<'static> {
        let icon = if self.expanded { EXPANDED } else { UNEXPANDED };
        let icon = Span::raw(format!("  {icon} "));
        let symbol = symbol_test_result(maybe_ok);
        let name = Span::raw(format!(" {}", self.name));
        Line::from(vec![icon, symbol, name])
    }

    fn to_lines_recursively(
        &self,
        grouped_by_module: &HashMap<String, Vec<&TestResult>>,
    ) -> Vec<Line<'static>> {
        let test_results = grouped_by_module.get(&self.name);

        let test_length: usize = self.tests.len();
        let test_results_length = test_results
            .map(|test_results| test_results.len())
            .unwrap_or(0);

        // If all of the tests are finished, check if all of the tests are successful to determine
        // test result symbol,
        let maybe_ok = if test_length == test_results_length {
            Some(test_results.into_iter().flatten().all(|res| {
                res.test
                    .as_ref()
                    .map(|test| test.result.is_ok())
                    .unwrap_or(false)
            }))
        } else {
            None
        };

        let mut lines = vec![self.to_line_summary(maybe_ok)];
        if self.expanded {
            let mut test_results_map: HashMap<String, &TestResult> = test_results
                .into_iter()
                .flatten()
                .map(|test| (test.unique_name(), *test))
                .collect();
            lines.extend(self.tests.iter().flat_map(|test| {
                let test_result =
                    test_results_map.remove(&test.info.unique_name(&self.project_name));
                test.to_lines_recursively(test_result)
            }));
        }
        lines
    }
}

#[derive(Debug, Clone)]
pub struct Test {
    info: TestInfo,
    /// true: the list item is expanded, false: not expanded
    pub expanded: bool,
}

impl Test {
    fn to_lines_recursively(&self, test_result: Option<&TestResult>) -> Vec<Line<'static>> {
        let more_than_one_http_call = test_result
            .map(|test_result| test_result.logs.len() > 1)
            .unwrap_or_default();

        let mut lines = {
            let ok = test_result
                .and_then(|test_result| test_result.test.as_ref().map(|test| test.result.is_ok()));
            let icon = if !more_than_one_http_call {
                " "
            } else if self.expanded {
                EXPANDED
            } else {
                UNEXPANDED
            };
            let icon = Span::raw(format!("    {icon} "));
            let symbol = symbol_test_result(ok);
            let name = Span::raw(format!(" {}", &self.info.name));
            vec![Line::from(vec![icon, symbol, name])]
        };

        if self.expanded {
            for http_call in test_result
                .into_iter()
                .flat_map(|test_result| test_result.logs.iter())
            {
                let indent = Span::raw("        ");
                let symbol = symbol_http_result(http_call.response.status);
                let name = Span::raw(format!(" {}", http_call.request.url));
                lines.push(Line::from(vec![indent, symbol, name]));
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
}

#[derive(Debug)]
pub struct TestListState {
    pub projects: Vec<Project>,
    pub list_state: ListState,
}

impl TestListState {
    pub fn new(projects: &[tanu_core::ProjectConfig], test_cases: &[TestInfo]) -> TestListState {
        let test_ignore_filter = TestIgnoreFilter::default();
        let grouped_by_module = test_cases
            .iter()
            .cloned()
            .map(|info| Test {
                info,
                expanded: false,
            })
            .into_group_map_by(|test| test.info.module.clone());

        let projects: Vec<_> = projects
            .iter()
            .map(|proj| Project {
                name: proj.name.clone(),
                expanded: true,
                modules: grouped_by_module
                    .clone()
                    .into_iter()
                    .map(|(module_name, tests)| Module {
                        project_name: proj.name.clone(),
                        name: module_name,
                        expanded: true,
                        tests: tests
                            .into_iter()
                            .filter(|test| test_ignore_filter.filter(proj, &test.info))
                            .collect(),
                    })
                    .filter(|module|
                        // Filter out module that has no test cases
                        !module.tests.is_empty())
                    .collect(),
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
                                for _http_call in test_results_map
                                    .get(&test.info.unique_name(&proj.name))
                                    .into_iter()
                                    .flat_map(|test_result| test_result.logs.iter())
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
                            n += 1;

                            if test.expanded {
                                for (index, _test_result) in test_result
                                    .into_iter()
                                    .flat_map(|test| test.logs.iter())
                                    .enumerate()
                                {
                                    if n == selected {
                                        return Some(TestCaseSelector {
                                            project: proj.name.clone(),
                                            module: Some(module.name.clone()),
                                            test: Some(test.info.full_name()),
                                            http_call_index: Some(index),
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
            tanu_core::ProjectConfig {
                name: "dev".into(),
                ..Default::default()
            },
            tanu_core::ProjectConfig {
                name: "staging".into(),
                ..Default::default()
            },
        ];
        let test_cases = vec![
            TestInfo {
                module: "foo".into(),
                name: "test1".into(),
            },
            TestInfo {
                module: "bar".into(),
                name: "test2".into(),
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
            tanu_core::ProjectConfig {
                name: "dev".into(),
                ..Default::default()
            },
            tanu_core::ProjectConfig {
                name: "staging".into(),
                ..Default::default()
            },
        ];
        let test_cases = vec![
            TestInfo {
                module: "foo".into(),
                name: "test1".into(),
            },
            TestInfo {
                module: "bar".into(),
                name: "test2".into(),
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
                        method: reqwest::Method::GET,
                        headers: reqwest::header::HeaderMap::new(),
                    },
                    response: tanu_core::http::LogResponse {
                        status: StatusCode::OK,
                        ..Default::default()
                    },
                }),
                Box::new(tanu_core::http::Log {
                    request: tanu_core::http::LogRequest {
                        url: "https://example.com/2".parse()?,
                        method: reqwest::Method::GET,
                        headers: reqwest::header::HeaderMap::new(),
                    },
                    response: tanu_core::http::LogResponse {
                        status: StatusCode::OK,
                        ..Default::default()
                    },
                }),
            ],
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
        assert_eq!(
            super::symbol_test_result(Some(true)),
            Span::styled("✓", Style::default().fg(Color::Green).bold())
        );
        assert_eq!(
            super::symbol_test_result(Some(false)),
            Span::styled("✘", Style::default().fg(Color::Red).bold())
        );
        assert_eq!(
            super::symbol_test_result(None),
            Span::styled("○", Style::default().bold())
        );
    }
}
