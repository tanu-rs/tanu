use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

/// A custom tab bar widget with visual styling similar to AngryOxide
pub struct CustomTabs<'a> {
    tabs: Vec<&'a str>,
    selected: usize,
    selected_style: Style,
    unselected_style: Style,
}

impl<'a> CustomTabs<'a> {
    pub fn new(tabs: Vec<&'a str>) -> Self {
        Self {
            tabs,
            selected: 0,
            selected_style: Style::default().fg(Color::Blue).bold(),
            unselected_style: Style::default(),
        }
    }

    pub fn select(mut self, selected: usize) -> Self {
        self.selected = selected;
        self
    }

    pub fn selected_style(mut self, style: Style) -> Self {
        self.selected_style = style;
        self
    }
}

impl Widget for CustomTabs<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.area() == 0 {
            return;
        }

        let y = area.y;
        let mut x = area.x + 1; // Start with a small offset from left

        // Render each tab
        for (idx, name) in self.tabs.iter().enumerate() {
            let is_selected = idx == self.selected;
            let tab_width = name.len() as u16 + 4; // "  name  "

            if x + tab_width > area.right() {
                break;
            }

            let style = if is_selected {
                self.selected_style
            } else {
                self.unselected_style
            };

            // Render tab content with padding
            let content = format!("  {}  ", name);
            buf.set_string(x, y, &content, style);

            // Bottom border - fill with line for selected tab
            if is_selected {
                // Underline effect for selected tab (using heavy line for bold appearance)
                for i in 0..tab_width {
                    buf.set_string(x + i, y + 1, "━", style);
                }
            }

            x += tab_width + 1; // +1 for spacing between tabs
        }
    }
}
