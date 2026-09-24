//! # Tanu TUI
//!
//! `tanu-tui` is a terminal-based user interface application for managing and executing tests
//! using the `tanu` framework. It is implemented using the ratatui library and follows the
//! Elm Architecture, which divides the logic into Model, Update, and View components. The
//! application has a status bar, a test tree, a details pane for the selected item (overview,
//! request/response, headers, payload, checks and errors), a logger, and charts of the
//! execution timeline and request latencies. It supports asynchronous test execution and user interaction via keyboard and mouse.
//!
//! ## UI Architecture (block diagram)
//!
//! ```text
//! +-------------------+     +-------------------+     +-------------------+
//! | Inputs            | --> | Update (Message)  | --> | Model (state)     |
//! | keys/mouse/events |     | command dispatch  |     | tests/results/log |
//! +-------------------+     +-------------------+     +-------------------+
//!          ^                                                   |
//!          |                                                   v
//!     +-------------------+ <-------- render/view -------- +-------------------+
//!     | Terminal frame    |                                | Widgets           |
//!     | layout/panes      |                                | List/Info/Logger  |
//!     +-------------------+                                +-------------------+
//!
//! Runner events --------> Model (results/logs) -> Info/Logger widgets
//! ```
mod widget;

use crossterm::event::{EventStream, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use eyre::WrapErr;
use futures::StreamExt;
use ratatui::{
    crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind},
    layout::Position,
    prelude::*,
    style::{Modifier, Style},
    text::Line,
    widgets::{BorderType, LineGauge, Paragraph},
    Frame,
};
use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant, SystemTime},
};
use tanu_core::{
    get_tanu_config,
    runner::{self, EventBody},
    Runner, TestInfo,
};
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, trace, warn};
use tracing_subscriber::layer::SubscriberExt;
use tui_logger::{TuiLoggerLevelOutput, TuiLoggerSmartWidget, TuiWidgetEvent, TuiWidgetState};

use crate::widget::{
    help::HelpWidget,
    info::{InfoState, InfoWidget},
    latency,
    list::{
        ExecutionStateController, RowRef, StatusFilter, TestCaseSelector, TestListState,
        TestListWidget,
    },
    theme::{self, fmt_duration, muted},
    timeline,
};

/// Represents result of a test case.
#[derive(Default, Clone, Debug)]
pub struct TestResult {
    pub project_name: String,
    pub module_name: String,
    pub name: String,
    pub logs: Vec<Box<tanu_core::http::Log>>,
    #[cfg(feature = "grpc")]
    pub grpc_logs: Vec<Box<tanu_core::grpc::Log>>,
    /// Checks (assertions) evaluated during the test.
    pub checks: Vec<tanu_core::runner::Check>,
    /// Number of times the test was retried.
    pub retries: usize,
    pub test: Option<tanu_core::runner::Test>,
}

impl TestResult {
    /// Unique test name including project and module names
    pub fn unique_name(&self) -> String {
        format!("{}::{}::{}", self.project_name, self.module_name, self.name)
    }

    /// true if the test finished successfully.
    pub fn is_ok(&self) -> bool {
        self.test.as_ref().is_some_and(|test| test.result.is_ok())
    }

    /// Wall-clock duration of the test including retries.
    pub fn duration(&self) -> Option<Duration> {
        self.test.as_ref().map(|test| test.request_time)
    }

    /// Number of HTTP and gRPC calls.
    pub fn call_count(&self) -> usize {
        #[cfg(feature = "grpc")]
        let grpc_count = self.grpc_logs.len();
        #[cfg(not(feature = "grpc"))]
        let grpc_count = 0;
        self.logs.len() + grpc_count
    }

    /// The call at `index`; HTTP calls come first, then gRPC calls.
    pub fn call(&self, index: usize) -> Option<Call<'_>> {
        if let Some(log) = self.logs.get(index) {
            return Some(Call::Http(log));
        }
        #[cfg(feature = "grpc")]
        if let Some(log) = self.grpc_logs.get(index - self.logs.len()) {
            return Some(Call::Grpc(log));
        }
        None
    }

    /// All calls; HTTP calls come first, then gRPC calls.
    pub fn calls(&self) -> impl Iterator<Item = Call<'_>> {
        (0..self.call_count()).filter_map(|index| self.call(index))
    }
}

/// A single HTTP or gRPC call made by a test.
#[derive(Debug, Clone, Copy)]
pub enum Call<'a> {
    Http(&'a tanu_core::http::Log),
    #[cfg(feature = "grpc")]
    Grpc(&'a tanu_core::grpc::Log),
}

impl Call<'_> {
    pub fn method(&self) -> String {
        match self {
            Call::Http(log) => log.request.method.to_string(),
            #[cfg(feature = "grpc")]
            Call::Grpc(_) => "gRPC".into(),
        }
    }

    pub fn status_label(&self) -> String {
        match self {
            Call::Http(log) => log.response.status.as_u16().to_string(),
            #[cfg(feature = "grpc")]
            Call::Grpc(log) => format!("{:?}", log.response.status_code),
        }
    }

    pub fn status_color(&self) -> Color {
        match self {
            Call::Http(log) => theme::status_color(log.response.status),
            #[cfg(feature = "grpc")]
            Call::Grpc(log) => {
                if log.response.status_code == tonic::Code::Ok {
                    theme::OK
                } else {
                    theme::FAIL
                }
            }
        }
    }

    /// URL path and query for HTTP, method path for gRPC.
    pub fn target(&self) -> String {
        match self {
            Call::Http(log) => {
                let url = &log.request.url;
                match url.query() {
                    Some(query) => format!("{}?{query}", url.path()),
                    None => url.path().to_string(),
                }
            }
            #[cfg(feature = "grpc")]
            Call::Grpc(log) => log.request.method.clone(),
        }
    }

    /// true if the call failed (HTTP 4xx/5xx or a non-OK gRPC status).
    pub fn is_error(&self) -> bool {
        match self {
            Call::Http(log) => {
                log.response.status.is_client_error() || log.response.status.is_server_error()
            }
            #[cfg(feature = "grpc")]
            Call::Grpc(log) => log.response.status_code != tonic::Code::Ok,
        }
    }

    pub fn duration(&self) -> Duration {
        match self {
            Call::Http(log) => log.response.duration_req,
            #[cfg(feature = "grpc")]
            Call::Grpc(log) => log.response.duration,
        }
    }
}

#[derive(
    Debug, Clone, Copy, Default, Eq, PartialEq, strum::FromRepr, strum::EnumString, strum::Display,
)]
enum Pane {
    #[default]
    List,
    Info,
    Logger,
    Chart,
}

