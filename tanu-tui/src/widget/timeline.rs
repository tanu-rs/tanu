//! Timeline of test executions per worker, similar to Allure's timeline view.
//!
//! Each row is a worker (lane) and each test is drawn as a horizontal bar from
//! its start to its end time, blue if it passed and red if it failed.
use ratatui::prelude::*;
use std::time::{Duration, SystemTime};

use crate::widget::theme::{self, fmt_duration, muted};

/// A test execution to draw on the timeline.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Worker that executed the test; negative if unknown.
    pub lane: isize,
    pub start: SystemTime,
    pub end: SystemTime,
    pub ok: bool,
    /// true if the test is selected in the test list.
    pub selected: bool,
    /// Caller-defined identifier returned for mouse clicks.
    pub key: usize,
}

/// A bar in plot coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Segment {
    row: u16,
    x0: u16,
    /// Exclusive.
    x1: u16,
    ok: bool,
    selected: bool,
    key: usize,
}

#[derive(Debug)]
struct TimelineLayout {
    /// Lane label of each slot (a slot is one or more lanes drawn together).
    labels: Vec<String>,
    /// Terminal rows per slot, including a gap row when there is room.
    slot_height: u16,
    /// Terminal rows of a bar within a slot.
    bar_height: u16,
    segments: Vec<Segment>,
    /// Wall-clock time covered by the plot.
    span: Duration,
}

/// Assigns lanes to entries without a worker id, packing them greedily so
/// that entries in the same lane do not overlap.
fn assign_lanes(entries: &mut [Entry]) {
    let first_free = entries.iter().map(|e| e.lane).max().unwrap_or(-1) + 1;
    let mut unassigned: Vec<usize> = (0..entries.len())
        .filter(|i| entries[*i].lane < 0)
        .collect();
    unassigned.sort_by_key(|i| entries[*i].start);

    // End time of the last entry in each extra lane.
    let mut lane_ends: Vec<SystemTime> = vec![];
    for i in unassigned {
        let entry = &mut entries[i];
        let lane = match lane_ends.iter().position(|end| *end <= entry.start) {
            Some(lane) => lane,
            None => {
                lane_ends.push(entry.start);
                lane_ends.len() - 1
            }
        };
        lane_ends[lane] = entry.end;
        entry.lane = first_free + lane as isize;
    }
}

fn layout(entries: &[Entry], width: u16, height: u16) -> TimelineLayout {
    let mut entries = entries.to_vec();
    assign_lanes(&mut entries);

    let mut lanes: Vec<isize> = entries.iter().map(|e| e.lane).collect();
    lanes.sort_unstable();
    lanes.dedup();

    // If there are more lanes than rows, several lanes share a row. If there are
    // fewer, lanes get taller (up to 4 rows including a gap row between lanes).
    let rows = (lanes.len() as u16).min(height.max(1)).max(1);
    let slot_height = (height / rows).clamp(1, 4);
    let bar_height = if slot_height >= 2 { slot_height - 1 } else { 1 };
    let row_of = |lane: isize| -> u16 {
        let index = lanes.binary_search(&lane).unwrap_or_default();
        (index * rows as usize / lanes.len().max(1)) as u16
    };
    let mut labels = vec![String::new(); rows as usize];
    for lane in lanes.iter().rev() {
        labels[row_of(*lane) as usize] = lane.to_string();
    }

    let t0 = entries.iter().map(|e| e.start).min();
    let t1 = entries.iter().map(|e| e.end).max();
    let span = match (t0, t1) {
        (Some(t0), Some(t1)) => t1.duration_since(t0).unwrap_or_default(),
        _ => Duration::ZERO,
    }
    .max(Duration::from_millis(1));
    let t0 = t0.unwrap_or(SystemTime::UNIX_EPOCH);

    let to_x = |t: SystemTime| -> f64 {
        let offset = t.duration_since(t0).unwrap_or_default();
        offset.as_secs_f64() / span.as_secs_f64() * width as f64
    };

    let segments = entries
        .iter()
        .map(|e| {
            let x0 = (to_x(e.start).floor() as u16).min(width.saturating_sub(1));
            let x1 = (to_x(e.end).ceil() as u16).clamp(x0 + 1, width.max(x0 + 1));
            Segment {
                row: row_of(e.lane),
                x0,
                x1,
                ok: e.ok,
                selected: e.selected,
                key: e.key,
            }
        })
        .collect();

    TimelineLayout {
        labels,
        slot_height,
        bar_height,
        segments,
        span,
    }
}

