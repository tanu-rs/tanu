//! Widgets to show requests/response details.
use ansi_to_tui::IntoText;
use itertools::Itertools;
use ratatui::{
    prelude::*,
    style::Style,
    widgets::{Block, Borders, Padding, Paragraph, Row, Table, TableState},
};
use style::palette::tailwind;
use tracing::*;

use crate::{widget::list::TestCaseSelector, TestResult};

#[derive(
    Debug, Default, Clone, Copy, Eq, PartialEq, strum::FromRepr, strum::EnumString, strum::Display,
)]
pub enum Tab {
    Call,
    #[default]
    Headers,
    Payload,
    Error,
}

pub struct InfoState {
    pub focused: bool,
    pub selected_tab: Tab,
    pub selected_test: Option<TestCaseSelector>,
    pub headers_req_state: TableState,
    pub headers_res_state: TableState,
    pub error_state: ErrorState,
}

impl InfoState {
    pub fn new() -> InfoState {
        InfoState {
            focused: false,
            selected_tab: Tab::default(),
            selected_test: None,
            headers_req_state: TableState::new(),
            headers_res_state: TableState::new(),
            error_state: ErrorState::default(),
        }
    }

    pub fn next_tab(&mut self) {
        let current_index = self.selected_tab as usize;
        let tab_counts = Tab::Error as usize + 1;
        let next_index = (current_index + 1) % tab_counts;
        if let Some(next_tab) = Tab::from_repr(next_index) {
            self.selected_tab = next_tab;
        }
    }

    pub fn prev_tab(&mut self) {
        let current_index = self.selected_tab as usize;
        let tab_counts = Tab::Error as usize + 1;
        let next_index = (current_index.checked_sub(1).unwrap_or(Tab::Error as usize)) % tab_counts;
        if let Some(next_tab) = Tab::from_repr(next_index) {
            self.selected_tab = next_tab;
        }
    }
}

pub struct InfoWidget {
    test_results: Vec<TestResult>,
}

impl StatefulWidget for InfoWidget {
    type State = InfoState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        match state.selected_tab {
            Tab::Call => {
                self.render_call(area, buf, state);
            }
            Tab::Headers => {
                self.render_headers(area, buf, state);
            }
            Tab::Payload => {
                self.render_payload(area, buf, state);
            }
            Tab::Error => {
                self.render_error(area, buf, state);
            }
        }
    }
}

impl InfoWidget {
    pub fn new(test_results: Vec<TestResult>) -> InfoWidget {
        InfoWidget { test_results }
    }

    fn get_selected_test_result(&self, state: &InfoState) -> Option<&TestResult> {
        let selector = state.selected_test.as_ref()?;
        let test_name = selector.test.as_ref()?;
        self.test_results.iter().find(|test_result| {
            let Some(test) = test_result.test.as_ref() else {
                return false;
            };
            test.metadata.full_name() == *test_name
        })
    }

    fn render_call(self, area: Rect, buf: &mut Buffer, state: &mut InfoState) {
        let Some(test_result) = self.get_selected_test_result(state) else {
            return;
        };

        let colors = TableColors::new();
        let mut rows = vec![
            Row::new(vec![
                "Project Name".into(),
                test_result.project_name.clone(),
            ])
            .height(1),
            Row::new(vec!["Test Name".into(), test_result.name.clone()]).height(1),
        ];
        if let [log, ..] = test_result.logs.as_slice() {
            rows.push(Row::new(vec!["Request URL".into(), log.request.url.to_string()]).height(1));
            rows.push(Row::new(vec!["Method".into(), log.request.method.to_string()]).height(1));
            rows.push(Row::new(vec!["Status".into(), log.response.status.to_string()]).height(1));
        }

        let widths = [Constraint::Percentage(30), Constraint::Percentage(70)];
        let table = Table::new(rows, widths)
            .style(Style::new().fg(colors.row_fg))
            .highlight_style(Style::default().fg(colors.selected_style_fg))
            .block(
                Block::new()
                    .borders(Borders::ALL)
                    .title("Request")
                    .padding(Padding::uniform(1)),
            )
            .style(Style::default().fg(tailwind::WHITE));

        ratatui::widgets::StatefulWidget::render(table, area, buf, &mut state.headers_res_state);
    }

