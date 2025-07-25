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
use itertools::Itertools;
use ratatui::{
    crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind},
    layout::Position,
    prelude::*,
    style::{Modifier, Style},
    text::Line,
    widgets::{
        block::{BorderType, Padding},
        Bar, BarChart, BarGroup, Block, Borders, LineGauge, Paragraph, Tabs,
    },
    Frame,
};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    time::Duration,
};
use tanu_core::{
    get_tanu_config,
    runner::{self, EventBody},
    Runner, TestInfo,
};
use tokio::sync::mpsc;
use tracing::{error, info, trace};
use tracing_subscriber::layer::SubscriberExt;
use tui_big_text::{BigText, PixelSize};
use tui_logger::{TuiLoggerLevelOutput, TuiLoggerSmartWidget, TuiWidgetEvent, TuiWidgetState};

pub const WHITESPACE: &str = "\u{00A0}";

const SELECTED_STYLE: Style = Style::new().bg(Color::Black).add_modifier(Modifier::BOLD);

use crate::widget::{
    info::{InfoState, InfoWidget, Tab},
    list::{ExecutionStateController, TestCaseSelector, TestListState, TestListWidget},
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
    Info,
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
    /// Stores the last mouse click event, if any. When `click` is not `None`, it indicates that the user has clicked on a specific area of the UI.
    click: Option<crossterm::event::MouseEvent>,
    /// Measures the frames per second (FPS).
    fps_counter: FpsCounter,
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
            fps_counter: FpsCounter::new(),
        }
    }

    fn next_pane(&mut self) {
        let current_index = self.current_pane as usize;
        let pane_counts = Pane::Logger as usize + 1;
        let next_index = (current_index + 1) % pane_counts;
        if let Some(next_pane) = Pane::from_repr(next_index) {
            self.current_pane = next_pane;
        }
        self.info_state.focused = self.current_pane == Pane::Info;
    }
}

#[derive(Debug)]
enum Message {
    Maximize,
    NextPane,
    ListSelect(CursorMovement),
    ListExpand,
    InfoSelect(CursorMovement),
    InfoTabSelect(TabMovement),
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

/// Reset the offset of the list or info pane.
fn offset_begin(model: &mut Model) {
    match model.info_state.selected_tab {
        Tab::Payload => {
            model.info_state.payload_state.scroll_offset = 0;
        }
        Tab::Error => {
            model.info_state.error_state.scroll_offset = 0;
        }
        _ => {}
    }
}

/// Move the offset of the list or info pane to the last.
fn offset_end(_model: &mut Model) {
    // TODO
}

/// Move down the offset of the list or info pane.
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

