//! The details pane.
//!
//! When a single call (HTTP/gRPC) is selected, the pane is composed of 4 tabs:
//! - Call: shows the test and request details
//! - Headers: shows the request and response headers
//! - Payload: shows the request and response payload
//! - Error: shows the checks and the error message if the test failed
//!
//! When a project, a module or a test with zero or multiple calls is selected,
//! an overview of the selection is shown instead.
use ansi_to_tui::IntoText;
use chrono::{DateTime, Local};
use itertools::Itertools;
use once_cell::sync::Lazy;
use ratatui::{
    prelude::*,
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Row, Table},
};
use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};
use syntect::{
    highlighting::{Theme, ThemeSet},
    parsing::SyntaxSet,
};
use tanu_core::get_tanu_config;
use tracing::{debug, warn};

use crate::{
    widget::{
        latency,
        list::{Counts, ExecutionState, RowRef, TestListState, TestState},
        tabbed_block::CustomTabs,
        theme::{self, fmt_duration, muted},
    },
    Call, TestResult,
};

#[derive(
    Debug, Default, Clone, Copy, Eq, PartialEq, strum::FromRepr, strum::EnumString, strum::Display,
)]
pub enum Tab {
    #[default]
    Call,
    Headers,
    Payload,
    Error,
}

/// Scrollable views of the details pane.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum View {
    Overview,
    Tab(Tab),
}

impl View {
    fn index(self) -> usize {
        match self {
            View::Overview => 0,
            View::Tab(tab) => tab as usize + 1,
        }
    }
}

/// Scroll position of a view.
#[derive(Debug, Default, Clone, Copy)]
pub struct ScrollState {
    pub offset: u16,
    /// The maximum offset computed during the last render.
    pub max: u16,
}

pub struct InfoState {
    pub focused: bool,
    pub maximized: bool,
    pub selected_tab: Tab,
    /// The row selected in the test list.
    pub selected: Option<RowRef>,
    /// true if the tabs were shown in the last render.
    showing_tabs: bool,
    /// Horizontal ranges `(tab, x_start, x_end, y)` of the tabs, for mouse clicks.
    tab_hits: Vec<(Tab, u16, u16, u16)>,
    scrolls: [ScrollState; 5],
}

impl InfoState {
    pub fn new() -> InfoState {
        InfoState {
            focused: false,
            maximized: false,
            selected_tab: Tab::default(),
            selected: None,
            showing_tabs: false,
            tab_hits: vec![],
            scrolls: Default::default(),
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

    /// Changes the selection, resetting scroll positions if it differs.
    pub fn select(&mut self, selected: Option<RowRef>) {
        if self.selected != selected {
            self.selected = selected;
            self.scrolls = Default::default();
        }
    }

    /// Returns the tab at the given screen position, if any.
    pub fn tab_at(&self, x: u16, y: u16) -> Option<Tab> {
        if !self.showing_tabs {
            return None;
        }
        self.tab_hits
            .iter()
            .find(|(_, start, end, row)| *row == y && (*start..*end).contains(&x))
            .map(|(tab, ..)| *tab)
    }

    fn view(&self) -> View {
        if self.showing_tabs {
            View::Tab(self.selected_tab)
        } else {
            View::Overview
        }
    }

    fn scroll_mut(&mut self) -> &mut ScrollState {
        let index = self.view().index();
        &mut self.scrolls[index]
    }

    pub fn scroll_down(&mut self, n: u16) {
        let scroll = self.scroll_mut();
        scroll.offset = scroll.offset.saturating_add(n).min(scroll.max);
    }

    pub fn scroll_up(&mut self, n: u16) {
        let scroll = self.scroll_mut();
        scroll.offset = scroll.offset.saturating_sub(n);
    }

    pub fn scroll_home(&mut self) {
        self.scroll_mut().offset = 0;
    }

    pub fn scroll_end(&mut self) {
        let scroll = self.scroll_mut();
        scroll.offset = scroll.max;
    }
}

pub struct InfoWidget<'a> {
    list: &'a TestListState,
}

impl StatefulWidget for InfoWidget<'_> {
    type State = InfoState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let selected = state.selected;
        let single_call = self.single_call(selected);

        let mut title = vec![Span::raw("Details")];
        if let Some(name) = self.selection_name(selected) {
            title.push(Span::styled(format!(" · {name}"), muted()));
        }
        if state.maximized {
            title.push(Span::styled(" [maximized]", muted()));
        }
        let block = theme::block(title, state.focused);
        let inner = block.inner(area);
        block.render(area, buf);
        let inner = inner.inner(Margin::new(1, 0));

        state.tab_hits.clear();
        let Some((test_result, call)) = single_call else {
            state.showing_tabs = false;
            let lines = self.overview_lines(selected, inner.width as usize);
            render_scrolled(lines, inner, buf, state.scroll_mut());
            return;
        };

        state.showing_tabs = true;
        let [layout_tabs, _, layout_content] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .areas(inner);

