use itertools::Itertools;
use ratatui::{
    prelude::*,
    widgets::{block::BorderType, Block, HighlightSpacing, List, ListState},
};
use tanu_core::{self, Filter, TestIgnoreFilter, TestMetadata};

use crate::SELECTED_STYLE;

const EXPANDED: &str = "▸";

const UNEXPANDED: &str = "▾";

pub struct TestListWidget<'a> {
    list_widget: List<'a>,
}

impl<'a> TestListWidget<'a> {
    pub fn new(is_focused: bool, projects: &[Project]) -> TestListWidget<'a> {
        let list_widget = List::new(projects.iter().flat_map(|p| p.to_list()))
            .block(
                Block::bordered()
                    .title("Tests (t)".bold())
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

impl Project {
    fn list_name(&self) -> String {
        let icon = if self.expanded { EXPANDED } else { UNEXPANDED };
        format!("{icon} {}", self.name)
    }

    fn to_list(&self) -> Vec<String> {
        if self.expanded {
            let mut list = vec![self.list_name()];
            list.extend(self.modules.iter().flat_map(|module| module.to_list()));
            list
        } else {
            vec![self.list_name()]
        }
    }
}

#[derive(Debug)]
pub struct Module {
    /// Module name
    pub name: String,
    /// true: the list item is expanded, false: not expanded
    pub expanded: bool,
    /// List of test cases under this module
    pub tests: Vec<TestMetadata>,
}

impl Module {
    fn list_name(&self) -> String {
        let icon = if self.expanded { EXPANDED } else { UNEXPANDED };
        format!("  {icon} {}", self.name)
    }

    fn to_list(&self) -> Vec<String> {
        if self.expanded {
            let mut list = vec![self.list_name()];
            list.extend(self.tests.iter().map(|test| format!("      {}", test.name)));
            list
        } else {
            vec![self.list_name()]
        }
    }
}

/// Used by `TestListState::select_test_case`. When test case is found from the list,
/// this object is provided, allowing the caller to get project, module and test name.
#[derive(Debug, Default)]
pub struct TestCaseSelector {
    /// selected project.
    pub project: String,
    /// selected module.
    pub module: Option<String>,
    /// selected test case.
    pub test: Option<String>,
}

#[derive(Debug)]
pub struct TestListState {
    pub projects: Vec<Project>,
    pub list_state: ListState,
}

impl TestListState {
    pub fn new(
        projects: &[tanu_core::ProjectConfig],
        test_cases: &[TestMetadata],
    ) -> TestListState {
        let test_ignore_filter = TestIgnoreFilter::default();
        let grouped_by_module = test_cases
            .iter()
            .cloned()
            .into_group_map_by(|test| test.module.clone());

        let projects: Vec<_> = projects
            .iter()
            .map(|proj| Project {
                name: proj.name.clone(),
                expanded: true,
                modules: grouped_by_module
                    .clone()
                    .into_iter()
                    .map(|(module_name, tests)| Module {
                        name: module_name,
                        expanded: true,
                        tests: tests
                            .into_iter()
                            .filter(|metadata| test_ignore_filter.filter(proj, metadata))
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
    pub fn expand(&mut self) {
        let Some(selected) = self.list_state.selected() else {
            return;
        };

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
                        for _ in &module.tests {
                            n += 1;
                        }
                    }
                }
            }
        }
    }

    /// Find currently selected test case from the list widget.
    pub fn select_test_case(&self) -> Option<TestCaseSelector> {
        let selected = self.list_state.selected()?;

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
                            if n == selected {
                                return Some(TestCaseSelector {
                                    project: proj.name.clone(),
                                    module: Some(module.name.clone()),
                                    test: Some(test.full_name()),
                                });
                            }
                            n += 1;
                        }
                    }
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::collections::HashMap;

    #[test]
    fn expand_init() {
        let projects = vec![];
        let test_cases = vec![];
        let mut state = TestListState::new(&projects, &test_cases);
        state.expand();
    }

    #[test]
    fn expand() {
        let projects = vec![
            tanu_core::ProjectConfig {
                name: "dev".into(),
                data: HashMap::new(),
                test_ignore: vec![],
            },
            tanu_core::ProjectConfig {
                name: "staging".into(),
                data: HashMap::new(),
                test_ignore: vec![],
            },
        ];
        let test_cases = vec![
            TestMetadata {
                module: "foo".into(),
                name: "test1".into(),
            },
            TestMetadata {
                module: "bar".into(),
                name: "test1".into(),
            },
        ];
        let mut state = TestListState::new(projects.as_slice(), &test_cases);
        assert!(state.projects[0].expanded);
        assert!(state.projects[1].expanded);
        assert!(state.projects[0].modules[0].expanded);
        assert!(state.projects[0].modules[1].expanded);
        assert!(state.projects[1].modules[0].expanded);
        assert!(state.projects[1].modules[1].expanded);

        state.expand();

        assert!(!state.projects[0].expanded);
        assert!(state.projects[1].expanded);
        assert!(state.projects[0].modules[0].expanded);
        assert!(state.projects[0].modules[1].expanded);
        assert!(state.projects[1].modules[0].expanded);
        assert!(state.projects[1].modules[1].expanded);

        state.expand();

        assert!(state.projects[0].expanded);
        assert!(state.projects[1].expanded);
        assert!(state.projects[0].modules[0].expanded);
        assert!(state.projects[0].modules[1].expanded);
        assert!(state.projects[1].modules[0].expanded);
        assert!(state.projects[1].modules[1].expanded);
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
        state.expand();
        assert_eq!(Some(3), state.list_state.selected());
    }
}
