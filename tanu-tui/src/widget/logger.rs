//! The logs pane.
//!
//! Log records from `tracing` (and from `log` through `tracing_log::LogTracer`) are
//! captured by [`LogLayer`] into a global ring buffer, and [`LoggerWidget`] renders the
//! tail of the buffer. The pane follows new records until it is scrolled up.
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
}

impl Default for LoggerState {
    fn default() -> Self {
        LoggerState {
            offset: 0,
            seen_lines: 0,
            level: MAX_LEVEL.load(Ordering::Relaxed),
            max_level: MAX_LEVEL.load(Ordering::Relaxed),
            show_target: true,
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
    }

    /// Shows less verbose levels.
    pub fn less_verbose(&mut self) {
        self.level = self.level.saturating_sub(1);
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
        Span::styled(level_label(record.level), style),
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
            if state.offset > 0 {
                // Keep the view still while new records arrive.
                state.offset = state
                    .offset
                    .saturating_add(total_lines.saturating_sub(state.seen_lines));
            }
            state.offset = state.offset.min(total_lines.saturating_sub(height));
            state.seen_lines = total_lines;

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
        let lines = lines.split_off(lines.len().saturating_sub(height));

        let mut title = vec![
            Span::raw("Logs"),
            Span::styled(format!(" ≤{}", state.level()), muted()),
        ];
        if !state.is_following() {
            title.push(Span::styled(format!(" ↑{}", state.offset), muted()));
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
    }
}