/// Represents cursor movement.
#[derive(Debug, Clone, Copy)]
enum CursorMovement {
    /// Move the cursor up by one line.
    Up,
    /// Move the cursor down by one line.
    Down,
    /// Move the cursor up by half of the pane height.
    UpHalfScreen,
    /// Move the cursor down by half of the pane height.
    DownHalfScreen,
    /// Move the cursor to the first line.
    Home,
    /// Move the cursor to the last line.
    End,
}

/// Represents tab movement.
#[derive(Debug, Clone, Copy)]
enum TabMovement {
    /// Move tab to the next.
    Next,
    /// Move tab to the previous.
    Prev,
}

/// Statistics of the latest run.
#[derive(Debug, Default)]
struct RunStats {
    started_at: Option<Instant>,
    /// Wall-clock start of the run, to select the tests of this run for the timeline.
    started_system: Option<SystemTime>,
    finished_at: Option<Instant>,
    /// Number of tests scheduled in the run.
    total: usize,
    /// Number of tests finished in the run.
    done: usize,
    /// Number of tests failed in the run.
    failed: usize,
    /// Number of retries in the run.
    retries: usize,
}

impl RunStats {
    fn start(&mut self, total: usize) {
        *self = RunStats {
            started_at: Some(Instant::now()),
            started_system: Some(SystemTime::now()),
            total,
            ..Default::default()
        };
    }

    fn elapsed(&self) -> Option<Duration> {
        let started_at = self.started_at?;
        Some(
            self.finished_at
                .unwrap_or_else(Instant::now)
                .duration_since(started_at),
        )
    }

    fn is_running(&self) -> bool {
        self.started_at.is_some() && self.finished_at.is_none()
    }
}

/// Screen areas of the panes from the last render, used for mouse handling.
#[derive(Debug, Default, Clone)]
struct Areas {
    list: Rect,
    info: Rect,
    logger: Rect,
    chart: Rect,
    /// Clickable counters in the status bar that filter the test list.
    filter_hits: Vec<(Rect, StatusFilter)>,
    /// Clickable timeline bars with the index into `Model::test_results`.
    timeline_hits: Vec<(Rect, usize)>,
}

/// Represents the state of the application, including the current pane, execution state, test cases, and UI components' states.
struct Model {
    /// Indicates whether the current pane is in maximized view mode
    maximizing: bool,
    /// Keeps track of which pane (List, Info, Logger) is currently focused
    current_pane: Pane,
    /// Manages the selection state for the list of test cases
    test_cases_list: TestListState,
    /// Contains the results of executed tests, including logs and the test itself
    test_results: Vec<TestResult>,
    /// Position of each result in `test_results` by `TestResult::unique_name`.
    result_index: HashMap<String, usize>,
    /// Maintains the state of the info pane, such as currently selected tab.
    info_state: InfoState,
    /// Holds the state of the logger pane, including any focus or visibility settings
    logger_state: TuiWidgetState,
    /// Whether the key bindings popup is shown.
    show_help: bool,
    /// Statistics of the latest run.
    run: RunStats,
    /// Screen areas from the last render.
    areas: Areas,
    /// Measures the frames per second (FPS). Only enabled with `TANU_TUI_DEBUG`.
    fps_counter: Option<FpsCounter>,
}

impl Model {
    fn new(test_cases: Vec<TestInfo>) -> Model {
        let cfg = get_tanu_config();
        let logger_state = TuiWidgetState::new();
        // Hide the log target selector by default; it can be toggled with `L`.
        logger_state.transition(TuiWidgetEvent::HideKey);
        Model {
            maximizing: false,
            current_pane: Pane::default(),
            test_cases_list: TestListState::new(&cfg.projects, &test_cases),
            test_results: vec![],
            result_index: HashMap::new(),
            info_state: InfoState::new(),
            logger_state,
            show_help: false,
            run: RunStats::default(),
            areas: Areas::default(),
            fps_counter: std::env::var("TANU_TUI_DEBUG")
                .is_ok()
                .then(FpsCounter::new),
        }
    }

    /// Stores a test result, replacing the result of a previous run of the same test.
    fn store_result(&mut self, result: TestResult) {
        match self.result_index.get(&result.unique_name()) {
            Some(&index) => self.test_results[index] = result,
            None => {
                self.result_index
                    .insert(result.unique_name(), self.test_results.len());
                self.test_results.push(result);
            }
        }
    }

    fn focus(&mut self, pane: Pane) {
        self.current_pane = pane;
        self.info_state.focused = pane == Pane::Info;
    }

    fn next_pane(&mut self) {
        let pane_counts = Pane::Chart as usize + 1;
        let next_index = (self.current_pane as usize + 1) % pane_counts;
        self.focus(Pane::from_repr(next_index).unwrap_or_default());
    }

    fn prev_pane(&mut self) {
        let pane_counts = Pane::Chart as usize + 1;
        let prev_index = (self.current_pane as usize + pane_counts - 1) % pane_counts;
        self.focus(Pane::from_repr(prev_index).unwrap_or_default());
    }

    /// Keeps the list selection valid and the details pane in sync with it.
    fn sync_selection(&mut self) {
        self.test_cases_list.clamp_selection();
        self.info_state.select(self.test_cases_list.selected_row());
    }

    /// Half of the height of the given pane, for Ctrl+U/Ctrl+D.
    fn half_page(area: Rect) -> usize {
        (area.height.saturating_sub(2) / 2).max(1) as usize
    }
}

#[derive(Debug)]
enum Message {
    Quit,
    Maximize,
    NextPane,
    PrevPane,
    ToggleHelp,
    ListSelect(CursorMovement),
    ListExpand,
    ListCollapseOrParent,
    ListExpandOrChild,
    /// Show the next (1) or previous (-1) project tab.
    SwitchProject(isize),
    InfoScroll(CursorMovement),
    InfoTabSelect(TabMovement),
    Logger(TuiWidgetEvent),
    ExecuteOne,
    ExecuteAll,
    SearchStart,
    SearchInput(char),
    SearchBackspace,
    /// Leave the search mode; `true` keeps the query as a filter.
    SearchEnd(bool),
    ClearFilters,
    CycleStatusFilter,
    JumpToFailure {
        forward: bool,
    },
    Mouse(MouseEvent),
}

#[derive(Debug)]
enum Command {
    ExecuteOne(TestCaseSelector),
    ExecuteAll,
}

