//! The logs pane.
//!
//! Log records from `tracing` (and from `log` through `tracing_log::LogTracer`) are
//! captured by [`LogLayer`] into a global ring buffer, and [`LoggerWidget`] renders the
//! tail of the buffer. The pane follows new records until it is scrolled up.
//!
//! The shown lines can be searched like in `less`: matches are highlighted, and the
//! view jumps from one matching line to the next.
use ansi_to_tui::IntoText;
use chrono::{DateTime, Local};
use ratatui::{prelude::*, widgets::Paragraph};
use std::{
    collections::VecDeque,
    fmt::Write as _,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex, MutexGuard,
    },
};
use tracing::{
    field::{Field, Visit},
    level_filters::LevelFilter,
    Event, Level, Metadata, Subscriber,
};
use tracing_log::{AsTrace, NormalizeEvent};
use tracing_subscriber::layer::{Context, Layer};

use crate::widget::theme::{self, muted};

/// Maximum number of records kept in memory; the oldest are dropped first.
const CAPACITY: usize = 10_000;

/// Levels from the least to the most verbose.
const LEVELS: [Level; 5] = [
    Level::ERROR,
    Level::WARN,
    Level::INFO,
    Level::DEBUG,
    Level::TRACE,
];

/// Index into [`LEVELS`] of the most verbose level captured by [`LogLayer`].
static MAX_LEVEL: AtomicUsize = AtomicUsize::new(LEVELS.len() - 1);

/// A captured log record.
#[derive(Debug, Clone)]
pub struct Record {
    pub time: DateTime<Local>,
    pub level: Level,
    pub target: String,
    /// The message followed by the other fields as `key=value`, with ANSI colors
    /// converted to styles. Has at least one line.
    pub lines: Vec<Line<'static>>,
}

impl Record {
    pub fn new(level: Level, target: String, message: &str) -> Record {
        let mut lines = message
            .into_text()
            .map(|text| text.lines)
            .unwrap_or_else(|_| {
                message
                    .lines()
                    .map(|line| Line::raw(line.to_string()))
                    .collect()
            });
        while lines.len() > 1 && lines.last().is_some_and(|line| line.width() == 0) {
            lines.pop();
        }
        if lines.is_empty() {
            lines.push(Line::default());
        }
        Record {
            time: Local::now(),
            level,
            target,
            lines,
        }
    }

    fn line_count(&self) -> usize {
        self.lines.len()
    }
}

/// Records captured by [`LogLayer`], oldest first.
static RECORDS: Mutex<VecDeque<Record>> = Mutex::new(VecDeque::new());

fn records() -> MutexGuard<'static, VecDeque<Record>> {
    // A panic while holding the lock leaves the buffer intact, so keep using it.
    RECORDS.lock().unwrap_or_else(|e| e.into_inner())
}

fn push(record: Record) {
    let mut records = records();
    if records.len() == CAPACITY {
        records.pop_front();
    }
    records.push_back(record);
}

/// Collects the message and fields of an event.
#[derive(Default)]
struct MessageVisitor {
    message: String,
    fields: String,
}

impl Visit for MessageVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message.push_str(value);
        } else {
            self.record_debug(field, &value);
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        match field.name() {
            "message" => {
                let _ = write!(self.message, "{value:?}");
            }
            // Metadata of records forwarded from the `log` crate.
            name if name.starts_with("log.") => {}
            name => {
                let _ = write!(self.fields, " {name}={value:?}");
            }
        }
    }
}

/// A `tracing` layer that stores events for the logs pane.
///
/// Records of tanu's own crates are filtered by `tanu_level`, everything else by `level`.
pub struct LogLayer {
    level: LevelFilter,
    tanu_level: LevelFilter,
}

impl LogLayer {
    pub fn new(level: log::LevelFilter, tanu_level: log::LevelFilter) -> LogLayer {
        let layer = LogLayer {
            level: level.as_trace(),
            tanu_level: tanu_level.as_trace(),
        };
        let max = layer.level.max(layer.tanu_level);
        let index = LEVELS.iter().rposition(|l| *l <= max).unwrap_or_default();
        MAX_LEVEL.store(index, Ordering::Relaxed);
        layer
    }