    fn render_headers(self, area: Rect, buf: &mut Buffer, state: &mut InfoState) {
        let Some(test_result) = self.get_selected_test_result(state) else {
            return;
        };

        trace!("rendering headers for {}", test_result.name);

        let [layout_req, layout_res] =
            Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
                .margin(0)
                .areas(area);

        let colors = TableColors::new();
        {
            if let [log, ..] = test_result.logs.as_slice() {
                let rows = log
                    .request
                    .headers
                    .iter()
                    .enumerate()
                    .flat_map(|(n, (k, v))| {
                        let color = match n % 2 {
                            0 => colors.normal_row_color,
                            _ => colors.alt_row_color,
                        };
                        let value = v
                            .to_str()
                            .inspect_err(|e| warn!("could not stringify header: {e:#}"))
                            .unwrap_or_default();
                        const PADDING: usize = 5;
                        let cell_width = (layout_res.width as f32 * 0.7) as usize - PADDING;
                        value
                            .chars()
                            .chunks(cell_width)
                            .into_iter()
                            .enumerate()
                            .map(|(n, chunked_text)| {
                                Row::new(vec![
                                    if n == 0 {
                                        format!(" {k} ")
                                    } else {
                                        String::new()
                                    },
                                    format!(" {} ", chunked_text.collect::<String>()),
                                ])
                                .bg(color)
                                .height(1)
                            })
                            .collect::<Vec<_>>()
                    });

                let widths = [Constraint::Percentage(30), Constraint::Percentage(70)];
                let table = Table::new(rows, widths)
                    .style(Style::new().fg(colors.row_fg))
                    .highlight_style(Style::default().fg(colors.selected_style_fg))
                    .header(
                        Row::new(vec![" Header ", " Value "]).style(
                            Style::default()
                                .fg(colors.header_fg)
                                .bg(colors.header_bg)
                                .bold(),
                        ),
                    )
                    .block(
                        Block::new()
                            .borders(Borders::ALL)
                            .title("Request")
                            .padding(Padding::uniform(1)),
                    )
                    .style(Style::default().fg(tailwind::WHITE));

                ratatui::widgets::StatefulWidget::render(
                    table,
                    layout_req,
                    buf,
                    &mut state.headers_res_state,
                );
            }
        }

        {
            if let [log, ..] = test_result.logs.as_slice() {
                let rows = log
                    .response
                    .headers
                    .iter()
                    .enumerate()
                    .flat_map(|(n, (k, v))| {
                        let color = match n % 2 {
                            0 => colors.normal_row_color,
                            _ => colors.alt_row_color,
                        };
                        let value = v
                            .to_str()
                            .inspect_err(|e| warn!("could not stringify header: {e:#}"))
                            .unwrap_or_default();
                        const PADDING: usize = 5;
                        let cell_width = (layout_res.width as f32 * 0.7) as usize - PADDING;
                        value
                            .chars()
                            .chunks(cell_width)
                            .into_iter()
                            .enumerate()
                            .map(|(n, chunked_text)| {
                                Row::new(vec![
                                    if n == 0 {
                                        format!(" {k} ")
                                    } else {
                                        String::new()
                                    },
                                    format!(" {} ", chunked_text.collect::<String>()),
                                ])
                                .bg(color)
                                .height(1)
                            })
                            .collect::<Vec<_>>()
                    });
                let widths = [Constraint::Percentage(30), Constraint::Percentage(70)];
                let table = Table::new(rows, widths)
                    .style(Style::new().fg(colors.row_fg))
                    .highlight_style(Style::default().fg(colors.selected_style_fg))
                    .header(
                        Row::new(vec![" Header ", " Value "]).style(
                            Style::default()
                                .fg(colors.header_fg)
                                .bg(colors.header_bg)
                                .bold(),
                        ),
                    )
                    .block(
                        Block::new()
                            .borders(Borders::ALL)
                            .title("Response")
                            .padding(Padding::uniform(1)),
                    )
                    .style(Style::default().fg(tailwind::WHITE));

                ratatui::widgets::StatefulWidget::render(
                    table,
                    layout_res,
                    buf,
                    &mut state.headers_req_state,
                );
            }
        }
    }

