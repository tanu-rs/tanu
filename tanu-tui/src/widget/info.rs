//! The widget is composed of 4 tabs:
//! - Call: shows the request details
//! - Headers: shows the request and response headers
//! - Payload: shows the request and response payload
//! - Error: shows the error message if the test failed
use ansi_to_tui::IntoText;
use chrono::{DateTime, Local};
use itertools::Itertools;
use once_cell::sync::Lazy;
use ratatui::{
    prelude::*,
    style::{Color, Style},
    widgets::{Block, Borders, Cell, Padding, Paragraph, Row, Table, TableState},
};
use std::time::SystemTime;
use style::palette::tailwind;
use syntect::{
    highlighting::{Theme, ThemeSet},
    parsing::SyntaxSet,
};
use tanu_core::get_tanu_config;
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
    pub payload_state: PayloadState,
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
            payload_state: PayloadState::default(),
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

#[cfg(feature = "grpc")]
pub enum SelectedCall<'a> {
    Http(&'a tanu_core::http::Log),
    Grpc(&'a tanu_core::grpc::Log),
}

#[cfg(not(feature = "grpc"))]
pub type SelectedCall<'a> = &'a tanu_core::http::Log;

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

/// Wrap the strings so that they can be displayed nicely in the table.
fn wrap_row(field: impl AsRef<str>, value: impl AsRef<str>, width: u16) -> Row<'static> {
    // Account for padding (2 spaces total: 1 before and 1 after)
    let opt = textwrap::Options::new((width.saturating_sub(2)) as usize)
        .break_words(true)
        .word_splitter(textwrap::WordSplitter::NoHyphenation);
    let wrapped_field = textwrap::fill(field.as_ref(), &opt);
    let wrapped_value = textwrap::fill(value.as_ref(), &opt);

    // Add padding to each line
    let padded_field = wrapped_field
        .lines()
        .map(|line| format!(" {} ", line))
        .collect::<Vec<_>>()
        .join("\n");
    let padded_value = wrapped_value
        .lines()
        .map(|line| format!(" {} ", line))
        .collect::<Vec<_>>()
        .join("\n");

    let height = padded_value.matches('\n').count() + 1;
    Row::new(vec![Cell::new(padded_field), Cell::new(padded_value)]).height(height as u16)
}

fn format_system_time(ts: SystemTime) -> String {
    if ts == SystemTime::UNIX_EPOCH {
        return "-".to_string();
    }

    let dt: DateTime<Local> = ts.into();
    dt.format("%Y-%m-%d %H:%M:%S%.3f %:z").to_string()
}

impl InfoWidget {
    pub fn new(test_results: Vec<TestResult>) -> InfoWidget {
        InfoWidget { test_results }
    }

    fn get_selected_test_result(
        &self,
        state: &InfoState,
    ) -> Option<(&TestResult, SelectedCall<'_>)> {
        let selector = state.selected_test.as_ref()?;
        let test_name = selector.test.as_ref()?;
        let test_result = self.test_results.iter().find(|test_result| {
            let Some(test) = test_result.test.as_ref() else {
                return false;
            };
            selector.project == test_result.project_name && test.info.full_name() == *test_name
        })?;

        #[cfg(feature = "grpc")]
        {
            // Try gRPC first if index is present
            if let Some(grpc_idx) = selector.grpc_call_index {
                if let Some(grpc_log) = test_result.grpc_logs.get(grpc_idx) {
                    return Some((test_result, SelectedCall::Grpc(grpc_log)));
                }
            }
            // Fall back to HTTP
            if let Some(http_idx) = selector.http_call_index {
                if let Some(http_log) = test_result.logs.get(http_idx) {
                    return Some((test_result, SelectedCall::Http(http_log)));
                }
            }
            None
        }

        #[cfg(not(feature = "grpc"))]
        {
            Some((
                test_result,
                test_result.logs.get(selector.http_call_index?)?,
            ))
        }
    }

