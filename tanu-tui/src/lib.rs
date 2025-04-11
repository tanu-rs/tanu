/// `tanu-tui` is a terminal-based user interface application for managing and executing tests
/// using the `tanu` framework. It is implemented using the ratatui library and follows the
/// Elm Architecture, which divides the logic into Model, Update, and View components, making it
/// easier to maintain and scale. The application has three primary panes: a list of tests, a console
/// for viewing logs, and a logger for tracing runtime messages. It supports asynchronous test execution
/// and user interaction via keyboard commands, providing an efficient, interactive environment for managing
/// test cases and monitoring their results.
mod widget;

use crossterm::event::KeyModifiers;
use eyre::WrapErr;
use futures::StreamExt;
use ratatui::{
    crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind},
    layout::Position,
    prelude::*,
    style::{palette::tailwind, Modifier, Style},
    text::Line,
    widgets::{
        block::{BorderType, Padding},
        BarChart, Block, Borders, Paragraph, Tabs,
    },
    Frame,
};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    time::Duration,
};
use tanu_core::{get_tanu_config, Runner, TestInfo};
use tokio::sync::mpsc;
use tracing::{debug, error, info, trace};
use tracing_subscriber::layer::SubscriberExt;
use tui_big_text::{BigText, PixelSize};
use tui_logger::{TuiLoggerLevelOutput, TuiLoggerSmartWidget, TuiWidgetEvent, TuiWidgetState};

pub const WHITESPACE: &str = "\u{00A0}";

const SELECTED_STYLE: Style = Style::new()
    .bg(tailwind::SLATE.c800)
    .add_modifier(Modifier::BOLD);

use crate::widget::{
    info::{InfoState, InfoWidget, Tab},
    list::{TestCaseSelector, TestListState, TestListWidget},
};

/// Represents result of a test case.
#[derive(Default, Clone, Debug)]
pub struct TestResult {
    pub project_name: String,
    pub module_name: String,
    pub name: String,
    pub logs: Vec<Box<tanu_core::http::Log>>,
    pub test: Option<tanu_core::runner::Test>,
}

impl TestResult {
    /// Unique test name including project and module names
    pub fn unique_name(&self) -> String {
        format!("{}::{}::{}", self.project_name, self.module_name, self.name)
    }
}

#[derive(
    Debug, Clone, Copy, Default, Eq, PartialEq, strum::FromRepr, strum::EnumString, strum::Display,
)]
enum Pane {
    #[default]
    List,
    Console,
    Logger,
}

/// Indicates the state of test execution.
#[derive(Debug, Clone, Copy)]
enum Execution {
    /// Executing or executed a test case.
    One,
    /// Executing or executed all of the test cases.
    All,
}

/// Represents cursor movement.
#[derive(Debug, Clone, Copy)]
enum CursorMovement {
    /// Move the cursor up by one line.
    Up,
    /// Move the cursor down by one line.
    Down,
    /// Move the cursor up by half of the screen height.
    UpHalfScreen,
    /// Move the cursor down by half of the screen height.
    DownHalfScreen,
}

/// Represents tab movement.
#[derive(Debug, Clone, Copy)]
enum TabMovement {
    /// Move tab to the next.
    Next,
    /// Move tab to the previous.
    Prev,
}

/// Represents the state of the application, including the current pane, execution state, test cases, and UI components' states.
struct Model {
    /// Indicates whether the current pane is in maximized view mode
    maximizing: bool,
    /// Keeps track of which pane (List, Console, Logger) is currently focused
    current_pane: Pane,
    /// Stores the current execution state, which can be either executing one test, all tests, or none
    current_exec: Option<Execution>,
    /// Manages the selection state for the list of test cases
    test_cases_list: TestListState,
    /// Contains the results of executed tests, including logs and the test itself
    test_results: Vec<TestResult>,
    /// Maintains the state of the info pange, such as currently selected tab.
    info_state: InfoState,
    /// Holds the state of the logger pane, including any focus or visibility settings
    logger_state: TuiWidgetState,
    /// Stores the last mouse click event, if any
    click: Option<crossterm::event::MouseEvent>,
}