fn update(model: &mut Model, msg: Message) -> Option<Command> {
    let list = &mut model.test_cases_list;
    match msg {
        Message::Quit => {}
        Message::Maximize => {
            model.maximizing = !model.maximizing;
        }
        Message::NextPane => model.next_pane(),
        Message::PrevPane => model.prev_pane(),
        Message::ToggleHelp => model.show_help = !model.show_help,
        Message::ListSelect(movement) => {
            let half_page = Model::half_page(model.areas.list);
            let selected = list.list_state.selected().unwrap_or_default();
            match movement {
                CursorMovement::Down => list.list_state.select_next(),
                CursorMovement::Up => list.list_state.select_previous(),
                CursorMovement::UpHalfScreen => {
                    list.list_state
                        .select(Some(selected.saturating_sub(half_page)));
                }
                CursorMovement::DownHalfScreen => {
                    list.list_state.select(Some(selected + half_page));
                }
                CursorMovement::Home => list.list_state.select_first(),
                CursorMovement::End => {
                    let len = list.visible_rows().len();
                    list.list_state.select(Some(len.saturating_sub(1)));
                }
            }
        }
        Message::ListExpand => list.expand(),
        Message::ListCollapseOrParent => list.collapse_or_parent(),
        Message::ListExpandOrChild => list.expand_or_child(),
        Message::SwitchProject(delta) => list.cycle_project(delta),
        Message::InfoScroll(movement) => {
            let half_page = Model::half_page(model.areas.info) as u16;
            let info = &mut model.info_state;
            match movement {
                CursorMovement::Down => info.scroll_down(1),
                CursorMovement::Up => info.scroll_up(1),
                CursorMovement::DownHalfScreen => info.scroll_down(half_page),
                CursorMovement::UpHalfScreen => info.scroll_up(half_page),
                CursorMovement::Home => info.scroll_home(),
                CursorMovement::End => info.scroll_end(),
            }
        }
        Message::InfoTabSelect(TabMovement::Next) => model.info_state.next_tab(),
        Message::InfoTabSelect(TabMovement::Prev) => model.info_state.prev_tab(),
        Message::Logger(event) => model.logger_state.transition(event),
        Message::ExecuteOne => {
            let selector = list.select_test_case()?;
            ExecutionStateController::execute_specified(list, &selector);
            model.run.start(list.counts().running);
            return Some(Command::ExecuteOne(selector));
        }
        Message::ExecuteAll => {
            model.test_results.clear();
            model.result_index.clear();
            ExecutionStateController::execute_all(list);
            model.run.start(list.counts().running);
            return Some(Command::ExecuteAll);
        }
        Message::SearchStart => {
            model.focus(Pane::List);
            let list = &mut model.test_cases_list;
            list.searching = true;
            list.search.clear();
            list.list_state.select_first();
        }
        Message::SearchInput(c) => {
            list.search.push(c);
            list.list_state.select_first();
        }
        Message::SearchBackspace => {
            list.search.pop();
            list.list_state.select_first();
        }
        Message::SearchEnd(keep) => {
            list.searching = false;
            if !keep {
                list.search.clear();
            }
        }
        Message::ClearFilters => {
            list.search.clear();
            list.status_filter = StatusFilter::All;
        }
        Message::CycleStatusFilter => {
            list.status_filter = list.status_filter.next();
            list.list_state.select_first();
        }
        Message::JumpToFailure { forward } => {
            model.focus(Pane::List);
            model.test_cases_list.jump_to_failure(forward);
        }
        Message::Mouse(mouse) => handle_mouse(model, mouse),
    }

    model.sync_selection();
    None
}

fn handle_mouse(model: &mut Model, mouse: MouseEvent) {
    const WHEEL_STEP: usize = 3;
    let position = Position::new(mouse.column, mouse.row);
    let areas = model.areas.clone();
    match mouse.kind {
        MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
            let down = mouse.kind == MouseEventKind::ScrollDown;
            if areas.list.contains(position) {
                let list = &mut model.test_cases_list;
                let selected = list.list_state.selected().unwrap_or_default();
                list.list_state.select(Some(if down {
                    selected + WHEEL_STEP
                } else {
                    selected.saturating_sub(WHEEL_STEP)
                }));
            } else if areas.info.contains(position) {
                if down {
                    model.info_state.scroll_down(WHEEL_STEP as u16);
                } else {
                    model.info_state.scroll_up(WHEEL_STEP as u16);
                }
            } else if areas.logger.contains(position) {
                model.logger_state.transition(if down {
                    TuiWidgetEvent::NextPageKey
                } else {
                    TuiWidgetEvent::PrevPageKey
                });
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some((_, filter)) = areas
                .filter_hits
                .iter()
                .find(|(area, _)| area.contains(position))
            {
                // Clicking a counter shows only those tests; clicking it again shows all.
                let list = &mut model.test_cases_list;
                list.status_filter = if list.status_filter == *filter {
                    StatusFilter::All
                } else {
                    *filter
                };
                list.list_state.select_first();
                model.focus(Pane::List);
            } else if areas.list.contains(position) {
                model.focus(Pane::List);
                let list = &mut model.test_cases_list;
                if let Some(p) = list.tab_at(mouse.column, mouse.row) {
                    list.show_project(p);
                } else if let Some(row) = mouse
                    .row
                    .checked_sub(list.list_area.y)
                    .filter(|_| list.list_area.contains(position))
                {
                    let index = list.list_state.offset() + row as usize;
                    if index < list.visible_rows().len() {
                        if list.list_state.selected() == Some(index) {
                            list.expand();
                        } else {
                            list.select_index(index);
                        }
                    }
                }
            } else if areas.info.contains(position) {
                model.focus(Pane::Info);
                if let Some(tab) = model.info_state.tab_at(mouse.column, mouse.row) {
                    model.info_state.selected_tab = tab;
                }
            } else if areas.logger.contains(position) {
                model.focus(Pane::Logger);
            } else if areas.chart.contains(position) {
                model.focus(Pane::Chart);
                if let Some(result) = areas
                    .timeline_hits
                    .iter()
                    .find(|(area, _)| area.contains(position))
                    .and_then(|(_, index)| model.test_results.get(*index))
                {
                    // Clicking a bar selects the test to show its details.
                    model.test_cases_list.select_test(
                        &result.project_name,
                        &result.module_name,
                        &result.name,
                    );
                }
            }
        }
        _ => {}
    }
}