    fn render_payload(self, area: Rect, buf: &mut Buffer, state: &mut InfoState) {
        let Some(test_result) = self.get_selected_test_result(state) else {
            return;
        };

        let [log, ..] = &test_result.logs.as_slice() else {
            return;
        };

        let body = &log.response.body;
        if body.is_empty() {
            return;
        }

        let json: serde_json::Value = serde_json::from_str(body).unwrap();
        let json_str = serde_json::to_string_pretty(&json).unwrap();
        let highlighted_json = {
            use syntect::{
                easy::HighlightLines,
                highlighting::{Style, ThemeSet},
                parsing::SyntaxSet,
                util::as_24_bit_terminal_escaped,
            };

            let syntax_set = SyntaxSet::load_defaults_newlines();
            let theme_set = ThemeSet::load_defaults();
            let syntax = syntax_set
                .find_syntax_by_extension("json")
                .expect("JSON syntax not found");
            let theme = &theme_set.themes["base16-mocha.dark"];
            let mut highlighter = HighlightLines::new(syntax, theme);
            json_str
                .lines()
                .map(|line| {
                    let ranges: Vec<(Style, &str)> =
                        highlighter.highlight_line(line, &syntax_set).unwrap();
                    as_24_bit_terminal_escaped(&ranges[..], true)
                })
                .join("\n")
        };
        let paragraph = Paragraph::new(highlighted_json.into_text().unwrap())
            .block(Block::bordered())
            .style(Style::new());

        paragraph.render(area, buf);
    }

    fn render_error(self, area: Rect, buf: &mut Buffer, state: &mut InfoState) {
        let Some(test_result) = self.get_selected_test_result(state) else {
            return;
        };

        let Some(test) = &test_result.test else {
            return;
        };

        let Err(e) = &test.result else {
            return;
        };

        let text: Text = e.to_string().into_text().unwrap();
        let paragraph = Paragraph::new(text)
            .block(Block::bordered())
            .scroll((state.error_state.scroll_offset, 0));

        paragraph.render(area, buf);
    }
}

#[derive(Default)]
pub struct ErrorState {
    pub scroll_offset: u16,
}

struct TableColors {
    header_bg: Color,
    header_fg: Color,
    row_fg: Color,
    selected_style_fg: Color,
    normal_row_color: Color,
    alt_row_color: Color,
}

impl TableColors {
    const fn new() -> TableColors {
        TableColors {
            header_bg: tailwind::BLUE.c900,
            header_fg: tailwind::SLATE.c950,
            row_fg: tailwind::SLATE.c400,
            selected_style_fg: tailwind::BLUE.c400,
            normal_row_color: tailwind::STONE.c900,
            alt_row_color: tailwind::STONE.c800,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn next_tab() -> eyre::Result<()> {
        let mut state = InfoState::new();
        state.next_tab();
        assert_eq!(Tab::Payload, state.selected_tab);
        state.next_tab();
        assert_eq!(Tab::Error, state.selected_tab);
        state.next_tab();
        assert_eq!(Tab::Call, state.selected_tab);
        state.next_tab();
        assert_eq!(Tab::Headers, state.selected_tab);

        Ok(())
    }
}
