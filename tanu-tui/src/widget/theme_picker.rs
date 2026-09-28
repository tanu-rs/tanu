//! Drop-down list to pick a color theme: the TUI theme (from the button at the
//! right of the status bar) or the payload theme (from the button of the
//! Payload tab).
//!
//! Moving the cursor previews the theme right away; `Enter` or a click keeps it,
//! `Esc` restores the theme that was active when the list was opened.

use ratatui::{
    prelude::*,
    widgets::{Clear, List, ListState},
};

use crate::widget::{
    info,
    theme::{self, selected_style},
};

/// Maximum number of themes shown at once.
const MAX_VISIBLE: u16 = 16;

/// Which theme a picker selects.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ThemeKind {
    /// The color theme of the TUI itself.
    #[default]
    Tui,
    /// The syntax-highlight theme of payloads.
    Payload,
}

impl ThemeKind {
    pub fn names(self) -> Vec<&'static str> {
        match self {
            ThemeKind::Tui => theme::names().collect(),
            ThemeKind::Payload => info::payload_themes().collect(),
        }
    }

    /// Name of the active theme.
    pub fn current_name(self) -> String {
        match self {
            ThemeKind::Tui => theme::current().name.to_string(),
            ThemeKind::Payload => info::payload_theme(),
        }
    }

    /// Width of the longest theme name.
    pub fn name_width(self) -> usize {
        self.names()
            .iter()
            .map(|n| n.len())
            .max()
            .unwrap_or_default()
    }

    fn current_index(self) -> Option<usize> {
        match self {
            ThemeKind::Tui => Some(theme::current_index()),
            ThemeKind::Payload => info::payload_theme_index(),
        }
    }

    fn set_index(self, index: usize) {
        match self {
            ThemeKind::Tui => theme::set_index(index),
            ThemeKind::Payload => info::set_payload_theme(index),
        }
    }

    fn title(self) -> &'static str {
        match self {
            ThemeKind::Tui => "Theme",
            ThemeKind::Payload => "Payload theme",
        }
    }
}

/// Button label that opens a picker, padded to the longest name so that the
/// button does not move when the theme changes.
pub fn button_label(kind: ThemeKind) -> Line<'static> {
    let width = kind.name_width();
    Line::from(format!(" ◐ {:<width$} ▾ ", kind.current_name())).style(
        Style::new()
            .fg(theme::on_accent())
            .bg(theme::accent())
            .bold(),
    )
}

#[derive(Debug, Default)]
pub struct ThemePickerState {
    /// true while the list is shown.
    pub open: bool,
    /// Which theme the open list selects.
    pub kind: ThemeKind,
    list_state: ListState,
    /// Theme index to restore on cancel.
    original: Option<usize>,
    /// Screen area of the button the list drops from.
    anchor: Rect,
    /// Screen area of the whole popup from the last render.
    popup_area: Rect,
    /// Screen area of the theme rows from the last render.
    list_area: Rect,
}

impl ThemePickerState {
    /// Opens the list next to `anchor` with the active theme selected.
    pub fn open(&mut self, kind: ThemeKind, anchor: Rect) {
        let current = kind.current_index();
        self.open = true;
        self.kind = kind;
        self.anchor = anchor;
        self.original = current;
        self.list_state = ListState::default().with_selected(Some(current.unwrap_or_default()));
    }

    /// Moves the cursor by `delta` rows and previews the theme under it.
    pub fn move_by(&mut self, delta: isize) {
        let len = self.kind.names().len();
        if len == 0 {
            return;
        }
        let selected = self.list_state.selected().unwrap_or_default() as isize;
        let next = (selected + delta).clamp(0, len as isize - 1) as usize;
        self.select(next);
    }

    /// Moves the cursor to the first (`false`) or last (`true`) theme.
    pub fn move_to_end(&mut self, last: bool) {
        let len = self.kind.names().len();
        self.select(if last { len.saturating_sub(1) } else { 0 });
    }

    fn select(&mut self, index: usize) {
        self.list_state.select(Some(index));
        self.kind.set_index(index);
    }

    /// Closes the list, keeping the previewed theme.
    pub fn confirm(&mut self) {
        self.open = false;
        self.original = None;
    }

    /// Closes the list and restores the theme active when it was opened.
    pub fn cancel(&mut self) {
        if let Some(original) = self.original.take() {
            self.kind.set_index(original);
        }
        self.open = false;
    }