/// Maps a key press to a message.
fn handle_key(model: &Model, key: KeyEvent) -> Option<Message> {
    trace!("key = {key:?}, current_pane = {:?}", model.current_pane);

    if key.kind != KeyEventKind::Press {
        return None;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    if model.show_help {
        return match key.code {
            KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                Some(Message::ToggleHelp)
            }
            _ => None,
        };
    }

    let list = &model.test_cases_list;
    if list.searching {
        return match key.code {
            KeyCode::Esc => Some(Message::SearchEnd(false)),
            KeyCode::Enter => Some(Message::SearchEnd(true)),
            KeyCode::Backspace => Some(Message::SearchBackspace),
            KeyCode::Up => Some(Message::ListSelect(CursorMovement::Up)),
            KeyCode::Down => Some(Message::ListSelect(CursorMovement::Down)),
            KeyCode::Char(c) if !ctrl => Some(Message::SearchInput(c)),
            _ => None,
        };
    }

    // Keys available in every pane.
    let global = match key.code {
        KeyCode::Char('q') => Some(Message::Quit),
        KeyCode::Esc if list.is_filtering() => Some(Message::ClearFilters),
        KeyCode::Esc => Some(Message::Quit),
        KeyCode::Char('c') if ctrl => Some(Message::Quit),
        KeyCode::Char('?') => Some(Message::ToggleHelp),
        KeyCode::Char('z') => Some(Message::Maximize),
        KeyCode::Tab => Some(Message::NextPane),
        KeyCode::BackTab => Some(Message::PrevPane),
        KeyCode::Char('r') | KeyCode::Char('2') => Some(Message::ExecuteOne),
        KeyCode::Char('R') | KeyCode::Char('1') => Some(Message::ExecuteAll),
        KeyCode::Char('[') => Some(Message::InfoTabSelect(TabMovement::Prev)),
        KeyCode::Char(']') => Some(Message::InfoTabSelect(TabMovement::Next)),
        KeyCode::Char('/') => Some(Message::SearchStart),
        KeyCode::Char('f') => Some(Message::CycleStatusFilter),
        KeyCode::Char('n') => Some(Message::JumpToFailure { forward: true }),
        KeyCode::Char('N') => Some(Message::JumpToFailure { forward: false }),
        KeyCode::Char('L') => Some(Message::Logger(TuiWidgetEvent::HideKey)),
        _ => None,
    };
    if global.is_some() {
        return global;
    }

    match (model.current_pane, key.code) {
        (Pane::List, KeyCode::Char('j') | KeyCode::Down) => {
            Some(Message::ListSelect(CursorMovement::Down))
        }
        (Pane::List, KeyCode::Char('k') | KeyCode::Up) => {
            Some(Message::ListSelect(CursorMovement::Up))
        }
        (Pane::List, KeyCode::Char('g') | KeyCode::Home) => {
            Some(Message::ListSelect(CursorMovement::Home))
        }
        (Pane::List, KeyCode::Char('G') | KeyCode::End) => {
            Some(Message::ListSelect(CursorMovement::End))
        }
        (Pane::List, KeyCode::Char('d')) if ctrl => {
            Some(Message::ListSelect(CursorMovement::DownHalfScreen))
        }
        (Pane::List, KeyCode::Char('u')) if ctrl => {
            Some(Message::ListSelect(CursorMovement::UpHalfScreen))
        }
        (Pane::List, KeyCode::PageDown) => {
            Some(Message::ListSelect(CursorMovement::DownHalfScreen))
        }
        (Pane::List, KeyCode::PageUp) => Some(Message::ListSelect(CursorMovement::UpHalfScreen)),
        // With several projects, the arrow keys switch project tabs; h/l always
        // navigate the tree.
        (Pane::List, KeyCode::Left) if list.projects.len() > 1 => Some(Message::SwitchProject(-1)),
        (Pane::List, KeyCode::Right) if list.projects.len() > 1 => Some(Message::SwitchProject(1)),
        (Pane::List, KeyCode::Char('h') | KeyCode::Left) => Some(Message::ListCollapseOrParent),
        (Pane::List, KeyCode::Char('l') | KeyCode::Right) => Some(Message::ListExpandOrChild),
        (Pane::List, KeyCode::Enter | KeyCode::Char(' ')) => Some(Message::ListExpand),
        (Pane::Info, KeyCode::Char('j') | KeyCode::Down) => {
            Some(Message::InfoScroll(CursorMovement::Down))
        }
        (Pane::Info, KeyCode::Char('k') | KeyCode::Up) => {
            Some(Message::InfoScroll(CursorMovement::Up))
        }
        (Pane::Info, KeyCode::Char('g') | KeyCode::Home) => {
            Some(Message::InfoScroll(CursorMovement::Home))
        }
        (Pane::Info, KeyCode::Char('G') | KeyCode::End) => {
            Some(Message::InfoScroll(CursorMovement::End))
        }
        (Pane::Info, KeyCode::Char('d')) if ctrl => {
            Some(Message::InfoScroll(CursorMovement::DownHalfScreen))
        }
        (Pane::Info, KeyCode::Char('u')) if ctrl => {
            Some(Message::InfoScroll(CursorMovement::UpHalfScreen))
        }
        (Pane::Info, KeyCode::PageDown) => {
            Some(Message::InfoScroll(CursorMovement::DownHalfScreen))
        }
        (Pane::Info, KeyCode::PageUp) => Some(Message::InfoScroll(CursorMovement::UpHalfScreen)),
        (Pane::Info, KeyCode::Char('h') | KeyCode::Left) => {
            Some(Message::InfoTabSelect(TabMovement::Prev))
        }
        (Pane::Info, KeyCode::Char('l') | KeyCode::Right) => {
            Some(Message::InfoTabSelect(TabMovement::Next))
        }
        (Pane::Logger, KeyCode::Char('j') | KeyCode::Down) => {
            Some(Message::Logger(TuiWidgetEvent::DownKey))
        }
        (Pane::Logger, KeyCode::Char('k') | KeyCode::Up) => {
            Some(Message::Logger(TuiWidgetEvent::UpKey))
        }
        (Pane::Logger, KeyCode::Char('h') | KeyCode::Left) => {
            Some(Message::Logger(TuiWidgetEvent::LeftKey))
        }
        (Pane::Logger, KeyCode::Char('l') | KeyCode::Right) => {
            Some(Message::Logger(TuiWidgetEvent::RightKey))
        }
        (Pane::Logger, KeyCode::PageUp) => Some(Message::Logger(TuiWidgetEvent::PrevPageKey)),
        (Pane::Logger, KeyCode::PageDown) => Some(Message::Logger(TuiWidgetEvent::NextPageKey)),
        (Pane::Logger, KeyCode::Char(' ')) => Some(Message::Logger(TuiWidgetEvent::SpaceKey)),
        (Pane::Logger, KeyCode::Char('F')) => Some(Message::Logger(TuiWidgetEvent::FocusKey)),
        (Pane::Logger, KeyCode::Char('H')) => Some(Message::Logger(TuiWidgetEvent::HideKey)),
        _ => None,
    }
}

/// Style of a counter that filters the test list when clicked; the active one is underlined.
fn filter_style(model: &Model, filter: StatusFilter, style: Style) -> Style {
    if model.test_cases_list.status_filter == filter {
        style.add_modifier(Modifier::UNDERLINED | Modifier::BOLD)
    } else {
        style
    }
}