        let failed = !test_result.is_ok();
        let error_badge = if failed {
            Span::styled(" ●", Style::new().fg(theme::fail()))
        } else if !test_result.checks.is_empty() {
            Span::styled(format!(" {}", test_result.checks.len()), muted())
        } else {
            Span::raw("")
        };
        let (req_headers, res_headers) = call_headers(call);
        let tabs = CustomTabs::new(vec![
            (Tab::Call.to_string(), Span::raw("")),
            (
                Tab::Headers.to_string(),
                Span::styled(
                    format!(" {}/{}", req_headers.len(), res_headers.len()),
                    muted(),
                ),
            ),
            (Tab::Payload.to_string(), Span::raw("")),
            (Tab::Error.to_string(), error_badge),
        ])
        .select(state.selected_tab as usize)
        .alert(if failed {
            Some(Tab::Error as usize)
        } else {
            None
        });
        for (index, start, end) in tabs.hit_areas(layout_tabs) {
            if let Some(tab) = Tab::from_repr(index) {
                state.tab_hits.push((tab, start, end, layout_tabs.y));
            }
        }
        tabs.render(layout_tabs, buf);

        let width = layout_content.width as usize;
        match state.selected_tab {
            Tab::Call => {
                let lines = call_lines(test_result, call, width);
                render_scrolled(lines, layout_content, buf, state.scroll_mut());
            }
            Tab::Headers => {
                render_headers(
                    req_headers,
                    res_headers,
                    layout_content,
                    buf,
                    state.scroll_mut(),
                );
            }
            Tab::Payload => render_payload(call, layout_content, buf, state.scroll_mut()),
            Tab::Error => {
                let lines = error_lines(test_result, width);
                render_scrolled(lines, layout_content, buf, state.scroll_mut());
            }
        }
    }
}

const KEY_WIDTH: usize = 16;

/// Key-value line(s); the value is wrapped and aligned to the value column.
fn kv(key: &str, value: impl AsRef<str>, style: Style, width: usize) -> Vec<Line<'static>> {
    let wrap_width = width.saturating_sub(KEY_WIDTH).max(10);
    let opts = textwrap::Options::new(wrap_width)
        .break_words(true)
        .word_splitter(textwrap::WordSplitter::NoHyphenation);
    textwrap::wrap(value.as_ref(), opts)
        .into_iter()
        .enumerate()
        .map(|(n, part)| {
            let key = if n == 0 {
                format!("{key:<KEY_WIDTH$}")
            } else {
                " ".repeat(KEY_WIDTH)
            };
            Line::from(vec![
                Span::styled(key, muted()),
                Span::styled(part.into_owned(), style),
            ])
        })
        .collect()
}

/// Key-value line with pre-styled value spans.
fn kv_spans(key: &str, value: Vec<Span<'static>>) -> Line<'static> {
    let mut spans = vec![Span::styled(format!("{key:<KEY_WIDTH$}"), muted())];
    spans.extend(value);
    Line::from(spans)
}

/// Section heading.
fn section(title: impl Into<String>) -> Line<'static> {
    Line::styled(title.into(), Style::new().fg(theme::accent()).bold())
}

/// Hard-wraps a styled line to the given width.
fn wrap_line(line: Line<'static>, width: usize) -> Vec<Line<'static>> {
    if width == 0 || line.width() <= width {
        return vec![line];
    }
    let line_style = line.style;
    let mut lines = vec![];
    let mut current: Vec<Span<'static>> = vec![];
    let mut current_width = 0;
    for span in line.spans {
        let style = span.style;
        let mut chunk = String::new();
        for ch in span.content.chars() {
            if current_width == width {
                if !chunk.is_empty() {
                    current.push(Span::styled(std::mem::take(&mut chunk), style));
                }
                lines.push(Line::from(std::mem::take(&mut current)).style(line_style));
                current_width = 0;
            }
            chunk.push(ch);
            current_width += 1;
        }
        if !chunk.is_empty() {
            current.push(Span::styled(chunk, style));
        }
    }
    if !current.is_empty() {
        lines.push(Line::from(current).style(line_style));
    }
    lines
}

/// Renders lines with vertical scrolling, updating the scroll bounds.
fn render_scrolled(
    lines: Vec<Line<'static>>,
    area: Rect,
    buf: &mut Buffer,
    scroll: &mut ScrollState,
) {
    scroll.max = lines.len().saturating_sub(area.height as usize) as u16;
    scroll.offset = scroll.offset.min(scroll.max);
    let has_more_below = scroll.offset < scroll.max;
    Paragraph::new(lines)
        .scroll((scroll.offset, 0))
        .render(area, buf);
    if has_more_below && area.height > 0 {
        buf.set_string(
            area.right().saturating_sub(3),
            area.bottom() - 1,
            " ↓ ",
            Style::new().fg(theme::accent()).bg(theme::selected_bg()),
        );
    }
}

fn format_system_time(ts: SystemTime) -> String {
    if ts == SystemTime::UNIX_EPOCH {
        return "-".to_string();
    }

    let dt: DateTime<Local> = ts.into();
    dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

fn status_spans(result: &TestResult) -> Vec<Span<'static>> {
    if result.is_ok() {
        vec![Span::styled(
            "✓ passed",
            Style::new().fg(theme::ok()).bold(),
        )]
    } else {
        vec![Span::styled(
            "✘ failed",
            Style::new().fg(theme::fail()).bold(),
        )]
    }
}

fn counts_spans(counts: Counts) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled(
        format!("✓ {} passed", counts.passed),
        Style::new().fg(theme::ok()),
    )];
    if counts.failed > 0 {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("✘ {} failed", counts.failed),
            Style::new().fg(theme::fail()).bold(),
        ));
    }
    if counts.running > 0 {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("⋯ {} running", counts.running),
            Style::new().fg(theme::running()),
        ));
    }
    if counts.pending() > 0 {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("○ {} not run", counts.pending()),
            muted(),
        ));
    }
    spans.push(Span::styled(format!("  of {}", counts.total), muted()));
    spans
}