    let terminal_height = crossterm::terminal::size()?.1 as usize;
    match msg {
        Message::Maximize => {
            model.maximizing = !model.maximizing;
        }
        Message::NextPane => {
            model.next_pane();
        }
        Message::ListSelect(CursorMovement::Down) => model.test_cases_list.list_state.select_next(),
        Message::ListSelect(CursorMovement::Up) => {
            model.test_cases_list.list_state.select_previous();
        }
        Message::ListSelect(CursorMovement::UpHalfScreen) => {
            let offset = terminal_height / 4;
            let selected = model
                .test_cases_list
                .list_state
                .selected()
                .unwrap_or_default();
            model
                .test_cases_list
                .list_state
                .select(Some(selected.saturating_sub(offset)));
        }
        Message::ListSelect(CursorMovement::DownHalfScreen) => {
            let offset = terminal_height / 4;
            let selected = model
                .test_cases_list
                .list_state
                .selected()
                .unwrap_or_default();
            model
                .test_cases_list
                .list_state
                .select(Some(selected + offset));
        }
        Message::ListSelect(CursorMovement::Home) => {
            model.test_cases_list.list_state.select_first();
        }
        Message::ListSelect(CursorMovement::End) => {
            model.test_cases_list.list_state.select_last();
        }
        Message::ListExpand => model.test_cases_list.expand(&model.test_results),
        Message::InfoSelect(CursorMovement::Down) => {
            offset_down(model, 1);
        }
        Message::InfoSelect(CursorMovement::DownHalfScreen) => {
            offset_down(model, (terminal_height / 2) as i16);
        }
        Message::InfoSelect(CursorMovement::Up) => {
            offset_up(model, 1);
        }
        Message::InfoSelect(CursorMovement::UpHalfScreen) => {
            offset_up(model, (terminal_height / 2) as i16);
        }
        Message::InfoSelect(CursorMovement::Home) => {
            offset_begin(model);
        }
        Message::InfoSelect(CursorMovement::End) => {
            offset_end(model);
        }
        Message::InfoTabSelect(TabMovement::Next) => {
            model.info_state.next_tab();
        }
        Message::InfoTabSelect(TabMovement::Prev) => {
            model.info_state.prev_tab();
        }

        Message::LoggerSelectDown => model.logger_state.transition(TuiWidgetEvent::DownKey),
        Message::LoggerSelectUp => model.logger_state.transition(TuiWidgetEvent::UpKey),
        Message::LoggerSelectLeft => model.logger_state.transition(TuiWidgetEvent::LeftKey),
        Message::LoggerSelectRight => model.logger_state.transition(TuiWidgetEvent::RightKey),
        Message::LoggerSelectSpace => model.logger_state.transition(TuiWidgetEvent::SpaceKey),
        Message::LoggerSelectHide => model.logger_state.transition(TuiWidgetEvent::HideKey),
        Message::LoggerSelectFocus => model.logger_state.transition(TuiWidgetEvent::FocusKey),
        Message::ExecuteOne => {
            model.current_exec = Some(Execution::One);
            let Some(selector) = model.test_cases_list.select_test_case(&model.test_results) else {
                return Ok(None);
            };
            ExecutionStateController::execute_specified(&mut model.test_cases_list, &selector);
            return Ok(Some(Command::ExecuteOne(selector)));
        }
        Message::ExecuteAll => {
            model.test_results.clear();
            model.current_exec = Some(Execution::All);
            ExecutionStateController::execute_all(&mut model.test_cases_list);
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

    let [layout_main, layout_menu, layout_gauge] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    let [layout_left, layout_right] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(layout_main);
    let [layout_rightup, layout_rightdown] =
        Layout::vertical([Constraint::Percentage(70), Constraint::Percentage(30)])
            .areas(layout_right);
    let [layout_histogram, layout_summary] =
        Layout::horizontal([Constraint::Percentage(70), Constraint::Percentage(30)])
            .areas(layout_rightdown);
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
    let [layout_logo, layout_fps] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(9)]).areas(layout_logo);
    let layout_menu_items = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(9),  // q
            Constraint::Length(13), // z
            Constraint::Length(12), // 1
            Constraint::Length(8),  // 2
            Constraint::Length(16), // tab
            Constraint::Length(15), // ←|→
            Constraint::Length(14), // ↑|↓
            Constraint::Length(26), // CTRL+U|CTRL+D
            Constraint::Length(10), // g
            Constraint::Length(9),  // G
            Constraint::Length(15), // Enter
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
            model.current_pane = Pane::Info;
            model.info_state.focused = true;
        } else if layout_tabs.contains(position) {
            model.current_pane = Pane::Info;
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

    let fps =
        Paragraph::new(format!("FPS:{:.1}", model.fps_counter.fps)).alignment(Alignment::Right);

    let ratio =
        (model.test_results.len() as f64 / model.test_cases_list.len() as f64).clamp(0.0, 1.0);
    let gauge = LineGauge::default()
        .block(
            Block::default()
                .borders(Borders::NONE)
                .padding(Padding::new(1, 1, 0, 0)),
        )
        .filled_style(Style::new().red())
        .unfilled_style(Style::new().black())
        .ratio(ratio)
        .label(if ratio == 0.0 {
            "".to_string() // Hide label when no tests are running
        } else {
            format!("{}%", (ratio * 100.0).round() as u32)
        });

    let menu_items = [
        ("[q]", "Quit"),
        ("[z]", "Maximize"),
        ("[1]", "Run ALL"),
        ("[2]", "Run"),
        ("[Tab]", "Next Pane"),
        ("[←|→]", "Next Tab"),
        ("[↑|↓]", "Up/Down"),
        if matches!(model.current_pane, Pane::List | Pane::Info) {
            ("[CTRL+U|D]", "Scroll Up/Down")
        } else {
            ("", "")
        },
        if matches!(model.current_pane, Pane::List | Pane::Info) {
            ("[g]", "First")
        } else {
            ("", "")
        },
        if matches!(model.current_pane, Pane::List | Pane::Info) {
            ("[G]", "Last")
        } else {
            ("", "")
        },
        if matches!(model.current_pane, Pane::List) {
            ("[Enter]", "Expand")
        } else {
            ("", "")
        },
    ];

    for (n, &(key, label)) in menu_items.iter().enumerate() {
        let menu_item = Paragraph::new(vec![Line::from(vec![
            Span::styled(key, Style::default().bold()),
            Span::styled(format!("{WHITESPACE}{label}"), Style::default()),
        ])])
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
        .style(Style::new().fg(Color::Blue))
        .lines(vec!["tanu".into()])
        .build();

    let test_list = TestListWidget::new(
        matches!(model.current_pane, Pane::List),
        &model.test_cases_list.projects,
    );

    let logger = TuiLoggerSmartWidget::default()
        .title_target("Selector".bold())
        .title_log("Logs".bold())
        .border_type(if matches!(model.current_pane, Pane::Logger) {
            BorderType::Thick
        } else {
            BorderType::Plain
        })
        .style_error(Style::default().fg(Color::Red))
        .style_warn(Style::default().fg(Color::Yellow))
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
        .unwrap_or_default();

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
        .bar_style(Style::default().fg(Color::Blue));

    let grouped_by_project = model
        .test_results
        .iter()
        .into_group_map_by(|result| result.project_name.clone());
    let project_test_summary: Vec<_> = get_tanu_config()
        .projects
        .iter()
        .filter_map(|project| {
            let test_results = grouped_by_project.get(&project.name)?;
            let successful = test_results
                .iter()
                .filter(|result| result.test.as_ref().is_some_and(|test| test.result.is_ok()))
                .count();
            Some((
                project.name.clone(),
                successful,
                test_results.len() - successful,
            ))
        })
        .collect();

    // Create bar groups for each project (maintaining original order)
    let bar_groups = project_test_summary
        .iter()
        .map(|(name, success, fail)| {
            BarGroup::default()
                .label(Line::from(name.to_owned()).centered())
                .bars(&[
                    Bar::default()
                        .value(*success as u64)
                        .text_value(format!("success: {success}"))
                        .value_style(Style::new().bg(Color::Green).fg(Color::Black))
                        .style(Color::Green),
                    Bar::default()
                        .value(*fail as u64)
                        .text_value(format!("fail: {fail}"))
                        .value_style(Style::new().bg(Color::Red).fg(Color::Black))
                        .style(Color::Red),
                ])
        })
        .collect::<Vec<_>>();

    // Create the bar chart with horizontal orientation
    let mut bar_chart = BarChart::default()
        .block(
            Block::new()
                .title("Summary".bold())
                .borders(Borders::ALL)
                .padding(Padding::new(0, 1, 1, 1)),
        )
        .direction(Direction::Horizontal)
        .bar_width(1)
        .bar_gap(0)
        .group_gap(2);

    for bar_group in bar_groups {
        bar_chart = bar_chart.data(bar_group);
    }

    if model.maximizing {
        match model.current_pane {
            Pane::List => {
                frame.render_stateful_widget(test_list, layout_main, &mut model.test_cases_list)
            }
            Pane::Info => frame.render_stateful_widget(info, layout_main, &mut model.info_state),
            Pane::Logger => frame.render_widget(logger, layout_main),
        }
    } else {
        frame.render_widget(fps, layout_fps);
        frame.render_widget(gauge, layout_gauge);
        frame.render_widget(logo, layout_logo);
        frame.render_stateful_widget(test_list, layout_list, &mut model.test_cases_list);
        frame.render_widget(logger, layout_logger);
        frame.render_widget(info_block, layout_rightup);
        frame.render_widget(tabs, layout_tabs);
        frame.render_stateful_widget(info, layout_info, &mut model.info_state);
        frame.render_widget(histogram, layout_histogram);
        frame.render_widget(bar_chart, layout_summary);
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
        let mut test_results_buffer = HashMap::<(String, String), TestResult>::new();

        while !self.should_exit {
            tokio::select! {
                _ = draw_interval.tick() => {
                    model.fps_counter.update();
                    let start_draw = std::time::Instant::now();
                    terminal.draw(|frame| view(&mut model, frame))?;
                    trace!("Took {:?} to draw", start_draw.elapsed());
                },
                _ = cmds_interval.tick() => {
                    let Some(cmd) = cmds.pop_front() else {
                        continue;
                    };

                    runner_tx.send(cmd)?;
                }
                _ = scrl_interval.tick() => {
                }
                _ = thrb_interval.tick() => {
                    ExecutionStateController::update_throbber(&mut model.test_cases_list);
                }
                _ = &mut runner_task => {
                }
                Ok(msg) = runner_rx.recv() => {
                    match msg {
                        runner::Event {project, module, test, body: EventBody::Start} => {
                            test_results_buffer.insert((project.clone(), test.clone()), TestResult {
                                project_name: project,
                                module_name: module,
                                name: test,
                                ..Default::default()
                            });
                        },
                        runner::Event {project: _, module: _, test: _, body: EventBody::Check(_)} => {
                        }
                        runner::Event {project, module: _, test, body: EventBody::Http(log)} => {
                            if let Some(test_result) = test_results_buffer.get_mut(&(project, test)) {
                                test_result.logs.push(log);
                            } else {
                                // TODO error
                            }
                        },
                        runner::Event {project: _, module: _, test: _, body: EventBody::Retry} => {
                        }
                        runner::Event {project, module, test: test_name, body: EventBody::End(test)} => {
                            if let Some(mut test_result) = test_results_buffer.remove(&(project.clone(), test_name.clone())) {
                                test_result.test = Some(test);
                                ExecutionStateController::on_test_updated(
                                    &mut model.test_cases_list,
                                    &project,
                                    &module,
                                    &test_name,
                                    test_result.clone(),
                                );
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

        match (current_pane, key.code, modifier) {
            (_, KeyCode::Char('z'), _) => Some(Message::Maximize),
            (_, KeyCode::BackTab, KeyModifiers::SHIFT) => {
                Some(Message::InfoTabSelect(TabMovement::Next))
            }
            (_, KeyCode::Tab, _) => Some(Message::NextPane),
            (Pane::Info, KeyCode::Char('j') | KeyCode::Down, _) => {
                Some(Message::InfoSelect(CursorMovement::Down))
            }
            (Pane::Info, KeyCode::Char('k') | KeyCode::Up, _) => {
                Some(Message::InfoSelect(CursorMovement::Up))
            }
            (Pane::Info, KeyCode::Char('h') | KeyCode::Left, _) => {
                Some(Message::InfoTabSelect(TabMovement::Prev))
            }
            (Pane::Info, KeyCode::Char('l') | KeyCode::Right, _) => {
                Some(Message::InfoTabSelect(TabMovement::Next))
            }
            (Pane::Info, KeyCode::Char('g') | KeyCode::Home, _) => {
                Some(Message::InfoSelect(CursorMovement::Home))
            }
            (Pane::Info, KeyCode::Char('G') | KeyCode::End, _) => {
                Some(Message::InfoSelect(CursorMovement::End))
            }
            (Pane::Info, KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                Some(Message::InfoSelect(CursorMovement::DownHalfScreen))
            }
            (Pane::Info, KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                Some(Message::InfoSelect(CursorMovement::UpHalfScreen))
            }
            (Pane::Info, KeyCode::Char('1'), _) => Some(Message::ExecuteAll),
            (Pane::List, KeyCode::Char('j') | KeyCode::Down, _) => {
                Some(Message::ListSelect(CursorMovement::Down))
            }
            (Pane::List, KeyCode::Char('k') | KeyCode::Up, _) => {
                Some(Message::ListSelect(CursorMovement::Up))
            }
            (Pane::List, KeyCode::Char('g') | KeyCode::Home, _) => {
                Some(Message::ListSelect(CursorMovement::Home))
            }
            (Pane::List, KeyCode::Char('G') | KeyCode::End, _) => {
                Some(Message::ListSelect(CursorMovement::End))
            }
            (Pane::List, KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                Some(Message::ListSelect(CursorMovement::DownHalfScreen))
            }
            (Pane::List, KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                Some(Message::ListSelect(CursorMovement::UpHalfScreen))
            }
            (Pane::List, KeyCode::Char('h') | KeyCode::Left, _) => {
                Some(Message::InfoTabSelect(TabMovement::Prev))
            }
            (Pane::List, KeyCode::Char('l') | KeyCode::Right, _) => {
                Some(Message::InfoTabSelect(TabMovement::Next))
            }
            (Pane::List, KeyCode::Enter, _) => Some(Message::ListExpand),
            (Pane::List, KeyCode::Char('1'), _) => Some(Message::ExecuteAll),
            (Pane::List, KeyCode::Char('2'), _) => Some(Message::ExecuteOne),
            (Pane::Logger, KeyCode::Char('j') | KeyCode::Down, _) => {
                Some(Message::LoggerSelectDown)
            }
            (Pane::Logger, KeyCode::Char('k') | KeyCode::Up, _) => Some(Message::LoggerSelectUp),
            (Pane::Logger, KeyCode::Char('h') | KeyCode::Left, _) => {
                Some(Message::LoggerSelectLeft)
            }
            (Pane::Logger, KeyCode::Char('l') | KeyCode::Right, _) => {
                Some(Message::LoggerSelectRight)
            }
            (Pane::Logger, KeyCode::Char(' '), _) => Some(Message::LoggerSelectSpace),
            (Pane::Logger, KeyCode::Char('H'), _) => Some(Message::LoggerSelectHide),
            (Pane::Logger, KeyCode::Char('F'), _) => Some(Message::LoggerSelectFocus),
            _ => {
                // Ignore other keys
                None
            }
        }
    }
}

/// Runs the tanu terminal user interface application.
///
/// Initializes and runs the interactive TUI for managing and executing tanu tests.
/// The TUI provides three main panes: test list, test information/console, and logger.
/// Users can navigate with keyboard shortcuts to select tests, run them individually
/// or in bulk, and monitor execution in real-time.
///
/// # Parameters
///
/// - `runner`: The configured test runner containing test cases and configuration
/// - `log_level`: General logging level for the TUI and external libraries
/// - `tanu_log_level`: Specific logging level for tanu framework components
///
/// # Features
///
/// - **Interactive Test Selection**: Browse and select tests with arrow keys
/// - **Real-time Execution**: Watch tests run with live updates and logs
/// - **HTTP Request Monitoring**: View detailed HTTP request/response data
/// - **Concurrent Execution**: Run multiple tests simultaneously
/// - **Filtering**: Filter tests by project, module, or name
/// - **Logging**: Integrated logger pane for debugging
///
/// # Keyboard Shortcuts
///
/// - `↑/↓`: Navigate test list
/// - `Enter`: Run selected test
/// - `a`: Run all tests
/// - `Tab`: Switch between panes
/// - `q`/`Esc`: Quit application
/// - `Ctrl+U/D`: Page up/down in test list
/// - `Home/End`: Go to first/last test
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
