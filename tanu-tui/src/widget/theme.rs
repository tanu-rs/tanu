//! Central color palette and style helpers shared by all widgets.
use http::StatusCode;
use ratatui::{
    style::{palette::tailwind, Color, Modifier, Style},
    text::Line,
    widgets::{Block, BorderType},
};
use std::time::Duration;

// The palette is blue-based. Green and red are used only for success and failure;
// everything else is a shade of blue.

/// Primary accent color (titles, key hints, focused borders, running tests).
pub const ACCENT: Color = tailwind::BLUE.c400;
/// Border color of unfocused panes.
pub const BORDER: Color = Color::Rgb(0x2a, 0x4a, 0x7a);
/// Border color of the focused pane.
pub const BORDER_FOCUSED: Color = ACCENT;
/// Secondary text such as metadata and hints.
pub const MUTED: Color = Color::Rgb(0x7f, 0x9c, 0xc9);
/// Successful test / request. A cool, soft green that sits with the blue palette.
pub const OK: Color = Color::Rgb(0x7e, 0xc6, 0x99);
/// Failed test / request. A soft red that sits with the blue palette.
pub const FAIL: Color = Color::Rgb(0xe0, 0x6c, 0x75);

// Chart palette, shared by the timeline and the latency histogram.

/// Passed tests and successful calls.
pub const BAR: Color = ACCENT;
/// Slightly darker shade of `BAR` to tell adjacent timeline bars apart.
pub const BAR_ALT: Color = tailwind::BLUE.c500;
/// Failed tests and error responses. Softer than `FAIL`: the test list and status
/// bar already flag failures, and error responses are often what a test expects.
pub const BAR_ERROR: Color = Color::Rgb(0xa8, 0x55, 0x5f);
/// Slightly darker shade of `BAR_ERROR` to tell adjacent timeline bars apart.
pub const BAR_ERROR_ALT: Color = Color::Rgb(0x8f, 0x48, 0x52);
/// Bars of the test selected in the test list.
pub const BAR_SELECTED: Color = tailwind::BLUE.c100;
/// Running or retried test.
pub const RUNNING: Color = ACCENT;
/// Background of the selected list row.
pub const SELECTED_BG: Color = Color::Rgb(0x1e, 0x3a, 0x6e);
/// Background of alternating table rows.
pub const ROW_ALT_BG: Color = Color::Rgb(0x10, 0x22, 0x44);

/// Style of the selected row in lists.
pub const SELECTED_STYLE: Style = Style::new().bg(SELECTED_BG).add_modifier(Modifier::BOLD);

/// Style for secondary text.
pub fn muted() -> Style {
    Style::new().fg(MUTED)
}

/// Border color of a pane according to the focus state.
pub fn border_style(focused: bool) -> Style {
    Style::new().fg(if focused { BORDER_FOCUSED } else { BORDER })
}

/// Bordered block styled according to the focus state.
pub fn block<'a>(title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
    // Focus is shown by color only; a thick border looks too heavy.
    let border_style = border_style(focused);
    let title: Line = title.into();
    let title = if focused {
        title.patch_style(Style::new().fg(ACCENT).bold())
    } else {
        title.patch_style(Style::new().bold())
    };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(title)
}

/// Color for an HTTP status code.
pub fn status_color(status: StatusCode) -> Color {
    match status.as_u16() {
        200..=299 => OK,
        400..=599 => FAIL,
        _ => ACCENT,
    }
}

/// Human friendly duration: `850ms`, `1.24s`, `2m03s`.
pub fn fmt_duration(d: Duration) -> String {
    let ms = d.as_millis();
    if ms == 0 {
        "<1ms".to_string()
    } else if ms < 1_000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.2}s", d.as_secs_f64())
    } else {
        let secs = d.as_secs();
        format!("{}m{:02}s", secs / 60, secs % 60)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn fmt_duration_units() {
        assert_eq!("<1ms", fmt_duration(Duration::from_micros(300)));
        assert_eq!("850ms", fmt_duration(Duration::from_millis(850)));
        assert_eq!("1.24s", fmt_duration(Duration::from_millis(1_240)));
        assert_eq!("2m03s", fmt_duration(Duration::from_secs(123)));
    }
}