/// The one-line status bar at the top, and the horizontal ranges `(filter, start, end)`
/// of its clickable counters relative to the start of the line.
fn status_bar(model: &Model) -> (Line<'static>, Vec<(StatusFilter, u16, u16)>) {
    let mut hits = vec![];
    let sep = || Span::styled(" │ ", Style::new().fg(theme::BORDER));
    let counts = model.test_cases_list.counts();
    let mut spans = vec![
        Span::styled(
            " tanu ",
            Style::new()
                .fg(Color::Black)
                .bg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" v{}", env!("CARGO_PKG_VERSION")), muted()),
        sep(),
    ];

    let run = &model.run;
    if run.is_running() {
        spans.push(Span::styled(
            format!("● running {}/{}", run.done, run.total),
            Style::new().fg(theme::RUNNING).bold(),
        ));
    } else if run.started_at.is_none() {
        spans.push(Span::styled("○ idle", muted()));
    } else if run.failed > 0 {
        spans.push(Span::styled(
            " FAILED ",
            Style::new()
                .fg(Color::Black)
                .bg(theme::FAIL)
                .add_modifier(Modifier::BOLD),
        ));
    } else {
        spans.push(Span::styled(
            " PASSED ",
            Style::new()
                .fg(Color::Black)
                .bg(theme::OK)
                .add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(sep());

    let mut push_counter = |spans: &mut Vec<Span<'static>>, filter, span: Span<'static>| {
        let start = spans.iter().map(|s| s.width()).sum::<usize>() as u16;
        let end = start + span.width() as u16;
        hits.push((filter, start, end));
        spans.push(span.patch_style(filter_style(model, filter, Style::new())));
    };
    push_counter(
        &mut spans,
        StatusFilter::Passed,
        Span::styled(format!("✓ {}", counts.passed), Style::new().fg(theme::OK)),
    );
    spans.push(Span::raw("  "));
    push_counter(
        &mut spans,
        StatusFilter::Failed,
        Span::styled(
            format!("✘ {}", counts.failed),
            if counts.failed > 0 {
                Style::new().fg(theme::FAIL).bold()
            } else {
                muted()
            },
        ),
    );
    if run.retries > 0 {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("↻ {}", run.retries),
            Style::new().fg(theme::RUNNING),
        ));
    }
    spans.push(Span::raw("  "));
    push_counter(
        &mut spans,
        StatusFilter::NotRun,
        Span::styled(format!("○ {}", counts.pending()), muted()),
    );
    spans.push(Span::styled(format!("  of {}", counts.total), muted()));
    let executed = counts.passed + counts.failed;
    if counts.failed > 0 && executed > 0 {
        spans.push(Span::styled(
            format!(
                "  {:.1}% passed",
                counts.passed as f64 / executed as f64 * 100.0
            ),
            muted(),
        ));
    }

    if let Some(elapsed) = run.elapsed() {
        spans.push(sep());
        spans.push(Span::raw(format!("⏱ {}", fmt_duration(elapsed))));
    }

    let projects = model
        .test_cases_list
        .projects
        .iter()
        .map(|p| p.name.as_str())
        .collect::<Vec<_>>();
    if !projects.is_empty() {
        spans.push(sep());
        spans.push(Span::styled(
            if projects.len() == 1 {
                "project ".to_string()
            } else {
                "projects ".to_string()
            },
            muted(),
        ));
        spans.push(Span::raw(projects.join(", ")));
    }
    (Line::from(spans), hits)
}

/// Context-sensitive key hints for the footer.
fn key_hints(model: &Model) -> Vec<(&'static str, String)> {
    let list = &model.test_cases_list;
    if list.searching {
        return vec![
            ("type", "to search".into()),
            ("Enter", "Apply".into()),
            ("Esc", "Cancel".into()),
            ("↑↓", "Move".into()),
        ];
    }
    let mut hints: Vec<(&'static str, String)> = vec![("r", "Run".into()), ("R", "Run all".into())];
    match model.current_pane {
        Pane::List => {
            if list.projects.len() > 1 {
                hints.push(("←→", "Project".into()));
            }
            hints.push(("⏎", "Expand".into()));
            hints.push(("/", "Search".into()));
            hints.push(("f", format!("Filter:{}", list.status_filter.label())));
            hints.push(("n/N", "Next fail".into()));
        }
        Pane::Info => {
            hints.push(("[ ]", "Tab".into()));
            hints.push(("j/k", "Scroll".into()));
            hints.push(("g/G", "Top/Bottom".into()));
        }
        Pane::Logger => {
            hints.push(("←→", "Level".into()));
            hints.push(("PgUp/Dn", "Scroll".into()));
            hints.push(("L", "Targets".into()));
        }
        Pane::Chart => {
            hints.push(("click", "Select test".into()));
        }
    }
    if list.is_filtering() {
        hints.push(("Esc", "Clear filter".into()));
    }
    hints.push(("Tab", "Pane".into()));
    hints.push((
        "z",
        if model.maximizing {
            "Restore".into()
        } else {
            "Maximize".into()
        },
    ));
    hints.push(("?", "Help".into()));
    hints.push(("q", "Quit".into()));
    hints
}

/// Renders hints into a line, dropping the ones that do not fit.
fn hints_line(hints: Vec<(&'static str, String)>, width: u16) -> Line<'static> {
    let mut spans = vec![Span::raw(" ")];
    let mut used = 1;
    for (key, label) in hints {
        let item_width = key.chars().count() + label.chars().count() + 3;
        if used + item_width > width as usize {
            break;
        }
        used += item_width;
        spans.push(Span::styled(
            key,
            Style::new().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(format!(" {label}  ")));
    }
    Line::from(spans)
}

/// `(project, module, test)` of the test selected in the test list.
fn selected_test_key(model: &Model) -> Option<(String, String, String)> {
    let list = &model.test_cases_list;
    match list.selected_row()? {
        RowRef::Test(p, m, t) | RowRef::Call(p, m, t, _) => {
            let project = list.project(p)?;
            let test = list.test(p, m, t)?;
            Some((
                project.name.clone(),
                test.info.module.clone(),
                test.info.name.clone(),
            ))
        }
        _ => None,
    }
}

fn is_selected(selected: &Option<(String, String, String)>, result: &TestResult) -> bool {
    selected.as_ref().is_some_and(|(project, module, name)| {
        *project == result.project_name && *module == result.module_name && *name == result.name
    })
}

/// Latency samples of all HTTP/gRPC calls.
fn latency_samples(model: &Model) -> Vec<latency::Sample> {
    let selected = selected_test_key(model);
    model
        .test_results
        .iter()
        .flat_map(|result| {
            let selected = is_selected(&selected, result);
            result.calls().map(move |call| latency::Sample {
                latency: call.duration(),
                error: call.is_error(),
                selected,
            })
        })
        .collect()
}

/// Timeline entries for the tests of the latest run.
fn timeline_entries(model: &Model) -> Vec<timeline::Entry> {
    let selected = selected_test_key(model);
    model
        .test_results
        .iter()
        .enumerate()
        .filter_map(|(key, result)| {
            let test = result.test.as_ref()?;
            if model
                .run
                .started_system
                .is_some_and(|started| test.started_at < started)
            {
                // Result of an earlier run.
                return None;
            }
            Some(timeline::Entry {
                lane: test.worker_id,
                start: test.started_at,
                end: test.ended_at,
                ok: test.result.is_ok(),
                selected: is_selected(&selected, result),
                key,
            })
        })
        .collect()
}

/// Renders the timeline chart and returns the clickable bars.
fn render_timeline(
    frame: &mut Frame,
    area: Rect,
    entries: &[timeline::Entry],
    focused: bool,
    maximized: bool,
) -> Vec<(Rect, usize)> {
    let block = chart_block(
        "Timeline",
        focused,
        maximized,
        timeline::summary(entries)
            .map(|summary| vec![Span::styled(summary, muted())])
            .unwrap_or_default(),
    );
    let inner = block.inner(area).inner(Margin::new(1, 0));
    frame.render_widget(block, area);
    if entries.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled("No tests run yet", muted())).centered(),
            inner.centered_vertically(Constraint::Length(1)),
        );
        return vec![];
    }
    timeline::render(entries, inner, frame.buffer_mut())
}

/// Block of a chart in the Charts pane, titled with the chart name and `details`.
fn chart_block(
    name: &'static str,
    focused: bool,
    maximized: bool,
    details: Vec<Span<'static>>,
) -> ratatui::widgets::Block<'static> {
    let mut title = vec![Span::raw(name)];
    title.extend(details);
    if maximized {
        title.push(Span::styled(" [maximized]", muted()));
    }
    theme::block(title, focused)
}