    fn filter_for(&self, target: &str) -> LevelFilter {
        let is_tanu = ["tanu", "tanu_core", "tanu_tui"].iter().any(|krate| {
            target
                .strip_prefix(krate)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with("::"))
        });
        if is_tanu {
            self.tanu_level
        } else {
            self.level
        }
    }
}

impl<S: Subscriber> Layer<S> for LogLayer {
    fn enabled(&self, metadata: &Metadata<'_>, _ctx: Context<'_, S>) -> bool {
        // Records from the `log` crate all have the target "log" until normalized in
        // `on_event`, so let them through here.
        metadata.target() == "log" || *metadata.level() <= self.filter_for(metadata.target())
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(self.level.max(self.tanu_level))
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let normalized = event.normalized_metadata();
        let metadata = normalized.as_ref().unwrap_or_else(|| event.metadata());
        if *metadata.level() > self.filter_for(metadata.target()) {
            return;
        }
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        let mut message = visitor.message;
        message.push_str(&visitor.fields);
        push(Record::new(
            *metadata.level(),
            metadata.target().to_string(),
            &message,
        ));
    }
}

/// View state of the logs pane.
#[derive(Debug)]
pub struct LoggerState {
    /// Number of lines scrolled up from the bottom; 0 follows new records.
    offset: usize,
    /// Number of lines when `offset` was last clamped, to keep the view still while
    /// new records arrive.
    seen_lines: usize,
    /// Index into [`LEVELS`] of the most verbose level shown.
    level: usize,
    /// Index into [`LEVELS`] of the most verbose level captured; more verbose levels
    /// would show nothing.
    max_level: usize,
    /// Whether the target (the Rust module path) of each record is shown.
    pub show_target: bool,
    /// Whether a search query is being typed.
    pub searching: bool,
    /// Text searched for in the shown lines, ignoring case; empty without a search.
    search: String,
    /// Number of lines from the bottom of the matching line last jumped to.
    current: Option<usize>,
    /// `offset` when the search started, restored when it is cancelled.
    origin: usize,
    /// Whether the search matches no line.
    no_match: bool,
    /// Number of lines shown at once, as of the last render.
    page: usize,
}

impl Default for LoggerState {
    fn default() -> Self {
        LoggerState {
            offset: 0,
            seen_lines: 0,
            level: MAX_LEVEL.load(Ordering::Relaxed),
            max_level: MAX_LEVEL.load(Ordering::Relaxed),
            show_target: true,
            searching: false,
            search: String::new(),
            current: None,
            origin: 0,
            no_match: false,
            page: 0,
        }
    }
}

impl LoggerState {
    pub fn new() -> LoggerState {
        LoggerState::default()
    }

    pub fn scroll_up(&mut self, lines: usize) {
        self.offset = self.offset.saturating_add(lines);
    }

    pub fn scroll_down(&mut self, lines: usize) {
        self.offset = self.offset.saturating_sub(lines);
    }

    pub fn scroll_home(&mut self) {
        self.offset = usize::MAX;
    }

    /// Scrolls to the bottom and follows new records.
    pub fn scroll_end(&mut self) {
        self.offset = 0;
    }

    /// Shows more verbose levels.
    pub fn more_verbose(&mut self) {
        self.level = (self.level + 1).min(self.max_level);
        self.current = None;
    }

    /// Shows less verbose levels.
    pub fn less_verbose(&mut self) {
        self.level = self.level.saturating_sub(1);
        self.current = None;
    }

    pub fn level(&self) -> Level {
        LEVELS[self.level]
    }

    pub fn is_following(&self) -> bool {
        self.offset == 0
    }

    fn shows(&self, level: Level) -> bool {
        level <= self.level()
    }

    /// Accounts for the lines added since the last call, keeping the view still.
    fn sync(&mut self, total_lines: usize) {
        let added = total_lines.saturating_sub(self.seen_lines);
        if self.offset > 0 {
            self.offset = self.offset.saturating_add(added);
        }
        if let Some(current) = &mut self.current {
            *current += added;
        }
        self.seen_lines = total_lines;
    }