    fn render_call(self, area: Rect, buf: &mut Buffer, state: &mut InfoState) {
        const FIELD_PERCENTAGE: u16 = 30;
        const VALUE_PERCENTAGE: u16 = 70;
        let value_width = area.width * VALUE_PERCENTAGE / 100 - 3;
        let Some((test_result, call)) = self.get_selected_test_result(state) else {
            return;
        };

        let colors = TableColors::new();
        let mut rows = vec![
            wrap_row("Project Name", &test_result.project_name, value_width),
            wrap_row("Test Name", &test_result.name, value_width),
        ];
        if let Some(test) = test_result.test.as_ref() {
            rows.push(wrap_row(
                "Test Started",
                format_system_time(test.started_at),
                value_width,
            ));
            rows.push(wrap_row(
                "Test Ended",
                format_system_time(test.ended_at),
                value_width,
            ));
            rows.push(wrap_row(
                "Test Duration",
                format!("{:?}", test.request_time),
                value_width,
            ));
        }

        #[cfg(feature = "grpc")]
        match call {
            SelectedCall::Http(http_call) => {
                rows.push(wrap_row("Request URL", &http_call.request.url, value_width));
                rows.push(wrap_row("Method", &http_call.request.method, value_width));
                rows.push(wrap_row(
                    "Status",
                    http_call.response.status.as_str(),
                    value_width,
                ));
                rows.push(wrap_row(
                    "Request Duration",
                    format!("{:?}", http_call.response.duration_req),
                    value_width,
                ));
            }
            SelectedCall::Grpc(grpc_call) => {
                rows.push(wrap_row("Method Path", &grpc_call.request.method, value_width));
                rows.push(wrap_row(
                    "Status Code",
                    format!("{:?} ({})", grpc_call.response.status_code, grpc_call.response.status_code as i32),
                    value_width,
                ));
                if !grpc_call.response.status_message.is_empty() {
                    rows.push(wrap_row(
                        "Status Message",
                        &grpc_call.response.status_message,
                        value_width,
                    ));
                }
                rows.push(wrap_row(
                    "Request Duration",
                    format!("{:?}", grpc_call.response.duration),
                    value_width,
                ));
            }
        }

        #[cfg(not(feature = "grpc"))]
        {
            rows.push(wrap_row("Request URL", &call.request.url, value_width));
            rows.push(wrap_row("Method", &call.request.method, value_width));
            rows.push(wrap_row(
                "Status",
                call.response.status.as_str(),
                value_width,
            ));
            rows.push(wrap_row(
                "Request Duration",
                format!("{:?}", call.response.duration_req),
                value_width,
            ));
        }

        // Apply alternating row colors
        let rows: Vec<Row> = rows
            .into_iter()
            .enumerate()
            .map(|(n, row)| {
                let color = match n % 2 {
                    0 => colors.normal_row_color,
                    _ => colors.alt_row_color,
                };
                row.bg(color)
            })
            .collect();

        let widths = [
            Constraint::Percentage(FIELD_PERCENTAGE),
            Constraint::Percentage(VALUE_PERCENTAGE),
        ];
        let table = Table::new(rows, widths)
            .style(Style::new().fg(colors.row_fg))
            .row_highlight_style(Style::default().fg(colors.selected_style_fg))
            .header(
                Row::new(vec![" Field ", " Value "]).style(
                    Style::default()
                        .fg(colors.header_fg)
                        .bg(colors.header_bg)
                        .bold(),
                ),
            )
            .block(
                Block::new()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Blue))
                    .title("Request")
                    .padding(Padding::uniform(1)),
            )
            .style(Style::default());

        ratatui::widgets::StatefulWidget::render(table, area, buf, &mut state.headers_res_state);
    }

    fn render_headers(self, area: Rect, buf: &mut Buffer, state: &mut InfoState) {
        let Some((test_result, call)) = self.get_selected_test_result(state) else {
            return;
        };

        trace!("rendering headers for {}", test_result.name);

        let [layout_req, layout_res] =
            Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
                .margin(0)
                .areas(area);

        let colors = TableColors::new();

        #[cfg(feature = "grpc")]
        let (req_rows, res_rows): (Vec<Row>, Vec<Row>) = match call {
            SelectedCall::Http(http_call) => {
                let req_rows: Vec<Row> = http_call
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
                    })
                    .collect();
                let res_rows: Vec<Row> = http_call
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
                    })
                    .collect();
                (req_rows, res_rows)
            }
            SelectedCall::Grpc(grpc_call) => {
                let req_rows: Vec<Row> = grpc_call
                    .request
                    .metadata
                    .iter()
                    .enumerate()
                    .flat_map(|(n, key_value)| {
                        use tonic::metadata::KeyAndValueRef;
                        let color = match n % 2 {
                            0 => colors.normal_row_color,
                            _ => colors.alt_row_color,
                        };
                        let (key, value_str) = match key_value {
                            KeyAndValueRef::Ascii(k, v) => {
                                (k.as_str(), v.to_str().unwrap_or("<invalid utf8>").to_string())
                            }
                            KeyAndValueRef::Binary(k, v) => {
                                (k.as_str(), format!("<binary {} bytes>", v.as_encoded_bytes().len()))
                            }
                        };
                        const PADDING: usize = 5;
                        let cell_width = (layout_res.width as f32 * 0.7) as usize - PADDING;
                        value_str
                            .chars()
                            .chunks(cell_width)
                            .into_iter()
                            .enumerate()
                            .map(|(i, chunked_text)| {
                                Row::new(vec![
                                    if i == 0 {
                                        format!(" {key} ")
                                    } else {
                                        String::new()
                                    },
                                    format!(" {} ", chunked_text.collect::<String>()),
                                ])
                                .bg(color)
                                .height(1)
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect();
                let res_rows: Vec<Row> = grpc_call
                    .response
                    .metadata
                    .iter()
                    .enumerate()
                    .flat_map(|(n, key_value)| {
                        use tonic::metadata::KeyAndValueRef;
                        let color = match n % 2 {
                            0 => colors.normal_row_color,
                            _ => colors.alt_row_color,
                        };
                        let (key, value_str) = match key_value {
                            KeyAndValueRef::Ascii(k, v) => {
                                (k.as_str(), v.to_str().unwrap_or("<invalid utf8>").to_string())
                            }
                            KeyAndValueRef::Binary(k, v) => {
                                (k.as_str(), format!("<binary {} bytes>", v.as_encoded_bytes().len()))
                            }
                        };
                        const PADDING: usize = 5;
                        let cell_width = (layout_res.width as f32 * 0.7) as usize - PADDING;
                        value_str
                            .chars()
                            .chunks(cell_width)
                            .into_iter()
                            .enumerate()
                            .map(|(i, chunked_text)| {
                                Row::new(vec![
                                    if i == 0 {
                                        format!(" {key} ")
                                    } else {
                                        String::new()
                                    },
                                    format!(" {} ", chunked_text.collect::<String>()),
                                ])
                                .bg(color)
                                .height(1)
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect();
                (req_rows, res_rows)
            }
        };

        #[cfg(not(feature = "grpc"))]
        let (req_rows, res_rows) = {
            let req_rows = call
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
            let res_rows = call
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
            (req_rows, res_rows)
        };

        {
            let rows = req_rows;

            let widths = [Constraint::Percentage(30), Constraint::Percentage(70)];
            let table = Table::new(rows, widths)
                .style(Style::new().fg(colors.row_fg))
                .row_highlight_style(Style::default().fg(colors.selected_style_fg))
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
                        .border_style(Style::default().fg(Color::Blue))
                        .title("Request")
                        .padding(Padding::uniform(1)),
                )
                .style(Style::default());

            ratatui::widgets::StatefulWidget::render(
                table,
                layout_req,
                buf,
                &mut state.headers_res_state,
            );
        }

        {
            let rows = res_rows;
            let widths = [Constraint::Percentage(30), Constraint::Percentage(70)];
            let table = Table::new(rows, widths)
                .style(Style::new().fg(colors.row_fg))
                .row_highlight_style(Style::default().fg(colors.selected_style_fg))
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
                        .border_style(Style::default().fg(Color::Blue))
                        .title("Response")
                        .padding(Padding::uniform(1)),
                )
                .style(Style::default());

            ratatui::widgets::StatefulWidget::render(
                table,
                layout_res,
                buf,
                &mut state.headers_req_state,
            );
        }
    }

    fn render_payload(self, area: Rect, buf: &mut Buffer, state: &mut InfoState) {
        let Some((_test_result, call)) = self.get_selected_test_result(state) else {
            return;
        };

        #[cfg(feature = "grpc")]
        let (theme_bg, highlighted_text) = match call {
            SelectedCall::Http(http_call) => {
                let body = &http_call.response.body;
                if body.is_empty() {
                    return;
                }

                let content_type = http_call
                    .response
                    .headers
                    .get("content-type")
                    .map(|v| v.to_str().unwrap_or_default());

                if content_type == Some("application/json") {
                    let json: serde_json::Value = match serde_json::from_str(body) {
                        Ok(json) => json,
                        Err(_) => return,
                    };
                    let json_str = serde_json::to_string_pretty(&json).unwrap();
                    let (theme_bg, highlighted_json) = highlight_source_code(json_str);
                    (Some(theme_bg), highlighted_json)
                } else {
                    (None, body.to_string())
                }
            }
            SelectedCall::Grpc(grpc_call) => {
                let req_msg = tanu_core::grpc::format_message(&grpc_call.request.message);
                let res_msg = tanu_core::grpc::format_message(&grpc_call.response.message);
                let combined = format!("Request Message:\n{}\n\nResponse Message:\n{}", req_msg, res_msg);
                (None, combined)
            }
        };

        #[cfg(not(feature = "grpc"))]
        let (theme_bg, highlighted_text) = {
            let body = &call.response.body;
            if body.is_empty() {
                return;
            }

            let content_type = call
                .response
                .headers
                .get("content-type")
                .map(|v| v.to_str().unwrap_or_default());

            if content_type == Some("application/json") {
                let json: serde_json::Value = match serde_json::from_str(body) {
                    Ok(json) => json,
                    Err(_) => return,
                };
                let json_str = serde_json::to_string_pretty(&json).unwrap();
                let (theme_bg, highlighted_json) = highlight_source_code(json_str);
                (Some(theme_bg), highlighted_json)
            } else {
                (None, body.to_string())
            }
        };

        // Split the highlighted JSON into lines
        let lines: Vec<&str> = highlighted_text.lines().collect();

        // Ensure scroll_offset is within bounds
        const BOARDER_AND_PADDING: usize = 4;
        let max_scroll_offset = lines
            .len()
            .saturating_sub(area.height as usize - BOARDER_AND_PADDING);
        state.payload_state.scroll_offset = state
            .payload_state
            .scroll_offset
            .min(max_scroll_offset as u16);

        // Calculate the visible range of lines
        let start_line = state.payload_state.scroll_offset as usize;
        let end_line = (start_line + area.height as usize).min(lines.len());
        let visible_lines = &lines[start_line..end_line];

        // Join the visible lines back into a single string
        let visible_text = visible_lines.join("\n");

        let paragraph = Paragraph::new(visible_text.into_text().unwrap())
            .block(
                Block::bordered()
                    .border_style(Style::default().fg(Color::Blue))
                    .padding(Padding::uniform(1)),
            )
            .scroll((0, 0)); // Reset scroll since we're slicing manually
        let paragraph = if let Some(theme_bg) = theme_bg {
            paragraph.bg(Color::Rgb(theme_bg.r, theme_bg.g, theme_bg.b))
        } else {
            paragraph
        };

        paragraph.render(area, buf);
    }

    fn render_error(self, area: Rect, buf: &mut Buffer, state: &mut InfoState) {
        let Some((test_result, _http_call)) = self.get_selected_test_result(state) else {
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
            .block(Block::bordered().border_style(Style::default().fg(Color::Blue)))
            .scroll((state.error_state.scroll_offset, 0));

        paragraph.render(area, buf);
    }
}

#[derive(Default)]
pub struct PayloadState {
    pub scroll_offset: u16,
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
            header_bg: Color::Blue,
            header_fg: Color::White,
            row_fg: Color::Black,
            selected_style_fg: Color::Blue,
            normal_row_color: tailwind::STONE.c900,
            alt_row_color: tailwind::STONE.c800,
        }
    }
}

static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: Lazy<ThemeSet> = Lazy::new(|| {
    let mut ts = ThemeSet::load_defaults();

    // Load all included themes
    for (name, content) in themes::get_all_themes() {
        let mut reader = std::io::Cursor::new(content);
        match syntect::highlighting::ThemeSet::load_from_reader(&mut reader) {
            Ok(theme) => {
                ts.themes.insert(name.to_string(), theme);
                debug!("Successfully loaded theme: {name}");
            }
            Err(e) => {
                warn!("Failed to load theme {name}: {e}");
            }
        }
    }

    ts
});

static THEME: Lazy<Theme> = Lazy::new(|| {
    const DEFAULT_THEME: &str = "Solarized (dark)";
    let color_theme = get_tanu_config().color_theme();
    let theme_name = color_theme
        .map(|s| format!("base16-{s}"))
        .unwrap_or(DEFAULT_THEME.into());

    match THEME_SET.themes.get(&theme_name) {
        Some(theme) => theme.clone(),
        None => {
            warn!("Theme '{theme_name}' not found, falling back to default");
            THEME_SET
                .themes
                .get(DEFAULT_THEME)
                .expect("Default theme '{DEFAULT_THEME}' not found")
                .clone()
        }
    }
});

// Include the generated themes module
include!(concat!(env!("OUT_DIR"), "/themes.rs"));

#[memoize::memoize]
fn highlight_source_code(source_code: String) -> (syntect::highlighting::Color, String) {
    use syntect::{
        easy::HighlightLines,
        highlighting::{Color, Style},
        util::as_24_bit_terminal_escaped,
    };

    let syntax = SYNTAX_SET
        .find_syntax_by_extension("json")
        .expect("JSON syntax not found");

    let theme_bg = THEME.settings.background.unwrap_or(Color::BLACK);
    let mut highlighter = HighlightLines::new(syntax, &THEME);

    let highlighted_with_line_numbers = source_code
        .lines()
        .enumerate()
        .map(|(line_number, line)| {
            let ranges: Vec<(Style, &str)> = highlighter.highlight_line(line, &SYNTAX_SET).unwrap();
            let highlighted_line = as_24_bit_terminal_escaped(&ranges[..], true);
            format!("{:>4} | {}", line_number + 1, highlighted_line) // Add line numbers
        })
        .join("\n");

    (theme_bg, highlighted_with_line_numbers)
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