fn render_histogram(
    frame: &mut Frame,
    area: Rect,
    samples: &[latency::Sample],
    focused: bool,
    maximized: bool,
) {
    let block = chart_block(
        "Latency",
        focused,
        maximized,
        latency::summary(samples).unwrap_or_default(),
    );
    let inner = block.inner(area).inner(Margin::new(1, 0));
    frame.render_widget(block, area);
    if samples.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled("No requests yet", muted())).centered(),
            inner.centered_vertically(Constraint::Length(1)),
        );
        return;
    }
    latency::render(samples, inner, frame.buffer_mut());
}

fn render_gauge(frame: &mut Frame, area: Rect, run: &RunStats) {
    if run.started_at.is_none() || run.total == 0 {
        return;
    }
    let ratio = (run.done as f64 / run.total as f64).clamp(0.0, 1.0);
    let color = if run.failed > 0 {
        theme::FAIL
    } else if run.is_running() {
        theme::ACCENT
    } else {
        theme::OK
    };
    let gauge = LineGauge::default()
        .filled_style(Style::new().fg(color))
        .unfilled_style(Style::new().fg(theme::BORDER))
        .filled_symbol("━")
        .unfilled_symbol("━")
        .ratio(ratio)
        .label(Line::styled(
            format!(
                "{}/{} {:>3}% ",
                run.done,
                run.total,
                (ratio * 100.0).round()
            ),
            Style::new().fg(color),
        ));
    frame.render_widget(gauge, area);
}

/// Construct UI.
fn view(model: &mut Model, frame: &mut Frame) {
    trace!("rendering view");
    model.sync_selection();

    let [layout_header, layout_main, layout_footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    let [layout_left, layout_right] =
        Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
            .areas(layout_main);
    // Logs take a short row under the tests.
    let [layout_list, layout_logger] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(10)]).areas(layout_left);
    let [layout_info, layout_chart] =
        Layout::vertical([Constraint::Percentage(60), Constraint::Percentage(40)])
            .areas(layout_right);
    let [layout_hints, layout_gauge] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(32)]).areas(layout_footer);

    // Header
    let [layout_status, layout_fps] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(12)]).areas(layout_header);
    let (status_line, status_hits) = status_bar(model);
    frame.render_widget(Paragraph::new(status_line), layout_status);
    let filter_hits: Vec<(Rect, StatusFilter)> = status_hits
        .into_iter()
        .map(|(filter, start, end)| {
            let x = layout_status.x + start;
            let area = Rect::new(x, layout_status.y, end - start, 1);
            (area.intersection(layout_status), filter)
        })
        .collect();
    if let Some(fps_counter) = &model.fps_counter {
        frame.render_widget(
            Paragraph::new(format!("FPS:{:.1}", fps_counter.fps))
                .alignment(Alignment::Right)
                .style(muted()),
            layout_fps,
        );
    }

    // Footer
    frame.render_widget(
        hints_line(key_hints(model), layout_hints.width),
        layout_hints,
    );
    render_gauge(frame, layout_gauge, &model.run);

    let maximized = model.maximizing;
    model.info_state.maximized = maximized && model.current_pane == Pane::Info;
    let (list_area, info_area, logger_area, chart_area) = if maximized {
        let hidden = Rect::default();
        match model.current_pane {
            Pane::List => (layout_main, hidden, hidden, hidden),
            Pane::Info => (hidden, layout_main, hidden, hidden),
            Pane::Logger => (hidden, hidden, layout_main, hidden),
            Pane::Chart => (hidden, hidden, hidden, layout_main),
        }
    } else {
        (layout_list, layout_info, layout_logger, layout_chart)
    };
    model.areas = Areas {
        list: list_area,
        info: info_area,
        logger: logger_area,
        chart: chart_area,
        filter_hits: vec![],
        timeline_hits: vec![],
    };

    if !list_area.is_empty() {
        let test_list = TestListWidget::new(
            model.current_pane == Pane::List,
            maximized,
            &model.test_cases_list,
        );
        frame.render_stateful_widget(test_list, list_area, &mut model.test_cases_list);
    }

    if !info_area.is_empty() {
        let info = InfoWidget::new(&model.test_cases_list);
        frame.render_stateful_widget(info, info_area, &mut model.info_state);
    }

    if !logger_area.is_empty() {
        let focused = model.current_pane == Pane::Logger;
        let border_style = theme::border_style(focused);
        let logger = TuiLoggerSmartWidget::default()
            .title_target("Targets".bold())
            .title_log(if maximized {
                "Logs [maximized]".bold()
            } else {
                "Logs".bold()
            })
            .border_type(BorderType::Rounded)
            .border_style(border_style)
            .highlight_style(Style::new().bg(theme::SELECTED_BG))
            .style_error(Style::default().fg(theme::FAIL))
            .style_warn(Style::default().fg(theme::ACCENT).bold())
            .style_info(Style::default())
            .style_debug(muted())
            .style_trace(muted())
            .output_separator('│')
            .output_timestamp(Some("%H:%M:%S".to_string()))
            .output_level(Some(TuiLoggerLevelOutput::Abbreviated))
            .output_target(false)
            .output_file(false)
            .output_line(false)
            .state(&model.logger_state);
        frame.render_widget(logger, logger_area);
    }

    let samples = latency_samples(model);
    let mut timeline_hits = vec![];
    if !chart_area.is_empty() {
        let focused = model.current_pane == Pane::Chart;
        // Side by side normally; stacked when maximized so the timeline gets the full width.
        let [layout_timeline, layout_latency] = if maximized {
            Layout::vertical([Constraint::Percentage(60), Constraint::Percentage(40)])
                .areas(chart_area)
        } else {
            Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
                .areas(chart_area)
        };
        let entries = timeline_entries(model);
        timeline_hits = render_timeline(frame, layout_timeline, &entries, focused, maximized);
        render_histogram(frame, layout_latency, &samples, focused, maximized);
    }

    model.areas.filter_hits = filter_hits;
    model.areas.timeline_hits = timeline_hits;

    if model.show_help {
        frame.render_widget(HelpWidget, frame.area());
    }
}