/// The first line of the error message without ANSI escapes.
fn error_summary(result: &TestResult) -> Option<String> {
    let Err(e) = &result.test.as_ref()?.result else {
        return None;
    };
    let text = e.to_string().into_text().ok()?;
    // Skip the `error:` / `panic:` prefix added by the runner, which may be on its own line.
    text.lines
        .iter()
        .map(|line| line.to_string())
        .map(|line| {
            let line = line.trim();
            line.strip_prefix("error:")
                .or_else(|| line.strip_prefix("panic:"))
                .unwrap_or(line)
                .trim()
                .to_string()
        })
        .find(|line| !line.is_empty())
}

/// One line per call: `GET    200 /path   123ms`.
fn call_row(call: Call<'_>, width: usize) -> Line<'static> {
    let duration = format!("{:>8}", fmt_duration(call.duration()));
    let prefix = format!("  {:<6} {:<4} ", call.method(), call.status_label());
    let room = width.saturating_sub(prefix.chars().count() + duration.len() + 1);
    let mut target = call.target();
    if target.chars().count() > room {
        target = target
            .chars()
            .take(room.saturating_sub(1))
            .collect::<String>()
            + "…";
    }
    let padding = room.saturating_sub(target.chars().count()) + 1;
    Line::from(vec![
        Span::styled(format!("  {:<6} ", call.method()), muted().bold()),
        Span::styled(
            format!("{:<4} ", call.status_label()),
            Style::new().fg(call.status_color()).bold(),
        ),
        Span::raw(target),
        Span::raw(" ".repeat(padding)),
        Span::styled(duration, muted()),
    ])
}

fn test_detail_lines(result: &TestResult, lines: &mut Vec<Line<'static>>, width: usize) {
    lines.push(kv_spans("Status", status_spans(result)));
    if let Some(test) = &result.test {
        lines.push(kv_spans(
            "Duration",
            vec![Span::raw(fmt_duration(test.request_time))],
        ));
        lines.extend(kv(
            "Started",
            format_system_time(test.started_at),
            Style::new(),
            width,
        ));
        if test.worker_id >= 0 {
            lines.extend(kv(
                "Worker",
                test.worker_id.to_string(),
                Style::new(),
                width,
            ));
        }
    }
    if result.retries > 0 {
        lines.push(kv_spans(
            "Retries",
            vec![Span::styled(
                result.retries.to_string(),
                Style::new().fg(theme::running()),
            )],
        ));
    }
}

fn check_lines(result: &TestResult, lines: &mut Vec<Line<'static>>, width: usize) {
    if result.checks.is_empty() {
        return;
    }
    let failed = result.checks.iter().filter(|c| !c.result).count();
    let mut heading = vec![Span::styled(
        "Checks",
        Style::new().fg(theme::accent()).bold(),
    )];
    heading.push(Span::styled(
        format!("  {} passed", result.checks.len() - failed),
        muted(),
    ));
    if failed > 0 {
        heading.push(Span::styled(
            format!(", {failed} failed"),
            Style::new().fg(theme::fail()),
        ));
    }
    lines.push(Line::from(heading));
    for check in &result.checks {
        let (symbol, style) = if check.result {
            ("✓ ", Style::new().fg(theme::ok()))
        } else {
            ("✘ ", Style::new().fg(theme::fail()))
        };
        // The expression may contain ANSI colors and a multi-line diff. Passed checks
        // show only the first line; failed checks show the full diff.
        let expr = check
            .expr
            .strip_prefix("check succeeded: ")
            .or_else(|| check.expr.strip_prefix("check failed: "))
            .unwrap_or(&check.expr);
        let text = expr
            .into_text()
            .unwrap_or_else(|_| Text::raw(expr.to_string()));
        let mut expr_lines = text
            .lines
            .into_iter()
            .filter(|line| !line.to_string().trim().is_empty());
        let Some(first) = expr_lines.next() else {
            continue;
        };
        let mut spans = vec![Span::styled(format!("  {symbol}"), style)];
        spans.extend(first.spans);
        lines.extend(wrap_line(Line::from(spans), width));
        if !check.result {
            for line in expr_lines {
                let mut spans = vec![Span::raw("    ")];
                spans.extend(line.spans);
                lines.extend(wrap_line(Line::from(spans), width));
            }
        }
    }
    lines.push(Line::default());
}

fn error_text_lines(result: &TestResult, lines: &mut Vec<Line<'static>>, width: usize) {
    let Some(Err(e)) = result.test.as_ref().map(|t| &t.result) else {
        return;
    };
    lines.push(Line::styled("Error", Style::new().fg(theme::fail()).bold()));
    let text = e
        .to_string()
        .into_text()
        .unwrap_or_else(|_| Text::raw(e.to_string()));
    for line in text.lines {
        lines.extend(wrap_line(line, width));
    }
}

fn error_lines(result: &TestResult, width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![];
    check_lines(result, &mut lines, width);
    if result.is_ok() {
        lines.push(Line::styled(
            "✓ No error. The test passed.",
            Style::new().fg(theme::ok()),
        ));
    } else {
        error_text_lines(result, &mut lines, width);
    }
    lines
}

fn call_lines(result: &TestResult, call: Call<'_>, width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![section("Test")];
    lines.extend(kv("Name", &result.name, Style::new().bold(), width));
    lines.extend(kv("Module", &result.module_name, Style::new(), width));
    lines.extend(kv("Project", &result.project_name, Style::new(), width));
    test_detail_lines(result, &mut lines, width);
    lines.push(Line::default());

    match call {
        Call::Http(log) => {
            lines.push(section("HTTP Request"));
            lines.push(kv_spans(
                "Method",
                vec![Span::styled(
                    log.request.method.to_string(),
                    Style::new().bold(),
                )],
            ));
            lines.extend(kv("URL", log.request.url.as_str(), Style::new(), width));
            let status = log.response.status;
            lines.push(kv_spans(
                "Status",
                vec![Span::styled(
                    format!(
                        "{} {}",
                        status.as_u16(),
                        status.canonical_reason().unwrap_or_default()
                    ),
                    Style::new().fg(call.status_color()).bold(),
                )],
            ));
            lines.push(kv_spans(
                "Duration",
                vec![Span::raw(fmt_duration(log.response.duration_req))],
            ));
            lines.extend(kv(
                "Started",
                format_system_time(log.started_at),
                Style::new(),
                width,
            ));
            let content_type = |headers: &http::HeaderMap| {
                headers
                    .get(http::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string)
            };
            if let Some(body) = log.request.body.as_deref().filter(|b| !b.is_empty()) {
                let ct = content_type(&log.request.headers).unwrap_or_default();
                lines.extend(kv(
                    "Request Body",
                    format!("{} {ct}", format_size(body.len())),
                    Style::new(),
                    width,
                ));
            }
            let ct = content_type(&log.response.headers).unwrap_or_default();
            lines.extend(kv(
                "Response Body",
                format!("{} {ct}", format_size(log.response.body.len())),
                Style::new(),
                width,
            ));
        }
        #[cfg(feature = "grpc")]
        Call::Grpc(log) => {
            lines.push(section("gRPC Request"));
            lines.extend(kv(
                "Method",
                &log.request.method,
                Style::new().bold(),
                width,
            ));
            lines.push(kv_spans(
                "Status",
                vec![Span::styled(
                    format!(
                        "{:?} ({})",
                        log.response.status_code, log.response.status_code as i32
                    ),
                    Style::new().fg(call.status_color()).bold(),
                )],
            ));
            if !log.response.status_message.is_empty() {
                lines.extend(kv(
                    "Status Message",
                    &log.response.status_message,
                    Style::new(),
                    width,
                ));
            }
            lines.push(kv_spans(
                "Duration",
                vec![Span::raw(fmt_duration(log.response.duration))],
            ));
            lines.extend(kv(
                "Started",
                format_system_time(log.started_at),
                Style::new(),
                width,
            ));
        }
    }
    lines
}

fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

type Headers = Vec<(String, String)>;

fn call_headers(call: Call<'_>) -> (Headers, Headers) {
    let http_headers = |headers: &http::HeaderMap| {
        headers
            .iter()
            .map(|(k, v)| {
                let value = v
                    .to_str()
                    .inspect_err(|e| warn!("could not stringify header: {e:#}"))
                    .unwrap_or_default();
                (k.to_string(), value.to_string())
            })
            .collect::<Vec<_>>()
    };
    match call {
        Call::Http(log) => (
            http_headers(&log.request.headers),
            http_headers(&log.response.headers),
        ),
        #[cfg(feature = "grpc")]
        Call::Grpc(log) => {
            let metadata = |metadata: &tonic::metadata::MetadataMap| {
                use tonic::metadata::KeyAndValueRef;
                metadata
                    .iter()
                    .map(|key_value| match key_value {
                        KeyAndValueRef::Ascii(k, v) => (
                            k.as_str().to_string(),
                            v.to_str().unwrap_or("<invalid utf8>").to_string(),
                        ),
                        KeyAndValueRef::Binary(k, v) => (
                            k.as_str().to_string(),
                            format!("<binary {} bytes>", v.as_encoded_bytes().len()),
                        ),
                    })
                    .collect::<Vec<_>>()
            };
            (
                metadata(&log.request.metadata),
                metadata(&log.response.metadata),
            )
        }
    }
}

/// Builds table rows for headers, wrapping long values over multiple rows.
fn header_rows(headers: Headers, width: u16) -> Vec<Row<'static>> {
    const PADDING: usize = 3;
    let key_width = headers
        .iter()
        .map(|(k, _)| k.len())
        .max()
        .unwrap_or_default()
        .min(width as usize / 3);
    let value_width = (width as usize).saturating_sub(key_width + PADDING).max(1);
    headers
        .into_iter()
        .enumerate()
        .flat_map(|(n, (key, value))| {
            let bg = if n % 2 == 0 {
                theme::bg()
            } else {
                theme::row_alt_bg()
            };
            value
                .chars()
                .chunks(value_width)
                .into_iter()
                .map(|chunk| chunk.collect::<String>())
                .collect::<Vec<_>>()
                .into_iter()
                .enumerate()
                .map(move |(i, chunk)| {
                    let key = if i == 0 { key.clone() } else { String::new() };
                    Row::new(vec![
                        Line::styled(key, Style::new().fg(theme::accent())),
                        Line::raw(chunk),
                    ])
                    .bg(bg)
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn headers_table(title: &str, headers: Headers, width: u16, skip: usize) -> Table<'static> {
    let count = headers.len();
    let key_width = headers
        .iter()
        .map(|(k, _)| k.len() as u16)
        .max()
        .unwrap_or_default()
        .min(width / 3);
    let rows = header_rows(headers, width).into_iter().skip(skip);
    Table::new(rows, [Constraint::Length(key_width), Constraint::Fill(1)])
        .column_spacing(2)
        .block(
            Block::new()
                .borders(Borders::TOP)
                .border_style(Style::new().fg(theme::border()))
                .title(Line::from(vec![
                    Span::styled(title.to_string(), Style::new().fg(theme::accent()).bold()),
                    Span::styled(format!(" ({count}) "), muted()),
                ])),
        )
}

fn render_headers(
    req: Headers,
    res: Headers,
    area: Rect,
    buf: &mut Buffer,
    scroll: &mut ScrollState,
) {
    let [layout_req, layout_res] =
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(area);

    // Both tables scroll together.
    let rows = header_rows(req.clone(), area.width)
        .len()
        .max(header_rows(res.clone(), area.width).len());
    scroll.max = rows.saturating_sub(1) as u16;
    scroll.offset = scroll.offset.min(scroll.max);
    let skip = scroll.offset as usize;

    Widget::render(
        headers_table("Request headers", req, area.width, skip),
        layout_req,
        buf,
    );
    Widget::render(
        headers_table("Response headers", res, area.width, skip),
        layout_res,
        buf,
    );
}

fn render_payload(call: Call<'_>, area: Rect, buf: &mut Buffer, scroll: &mut ScrollState) {
    let (theme_bg, highlighted_text) = match call {
        Call::Http(http_call) => match build_http_payload_text(http_call) {
            Some(result) => result,
            None => {
                scroll.max = 0;
                Paragraph::new(Line::styled("No payload", muted())).render(area, buf);
                return;
            }
        },
        #[cfg(feature = "grpc")]
        Call::Grpc(grpc_call) => {
            let req_msg = tanu_core::grpc::format_message(&grpc_call.request.message);
            let res_msg = tanu_core::grpc::format_message(&grpc_call.response.message);
            let combined = format!(
                "Request Message:\n{}\n\nResponse Message:\n{}",
                req_msg, res_msg
            );
            (None, combined)
        }
    };

    // Split the highlighted text into lines and convert only the visible ones,
    // since converting ANSI escapes of large payloads on every frame is costly.
    let lines: Vec<&str> = highlighted_text.lines().collect();
    scroll.max = lines.len().saturating_sub(area.height as usize) as u16;
    scroll.offset = scroll.offset.min(scroll.max);

    let start_line = scroll.offset as usize;
    let end_line = (start_line + area.height as usize).min(lines.len());
    let visible_text = lines[start_line..end_line].join("\n");

    let text = visible_text
        .into_text()
        .unwrap_or_else(|_| Text::raw(visible_text.clone()));
    let paragraph = Paragraph::new(text);
    let paragraph = if let Some(theme_bg) = theme_bg {
        paragraph.bg(Color::Rgb(theme_bg.r, theme_bg.g, theme_bg.b))
    } else {
        paragraph
    };
    paragraph.render(area, buf);
}

impl<'a> InfoWidget<'a> {
    pub fn new(list: &'a TestListState) -> InfoWidget<'a> {
        InfoWidget { list }
    }

    fn test_state(&self, selected: Option<RowRef>) -> Option<&'a TestState> {
        match selected? {
            RowRef::Test(p, m, t) | RowRef::Call(p, m, t, _) => self.list.test(p, m, t),
            _ => None,
        }
    }

    /// Returns the call to show in tabs: the selected call, or the only call of the selected test.
    fn single_call(&self, selected: Option<RowRef>) -> Option<(&'a TestResult, Call<'a>)> {
        let result = self.test_state(selected)?.execution_state.result()?;
        match selected? {
            RowRef::Call(_, _, _, c) => Some((result, result.call(c)?)),
            RowRef::Test(..) if result.call_count() == 1 => Some((result, result.call(0)?)),
            _ => None,
        }
    }

    fn selection_name(&self, selected: Option<RowRef>) -> Option<String> {
        match selected? {
            RowRef::Project(p) => self.list.project(p).map(|p| p.name.clone()),
            RowRef::Module(p, m) => self
                .list
                .module(p, m)
                .map(|m| self.list.display_module_name(&m.name).to_string()),
            RowRef::Test(p, m, t) | RowRef::Call(p, m, t, _) => {
                self.list.test(p, m, t).map(|t| t.info.name.clone())
            }
        }
    }

    fn overview_lines(&self, selected: Option<RowRef>, width: usize) -> Vec<Line<'static>> {
        match selected {
            None => empty_lines(),
            Some(RowRef::Project(p)) => {
                let Some(project) = self.list.project(p) else {
                    return vec![];
                };
                let tests = project
                    .modules
                    .iter()
                    .flat_map(|m| m.tests.iter().map(move |t| (m.name.as_str(), t)));
                let mut lines = vec![section(format!("Project {}", project.name))];
                lines.extend(kv(
                    "Modules",
                    project.modules.len().to_string(),
                    Style::new(),
                    width,
                ));
                self.group_lines(project.counts(), tests, &mut lines, width);
                lines
            }
            Some(RowRef::Module(p, m)) => {
                let Some(module) = self.list.module(p, m) else {
                    return vec![];
                };
                let tests = module.tests.iter().map(|t| (module.name.as_str(), t));
                let mut lines = vec![section(format!(
                    "Module {}",
                    self.list.display_module_name(&module.name)
                ))];
                lines.extend(kv("Path", &module.name, muted(), width));
                self.group_lines(module.counts(), tests, &mut lines, width);
                lines
            }
            Some(RowRef::Test(p, m, t)) | Some(RowRef::Call(p, m, t, _)) => {
                let Some(test) = self.list.test(p, m, t) else {
                    return vec![];
                };
                test_overview_lines(test, width)
            }
        }
    }

    /// Summary of a project or module.
    fn group_lines<'t>(
        &self,
        counts: Counts,
        tests: impl Iterator<Item = (&'t str, &'t TestState)> + Clone,
        lines: &mut Vec<Line<'static>>,
        width: usize,
    ) {
        lines.push(kv_spans("Tests", counts_spans(counts)));
        let results = tests
            .clone()
            .filter_map(|(module, test)| {
                test.execution_state
                    .result()
                    .map(|result| (module, test, result))
            })
            .collect::<Vec<_>>();
        let total_time: std::time::Duration =
            results.iter().filter_map(|(_, _, r)| r.duration()).sum();
        let calls: usize = results.iter().map(|(_, _, r)| r.call_count()).sum();
        if !results.is_empty() {
            lines.push(kv_spans(
                "Test time",
                vec![
                    Span::raw(fmt_duration(total_time)),
                    Span::styled(" (sum of all tests)", muted()),
                ],
            ));
            lines.extend(kv("Calls", calls.to_string(), Style::new(), width));
        }
        endpoint_lines(results.iter().flat_map(|(_, _, r)| r.calls()), lines, width);

        let name = |module: &str, test: &TestState| {
            format!(
                "{}::{}",
                self.list.display_module_name(module),
                test.info.name
            )
        };

        let failed = results
            .iter()
            .filter(|(_, _, r)| !r.is_ok())
            .collect::<Vec<_>>();
        if !failed.is_empty() {
            lines.push(Line::default());
            lines.push(Line::styled(
                format!("Failed tests ({})", failed.len()),
                Style::new().fg(theme::fail()).bold(),
            ));
            for (module, test, result) in failed {
                lines.extend(wrap_line(
                    Line::from(vec![
                        Span::styled("  ✘ ", Style::new().fg(theme::fail())),
                        Span::styled(name(module, test), Style::new().bold()),
                    ]),
                    width,
                ));
                if let Some(summary) = error_summary(result) {
                    lines.extend(wrap_line(
                        Line::styled(format!("    {summary}"), muted()),
                        width,
                    ));
                }
            }
        }

        let mut slowest = results
            .iter()
            .filter_map(|(module, test, r)| r.duration().map(|d| (d, *module, *test)))
            .collect::<Vec<_>>();
        slowest.sort_by_key(|(duration, ..)| std::cmp::Reverse(*duration));
        if !slowest.is_empty() {
            lines.push(Line::default());
            lines.push(section("Slowest tests"));
            for (duration, module, test) in slowest.into_iter().take(5) {
                lines.extend(wrap_line(
                    Line::from(vec![
                        Span::styled(format!("  {:>8}  ", fmt_duration(duration)), muted()),
                        Span::raw(name(module, test)),
                    ]),
                    width,
                ));
            }
        }

        if results.is_empty() && counts.running == 0 {
            lines.push(Line::default());
            lines.push(hint_line(&[("r", "run the selection"), ("R", "run all")]));
        }
    }
}

/// Maximum number of endpoints listed in an overview.
const MAX_ENDPOINTS: usize = 10;

/// Calls grouped by method and endpoint, slowest (p95) first.
fn endpoint_lines<'c>(
    calls: impl Iterator<Item = Call<'c>>,
    lines: &mut Vec<Line<'static>>,
    width: usize,
) {
    let mut groups: HashMap<(String, String), (Vec<Duration>, usize)> = HashMap::new();
    for call in calls {
        let (latencies, errors) = groups.entry((call.method(), call.endpoint())).or_default();
        latencies.push(call.duration());
        *errors += call.is_error() as usize;
    }
    if groups.is_empty() {
        return;
    }
    let mut rows = groups
        .into_iter()
        .map(|((method, endpoint), (mut latencies, errors))| {
            latencies.sort_unstable();
            let p95 = latency::percentile(&latencies, 0.95);
            (method, endpoint, latencies, errors, p95)
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| b.4.cmp(&a.4).then_with(|| a.1.cmp(&b.1)));

    // `  METHOD path…  count  p50  p95  err`
    let numbers = |n: &str, p50: &str, p95: &str| format!(" {n:>5} {p50:>7} {p95:>7} ");
    let numbers_width = numbers("", "", "").len() + 4;
    // Keep the numbers next to the paths in a wide pane.
    let longest = rows.iter().map(|r| r.1.chars().count()).max().unwrap_or(0);
    let room = width
        .saturating_sub(2 + 7 + numbers_width)
        .min(longest)
        .max(8);
    let fit = |text: &str| {
        if text.chars().count() > room {
            text.chars().take(room - 1).collect::<String>() + "…"
        } else {
            format!("{text:<room$}")
        }
    };

    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("Endpoints", Style::new().fg(theme::accent()).bold()),
        Span::styled(format!("  {}", rows.len()), muted()),
    ]));
    lines.push(Line::styled(
        format!(
            "  {:<7}{}{}{:>4}",
            "",
            fit(""),
            numbers("calls", "p50", "p95"),
            "err"
        ),
        muted(),
    ));
    let hidden = rows.len().saturating_sub(MAX_ENDPOINTS);
    for (method, endpoint, latencies, errors, p95) in rows.into_iter().take(MAX_ENDPOINTS) {
        let err = match errors * 100 / latencies.len() {
            0 if errors > 0 => "<1%".to_string(),
            pct => format!("{pct}%"),
        };
        lines.push(Line::from(vec![
            Span::styled(format!("  {method:<7}"), muted().bold()),
            Span::raw(fit(&endpoint)),
            Span::styled(
                numbers(
                    &latencies.len().to_string(),
                    &fmt_duration(latency::percentile(&latencies, 0.5)),
                    &fmt_duration(p95),
                ),
                muted(),
            ),
            Span::styled(
                format!("{err:>4}"),
                if errors > 0 {
                    Style::new().fg(theme::fail()).bold()
                } else {
                    muted()
                },
            ),
        ]));
    }
    if hidden > 0 {
        lines.push(Line::styled(format!("  … and {hidden} more"), muted()));
    }
}

