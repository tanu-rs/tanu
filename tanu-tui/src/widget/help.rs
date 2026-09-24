//! Popup listing all key bindings.
use ratatui::{
    prelude::*,
    widgets::{Clear, Paragraph},
};

use crate::widget::theme::{self, muted};

/// Key bindings grouped by section.
pub const KEY_BINDINGS: &[(&str, &[(&str, &str)])] = &[
    (
        "Global",
        &[
            ("r / 2", "Run the selected project, module or test"),
            ("R / 1", "Run all tests"),
            ("Tab / Shift+Tab", "Focus next / previous pane"),
            ("[ / ]", "Previous / next details tab"),
            ("z", "Maximize the focused pane"),
            ("?", "Toggle this help"),
            ("q / Esc", "Quit (Esc closes search/help first)"),
        ],
    ),
    (
        "Tests",
        &[
            ("j k / ↑ ↓", "Move the cursor"),
            ("g G / Home End", "First / last row"),
            ("Ctrl+U / Ctrl+D", "Move half a page up / down"),
            ("Enter / Space", "Expand or collapse"),
            ("← →", "Previous / next project (with several projects)"),
            ("h l", "Collapse / expand, or go to parent / child"),
            ("/", "Search tests and modules"),
            ("f", "Cycle filter: all, failed, passed, not run"),
            ("n / N", "Jump to next / previous failed test"),
        ],
    ),
    (
        "Details",
        &[
            ("h l / ← →", "Previous / next tab"),
            ("j k / ↑ ↓", "Scroll"),
            ("Ctrl+U / Ctrl+D", "Scroll half a page"),
            ("g G", "Scroll to top / bottom"),
        ],
    ),
    (
        "Logs",
        &[
            ("j k / ↑ ↓", "Select log target"),
            ("h l / ← →", "Change the level of the target"),
            ("PgUp / PgDn", "Scroll the logs"),
            ("L / H", "Toggle the target selector"),
            ("Space", "Toggle hiding of targets with log level off"),
            ("F", "Focus the selected target"),
        ],
    ),
    ("Charts", &[("z", "Maximize to see the timeline in detail")]),
    (
        "Mouse",
        &[
            ("Click", "Focus a pane, select a row or tab"),
            (
                "Click ✓ ✘ ○",
                "Show only passed / failed / not-run tests (status bar)",
            ),
            ("Click a bar", "Select the test in the timeline"),
            ("Wheel", "Scroll the pane under the cursor"),
        ],
    ),
];

pub struct HelpWidget;

impl HelpWidget {
    fn lines() -> Vec<Line<'static>> {
        const KEY_WIDTH: usize = 18;
        let mut lines = vec![];
        for (n, (section, bindings)) in KEY_BINDINGS.iter().enumerate() {
            if n > 0 {
                lines.push(Line::default());
            }
            lines.push(Line::styled(
                section.to_string(),
                Style::new().fg(theme::ACCENT).bold(),
            ));
            for (key, description) in bindings.iter() {
                lines.push(Line::from(vec![
                    Span::styled(format!("  {key:<KEY_WIDTH$}"), Style::new().bold()),
                    Span::raw(description.to_string()),
                ]));
            }
        }
        lines
    }
}

impl Widget for HelpWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let lines = Self::lines();
        let width = lines
            .iter()
            .map(|line| line.width() as u16)
            .max()
            .unwrap_or_default()
            + 4;
        let height = lines.len() as u16 + 2;
        let popup = area.centered(
            Constraint::Length(width.min(area.width)),
            Constraint::Length(height.min(area.height)),
        );

        Clear.render(popup, buf);
        Paragraph::new(lines)
            .block(
                theme::block(" Key bindings ", true)
                    .title_bottom(Line::styled(" press ? or Esc to close ", muted()).centered())
                    .padding(ratatui::widgets::Padding::horizontal(1)),
            )
            .render(popup, buf);
    }
}