    /// Starts typing a search query.
    pub fn start_search(&mut self) {
        self.clear_search();
        self.searching = true;
        self.origin = self.offset;
    }

    pub fn search_input(&mut self, c: char) {
        self.search.push(c);
        self.search_changed();
    }

    pub fn search_backspace(&mut self) {
        self.search.pop();
        self.search_changed();
    }

    /// Searches again from where the search started.
    fn search_changed(&mut self) {
        self.offset = self.origin;
        self.current = None;
        self.no_match = false;
        self.jump_to_match(true);
    }

    /// Stops typing the query; `keep` leaves the search active, otherwise the view
    /// goes back to where the search started.
    pub fn end_search(&mut self, keep: bool) {
        self.searching = false;
        if !keep {
            self.offset = self.origin;
            self.clear_search();
        }
    }

    pub fn clear_search(&mut self) {
        self.search.clear();
        self.current = None;
        self.no_match = false;
    }

    /// true if matches of a search query are highlighted.
    pub fn has_search(&self) -> bool {
        !self.search.is_empty()
    }

    /// Jumps to the next matching line below (`forward`) or above, wrapping around.
    /// A new search starts at the last match on the page or above it, as the newest
    /// logs are at the bottom.
    pub fn jump_to_match(&mut self, forward: bool) {
        if !self.has_search() {
            return;
        }
        let query = folded(&self.search);
        // Lines from the bottom of the matching lines, newest first.
        let mut matches = vec![];
        let mut total_lines = 0;
        for record in records().iter().rev().filter(|r| self.shows(r.level)) {
            let lines = record_lines(record, self.show_target);
            for (n, line) in lines.iter().rev().enumerate() {
                if !match_ranges(&line_chars(line), &query).is_empty() {
                    matches.push(total_lines + n);
                }
            }
            total_lines += lines.len();
        }
        self.sync(total_lines);

        let page = self.page.max(1);
        self.offset = self.offset.min(total_lines.saturating_sub(page));
        let offset = self.offset;
        let target = match self.current {
            Some(current) if forward => matches
                .iter()
                .rfind(|line| **line < current)
                .or(matches.last()),
            Some(current) => matches
                .iter()
                .find(|line| **line > current)
                .or(matches.first()),
            None => matches
                .iter()
                .find(|line| **line >= offset)
                .or(matches.last()),
        };
        self.current = target.copied();
        self.no_match = target.is_none();
        if let Some(line) = self.current {
            if !(offset..offset + page).contains(&line) {
                self.offset = line.saturating_sub(page / 2);
            }
        }
    }
}

/// The characters of `text` with the case folded, to search ignoring case.
fn folded(text: &str) -> Vec<char> {
    text.chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect()
}

/// The case-folded characters of a rendered line.
fn line_chars(line: &Line) -> Vec<char> {
    line.spans
        .iter()
        .flat_map(|span| folded(&span.content))
        .collect()
}

/// Ranges of the characters of `text` matching `query`; both are case-folded.
fn match_ranges(text: &[char], query: &[char]) -> Vec<std::ops::Range<usize>> {
    let mut ranges = vec![];
    let mut start = 0;
    while !query.is_empty() && start + query.len() <= text.len() {
        if text[start..].starts_with(query) {
            ranges.push(start..start + query.len());
            start += query.len();
        } else {
            start += 1;
        }
    }
    ranges
}