/// Tracks the test results of running tests until they end.
#[derive(Default)]
struct ResultsBuffer {
    running: HashMap<(String, String), TestResult>,
}

/// Applies a runner event to the model.
fn on_runner_event(model: &mut Model, buffer: &mut ResultsBuffer, event: runner::Event) {
    let runner::Event {
        project,
        module,
        test: test_name,
        body,
    } = event;
    let key = (project.clone(), test_name.clone());
    match body {
        EventBody::Start => {
            buffer.running.insert(
                key,
                TestResult {
                    project_name: project,
                    module_name: module,
                    name: test_name,
                    ..Default::default()
                },
            );
        }
        EventBody::Check(check) => {
            if let Some(test_result) = buffer.running.get_mut(&key) {
                test_result.checks.push(*check);
            }
        }
        EventBody::Call(log) => {
            if let Some(test_result) = buffer.running.get_mut(&key) {
                match log {
                    runner::CallLog::Http(http_log) => test_result.logs.push(http_log),
                    #[cfg(feature = "grpc")]
                    runner::CallLog::Grpc(grpc_log) => test_result.grpc_logs.push(grpc_log),
                }
            }
        }
        EventBody::Retry(_) => {
            if let Some(test_result) = buffer.running.get_mut(&key) {
                test_result.retries += 1;
            }
            model.run.retries += 1;
        }
        EventBody::End(test) => {
            let Some(mut test_result) = buffer.running.remove(&key) else {
                return;
            };
            test_result.test = Some(test);
            model.run.done += 1;
            if !test_result.is_ok() {
                model.run.failed += 1;
            }
            ExecutionStateController::on_test_updated(
                &mut model.test_cases_list,
                &project,
                &module,
                &test_name,
                test_result.clone(),
            );
            model.store_result(test_result);
        }
        EventBody::Summary(_summary) => {
            model.run.finished_at = Some(Instant::now());
        }
    }
}

/// The Runtime the application.
struct Runtime {
    should_exit: bool,
}

impl Runtime {
    const FRAMES_PER_SECOND: f32 = 60.0;

    fn new() -> Runtime {
        Runtime { should_exit: false }
    }

    async fn run(
        mut self,
        mut runner: Runner,
        mut terminal: ratatui::DefaultTerminal,
    ) -> eyre::Result<()> {
        let period = Duration::from_secs_f32(1.0 / Self::FRAMES_PER_SECOND);
        let mut draw_interval = tokio::time::interval(period);
        let mut cmds_interval = tokio::time::interval(period);
        let mut thrb_interval = tokio::time::interval(Duration::from_secs_f32(0.1));
        let mut event_stream = EventStream::new();

        let test_cases = runner.list().into_iter().cloned().collect();
        let mut model = Model::new(test_cases);
        let mut cmds = VecDeque::<Command>::new();

        let (runner_tx, mut runner_rx, mut runner_task) = {
            let (runner_tx, mut runner_rx) = mpsc::unbounded_channel::<Command>();
            let runner_task = tokio::spawn(async move {
                while let Some(cmd) = runner_rx.recv().await {
                    match cmd {
                        Command::ExecuteOne(selector) => {
                            info!(
                                "running the selected test case: project={} module={} test={}",
                                selector.project,
                                selector.module.as_deref().unwrap_or_default(),
                                selector.test.as_deref().unwrap_or_default()
                            );
                            if let Err(e) = runner
                                .run(
                                    &[selector.project],
                                    selector.module.into_iter().collect::<Vec<_>>().as_slice(),
                                    selector.test.into_iter().collect::<Vec<_>>().as_slice(),
                                )
                                .await
                            {
                                error!("{e:#}");
                            }
                        }
                        Command::ExecuteAll => {
                            info!("running all test cases");
                            if let Err(e) = runner.run(&[], &[], &[]).await {
                                error!("{e:#}");
                            }
                        }
                    }
                }
                info!("command queue for tanu runner terminated");
            });
            let runner_rx = tanu_core::runner::subscribe()?;
            (runner_tx, runner_rx, runner_task)
        };
        let mut results_buffer = ResultsBuffer::default();

        // Redraw only when something changed, at most `FRAMES_PER_SECOND` times a
        // second. Log lines arrive outside of the event loop, so the screen is also
        // refreshed every `IDLE_REDRAW` while nothing else happens.
        const IDLE_REDRAW: Duration = Duration::from_millis(500);
        let mut dirty = true;
        let mut last_draw = Instant::now();
        let mut runner_open = true;

        while !self.should_exit && !panic_occurred() {
            tokio::select! {
                _ = draw_interval.tick() => {
                    if !dirty && last_draw.elapsed() < IDLE_REDRAW {
                        continue;
                    }
                    if let Some(fps_counter) = &mut model.fps_counter {
                        fps_counter.update();
                    }
                    let start_draw = std::time::Instant::now();
                    terminal.draw(|frame| view(&mut model, frame))?;
                    trace!("Took {:?} to draw", start_draw.elapsed());
                    dirty = false;
                    last_draw = Instant::now();
                },
                _ = cmds_interval.tick() => {
                    if let Some(cmd) = cmds.pop_front() {
                        let _ = runner_tx.send(cmd);
                    }
                }
                _ = thrb_interval.tick() => {
                    // Spinners and the elapsed time only change while tests run.
                    if model.run.is_running() {
                        ExecutionStateController::update_throbber(&mut model.test_cases_list);
                        dirty = true;
                    }
                }
                _ = &mut runner_task => {
                }
                event = runner_rx.recv(), if runner_open => {
                    // Handle every pending event before the next draw, so that a burst
                    // of events does not wait for one frame per event.
                    let mut event = event;
                    loop {
                        match event {
                            Ok(event) => on_runner_event(&mut model, &mut results_buffer, event),
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                warn!("TUI fell behind and missed {n} runner events");
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                runner_open = false;
                                break;
                            }
                        }
                        event = match runner_rx.try_recv() {
                            Ok(event) => Ok(event),
                            Err(broadcast::error::TryRecvError::Lagged(n)) => {
                                Err(broadcast::error::RecvError::Lagged(n))
                            }
                            Err(_) => break,
                        };
                    }
                    dirty = true;
                }
                Some(Ok(event)) = event_stream.next() => {
                    dirty = true;
                    let msg = match event {
                        Event::Key(key) => handle_key(&model, key),
                        Event::Mouse(mouse) => Some(Message::Mouse(mouse)),
                        _ => None,
                    };
                    let Some(msg) = msg else {
                        continue;
                    };
                    if matches!(msg, Message::Quit) {
                        self.should_exit = true;
                        continue;
                    }
                    if let Some(cmd) = update(&mut model, msg) {
                        cmds.push_back(cmd);
                    }
                }
            }
        }