fn hint_line(hints: &[(&str, &str)]) -> Line<'static> {
    let mut spans = vec![Span::styled("Press ", muted())];
    for (n, (key, label)) in hints.iter().enumerate() {
        if n > 0 {
            spans.push(Span::styled(", ", muted()));
        }
        spans.push(Span::styled(
            key.to_string(),
            Style::new().fg(theme::accent()).bold(),
        ));
        spans.push(Span::styled(format!(" to {label}"), muted()));
    }
    Line::from(spans)
}

fn empty_lines() -> Vec<Line<'static>> {
    vec![
        section("Nothing selected"),
        Line::default(),
        hint_line(&[("R", "run all tests"), ("?", "show all key bindings")]),
    ]
}

fn test_overview_lines(test: &TestState, width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![section(format!("Test {}", test.info.name))];
    lines.extend(kv("Module", &test.info.module, muted(), width));
    if let Some(group) = &test.info.serial_group {
        lines.extend(kv("Serial group", group, Style::new(), width));
    }
    if test.info.ordered {
        lines.extend(kv("Ordered", "yes", Style::new(), width));
    }

    let result = match &test.execution_state {
        ExecutionState::Initialized => {
            lines.push(kv_spans("Status", vec![Span::styled("○ not run", muted())]));
            lines.push(Line::default());
            lines.push(hint_line(&[("r", "run this test")]));
            return lines;
        }
        ExecutionState::Executing(_) => {
            lines.push(kv_spans(
                "Status",
                vec![Span::styled("⋯ running", Style::new().fg(theme::running()))],
            ));
            return lines;
        }
        ExecutionState::Executed(result) => result,
    };

    test_detail_lines(result, &mut lines, width);
    lines.push(Line::default());

    let calls = result.calls().collect::<Vec<_>>();
    if calls.is_empty() {
        lines.push(Line::styled("No HTTP/gRPC calls recorded", muted()));
    } else {
        lines.push(Line::from(vec![
            Span::styled("Calls", Style::new().fg(theme::accent()).bold()),
            Span::styled(format!("  {}", calls.len()), muted()),
        ]));
        lines.extend(calls.into_iter().map(|call| call_row(call, width)));
        lines.push(Line::default());
        lines.push(hint_line(&[(
            "Enter",
            "expand the calls and inspect headers/payload",
        )]));
    }
    lines.push(Line::default());

    check_lines(result, &mut lines, width);
    error_text_lines(result, &mut lines, width);
    lines
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

/// Formats a request body for display: pretty-prints if valid JSON and the
/// content-type advertises JSON, otherwise returns the raw text unchanged.
fn pretty_print_if_json(body: &str, content_type: &str) -> String {
    if content_type.starts_with("application/json") {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
            return serde_json::to_string_pretty(&json).unwrap_or_else(|_| body.to_string());
        }
    }
    body.to_string()
}