    /// true if `position` is inside the popup.
    pub fn contains(&self, position: Position) -> bool {
        self.popup_area.contains(position)
    }

    /// Selects the theme at `position`, if there is one. Returns true if selected.
    pub fn click(&mut self, position: Position) -> bool {
        if !self.list_area.contains(position) {
            return false;
        }
        let index = self.list_state.offset() + (position.y - self.list_area.y) as usize;
        if index >= self.kind.names().len() {
            return false;
        }
        self.select(index);
        true
    }
}

pub struct ThemePickerWidget;

impl StatefulWidget for ThemePickerWidget {
    type State = ThemePickerState;

    /// Renders the list right-aligned to the button within `area`: below the
    /// button in the upper half of the screen, above it in the lower half.
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let themes = state.kind.names();
        let anchor = state.anchor;
        let footer = " ⏎ keep · Esc cancel ";
        let width = (state.kind.name_width() as u16 + 6)
            .max(footer.chars().count() as u16 + 2)
            .max(anchor.width)
            .min(area.width);
        let below = anchor.y < area.y + area.height / 2;
        let room = if below {
            area.bottom().saturating_sub(anchor.bottom())
        } else {
            anchor.y.saturating_sub(area.y)
        };
        let height = (themes.len() as u16 + 2).min(MAX_VISIBLE + 2).min(room);
        if height < 3 {
            state.popup_area = Rect::default();
            state.list_area = Rect::default();
            return;
        }
        let x = anchor.right().saturating_sub(width).max(area.x);
        let y = if below {
            anchor.bottom()
        } else {
            anchor.y - height
        };
        let popup = Rect::new(x, y, width, height);

        let title = Line::from(vec![
            Span::raw(state.kind.title()),
            Span::styled(
                format!(" {}/{}", selected_position(state), themes.len()),
                theme::muted(),
            ),
        ]);
        let block = theme::block(title, true)
            .title_bottom(Line::styled(footer, theme::muted()).right_aligned());
        let list_area = block.inner(popup);

        Clear.render(popup, buf);
        let list = List::new(themes.iter().map(|name| Line::raw(format!(" {name}"))))
            .style(theme::base_style())
            .block(block)
            .highlight_style(selected_style())
            .highlight_symbol(Line::styled("▌", Style::new().fg(theme::accent())));
        StatefulWidget::render(list, popup, buf, &mut state.list_state);

        state.popup_area = popup;
        state.list_area = list_area;
    }
}

/// 1-based position of the cursor, for the title.
fn selected_position(state: &ThemePickerState) -> usize {
    state.list_state.selected().map_or(0, |s| s + 1)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn cancel_restores_the_original_theme() {
        let _guard = info::test::PAYLOAD_THEME_LOCK.lock().unwrap();
        let anchor = Rect::new(60, 40, 30, 1);
        let mut state = ThemePickerState::default();
        info::set_payload_theme(0);
        let original = info::payload_theme();

        state.open(ThemeKind::Payload, anchor);
        state.move_by(3);
        assert_ne!(original, info::payload_theme());
        state.cancel();
        assert!(!state.open);
        assert_eq!(original, info::payload_theme());

        // Clicking a row selects it; confirm keeps it.
        let mut buf = Buffer::empty(Rect::new(0, 0, 100, 50));
        state.open(ThemeKind::Payload, anchor);
        ThemePickerWidget.render(buf.area, &mut buf, &mut state);
        let row = Position::new(state.list_area.x + 1, state.list_area.y + 2);
        assert!(state.click(row));
        state.confirm();
        let picked = info::payload_themes().nth(2).unwrap().to_string();
        assert_eq!(picked, info::payload_theme());
        info::set_payload_theme(0);
    }

    #[test]
    fn opens_below_a_button_at_the_top() {
        let area = Rect::new(0, 0, 100, 50);
        let anchor = Rect::new(70, 0, 30, 1);
        let mut state = ThemePickerState::default();
        state.open(ThemeKind::Tui, anchor);
        let mut buf = Buffer::empty(area);
        ThemePickerWidget.render(area, &mut buf, &mut state);
        assert_eq!(anchor.bottom(), state.popup_area.y);
        assert_eq!(anchor.right(), state.popup_area.right());
        assert_eq!(theme::THEMES.len() as u16 + 2, state.popup_area.height);
        // Closing without moving leaves the theme untouched.
        state.cancel();
        assert!(!state.open);
    }
}