/// Patches `style` onto the parts of a line matching `query`.
fn highlight(line: Line<'static>, query: &[char], style: Style) -> Line<'static> {
    let ranges = match_ranges(&line_chars(&line), query);
    if ranges.is_empty() {
        return line;
    }
    let mut spans = vec![];
    let mut position = 0;
    for span in &line.spans {
        // Split the span where a match starts or ends.
        let mut run = String::new();
        let mut run_matches = false;
        let mut flush = |run: &mut String, matches: bool| {
            if !run.is_empty() {
                let run_style = if matches {
                    span.style.patch(style)
                } else {
                    span.style
                };
                spans.push(Span::styled(std::mem::take(run), run_style));
            }
        };
        for c in span.content.chars() {
            let matches = ranges.iter().any(|range| range.contains(&position));
            if matches != run_matches {
                flush(&mut run, run_matches);
                run_matches = matches;
            }
            run.push(c);
            position += 1;
        }
        flush(&mut run, run_matches);
    }
    Line::from(spans)
}

pub struct LoggerWidget {
    focused: bool,
    maximized: bool,
}

impl LoggerWidget {
    pub fn new(focused: bool, maximized: bool) -> LoggerWidget {
        LoggerWidget { focused, maximized }
    }
}

fn level_style(level: Level) -> Style {
    match level {
        Level::ERROR => Style::new().fg(theme::fail()),
        Level::WARN => Style::new().fg(theme::accent()).bold(),
        Level::INFO => Style::new(),
        _ => muted(),
    }
}

/// Style of the level cell: a badge for the levels worth noticing.
fn level_badge_style(level: Level) -> Style {
    let badge = |bg| Style::new().fg(theme::on_accent()).bg(bg).bold();
    match level {
        Level::ERROR => badge(theme::fail()),
        Level::WARN => badge(theme::accent()),
        Level::INFO => badge(theme::ok()),
        Level::DEBUG => badge(theme::current().muted),
        _ => muted(),
    }
}

fn level_label(level: Level) -> &'static str {
    match level {
        Level::ERROR => "E",
        Level::WARN => "W",
        Level::INFO => "I",
        Level::DEBUG => "D",
        Level::TRACE => "T",
    }
}

/// Renders a record into lines; continuation lines are indented under the message.
fn record_lines(record: &Record, show_target: bool) -> Vec<Line<'static>> {
    let sep = || Span::styled("│", Style::new().fg(theme::border()));
    let style = level_style(record.level);
    let mut prefix = vec![
        Span::styled(record.time.format("%H:%M:%S").to_string(), muted()),
        sep(),
        Span::styled(
            format!(" {} ", level_label(record.level)),
            level_badge_style(record.level),
        ),
        sep(),
    ];
    if show_target {
        prefix.push(Span::styled(record.target.clone(), muted()));
        prefix.push(sep());
    }
    let indent = " ".repeat(prefix.iter().map(Span::width).sum());
    // ANSI colors in the message take precedence over the level style.
    let spans = |line: &Line<'static>| {
        line.spans
            .iter()
            .map(|span| {
                let span_style = line.style.patch(span.style);
                Span::styled(span.content.clone(), style.patch(span_style))
            })
            .collect::<Vec<_>>()
    };
    record
        .lines
        .iter()
        .enumerate()
        .map(|(n, line)| {
            let mut line_spans = if n == 0 {
                std::mem::take(&mut prefix)
            } else {
                vec![Span::raw(indent.clone())]
            };
            line_spans.extend(spans(line));
            Line::from(line_spans)
        })
        .collect()
}

impl StatefulWidget for LoggerWidget {
    type State = LoggerState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let height = area.height.saturating_sub(2) as usize;

        // Copy out only what is shown, so no log is emitted while holding the lock.
        let (shown, bottom_cut) = {
            let records = records();
            let total_lines: usize = records
                .iter()
                .filter(|r| state.shows(r.level))
                .map(Record::line_count)
                .sum();
            state.sync(total_lines);
            state.offset = state.offset.min(total_lines.saturating_sub(height));
            state.page = height;

            // Walk back from the newest record until the page is filled.
            let mut skip = state.offset;
            let mut need = height;
            let mut shown = vec![];
            // Lines of the newest shown record scrolled out at the bottom.
            let mut bottom_cut = 0;
            for record in records.iter().rev().filter(|r| state.shows(r.level)) {
                if need == 0 {
                    break;
                }
                let count = record.line_count();
                if skip >= count {
                    skip -= count;
                    continue;
                }
                if shown.is_empty() {
                    bottom_cut = skip;
                }
                shown.push(record.clone());
                need = need.saturating_sub(count - skip);
                skip = 0;
            }
            shown.reverse();
            (shown, bottom_cut)
        };

