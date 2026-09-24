use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

use crate::widget::theme;

/// Horizontal padding on each side of a tab label.
const TAB_PADDING: u16 = 2;
/// Space between tabs.
const TAB_GAP: u16 = 1;

/// A tab bar that underlines the selected tab.
pub struct CustomTabs<'a> {
    /// Tab labels with an optional badge (e.g. a count or an error marker).
    tabs: Vec<(String, Span<'a>)>,
    selected: usize,
    /// Indices of tabs to highlight as alerts (e.g. the Error tab of a failed test).
    alerts: Vec<usize>,
}

impl<'a> CustomTabs<'a> {
    pub fn new(tabs: Vec<(String, Span<'a>)>) -> Self {
        Self {
            tabs,
            selected: 0,
            alerts: vec![],
        }
    }

    pub fn select(mut self, selected: usize) -> Self {
        self.selected = selected;
        self
    }

    pub fn alert(mut self, alert: Option<usize>) -> Self {
        self.alerts = alert.into_iter().collect();
        self
    }

    pub fn alerts(mut self, alerts: Vec<usize>) -> Self {
        self.alerts = alerts;
        self
    }

    fn tab_width(label: &str, badge: &Span) -> u16 {
        Line::raw(label).width() as u16 + badge.width() as u16 + TAB_PADDING * 2
    }

    /// Horizontal ranges `(index, x_start, x_end)` occupied by each visible tab.
    pub fn hit_areas(&self, area: Rect) -> Vec<(usize, u16, u16)> {
        let mut x = area.x;
        let mut hits = vec![];
        for (idx, (label, badge)) in self.tabs.iter().enumerate() {
            let width = Self::tab_width(label, badge);
            if x + width > area.right() {
                break;
            }
            hits.push((idx, x, x + width));
            x += width + TAB_GAP;
        }
        hits
    }
}

impl Widget for CustomTabs<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.area() == 0 {
            return;
        }

        // Thin baseline under all tabs.
        if area.height > 1 {
            for x in area.left()..area.right() {
                buf.set_string(x, area.y + 1, "─", Style::new().fg(theme::border()));
            }
        }

        for (idx, start, end) in self.hit_areas(area) {
            let (label, badge) = &self.tabs[idx];
            let is_selected = idx == self.selected;
            let mut style = if is_selected {
                Style::new()
                    .fg(theme::accent())
                    .add_modifier(Modifier::BOLD)
            } else {
                theme::muted()
            };
            if self.alerts.contains(&idx) {
                style = style.fg(theme::fail());
            }

            let padding = " ".repeat(TAB_PADDING as usize);
            let line = Line::from(vec![
                Span::raw(padding.clone()),
                Span::styled(label.clone(), style),
                badge.clone(),
                Span::raw(padding),
            ]);
            buf.set_line(start, area.y, &line, end - start);

            // Underline the selected tab (heavy line for a bold appearance).
            if is_selected && area.height > 1 {
                for x in start..end {
                    buf.set_string(x, area.y + 1, "━", style);
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn hit_areas() {
        let tabs = CustomTabs::new(vec![
            ("Call".into(), Span::raw("")),
            ("Error".into(), Span::raw(" ●")),
        ]);
        let area = Rect::new(10, 0, 30, 2);
        // "  Call  " = 8, "  Error ●  " = 11
        assert_eq!(vec![(0, 10, 18), (1, 19, 30)], tabs.hit_areas(area));

        // Tabs that do not fit are not rendered.
        let area = Rect::new(0, 0, 12, 2);
        assert_eq!(vec![(0, 0, 8)], tabs.hit_areas(area));
    }
}