impl Model {
    fn new(test_cases: Vec<TestInfo>) -> Model {
        let cfg = get_tanu_config();
        Model {
            maximizing: false,
            current_pane: Pane::default(),
            current_exec: None,
            test_cases_list: TestListState::new(&cfg.projects, &test_cases),
            test_results: vec![],
            info_state: InfoState::new(),
            logger_state: TuiWidgetState::new(),
            click: None,
        }
    }

    fn next_pane(&mut self) {
        let current_index = self.current_pane as usize;
        let pane_counts = Pane::Logger as usize + 1;
        let next_index = (current_index + 1) % pane_counts;
        if let Some(next_pane) = Pane::from_repr(next_index) {
            self.current_pane = next_pane;
        }
        self.info_state.focused = self.current_pane == Pane::Console;
    }
}

#[derive(Debug)]
enum Message {
    Maximize,
    NextPane,
    ListSelectNext,
    ListSelectPrev,
    ListSelectFirst,
    ListSelectLast,
    ListExpand,
    ConsoleSelect(CursorMovement),
    ConsoleSelectFirst,
    ConsoleSelectLast,
    ConsoleShowHttpLog,
    ConsoleTabSelect(TabMovement),
    LoggerSelectDown,
    LoggerSelectUp,
    LoggerSelectLeft,
    LoggerSelectRight,
    LoggerSelectSpace,
    LoggerSelectHide,
    LoggerSelectFocus,
    ExecuteOne,
    ExecuteAll,
    SelectPane(crossterm::event::MouseEvent),
}

#[derive(Debug)]
enum Command {
    ExecuteOne(TestCaseSelector),
    ExecuteAll,
}

/// Move down the offset of the model.
fn offset_down(model: &mut Model, val: i16) {
    match model.info_state.selected_tab {
        Tab::Payload => {
            model.info_state.payload_state.scroll_offset += val as u16;
        }
        Tab::Error => {
            model.info_state.error_state.scroll_offset += val as u16;
        }
        _ => {}
    }
}

/// Move up the offset of the model.
fn offset_up(model: &mut Model, val: i16) {
    match model.info_state.selected_tab {
        Tab::Payload => {
            model.info_state.payload_state.scroll_offset = model
                .info_state
                .payload_state
                .scroll_offset
                .saturating_sub(val as u16);
        }
        Tab::Error => {
            model.info_state.error_state.scroll_offset = model
                .info_state
                .error_state
                .scroll_offset
                .saturating_sub(val as u16);
        }
        _ => {}
    }
    if model.info_state.selected_tab == Tab::Error {}
}