        // The oldest shown record may be partially scrolled out at the top, and the
        // newest one at the bottom.
        let mut lines: Vec<Line> = shown
            .iter()
            .flat_map(|record| record_lines(record, state.show_target))
            .collect();
        lines.truncate(lines.len() - bottom_cut);
        let mut lines = lines.split_off(lines.len().saturating_sub(height));

        if state.has_search() {
            let query = folded(&state.search);
            let matched = Style::new().add_modifier(Modifier::REVERSED);
            let current = Style::new()
                .fg(theme::on_accent())
                .bg(theme::accent())
                .bold();
            let bottom = state.offset + lines.len().saturating_sub(1);
            lines = lines
                .into_iter()
                .enumerate()
                .map(|(n, line)| {
                    let is_current = state.current == Some(bottom - n);
                    highlight(line, &query, if is_current { current } else { matched })
                })
                .collect();
        }

        let mut title = vec![
            Span::raw("Logs"),
            Span::styled(format!(" ≤{}", state.level()), muted()),
        ];
        if !state.is_following() {
            title.push(Span::styled(format!(" ↑{}", state.offset), muted()));
        }
        if state.has_search() || state.searching {
            title.push(Span::styled(
                format!(" /{}", state.search),
                Style::new().fg(theme::running()),
            ));
        }
        if state.no_match {
            title.push(Span::styled(" no match", Style::new().fg(theme::fail())));
        }
        if self.maximized {
            title.push(Span::styled(" [maximized]", muted()));
        }
        let block = theme::block(title, self.focused);
        if lines.is_empty() {
            let inner = block.inner(area);
            block.render(area, buf);
            Paragraph::new(Line::styled("No logs yet", muted()))
                .centered()
                .render(inner.centered_vertically(Constraint::Length(1)), buf);
            return;
        }
        Paragraph::new(lines).block(block).render(area, buf);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn rendered(state: &mut LoggerState, height: u16) -> Vec<String> {
        let area = Rect::new(0, 0, 40, height);
        let mut buf = Buffer::empty(area);
        LoggerWidget::new(false, false).render(area, &mut buf, state);
        (1..height - 1)
            .map(|y| {
                (1..area.width - 1)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn parses_ansi_colors() {
        let record = Record::new(Level::INFO, "test".into(), "\x1b[31mred\x1b[0m plain\n");
        assert_eq!(record.line_count(), 1);
        let line = &record_lines(&record, false)[0];
        let red = line.spans.iter().find(|s| s.content == "red").unwrap();
        assert_eq!(red.style.fg, Some(Color::Red));
        assert!(!line.to_string().contains('\x1b'));
    }

    #[test]
    fn badges_levels_worth_noticing() {
        let level_cell = |level| {
            let record = Record::new(level, "test".into(), "message");
            record_lines(&record, false)[0].spans[2].clone()
        };
        let info = level_cell(Level::INFO);
        assert_eq!(info.content, " I ");
        assert_eq!(info.style.bg, Some(theme::ok()));
        assert_eq!(info.style.fg, Some(theme::on_accent()));
        assert_eq!(level_cell(Level::ERROR).style.bg, Some(theme::fail()));
        assert_eq!(level_cell(Level::WARN).style.bg, Some(theme::accent()));
        assert_eq!(
            level_cell(Level::DEBUG).style.bg,
            Some(theme::current().muted)
        );
        assert_eq!(level_cell(Level::TRACE).style.bg, None);
    }

    #[test]
    fn highlights_matches_ignoring_case() {
        let record = Record::new(Level::INFO, "test".into(), "\x1b[31mfoo\x1b[0mBAR foobar");
        let line = record_lines(&record, false).remove(0);
        let style = Style::new().add_modifier(Modifier::REVERSED);
        let text = line.to_string();

        let highlighted = highlight(line.clone(), &folded("OBa"), style);
        assert_eq!(highlighted.to_string(), text);
        let matched = highlighted
            .spans
            .iter()
            .filter(|span| span.style.add_modifier.contains(Modifier::REVERSED))
            .collect::<Vec<_>>();
        // The first match spans two differently colored parts.
        let contents = matched.iter().map(|s| &*s.content).collect::<Vec<_>>();
        assert_eq!(contents, ["o", "BA", "oba"]);
        assert_eq!(matched[0].style.fg, Some(Color::Red));

        assert_eq!(highlight(line.clone(), &folded("zzz"), style), line);
    }

    #[test]
    fn filters_tanu_targets_by_tanu_level() {
        // Not `LogLayer::new`, which sets the global `MAX_LEVEL` other tests depend on.
        let layer = LogLayer {
            level: LevelFilter::WARN,
            tanu_level: LevelFilter::DEBUG,
        };
        assert_eq!(layer.filter_for("tanu"), LevelFilter::DEBUG);
        assert_eq!(layer.filter_for("tanu_core::http"), LevelFilter::DEBUG);
        assert_eq!(layer.filter_for("tanu_tui"), LevelFilter::DEBUG);
        assert_eq!(
            layer.filter_for("tanu_integration_tests"),
            LevelFilter::WARN
        );
        assert_eq!(layer.filter_for("reqwest"), LevelFilter::WARN);
    }

    // The only test touching the global buffer, as tests run in parallel.
    #[test]
    fn follows_scrolls_and_filters() {
        for (n, level) in [Level::INFO, Level::DEBUG, Level::ERROR, Level::INFO]
            .into_iter()
            .enumerate()
        {
            let message = if n == 2 {
                "m2\ncontinued".to_string()
            } else {
                format!("m{n}")
            };
            push(Record::new(level, "test".into(), &message));
        }
        let messages = |lines: Vec<String>| {
            lines
                .iter()
                .map(|line| line.rsplit('│').next().unwrap().trim().to_string())
                .collect::<Vec<_>>()
        };

        let mut state = LoggerState::new();
        assert_eq!(messages(rendered(&mut state, 5)), ["m2", "continued", "m3"]);

        state.scroll_up(1);
        assert_eq!(messages(rendered(&mut state, 5)), ["m1", "m2", "continued"]);

        // New records do not move a scrolled view.
        push(Record::new(Level::INFO, "test".into(), "m4"));
        assert_eq!(messages(rendered(&mut state, 5)), ["m1", "m2", "continued"]);

        state.scroll_home();
        assert_eq!(messages(rendered(&mut state, 5)), ["m0", "m1", "m2"]);

        state.scroll_end();
        state.less_verbose();
        state.less_verbose();
        assert_eq!(state.level(), Level::INFO);
        assert_eq!(
            messages(rendered(&mut state, 7)),
            ["m0", "m2", "continued", "m3", "m4"]
        );

        // Shown lines are m0, m2, continued, m3, m4; a page shows three of them.
        let mut state = LoggerState::new();
        state.less_verbose();
        state.less_verbose();
        rendered(&mut state, 5);
        state.start_search();
        state.search_input('M');
        // The newest match is already on the page.
        assert_eq!(state.current, Some(0));
        assert!(state.is_following());

        state.search_input('0');
        assert_eq!(messages(rendered(&mut state, 5)), ["m0", "m2", "continued"]);
        state.search_backspace();
        state.end_search(true);
        assert_eq!(state.current, Some(0));
        assert_eq!(messages(rendered(&mut state, 5)), ["continued", "m3", "m4"]);

        // Backward goes up, skipping the line that does not match.
        state.jump_to_match(false);
        assert_eq!(state.current, Some(1));
        state.jump_to_match(false);
        assert_eq!(state.current, Some(3));
        assert_eq!(messages(rendered(&mut state, 5)), ["m0", "m2", "continued"]);
        state.jump_to_match(true);
        assert_eq!(state.current, Some(1));
        // Forward from the newest match wraps around to the oldest.
        state.jump_to_match(true);
        state.jump_to_match(true);
        assert_eq!(state.current, Some(4));

        // Cancelling a search restores the view.
        state.scroll_end();
        state.start_search();
        for c in "m0".chars() {
            state.search_input(c);
        }
        assert!(!state.is_following());
        state.end_search(false);
        assert!(state.is_following());
        assert!(!state.has_search());

        state.start_search();
        state.search_input('?');
        assert!(state.no_match);
        assert!(state.is_following());
    }
}
