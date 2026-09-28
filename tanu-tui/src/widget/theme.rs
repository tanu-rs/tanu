//! Color themes and style helpers shared by all widgets.
//!
//! The active theme is global so widgets can read it without threading it through
//! every render function. It is chosen from `[tui] theme` in `tanu.toml` and can be
//! cycled at runtime.
use http::StatusCode;
use ratatui::{
    style::{palette::tailwind, Color, Modifier, Style},
    text::Line,
    widgets::{Block, BorderType},
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// A color palette for the TUI.
///
/// Green and red-ish colors are used only for success and failure; everything else
/// derives from the accent color.
#[derive(Debug)]
pub struct Theme {
    /// Name used in `tanu.toml` and shown in the help popup.
    pub name: &'static str,
    /// Background of the whole screen. `None` keeps the terminal background.
    pub bg: Option<Color>,
    /// Default text color. `None` keeps the terminal foreground.
    pub fg: Option<Color>,
    /// Primary accent color (titles, key hints, focused borders).
    pub accent: Color,
    /// Text drawn on top of an `accent`, `ok` or `fail` background (status badges).
    pub on_accent: Color,
    /// Border color of unfocused panes.
    pub border: Color,
    /// Secondary text such as metadata and hints.
    pub muted: Color,
    /// Successful test / request.
    pub ok: Color,
    /// Failed test / request.
    pub fail: Color,
    /// Running or retried test.
    pub running: Color,
    /// Chart bars of passed tests and successful calls.
    pub bar: Color,
    /// Slightly different shade of `bar` to tell adjacent timeline bars apart.
    pub bar_alt: Color,
    /// Chart bars of failed tests and error responses. Softer than `fail`: the test
    /// list and status bar already flag failures, and error responses are often what
    /// a test expects.
    pub bar_error: Color,
    /// Slightly different shade of `bar_error` to tell adjacent timeline bars apart.
    pub bar_error_alt: Color,
    /// Bars of the test selected in the test list.
    pub bar_selected: Color,
    /// Background of the selected list row.
    pub selected_bg: Color,
    /// Background of alternating table rows.
    pub row_alt_bg: Color,
}

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// The default palette. Uses the terminal background.
const PURPLE: Theme = Theme {
    name: "purple",
    bg: None,
    fg: None,
    accent: tailwind::VIOLET.c400,
    on_accent: Color::Black,
    border: rgb(0x4a3a7a),
    muted: rgb(0xa497cf),
    ok: rgb(0x7ec699),
    fail: rgb(0xe06c75),
    running: tailwind::VIOLET.c400,
    bar: tailwind::VIOLET.c400,
    bar_alt: tailwind::VIOLET.c500,
    bar_error: rgb(0xa8555f),
    bar_error_alt: rgb(0x8f4852),
    bar_selected: tailwind::VIOLET.c100,
    selected_bg: rgb(0x3b2a6e),
    row_alt_bg: rgb(0x1e1640),
};

/// The original blue palette. Uses the terminal background.
const BLUE: Theme = Theme {
    name: "blue",
    bg: None,
    fg: None,
    accent: tailwind::BLUE.c400,
    on_accent: Color::Black,
    border: rgb(0x2a4a7a),
    muted: rgb(0x7f9cc9),
    ok: rgb(0x7ec699),
    fail: rgb(0xe06c75),
    running: tailwind::BLUE.c400,
    bar: tailwind::BLUE.c400,
    bar_alt: tailwind::BLUE.c500,
    bar_error: rgb(0xa8555f),
    bar_error_alt: rgb(0x8f4852),
    bar_selected: tailwind::BLUE.c100,
    selected_bg: rgb(0x1e3a6e),
    row_alt_bg: rgb(0x102244),
};

const DRACULA: Theme = Theme {
    name: "dracula",
    bg: Some(rgb(0x282a36)),
    fg: Some(rgb(0xf8f8f2)),
    accent: rgb(0xbd93f9),
    on_accent: rgb(0x282a36),
    border: rgb(0x44475a),
    muted: rgb(0x7f8bbd),
    ok: rgb(0x50fa7b),
    fail: rgb(0xff5555),
    running: rgb(0xbd93f9),
    bar: rgb(0xbd93f9),
    bar_alt: rgb(0x9580d8),
    bar_error: rgb(0xc0525a),
    bar_error_alt: rgb(0xa3464d),
    bar_selected: rgb(0xf8f8f2),
    selected_bg: rgb(0x44475a),
    row_alt_bg: rgb(0x2f3241),
};

const NORD: Theme = Theme {
    name: "nord",
    bg: Some(rgb(0x2e3440)),
    fg: Some(rgb(0xd8dee9)),
    accent: rgb(0x88c0d0),
    on_accent: rgb(0x2e3440),
    border: rgb(0x4c566a),
    muted: rgb(0x8a94a7),
    ok: rgb(0xa3be8c),
    fail: rgb(0xbf616a),
    running: rgb(0x88c0d0),
    bar: rgb(0x88c0d0),
    bar_alt: rgb(0x81a1c1),
    bar_error: rgb(0xbf616a),
    bar_error_alt: rgb(0xa5545c),
    bar_selected: rgb(0xeceff4),
    selected_bg: rgb(0x434c5e),
    row_alt_bg: rgb(0x3b4252),
};

const GRUVBOX: Theme = Theme {
    name: "gruvbox",
    bg: Some(rgb(0x282828)),
    fg: Some(rgb(0xebdbb2)),
    accent: rgb(0xfabd2f),
    on_accent: rgb(0x282828),
    border: rgb(0x504945),
    muted: rgb(0xa89984),
    ok: rgb(0xb8bb26),
    fail: rgb(0xfb4934),
    running: rgb(0xfabd2f),
    bar: rgb(0xfabd2f),
    bar_alt: rgb(0xd79921),
    bar_error: rgb(0xcc241d),
    bar_error_alt: rgb(0x9d0006),
    bar_selected: rgb(0xfbf1c7),
    selected_bg: rgb(0x504945),
    row_alt_bg: rgb(0x32302f),
};

const CATPPUCCIN_MOCHA: Theme = Theme {
    name: "catppuccin-mocha",
    bg: Some(rgb(0x1e1e2e)),
    fg: Some(rgb(0xcdd6f4)),
    accent: rgb(0xcba6f7),
    on_accent: rgb(0x1e1e2e),
    border: rgb(0x585b70),
    muted: rgb(0x9399b2),
    ok: rgb(0xa6e3a1),
    fail: rgb(0xf38ba8),
    running: rgb(0xcba6f7),
    bar: rgb(0xcba6f7),
    bar_alt: rgb(0xb4befe),
    bar_error: rgb(0xc46f87),
    bar_error_alt: rgb(0xa85d73),
    bar_selected: rgb(0xf5e0dc),
    selected_bg: rgb(0x45475a),
    row_alt_bg: rgb(0x262637),
};

const TOKYO_NIGHT: Theme = Theme {
    name: "tokyo-night",
    bg: Some(rgb(0x1a1b26)),
    fg: Some(rgb(0xc0caf5)),
    accent: rgb(0x7aa2f7),
    on_accent: rgb(0x1a1b26),
    border: rgb(0x3b4261),
    muted: rgb(0x737aa2),
    ok: rgb(0x9ece6a),
    fail: rgb(0xf7768e),
    running: rgb(0x7aa2f7),
    bar: rgb(0x7aa2f7),
    bar_alt: rgb(0x5f86d6),
    bar_error: rgb(0xc95f73),
    bar_error_alt: rgb(0xa84f60),
    bar_selected: rgb(0xc0caf5),
    selected_bg: rgb(0x283457),
    row_alt_bg: rgb(0x1f2335),
};

const ONE_DARK: Theme = Theme {
    name: "one-dark",
    bg: Some(rgb(0x282c34)),
    fg: Some(rgb(0xabb2bf)),
    accent: rgb(0x61afef),
    on_accent: rgb(0x282c34),
    border: rgb(0x3e4451),
    muted: rgb(0x7f848e),
    ok: rgb(0x98c379),
    fail: rgb(0xe06c75),
    running: rgb(0x61afef),
    bar: rgb(0x61afef),
    bar_alt: rgb(0x4d8fd1),
    bar_error: rgb(0xb85a62),
    bar_error_alt: rgb(0x9a4c53),
    bar_selected: rgb(0xe6e6e6),
    selected_bg: rgb(0x3e4451),
    row_alt_bg: rgb(0x2c313a),
};

const MONOKAI: Theme = Theme {
    name: "monokai",
    bg: Some(rgb(0x272822)),
    fg: Some(rgb(0xf8f8f2)),
    accent: rgb(0x66d9ef),
    on_accent: rgb(0x272822),
    border: rgb(0x49483e),
    muted: rgb(0x908b73),
    ok: rgb(0xa6e22e),
    fail: rgb(0xf92672),
    running: rgb(0x66d9ef),
    bar: rgb(0x66d9ef),
    bar_alt: rgb(0x4fb3c8),
    bar_error: rgb(0xc7305f),
    bar_error_alt: rgb(0xa6284f),
    bar_selected: rgb(0xf8f8f2),
    selected_bg: rgb(0x49483e),
    row_alt_bg: rgb(0x2f302a),
};

const SOLARIZED_DARK: Theme = Theme {
    name: "solarized-dark",
    bg: Some(rgb(0x002b36)),
    fg: Some(rgb(0x93a1a1)),
    accent: rgb(0x268bd2),
    on_accent: rgb(0x002b36),
    border: rgb(0x073642),
    muted: rgb(0x839496),
    ok: rgb(0x859900),
    fail: rgb(0xdc322f),
    running: rgb(0x268bd2),
    bar: rgb(0x268bd2),
    bar_alt: rgb(0x2176b3),
    bar_error: rgb(0xb3352f),
    bar_error_alt: rgb(0x942c27),
    bar_selected: rgb(0xeee8d5),
    selected_bg: rgb(0x073642),
    row_alt_bg: rgb(0x01313d),
};

const EVERFOREST: Theme = Theme {
    name: "everforest",
    bg: Some(rgb(0x2d353b)),
    fg: Some(rgb(0xd3c6aa)),
    accent: rgb(0x7fbbb3),
    on_accent: rgb(0x2d353b),
    border: rgb(0x475258),
    muted: rgb(0x859289),
    ok: rgb(0xa7c080),
    fail: rgb(0xe67e80),
    running: rgb(0x7fbbb3),
    bar: rgb(0x7fbbb3),
    bar_alt: rgb(0x6a9d96),
    bar_error: rgb(0xb86566),
    bar_error_alt: rgb(0x9a5556),
    bar_selected: rgb(0xd3c6aa),
    selected_bg: rgb(0x3d484d),
    row_alt_bg: rgb(0x343f44),
};

const KANAGAWA: Theme = Theme {
    name: "kanagawa",
    bg: Some(rgb(0x1f1f28)),
    fg: Some(rgb(0xdcd7ba)),
    accent: rgb(0x7e9cd8),
    on_accent: rgb(0x1f1f28),
    border: rgb(0x54546d),
    muted: rgb(0x727169),
    ok: rgb(0x98bb6c),
    fail: rgb(0xe46876),
    running: rgb(0x7e9cd8),
    bar: rgb(0x7e9cd8),
    bar_alt: rgb(0x6a84bd),
    bar_error: rgb(0xb55a66),
    bar_error_alt: rgb(0x984c56),
    bar_selected: rgb(0xdcd7ba),
    selected_bg: rgb(0x2d4f67),
    row_alt_bg: rgb(0x2a2a37),
};

const ROSE_PINE: Theme = Theme {
    name: "rose-pine",
    bg: Some(rgb(0x191724)),
    fg: Some(rgb(0xe0def4)),
    accent: rgb(0xc4a7e7),
    on_accent: rgb(0x191724),
    border: rgb(0x403d52),
    muted: rgb(0x908caa),
    ok: rgb(0x9ccfd8),
    fail: rgb(0xeb6f92),
    running: rgb(0xc4a7e7),
    bar: rgb(0xc4a7e7),
    bar_alt: rgb(0xa88fce),
    bar_error: rgb(0xbd5b78),
    bar_error_alt: rgb(0xa04d66),
    bar_selected: rgb(0xe0def4),
    selected_bg: rgb(0x403d52),
    row_alt_bg: rgb(0x1f1d2e),
};

const GITHUB_DARK: Theme = Theme {
    name: "github-dark",
    bg: Some(rgb(0x0d1117)),
    fg: Some(rgb(0xe6edf3)),
    accent: rgb(0x58a6ff),
    on_accent: rgb(0x0d1117),
    border: rgb(0x30363d),
    muted: rgb(0x8b949e),
    ok: rgb(0x3fb950),
    fail: rgb(0xf85149),
    running: rgb(0x58a6ff),
    bar: rgb(0x58a6ff),
    bar_alt: rgb(0x3d8be0),
    bar_error: rgb(0xc24a44),
    bar_error_alt: rgb(0xa13e39),
    bar_selected: rgb(0xe6edf3),
    selected_bg: rgb(0x1c2d41),
    row_alt_bg: rgb(0x161b22),
};

/// Light theme for light terminals.
const CATPPUCCIN_LATTE: Theme = Theme {
    name: "catppuccin-latte",
    bg: Some(rgb(0xeff1f5)),
    fg: Some(rgb(0x4c4f69)),
    accent: rgb(0x1e66f5),
    on_accent: rgb(0xeff1f5),
    border: rgb(0xbcc0cc),
    muted: rgb(0x7c7f93),
    ok: rgb(0x40a02b),
    fail: rgb(0xd20f39),
    running: rgb(0x1e66f5),
    bar: rgb(0x1e66f5),
    bar_alt: rgb(0x7287fd),
    bar_error: rgb(0xe64553),
    bar_error_alt: rgb(0xc43a4a),
    bar_selected: rgb(0x4c4f69),
    selected_bg: rgb(0xccd0da),
    row_alt_bg: rgb(0xe6e9ef),
};

/// Light theme for light terminals.
const SOLARIZED_LIGHT: Theme = Theme {
    name: "solarized-light",
    bg: Some(rgb(0xfdf6e3)),
    fg: Some(rgb(0x586e75)),
    accent: rgb(0x268bd2),
    on_accent: rgb(0xfdf6e3),
    border: rgb(0xd6cfb9),
    muted: rgb(0x657b83),
    ok: rgb(0x859900),
    fail: rgb(0xdc322f),
    running: rgb(0x268bd2),
    bar: rgb(0x268bd2),
    bar_alt: rgb(0x6c71c4),
    bar_error: rgb(0xcb4b16),
    bar_error_alt: rgb(0xb3352f),
    bar_selected: rgb(0x073642),
    selected_bg: rgb(0xeee8d5),
    row_alt_bg: rgb(0xf5eedb),
};

/// Light theme for light terminals.
const GITHUB_LIGHT: Theme = Theme {
    name: "github-light",
    bg: Some(rgb(0xffffff)),
    fg: Some(rgb(0x1f2328)),
    accent: rgb(0x0969da),
    on_accent: rgb(0xffffff),
    border: rgb(0xd0d7de),
    muted: rgb(0x59636e),
    ok: rgb(0x1a7f37),
    fail: rgb(0xcf222e),
    running: rgb(0x0969da),
    bar: rgb(0x0969da),
    bar_alt: rgb(0x218bff),
    bar_error: rgb(0xd1242f),
    bar_error_alt: rgb(0xa40e26),
    bar_selected: rgb(0x1f2328),
    selected_bg: rgb(0xddf4ff),
    row_alt_bg: rgb(0xf6f8fa),
};

/// Built-in themes. The first one is the default.
pub static THEMES: &[Theme] = &[
    PURPLE,
    BLUE,
    DRACULA,
    NORD,
    GRUVBOX,
    CATPPUCCIN_MOCHA,
    TOKYO_NIGHT,
    ONE_DARK,
    MONOKAI,
    SOLARIZED_DARK,
    EVERFOREST,
    KANAGAWA,
    ROSE_PINE,
    GITHUB_DARK,
    // Light themes last.
    CATPPUCCIN_LATTE,
    SOLARIZED_LIGHT,
    GITHUB_LIGHT,
];

/// Index into `THEMES` of the active theme.
static CURRENT: AtomicUsize = AtomicUsize::new(0);

/// The active theme.
pub fn current() -> &'static Theme {
    &THEMES[CURRENT.load(Ordering::Relaxed) % THEMES.len()]
}