/// Summary for the chart title: wall time, number of workers and utilization.
pub fn summary(entries: &[Entry]) -> Option<String> {
    let t0 = entries.iter().map(|e| e.start).min()?;
    let t1 = entries.iter().map(|e| e.end).max()?;
    let wall = t1.duration_since(t0).unwrap_or_default();
    let mut entries = entries.to_vec();
    assign_lanes(&mut entries);
    let mut lanes: Vec<isize> = entries.iter().map(|e| e.lane).collect();
    lanes.sort_unstable();
    lanes.dedup();
    let busy: f64 = entries
        .iter()
        .map(|e| {
            e.end
                .duration_since(e.start)
                .unwrap_or_default()
                .as_secs_f64()
        })
        .sum();
    let capacity = wall.as_secs_f64() * lanes.len() as f64;
    let utilization = if capacity > 0.0 {
        (busy / capacity * 100.0).min(100.0)
    } else {
        0.0
    };
    Some(format!(
        "  {} tests · {} · {} workers · {:.0}% busy",
        entries.len(),
        fmt_duration(wall),
        lanes.len(),
        utilization
    ))
}

/// Renders the timeline into `area` and returns the clickable area of each bar with its key.
pub fn render(entries: &[Entry], area: Rect, buf: &mut Buffer) -> Vec<(Rect, usize)> {
    if entries.is_empty() || area.height < 2 || area.width < 8 {
        return vec![];
    }

    let label_width = entries
        .iter()
        .map(|e| e.lane.max(0).to_string().len() as u16)
        .max()
        .unwrap_or(1)
        .max(2)
        + 1;
    let [layout_labels, layout_plot] =
        Layout::horizontal([Constraint::Length(label_width), Constraint::Fill(1)]).areas(area);
    let [layout_plot, layout_axis] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(layout_plot);
    let layout_labels = Rect {
        height: layout_plot.height,
        ..layout_labels
    };

    let timeline = layout(entries, layout_plot.width, layout_plot.height);

    // Lane labels
    for (row, label) in timeline.labels.iter().enumerate() {
        buf.set_string(
            layout_labels.x,
            layout_labels.y + row as u16 * timeline.slot_height,
            format!("{label:>width$}", width = label_width as usize - 1),
            muted(),
        );
    }

    // Consecutive tests in a lane alternate between two shades so that they can be
    // told apart without drawing separators.
    let mut by_position: Vec<usize> = (0..timeline.segments.len()).collect();
    by_position.sort_by_key(|i| (timeline.segments[*i].row, timeline.segments[*i].x0));
    let mut alternate = vec![false; timeline.segments.len()];
    for pair in by_position.windows(2) {
        let (prev, next) = (pair[0], pair[1]);
        if timeline.segments[prev].row == timeline.segments[next].row {
            alternate[next] = !alternate[prev];
        }
    }

    // With one row per lane, bars are drawn slightly shorter than the row so that
    // neighbouring lanes do not touch; taller lanes have a gap row instead.
    let symbol = if timeline.slot_height == 1 {
        "▆"
    } else {
        "█"
    };

    // Passed first, then failed and selected on top so that they stay visible.
    let mut order: Vec<usize> = (0..timeline.segments.len()).collect();
    order.sort_by_key(|i| {
        let s = &timeline.segments[*i];
        (s.selected, !s.ok)
    });
    // Paint bars into a grid of colors first and write each cell once: many short
    // tests overlap in the same cells, and writing the buffer is the expensive part.
    let width = layout_plot.width as usize;
    let mut grid: Vec<Option<Color>> = vec![None; width * timeline.labels.len()];
    let mut hits = vec![];
    for i in order {
        let segment = &timeline.segments[i];
        let color = match (segment.selected, segment.ok, alternate[i]) {
            (true, ..) => theme::bar_selected(),
            (false, true, false) => theme::bar(),
            (false, true, true) => theme::bar_alt(),
            (false, false, false) => theme::bar_error(),
            (false, false, true) => theme::bar_error_alt(),
        };
        let row = segment.row as usize * width;
        grid[row + segment.x0 as usize..row + segment.x1 as usize].fill(Some(color));
        hits.push((
            Rect::new(
                layout_plot.x + segment.x0,
                layout_plot.y + segment.row * timeline.slot_height,
                segment.x1 - segment.x0,
                timeline.bar_height,
            ),
            segment.key,
        ));
    }
    for (slot, colors) in grid.chunks(width.max(1)).enumerate() {
        let y = layout_plot.y + slot as u16 * timeline.slot_height;
        for dy in 0..timeline.bar_height {
            for (x, color) in colors.iter().enumerate() {
                let Some(color) = color else {
                    continue;
                };
                if let Some(cell) = buf.cell_mut((layout_plot.x + x as u16, y + dy)) {
                    cell.set_symbol(symbol).set_fg(*color);
                }
            }
        }
    }

    // Time axis
    const TICK_SPACING: u16 = 14;
    let mut x = 0;
    while x < layout_plot.width {
        let t = timeline.span.mul_f64(x as f64 / layout_plot.width as f64);
        let label = if x == 0 {
            "┆0".to_string()
        } else {
            format!("┆{}", fmt_duration(t))
        };
        if x + label.chars().count() as u16 <= layout_plot.width {
            buf.set_string(layout_axis.x + x, layout_axis.y, label, muted());
        }
        x += TICK_SPACING;
    }

    // Later bars were drawn on top; clicks should hit them first.
    hits.reverse();
    hits
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    fn entry(lane: isize, start_ms: u64, end_ms: u64, ok: bool, key: usize) -> Entry {
        let t =
            |ms| SystemTime::UNIX_EPOCH + Duration::from_secs(1_000) + Duration::from_millis(ms);
        Entry {
            lane,
            start: t(start_ms),
            end: t(end_ms),
            ok,
            selected: false,
            key,
        }
    }

    #[test]
    fn layout_scales_to_width() {
        let entries = vec![
            entry(0, 0, 500, true, 0),
            entry(0, 500, 1000, false, 1),
            entry(3, 250, 750, true, 2),
        ];
        let timeline = layout(&entries, 100, 10);
        assert_eq!(vec!["0", "3"], timeline.labels);
        // Two lanes in 10 rows: each lane gets a 3-row bar and a gap row.
        assert_eq!((4, 3), (timeline.slot_height, timeline.bar_height));
        assert_eq!(Duration::from_millis(1000), timeline.span);
        let bounds: Vec<_> = timeline
            .segments
            .iter()
            .map(|s| (s.row, s.x0, s.x1, s.key))
            .collect();
        assert_eq!(vec![(0, 0, 50, 0), (0, 50, 100, 1), (1, 25, 75, 2)], bounds);
    }

    #[test]
    fn short_tests_are_at_least_one_cell_wide() {
        let entries = vec![entry(0, 0, 1000, true, 0), entry(1, 999, 1000, true, 1)];
        let timeline = layout(&entries, 10, 10);
        assert_eq!((9, 10), (timeline.segments[1].x0, timeline.segments[1].x1));
    }

    #[test]
    fn lanes_share_rows_when_there_is_no_room() {
        let entries: Vec<_> = (0..8).map(|n| entry(n, 0, 10, true, n as usize)).collect();
        let timeline = layout(&entries, 10, 4);
        assert_eq!(4, timeline.labels.len());
        assert_eq!((1, 1), (timeline.slot_height, timeline.bar_height));
        let rows: Vec<_> = timeline.segments.iter().map(|s| s.row).collect();
        assert_eq!(vec![0, 0, 1, 1, 2, 2, 3, 3], rows);
    }

    #[test]
    fn unknown_workers_are_packed_into_lanes() {
        let mut entries = vec![
            entry(-1, 0, 100, true, 0),
            entry(-1, 50, 150, true, 1),
            entry(-1, 100, 200, true, 2),
        ];
        assign_lanes(&mut entries);
        let lanes: Vec<_> = entries.iter().map(|e| e.lane).collect();
        assert_eq!(vec![0, 1, 0], lanes);
    }

    #[test]
    fn summary_reports_utilization() {
        let entries = vec![entry(0, 0, 1000, true, 0), entry(1, 0, 500, true, 1)];
        assert_eq!(
            Some("  2 tests · 1.00s · 2 workers · 75% busy".to_string()),
            summary(&entries)
        );
        assert_eq!(None, summary(&[]));
    }
}
