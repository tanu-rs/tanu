//! Latency histogram of HTTP/gRPC calls with log-scale buckets.
//!
//! Buckets grow as 1, 2, 5, 10, 20, 50, ... ms so that fast and slow calls are both
//! visible. Each bar is stacked from the bottom: calls of the test selected in the
//! test list (light blue), error responses (muted red), and the other calls (blue).
//!
//! A one-row bar above the histogram shows the mix of status code classes.
use ratatui::prelude::*;
use std::time::Duration;

use crate::widget::theme::{self, fmt_duration, muted};

/// A call to count in the histogram.
#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub latency: Duration,
    /// true if the call failed (HTTP 4xx/5xx, non-OK gRPC status).
    pub error: bool,
    /// true if the call belongs to the test selected in the test list.
    pub selected: bool,
    pub class: StatusClass,
}

/// Class of a response status. gRPC OK counts as `Success`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumIter)]
pub enum StatusClass {
    Success,
    Redirect,
    ClientError,
    ServerError,
    GrpcError,
    /// 1xx and non-standard codes.
    Other,
}

impl StatusClass {
    fn label(self) -> &'static str {
        match self {
            StatusClass::Success => "2xx",
            StatusClass::Redirect => "3xx",
            StatusClass::ClientError => "4xx",
            StatusClass::ServerError => "5xx",
            StatusClass::GrpcError => "gRPC err",
            StatusClass::Other => "other",
        }
    }

    fn color(self) -> Color {
        match self {
            StatusClass::Success => theme::ok(),
            StatusClass::Redirect => theme::accent(),
            StatusClass::ClientError => theme::bar_error(),
            StatusClass::ServerError | StatusClass::GrpcError => theme::fail(),
            StatusClass::Other => theme::border(),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Bucket {
    /// Inclusive lower bound in microseconds.
    lower_us: u128,
    ok: u64,
    error: u64,
    selected: u64,
}

impl Bucket {
    fn total(&self) -> u64 {
        self.ok + self.error + self.selected
    }
}

/// Upper bounds (exclusive, in microseconds) of the buckets: 1ms, 2ms, 5ms, 10ms, ...
fn edges_up_to(max_us: u128) -> Vec<u128> {
    let mut edges = vec![];
    let mut magnitude = 1_000u128;
    loop {
        for m in [1, 2, 5] {
            let edge = m * magnitude;
            edges.push(edge);
            if edge > max_us {
                return edges;
            }
        }
        magnitude *= 10;
    }
}

/// Groups samples into log-scale buckets, trimming empty buckets at both ends.
fn buckets(samples: &[Sample]) -> Vec<Bucket> {
    let Some(max_us) = samples.iter().map(|s| s.latency.as_micros()).max() else {
        return vec![];
    };
    let edges = edges_up_to(max_us);
    let mut buckets: Vec<Bucket> = std::iter::once(0)
        .chain(edges.iter().copied())
        .take(edges.len())
        .map(|lower_us| Bucket {
            lower_us,
            ..Default::default()
        })
        .collect();
    for sample in samples {
        let us = sample.latency.as_micros();
        let index = edges
            .iter()
            .position(|edge| us < *edge)
            .unwrap_or(edges.len() - 1);
        let bucket = &mut buckets[index];
        if sample.selected {
            bucket.selected += 1;
        } else if sample.error {
            bucket.error += 1;
        } else {
            bucket.ok += 1;
        }
    }
    let first = buckets.iter().position(|b| b.total() > 0).unwrap_or(0);
    let last = buckets.iter().rposition(|b| b.total() > 0).unwrap_or(0);
    buckets[first..=last].to_vec()
}

/// Label of a bucket's lower bound: `0`, `1ms`, `20ms`, `1s`, `10s`.
fn edge_label(us: u128) -> String {
    match us {
        0 => "0".into(),
        us if us < 1_000_000 => format!("{}ms", us / 1_000),
        us => format!("{}s", us / 1_000_000),
    }
}

/// Nearest-rank percentile (`p` in 0.0..=1.0) of sorted durations; zero if empty.
pub fn percentile(sorted: &[Duration], p: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let rank = (p * (sorted.len() - 1) as f64).round() as usize;
    sorted[rank]
}

/// Title details: percentiles, max and the number of failed calls.
pub fn summary(samples: &[Sample]) -> Option<Vec<Span<'static>>> {
    let mut latencies: Vec<Duration> = samples.iter().map(|s| s.latency).collect();
    if latencies.is_empty() {
        return None;
    }
    latencies.sort_unstable();
    let pct = |p: f64| fmt_duration(percentile(&latencies, p));
    let mut spans = vec![Span::styled(
        format!(
            "  {} calls · p50 {} · p95 {} · p99 {} · max {}",
            latencies.len(),
            pct(0.5),
            pct(0.95),
            pct(0.99),
            fmt_duration(*latencies.last().unwrap_or(&Duration::ZERO)),
        ),
        muted(),
    )];
    let errors = samples.iter().filter(|s| s.error).count();
    if errors > 0 {
        spans.push(Span::styled(
            if errors == 1 {
                " · 1 error response".to_string()
            } else {
                format!(" · {errors} error responses")
            },
            Style::new().fg(theme::bar_error()),
        ));
    }
    Some(spans)
}

const BLOCKS: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

/// Heights in eighths of a row of the stacked segments (selected, error, ok) of a bar.
///
/// Only the top of the bar has sub-row precision; lower segments take whole rows so
/// that each row has a single color. A non-empty segment is at least one row tall
/// (or one eighth if it is the top), so that e.g. a single error response stays visible.
fn segment_heights(bucket: &Bucket, max_total: u64, plot_rows: u16) -> [u32; 3] {
    let total_eighths = if bucket.total() == 0 {
        0
    } else {
        ((bucket.total() as f64 / max_total as f64 * plot_rows as f64 * 8.0).round() as u32).max(1)
    };
    let counts = [bucket.selected, bucket.error, bucket.ok];
    let top = counts.iter().rposition(|c| *c > 0).unwrap_or(0);
    let mut heights = [0u32; 3];
    let mut used = 0;
    for (i, count) in counts.iter().enumerate() {
        if *count == 0 {
            continue;
        }
        if i == top {
            heights[i] = total_eighths.saturating_sub(used).max(1);
        } else {
            let share = *count as f64 / bucket.total() as f64 * total_eighths as f64;
            let rows = ((share / 8.0).round() as u32).max(1);
            heights[i] = rows * 8;
            used += rows * 8;
        }
    }
    heights
}

/// Renders the histogram into `area`.
pub fn render(samples: &[Sample], area: Rect, buf: &mut Buffer) {
    let buckets = buckets(samples);
    if buckets.is_empty() || area.height < 3 || area.width < 4 {
        return;
    }

    // Rows: count labels on top, bars, bucket labels at the bottom.
    let plot_rows = area.height - 2;
    let n = buckets.len() as u16;
    const GAP: u16 = 1;
    let bar_width = ((area.width + GAP) / n).saturating_sub(GAP).clamp(1, 10);
    let chart_width = n * (bar_width + GAP) - GAP;
    let x0 = area.x + area.width.saturating_sub(chart_width) / 2;
    // The first row is kept for counts above the tallest bar.
    let label_y = area.y + area.height - 1;
    let base_y = label_y - 1;
    let max_total = buckets.iter().map(Bucket::total).max().unwrap_or(1).max(1);

    // Show every k-th label so that labels do not overlap.
    let label_width = buckets
        .iter()
        .map(|b| edge_label(b.lower_us).len() as u16)
        .max()
        .unwrap_or(1);
    let label_every = (label_width + 1).div_ceil(bar_width + GAP).max(1) as usize;

    for (i, bucket) in buckets.iter().enumerate() {
        let x = x0 + i as u16 * (bar_width + GAP);
        let heights = segment_heights(bucket, max_total, plot_rows);
        let colors = [theme::bar_selected(), theme::bar_error(), theme::bar()];

        // Draw each segment from the bottom up.
        let mut base = 0u32;
        for (height, color) in heights.iter().zip(colors) {
            let mut filled = 0;
            while filled < *height {
                let row = (base + filled) / 8;
                if row >= plot_rows as u32 {
                    break;
                }
                let eighths = (*height - filled).min(8);
                let symbol = BLOCKS[eighths as usize - 1];
                for dx in 0..bar_width {
                    if let Some(cell) = buf.cell_mut((x + dx, base_y - row as u16)) {
                        cell.set_symbol(symbol).set_fg(color);
                    }
                }
                filled += eighths;
            }
            base += height;
        }

        // Count above the bar, if it fits.
        let count = bucket.total().to_string();
        let top_row = base.div_ceil(8).min(plot_rows as u32) as u16;
        if bucket.total() > 0 && count.len() as u16 <= bar_width + GAP {
            let cx = x + bar_width.saturating_sub(count.len() as u16) / 2;
            buf.set_string(cx, base_y - top_row, &count, muted());
        }

        // Bucket label under the bar.
        if i % label_every == 0 {
            buf.set_string(x, label_y, edge_label(bucket.lower_us), muted());
        }
    }
}

/// Widths of the segments of a bar of `width` cells proportional to `counts`.
///
/// A non-zero count gets at least one cell (if there is room), so that e.g. a single
/// 5xx response stays visible; the widths add up to `width` if any count is non-zero.
fn segment_widths(counts: &[u64], width: u16) -> Vec<u16> {
    let total: u64 = counts.iter().sum();
    if total == 0 {
        return vec![0; counts.len()];
    }
    let mut widths: Vec<u16> = counts
        .iter()
        .map(|&c| {
            if c == 0 {
                0
            } else {
                ((c as f64 / total as f64 * width as f64).round() as u16).max(1)
            }
        })
        .collect();
    // Fix rounding by adjusting the widest segment.
    let sum: u16 = widths.iter().sum();
    if let Some(widest) = (0..widths.len()).max_by_key(|&i| widths[i]) {
        widths[widest] = (widths[widest] + width).saturating_sub(sum).max(1);
    }
    // Still too wide when there are more non-zero classes than cells.
    while widths.iter().sum::<u16>() > width {
        let Some(i) = widths.iter().rposition(|w| *w > 0) else {
            break;
        };
        widths[i] -= 1;
    }
    widths
}

/// Renders the mix of status code classes as a one-row stacked bar into `area`.
pub fn render_status_bar(samples: &[Sample], area: Rect, buf: &mut Buffer) {
    use strum::IntoEnumIterator;
    let classes: Vec<StatusClass> = StatusClass::iter().collect();
    let counts: Vec<u64> = classes
        .iter()
        .map(|class| samples.iter().filter(|s| s.class == *class).count() as u64)
        .collect();
    let mut x = area.x;
    for ((class, count), width) in classes
        .iter()
        .zip(&counts)
        .zip(segment_widths(&counts, area.width))
    {
        if width == 0 {
            continue;
        }
        let segment = Rect::new(x, area.y, width, 1);
        buf.set_style(segment, Style::new().bg(class.color()));
        // Label inside the segment if it fits: `2xx 120`, else `2xx`.
        let full = format!("{} {count}", class.label());
        let label = [full.as_str(), class.label()]
            .into_iter()
            .find(|label| (label.len() as u16) < width);
        if let Some(label) = label {
            buf.set_string(
                x + 1,
                area.y,
                label,
                Style::new().fg(theme::on_accent()).bold(),
            );
        }
        x += width;
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    fn sample(ms: f64, error: bool, selected: bool) -> Sample {
        Sample {
            latency: Duration::from_secs_f64(ms / 1000.0),
            error,
            selected,
            class: StatusClass::Success,
        }
    }

    #[test]
    fn log_scale_edges() {
        assert_eq!(vec![1_000], edges_up_to(0));
        assert_eq!(vec![1_000, 2_000, 5_000], edges_up_to(3_000));
        assert_eq!(
            vec![1_000, 2_000, 5_000, 10_000, 20_000],
            edges_up_to(10_000)
        );
    }

    #[test]
    fn buckets_are_trimmed_and_counted() {
        let samples = vec![
            sample(3.0, false, false),
            sample(4.0, true, false),
            sample(30.0, false, true),
        ];
        let buckets = buckets(&samples);
        let summary: Vec<_> = buckets
            .iter()
            .map(|b| (edge_label(b.lower_us), b.ok, b.error, b.selected))
            .collect();
        assert_eq!(
            vec![
                ("2ms".to_string(), 1, 1, 0),
                ("5ms".to_string(), 0, 0, 0),
                ("10ms".to_string(), 0, 0, 0),
                ("20ms".to_string(), 0, 0, 1),
            ],
            summary
        );
    }

    #[test]
    fn sub_millisecond_calls_go_to_the_first_bucket() {
        let buckets = buckets(&[sample(0.2, false, false), sample(1.5, false, false)]);
        assert_eq!(0, buckets[0].lower_us);
        assert_eq!(1, buckets[0].ok);
        assert_eq!(1, buckets[1].ok);
    }

    #[test]
    fn edge_labels() {
        assert_eq!("0", edge_label(0));
        assert_eq!("20ms", edge_label(20_000));
        assert_eq!("2s", edge_label(2_000_000));
    }

    #[test]
    fn single_error_stays_visible() {
        let bucket = Bucket {
            lower_us: 0,
            ok: 99,
            error: 1,
            selected: 0,
        };
        // 10 rows = 80 eighths: the error takes one row, ok the rest.
        assert_eq!([0, 8, 72], segment_heights(&bucket, 100, 10));
    }

    #[test]
    fn small_bucket_is_at_least_one_eighth() {
        let bucket = Bucket {
            lower_us: 0,
            ok: 1,
            error: 0,
            selected: 0,
        };
        assert_eq!([0, 0, 1], segment_heights(&bucket, 1000, 10));
    }

    #[test]
    fn summary_line() {
        let samples = vec![sample(1.0, false, false), sample(3.0, true, false)];
        let text: String = summary(&samples)
            .unwrap()
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert_eq!(
            "  2 calls · p50 3ms · p95 3ms · p99 3ms · max 3ms · 1 error response",
            text
        );
        assert!(summary(&[]).is_none());
    }

    #[test]
    fn percentiles() {
        let sorted: Vec<_> = (1..=100).map(Duration::from_millis).collect();
        assert_eq!(Duration::from_millis(51), percentile(&sorted, 0.5));
        assert_eq!(Duration::from_millis(95), percentile(&sorted, 0.95));
        assert_eq!(Duration::ZERO, percentile(&[], 0.5));
    }

    #[test]
    fn status_bar_segments_fill_the_width() {
        assert_eq!(vec![0, 0], segment_widths(&[0, 0], 10));
        assert_eq!(vec![7, 3], segment_widths(&[70, 30], 10));
        // A single error stays visible.
        assert_eq!(vec![9, 0, 1], segment_widths(&[999, 0, 1], 10));
        // Rounding never overflows the width.
        let widths = segment_widths(&[1, 1, 1], 10);
        assert_eq!(10, widths.iter().sum::<u16>());
        assert!(widths.iter().all(|w| *w >= 1));
        // More classes than cells.
        assert_eq!(2, segment_widths(&[1, 1, 1], 2).iter().sum::<u16>());
    }
}