/// Index into `THEMES` of the active theme.
pub fn current_index() -> usize {
    CURRENT.load(Ordering::Relaxed) % THEMES.len()
}

/// Activates the theme at `index` of `THEMES`; out-of-range indices are ignored.
pub fn set_index(index: usize) {
    if index < THEMES.len() {
        CURRENT.store(index, Ordering::Relaxed);
    }
}

/// Names of the built-in themes.
pub fn names() -> impl Iterator<Item = &'static str> {
    THEMES.iter().map(|t| t.name)
}

/// Activates the theme called `name`, ignoring case and `-` vs `_`.
/// Returns `false` and keeps the current theme if there is no such theme.
pub fn set_by_name(name: &str) -> bool {
    let name = name.trim().to_lowercase().replace('_', "-");
    match THEMES.iter().position(|t| t.name == name) {
        Some(i) => {
            CURRENT.store(i, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

/// Activates the next theme, wrapping around, and returns it.
pub fn cycle() -> &'static Theme {
    let next = (CURRENT.load(Ordering::Relaxed) + 1) % THEMES.len();
    CURRENT.store(next, Ordering::Relaxed);
    &THEMES[next]
}

pub fn accent() -> Color {
    current().accent
}

pub fn on_accent() -> Color {
    current().on_accent
}

pub fn border() -> Color {
    current().border
}

pub fn ok() -> Color {
    current().ok
}

pub fn fail() -> Color {
    current().fail
}

pub fn running() -> Color {
    current().running
}

pub fn bar() -> Color {
    current().bar
}

pub fn bar_alt() -> Color {
    current().bar_alt
}

pub fn bar_error() -> Color {
    current().bar_error
}

pub fn bar_error_alt() -> Color {
    current().bar_error_alt
}

pub fn bar_selected() -> Color {
    current().bar_selected
}

pub fn selected_bg() -> Color {
    current().selected_bg
}

pub fn row_alt_bg() -> Color {
    current().row_alt_bg
}

/// Background for rows that are not highlighted; the terminal background by default.
pub fn bg() -> Color {
    current().bg.unwrap_or(Color::Reset)
}

/// Base style of the whole screen: the theme's background and text color.
pub fn base_style() -> Style {
    let theme = current();
    let mut style = Style::new();
    if let Some(bg) = theme.bg {
        style = style.bg(bg);
    }
    if let Some(fg) = theme.fg {
        style = style.fg(fg);
    }
    style
}

/// Style of the selected row in lists.
pub fn selected_style() -> Style {
    Style::new().bg(selected_bg()).add_modifier(Modifier::BOLD)
}

/// Style for secondary text.
pub fn muted() -> Style {
    Style::new().fg(current().muted)
}

/// Border color of a pane according to the focus state.
pub fn border_style(focused: bool) -> Style {
    Style::new().fg(if focused { accent() } else { border() })
}

/// Bordered block styled according to the focus state.
pub fn block<'a>(title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
    // Focus is shown by color only; a thick border looks too heavy.
    let border_style = border_style(focused);
    let title: Line = title.into();
    let title = if focused {
        title.patch_style(Style::new().fg(accent()).bold())
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
        200..=299 => ok(),
        400..=599 => fail(),
        _ => accent(),
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

    #[test]
    fn theme_names_are_unique() {
        let mut names = names().collect::<Vec<_>>();
        names.sort();
        names.dedup();
        assert_eq!(THEMES.len(), names.len());
    }

    /// WCAG contrast ratio of two colors.
    fn contrast(a: Color, b: Color) -> f64 {
        fn luminance(color: Color) -> f64 {
            let Color::Rgb(r, g, b) = color else {
                panic!("expected an RGB color, got {color:?}");
            };
            let linear = |c: u8| {
                let c = c as f64 / 255.0;
                if c <= 0.03928 {
                    c / 12.92
                } else {
                    ((c + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
        }
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    /// Badge text and muted text stay readable (3:1, the WCAG minimum for bold text).
    #[test]
    fn themes_have_readable_contrast() {
        for theme in THEMES.iter().filter(|t| t.bg.is_some()) {
            for (what, badge) in [
                ("accent", theme.accent),
                ("ok", theme.ok),
                ("fail", theme.fail),
            ] {
                let ratio = contrast(theme.on_accent, badge);
                assert!(
                    ratio >= 2.95,
                    "{}: on_accent on {what} is {ratio:.2}",
                    theme.name
                );
            }
            let ratio = contrast(theme.muted, theme.bg.unwrap());
            assert!(ratio >= 2.95, "{}: muted on bg is {ratio:.2}", theme.name);
        }
    }

    // The active theme is global, so everything that changes it runs in one test.
    #[test]
    fn select_and_cycle_themes() {
        assert!(set_by_name("Catppuccin_Mocha"));
        assert_eq!("catppuccin-mocha", current().name);

        assert!(!set_by_name("no-such-theme"));
        assert_eq!("catppuccin-mocha", current().name);

        assert!(set_by_name(THEMES.last().unwrap().name));
        assert_eq!(THEMES[0].name, cycle().name);
        assert_eq!(THEMES[1].name, cycle().name);

        assert!(set_by_name("purple"));
        assert_eq!(THEMES[0].name, current().name);
    }
}