/// Builds the payload panel text for an HTTP call.
///
/// Returns `None` when both the request and response bodies are empty (the
/// panel should then be hidden). Returns `Some((theme_bg, text))` where
/// `theme_bg` is set when syntax-highlighting was applied.
///
/// Behaviour:
/// - If there is **no request body**, mirrors the original behaviour: JSON
///   response bodies are syntax-highlighted; others are shown as plain text.
/// - If a **request body is present**, both bodies are shown under labelled
///   sections ("Request Body:" / "Response Body:") without syntax highlighting,
///   mirroring the gRPC combined-message display.
fn build_http_payload_text(
    http_call: &tanu_core::http::Log,
) -> Option<(Option<syntect::highlighting::Color>, String)> {
    let req_body = http_call.request.body.as_deref().unwrap_or("").trim();
    let res_body = http_call.response.body.trim();

    if req_body.is_empty() && res_body.is_empty() {
        return None;
    }

    // Over the cap, skip pretty-printing and syntax highlighting: the highlighter
    // is memoized, so a huge body would be cached for the rest of the session.
    let max_body_size = get_tanu_config().max_body_size();

    // No request body — keep original behaviour for response-only display.
    if req_body.is_empty() {
        if let Some(truncated) = max_body_size.truncate(res_body) {
            return Some((None, truncated));
        }
        let content_type = http_call
            .response
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if content_type.starts_with("application/json") {
            let json: serde_json::Value = serde_json::from_str(res_body).ok()?;
            let json_str = serde_json::to_string_pretty(&json).unwrap();
            let (theme_bg, highlighted_json) = highlight_source_code(json_str);
            return Some((Some(theme_bg), highlighted_json));
        }
        return Some((None, res_body.to_string()));
    }

    // Request body present — build a combined section display.
    let req_ct = http_call
        .request
        .headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let res_ct = http_call
        .response
        .headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let mut text = format!(
        "Request Body:\n{}",
        max_body_size
            .truncate(req_body)
            .unwrap_or_else(|| pretty_print_if_json(req_body, req_ct))
    );
    if !res_body.is_empty() {
        text.push_str(&format!(
            "\n\nResponse Body:\n{}",
            max_body_size
                .truncate(res_body)
                .unwrap_or_else(|| pretty_print_if_json(res_body, res_ct))
        ));
    }
    Some((None, text))
}

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
        assert_eq!(Tab::Call, state.selected_tab);
        state.next_tab();
        assert_eq!(Tab::Headers, state.selected_tab);
        state.next_tab();
        assert_eq!(Tab::Payload, state.selected_tab);
        state.next_tab();
        assert_eq!(Tab::Error, state.selected_tab);
        state.next_tab();
        assert_eq!(Tab::Call, state.selected_tab);
        state.prev_tab();
        assert_eq!(Tab::Error, state.selected_tab);

        Ok(())
    }

    #[test]
    fn scroll_is_bounded() {
        let mut state = InfoState::new();
        state.scrolls[View::Overview.index()].max = 5;
        state.scroll_down(3);
        state.scroll_down(3);
        assert_eq!(5, state.scrolls[0].offset);
        state.scroll_up(10);
        assert_eq!(0, state.scrolls[0].offset);
        state.scroll_end();
        assert_eq!(5, state.scrolls[0].offset);
        state.scroll_home();
        assert_eq!(0, state.scrolls[0].offset);

        // Changing the selection resets the scroll position.
        state.scroll_end();
        state.select(Some(RowRef::Project(0)));
        assert_eq!(0, state.scrolls[0].offset);
    }

    #[test]
    fn wrap_line_keeps_styles() {
        let line = Line::from(vec![
            Span::styled("abcd", Style::new().fg(Color::Red)),
            Span::raw("efg"),
        ]);
        let lines = wrap_line(line, 3);
        assert_eq!(3, lines.len());
        assert_eq!("abc", lines[0].to_string());
        assert_eq!("def", lines[1].to_string());
        assert_eq!("g", lines[2].to_string());
        assert_eq!(Some(Color::Red), lines[1].spans[0].style.fg);
        assert_eq!(None, lines[1].spans[1].style.fg);
    }

    #[test]
    fn kv_wraps_value() {
        let lines = kv("Key", "a".repeat(30), Style::new(), KEY_WIDTH + 10);
        assert_eq!(3, lines.len());
        assert!(lines[0].to_string().starts_with("Key "));
        assert!(lines[1].to_string().starts_with(&" ".repeat(KEY_WIDTH)));
    }
}