        // Note: Terminal cleanup is handled by restore_terminal() in the run() function
        // or by the panic hook if a panic occurs
        Ok(())
    }
}

/// Flag to signal that a panic occurred and the TUI should exit.
static PANIC_OCCURRED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn panic_occurred() -> bool {
    PANIC_OCCURRED.load(std::sync::atomic::Ordering::SeqCst)
}

/// Restores the terminal to its original state.
fn restore_terminal() {
    use std::io::Write;
    let _ = crossterm::terminal::disable_raw_mode();
    let mut stdout = std::io::stdout().lock();
    let _ = crossterm::execute!(
        stdout,
        crossterm::event::DisableMouseCapture,
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::cursor::Show,
    );
    let _ = stdout.flush();
}

/// Installs a panic hook that restores the terminal and exits cleanly.
fn install_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        PANIC_OCCURRED.store(true, std::sync::atomic::Ordering::SeqCst);
        restore_terminal();
        original_hook(panic_info);
    }));
}

/// Runs the tanu terminal user interface application.
///
/// Initializes and runs the interactive TUI for managing and executing tanu tests.
/// The TUI provides a test tree, a details pane, a logger, and timeline and latency charts.
/// Users can navigate with keyboard shortcuts or the mouse to select tests, run them
/// individually or in bulk, and monitor execution in real-time.
///
/// # Parameters
///
/// - `runner`: The configured test runner containing test cases and configuration
/// - `log_level`: General logging level for the TUI and external libraries
/// - `tanu_log_level`: Specific logging level for tanu framework components
///
/// # Features
///
/// - **Interactive Test Selection**: Browse the test tree and inspect results
/// - **Real-time Execution**: Watch tests run with live progress, counts and logs
/// - **HTTP Request Monitoring**: View request/response details, headers and payloads
/// - **Checks and Errors**: See every assertion evaluated by a test
/// - **Search and Filters**: Search tests by name and filter by status
/// - **Logging**: Integrated logger pane for debugging
///
/// # Keyboard Shortcuts
///
/// - `r`: Run the selected project, module or test
/// - `R`: Run all tests
/// - `↑/↓` or `j/k`: Navigate the test tree
/// - `Enter`: Expand/collapse the selected item
/// - `/`: Search tests, `f`: filter by status, `n/N`: jump to next/previous failure
/// - `Tab`: Switch between panes, `[`/`]`: switch details tabs
/// - `?`: Show all key bindings
/// - `q`/`Esc`: Quit application
///
/// Set `TANU_TUI_DEBUG=1` to show the frame rate.
///
/// # Examples
///
/// ```rust,ignore
/// use tanu_core::Runner;
/// use tanu_tui::run;
///
/// let runner = Runner::new();
/// run(runner, log::LevelFilter::Info, log::LevelFilter::Debug).await?;
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - Terminal initialization fails
/// - Logger setup fails
/// - Test execution encounters unrecoverable errors
/// - TUI rendering fails
pub async fn run(
    runner: Runner,
    log_level: log::LevelFilter,
    tanu_log_level: log::LevelFilter,
) -> eyre::Result<()> {
    tracing_log::LogTracer::init()?;
    tui_logger::init_logger(log_level)?;
    tui_logger::set_level_for_target("tanu", tanu_log_level);
    tui_logger::set_level_for_target("tanu_core", tanu_log_level);
    tui_logger::set_level_for_target("tanu_core::assertion", tanu_log_level);
    tui_logger::set_level_for_target("tanu_core::config", tanu_log_level);
    tui_logger::set_level_for_target("tanu_core::http", tanu_log_level);
    tui_logger::set_level_for_target("tanu_core::reporter", tanu_log_level);
    tui_logger::set_level_for_target("tanu_core::runner", tanu_log_level);
    tui_logger::set_level_for_target("tanu_tui", tanu_log_level);
    tui_logger::set_level_for_target("tanu_tui::widget", tanu_log_level);
    tui_logger::set_level_for_target("tanu_tui::widget::info", tanu_log_level);
    tui_logger::set_level_for_target("tanu_tui::widget::list", tanu_log_level);
    let subscriber =
        tracing_subscriber::Registry::default().with(tui_logger::TuiTracingSubscriberLayer);
    tracing::subscriber::set_global_default(subscriber)
        .wrap_err("failed to set global default subscriber")?;

    if std::env::var("RUST_BACKTRACE").is_err() {
        std::env::set_var("RUST_BACKTRACE", "full");
    }
    if std::env::var("COLORBT_SHOW_HIDDEN").is_err() {
        std::env::set_var("COLORBT_SHOW_HIDDEN", "1");
    }

    dotenvy::dotenv().ok();

    install_panic_hook();
    PANIC_OCCURRED.store(false, std::sync::atomic::Ordering::SeqCst);

    // Reset terminal in case a previous run crashed
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::event::DisableMouseCapture,
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::cursor::Show
    );

    let mut terminal = ratatui::init();
    terminal.clear()?;
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;

    let runtime = Runtime::new();
    let result = runtime.run(runner, terminal).await;
    restore_terminal();

    println!("tanu-tui terminated with {result:?}");
    result
}

struct FpsCounter {
    frame_count: usize,
    last_second: std::time::Instant,
    fps: f64,
}

impl FpsCounter {
    fn new() -> Self {
        Self {
            frame_count: 0,
            last_second: std::time::Instant::now(),
            fps: 0.0,
        }
    }

    fn update(&mut self) {
        self.frame_count += 1;
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_second).as_secs_f64();

        if elapsed >= 1.0 {
            self.fps = self.frame_count as f64 / elapsed;
            self.frame_count = 0;
            self.last_second = now;
        }
    }
}