async fn update(model: &mut Model, msg: Message) -> eyre::Result<Option<Command>> {
    model.click = None;
    match msg {
        Message::Maximize => {
            model.maximizing = !model.maximizing;
        }
        Message::NextPane => {
            model.next_pane();
        }
        Message::ListSelectNext => model.test_cases_list.list_state.select_next(),
        Message::ListSelectPrev => model.test_cases_list.list_state.select_previous(),
        Message::ListSelectFirst => model.test_cases_list.list_state.select_first(),
        Message::ListSelectLast => model.test_cases_list.list_state.select_last(),
        Message::ListExpand => model.test_cases_list.expand(&model.test_results),
        Message::ConsoleSelect(CursorMovement::Down) => {
            offset_down(model, 1);
        }
        Message::ConsoleSelect(CursorMovement::DownHalfScreen) => {
            offset_down(model, (crossterm::terminal::size()?.1 / 2) as i16);
        }
        Message::ConsoleSelect(CursorMovement::Up) => {
            offset_up(model, 1);
        }
        Message::ConsoleSelect(CursorMovement::UpHalfScreen) => {
            offset_up(model, (crossterm::terminal::size()?.1 / 2) as i16);
        }
        Message::ConsoleTabSelect(TabMovement::Next) => {
            model.info_state.next_tab();
        }
        Message::ConsoleTabSelect(TabMovement::Prev) => {
            model.info_state.prev_tab();
        }
        Message::ConsoleSelectFirst => {}

        Message::ConsoleSelectLast => {}
        Message::ConsoleShowHttpLog => {}
        Message::LoggerSelectDown => model.logger_state.transition(TuiWidgetEvent::DownKey),
        Message::LoggerSelectUp => model.logger_state.transition(TuiWidgetEvent::UpKey),
        Message::LoggerSelectLeft => model.logger_state.transition(TuiWidgetEvent::LeftKey),
        Message::LoggerSelectRight => model.logger_state.transition(TuiWidgetEvent::RightKey),
        Message::LoggerSelectSpace => model.logger_state.transition(TuiWidgetEvent::SpaceKey),
        Message::LoggerSelectHide => model.logger_state.transition(TuiWidgetEvent::HideKey),
        Message::LoggerSelectFocus => model.logger_state.transition(TuiWidgetEvent::FocusKey),
        Message::ExecuteOne => {
            model.test_results.clear();
            model.current_exec = Some(Execution::One);
            if let Some(selector) = model.test_cases_list.select_test_case(&model.test_results) {
                return Ok(Some(Command::ExecuteOne(selector)));
            }
        }
        Message::ExecuteAll => {
            model.test_results.clear();
            model.current_exec = Some(Execution::All);
            return Ok(Some(Command::ExecuteAll));
        }
        Message::SelectPane(click) => {
            model.click = Some(click);
        }
    }

    model.info_state.selected_test = model.test_cases_list.select_test_case(&model.test_results);

    Ok(None)
}

/// Construct UI.
fn view(model: &mut Model, frame: &mut Frame) {
    trace!("rendering view");

    let [layout_main, layout_menu] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(frame.area());
    let [layout_left, layout_right] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(layout_main);
    let [layout_rightup, layout_rightdown] =
        Layout::vertical([Constraint::Percentage(70), Constraint::Percentage(30)])
            .areas(layout_right);
    let layout_right_inner = Layout::default()
        .constraints([Constraint::Percentage(100)])
        .margin(1)
        .split(layout_rightup)[0];
    let [_, layout_tabs, layout_info] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Min(0),
    ])
    .areas(layout_right_inner);
    let [layout_logo, layout_list, layout_logger] = Layout::vertical([
        Constraint::Min(3),
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ])
    .areas(layout_left);
    let layout_menu_items = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(9),
            Constraint::Length(13),
            Constraint::Length(12),
            Constraint::Length(12),
        ])
        .split(layout_menu);

    // Handle mouse click events on UI. If position is in the list pane area, switch to it.
    let click_position = model.click.as_ref().map(|click| {
        let x = click.column;
        let y = click.row;
        Position::from((x, y))
    });
    if let Some(position) = click_position {
        if layout_list.contains(position) {
            model.current_pane = Pane::List;
            model.info_state.focused = false;
        } else if layout_info.contains(position) {
            model.current_pane = Pane::Console;
            model.info_state.focused = true;
        } else if layout_tabs.contains(position) {
            model.current_pane = Pane::Console;
            model.info_state.focused = true;

            // Check which tab was clicked.
            let mut left = layout_tabs.left();
            for tab in [Tab::Call, Tab::Headers, Tab::Payload, Tab::Error] {
                const TAB_PADDING: u16 = 4;
                const TAB_DIVIDER: u16 = 1;
                let tab_length = tab.to_string().len() as u16 + TAB_PADDING;
                if position.x >= left && position.x <= left + tab_length {
                    model.info_state.selected_tab = tab;
                    break;
                }
                left += tab_length + TAB_DIVIDER;
            }
        } else if layout_logger.contains(position) {
            model.current_pane = Pane::Logger;
            model.info_state.focused = false;
        }
    }

    let menu_items = [
        ("q", "Quit"),
        ("z", "Maximize"),
        ("1", "Run ALL"),
        ("2", "Run"),
    ];

    for (n, &(key, label)) in menu_items.iter().enumerate() {
        let menu_item = Paragraph::new(vec![Line::from(vec![
            Span::styled(
                format!("{WHITESPACE}{key}{WHITESPACE}"),
                Style::default().bg(tailwind::TEAL.c900),
            ),
            Span::styled(format!("{WHITESPACE}{label}"), Style::default()),
        ])])
        .style(Style::default().add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::NONE));
        frame.render_widget(menu_item, layout_menu_items[n]);
    }

    let info_block = Block::default()
        .border_type(if model.info_state.focused {
            BorderType::Thick
        } else {
            BorderType::Plain
        })
        .borders(Borders::ALL)
        .title("Request/Response".bold());

    let tabs = Tabs::new(
        [Tab::Call, Tab::Headers, Tab::Payload, Tab::Error]
            .iter()
            .map(|tab| {
                let text = tab.to_string();
                Line::from(format!("  {text}  ").bold())
            }),
    )
    .select(model.info_state.selected_tab as usize)
    .highlight_style(Style::default().reversed())
    .block(Block::default().borders(Borders::BOTTOM))
    .padding("", "")
    .divider("|");

    let info = InfoWidget::new(model.test_results.clone());

    let logo = BigText::builder()
        .pixel_size(PixelSize::Sextant)
        .style(Style::new().fg(tailwind::TEAL.c800))
        .lines(vec!["tanu".into()])
        .build();

    let test_list = TestListWidget::new(
        matches!(model.current_pane, Pane::List),
        &model.test_cases_list.projects,
        &model.test_results,
    );

    let logger = TuiLoggerSmartWidget::default()
        .title_target("Selector".bold())
        .title_log("Logs".bold())
        .border_type(if matches!(model.current_pane, Pane::Logger) {
            BorderType::Thick
        } else {
            BorderType::Plain
        })
        .style_error(Style::default().fg(tailwind::RED.c900))
        .style_warn(Style::default().fg(tailwind::AMBER.c900))
        .style_info(Style::default())
        .style_debug(Style::default().dim())
        .style_trace(Style::default().dim())
        .output_separator('|')
        .output_timestamp(None)
        .output_level(Some(TuiLoggerLevelOutput::Long))
        .output_target(false)
        .output_file(false)
        .output_line(false)
        .state(&model.logger_state);

    const BAR_WIDTH: usize = 5;
    let max_duration = model
        .test_results
        .iter()
        .flat_map(|test| {
            test.logs
                .iter()
                .map(|log| log.response.duration_req.as_millis())
        })
        .max()
        .unwrap_or(0);

    // Decide such number of buckets that histogram bars stretch to the width of the pane.
    let pane_width = layout_rightdown.width as usize;
    let mut num_buckets = (pane_width / BAR_WIDTH).max(1);
    if model.test_results.is_empty() {
        num_buckets = 1;
    }

    fn decide_bar_size(value: u128) -> u128 {
        let exponent = (value as f64).log10().ceil() as i32 - 1;
        let magnitude = if exponent >= 0 {
            10u128.saturating_pow(exponent as u32)
        } else {
            1 // Default to 1 if the exponent is negative
        };
        value.div_ceil(magnitude) * magnitude
    }

    let bucket_size = decide_bar_size((max_duration / num_buckets as u128).max(1));

    let mut buckets: BTreeMap<u64, usize> = (1..num_buckets).map(|i| (i as u64, 0)).collect();
    for test in &model.test_results {
        for log in &test.logs {
            let bucket = ((log.response.duration_req.as_millis() as f64) / (bucket_size as f64))
                .ceil() as u64;
            *buckets.entry(bucket).or_default() += 1;
        }
    }

    let histogram_raw_data = buckets
        .iter()
        .map(|(k, v)| ((k * bucket_size as u64).to_string(), *v as u64))
        .collect::<Vec<_>>();
    let histogram_data = histogram_raw_data
        .iter()
        .map(|(k, v)| (k.as_str(), *v))
        .collect::<Vec<_>>();
    let histogram: BarChart<'_> = BarChart::default()
        .data(&histogram_data)
        .block(
            Block::new()
                .title("Latency [ms]".bold())
                .borders(Borders::ALL)
                .padding(Padding::top(1)),
        )
        .bar_width(BAR_WIDTH as u16)
        .bar_gap(1)
        .bar_style(Style::default().fg(tailwind::BLUE.c900));

    if model.maximizing {
        match model.current_pane {
            Pane::List => {
                frame.render_stateful_widget(test_list, layout_main, &mut model.test_cases_list)
            }
            Pane::Console => frame.render_stateful_widget(info, layout_main, &mut model.info_state),
            Pane::Logger => frame.render_widget(logger, layout_main),
        }
    } else {
        frame.render_widget(logo, layout_logo);
        frame.render_stateful_widget(test_list, layout_list, &mut model.test_cases_list);
        frame.render_widget(logger, layout_logger);
        frame.render_widget(info_block, layout_rightup);
        frame.render_widget(tabs, layout_tabs);
        frame.render_stateful_widget(info, layout_info, &mut model.info_state);
        frame.render_widget(histogram, layout_rightdown);
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
        let mut scrl_interval = tokio::time::interval(Duration::from_secs_f32(0.05));
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
                            debug!("running selected test cases: selector = {selector:?}");
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
                            debug!("running all test cases");
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
        let mut test_results_buffer = HashMap::<(String, String), TestResult>::new();

        while !self.should_exit {
            tokio::select! {
                _ = draw_interval.tick() => {
                    terminal.draw(|frame| view(&mut model, frame))?;
                },
                _ = cmds_interval.tick() => {
                    let Some(cmd) = cmds.pop_front() else {
                        continue;
                    };

                    runner_tx.send(cmd)?;
                }
                _ = scrl_interval.tick() => {
                }
                _ = &mut runner_task => {
                }
                Ok(msg) = runner_rx.recv() => {
                    match msg {
                        tanu_core::runner::Message::Start(project_name, module_name, test_name) => {
                            test_results_buffer.insert((project_name.clone(), test_name.clone()), TestResult {
                                project_name,
                                module_name,
                                name: test_name,
                                ..Default::default()
                            });
                        },
                        tanu_core::runner::Message::HttpLog(project_name, _module_name, name, log) => {
                            if let Some(test_result) =  test_results_buffer.get_mut(&(project_name, name)) {
                                test_result.logs.push(log);
                            } else {
                                // TODO error
                            }
                        },
                        tanu_core::runner::Message::End(project_name, _module_name, name, test) => {
                            if let Some(mut test_result) = test_results_buffer.remove(&(project_name,name)) {
                                test_result.test = Some(test);
                                model.test_results.push(test_result);
                            } else {
                                // TODO error
                            }
                        },

                    }
                }
                Some(Ok(event)) = event_stream.next() => {
                    let msg = match event {
                        Event::Key(key) => {
                            match key.code {
                                KeyCode::Char('q') | KeyCode::Esc => {
                                    self.should_exit = true;
                                    continue;
                                },
                                _ => {
                                    self.handle_key(key, model.current_pane)
                                }
                            }
                        },
                        Event::Mouse(mouse) => {
                            // Only send SelectPane message for click events
                            if mouse.kind == crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left) {
                                Some(Message::SelectPane(mouse))
                            } else {
                                None
                            }
                        },
                        _ => {
                            continue;
                        }
                    };
                    let Some(msg) = msg else {
                        continue;
                    };
                    if let Some(cmd) = update(&mut model, msg).await? {
                        cmds.push_back(cmd);
                    }
                    trace!("updated {:?}", model.test_cases_list);
                }
            }
        }

        crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture
        )?;
        crossterm::terminal::disable_raw_mode()?;

        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent, current_pane: Pane) -> Option<Message> {
        trace!("key = {key:?}, current_pane = {current_pane:?}");

        if key.kind != KeyEventKind::Press {
            return None;
        }
        let modifier = key.modifiers;

        match (key.code, modifier) {
            (KeyCode::Char('z'), _) => return Some(Message::Maximize),
            (KeyCode::BackTab, KeyModifiers::SHIFT) => {
                return Some(Message::ConsoleTabSelect(TabMovement::Next))
            }
            (KeyCode::Tab, _) => return Some(Message::NextPane),
            _ => {}
        }

        match current_pane {
            Pane::Console => {
                match (key.code, modifier) {
                    (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                        return Some(Message::ConsoleSelect(CursorMovement::DownHalfScreen));
                    }
                    (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                        return Some(Message::ConsoleSelect(CursorMovement::UpHalfScreen));
                    }
                    _ => {}
                }

                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        Some(Message::ConsoleSelect(CursorMovement::Down))
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        Some(Message::ConsoleSelect(CursorMovement::Up))
                    }
                    KeyCode::Char('h') | KeyCode::Left => {
                        Some(Message::ConsoleTabSelect(TabMovement::Prev))
                    }
                    KeyCode::Char('l') | KeyCode::Right => {
                        Some(Message::ConsoleTabSelect(TabMovement::Next))
                    }
                    KeyCode::Char('g') | KeyCode::Home => Some(Message::ConsoleSelectFirst),
                    KeyCode::Char('G') | KeyCode::End => Some(Message::ConsoleSelectLast),
                    KeyCode::Enter => Some(Message::ConsoleShowHttpLog),
                    KeyCode::Char('1') => Some(Message::ExecuteAll),
                    _ => None,
                }
            }
            Pane::List => match key.code {
                KeyCode::Char('j') | KeyCode::Down => Some(Message::ListSelectNext),
                KeyCode::Char('k') | KeyCode::Up => Some(Message::ListSelectPrev),
                KeyCode::Char('g') | KeyCode::Home => Some(Message::ListSelectFirst),
                KeyCode::Char('G') | KeyCode::End => Some(Message::ListSelectLast),
                KeyCode::Char('h') | KeyCode::Left => {
                    Some(Message::ConsoleTabSelect(TabMovement::Prev))
                }
                KeyCode::Char('l') | KeyCode::Right => {
                    Some(Message::ConsoleTabSelect(TabMovement::Next))
                }
                KeyCode::Enter => Some(Message::ListExpand),
                KeyCode::Char('1') => Some(Message::ExecuteAll),
                KeyCode::Char('2') => Some(Message::ExecuteOne),
                _ => None,
            },
            Pane::Logger => match key.code {
                KeyCode::Char('j') | KeyCode::Down => Some(Message::LoggerSelectDown),
                KeyCode::Char('k') | KeyCode::Up => Some(Message::LoggerSelectUp),
                KeyCode::Char('h') | KeyCode::Left => Some(Message::LoggerSelectLeft),
                KeyCode::Char('l') | KeyCode::Right => Some(Message::LoggerSelectRight),
                KeyCode::Char(' ') => Some(Message::LoggerSelectSpace),
                KeyCode::Char('H') => Some(Message::LoggerSelectHide),
                KeyCode::Char('F') => Some(Message::LoggerSelectFocus),
                _ => None,
            },
        }
    }
}

/// Run tanu-tui app.
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
        tracing_subscriber::Registry::default().with(tui_logger::tracing_subscriber_layer());
    tracing::subscriber::set_global_default(subscriber)
        .wrap_err("failed to set global default subscriber")?;

    if std::env::var("RUST_BACKTRACE").is_err() {
        std::env::set_var("RUST_BACKTRACE", "full");
    }
    if std::env::var("COLORBT_SHOW_HIDDEN").is_err() {
        std::env::set_var("COLORBT_SHOW_HIDDEN", "1");
    }

    dotenv::dotenv().ok();
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
    ratatui::restore();
    println!("tanu-tui terminated with {result:?}");
    result
}
