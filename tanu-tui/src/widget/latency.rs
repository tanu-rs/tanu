//! Latency histogram of HTTP/gRPC calls with log-scale buckets.
//!
//! Buckets grow as 1, 2, 5, 10, 20, 50, ... ms so that fast and slow calls are both
//! visible. Each bar is stacked from the bottom: calls of the test selected in the
//! test list (light blue), error responses (muted red), and the other calls (blue).
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

/// Title details: percentiles, max and the number of failed calls.
pub fn summary(samples: &[Sample]) -> Option<Vec<Span<'static>>> {
    let mut latencies: Vec<Duration> = samples.iter().map(|s| s.latency).collect();
    if latencies.is_empty() {
        return None;
    }
    latencies.sort_unstable();
    let pct = |p: f64| {
        let rank = (p * (latencies.len() - 1) as f64).round() as usize;
        fmt_duration(latencies[rank])
    };
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
            Style::new().fg(theme::BAR_ERROR),
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
        let colors = [theme::BAR_SELECTED, theme::BAR_ERROR, theme::BAR];

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

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    fn sample(ms: f64, error: bool, selected: bool) -> Sample {
        Sample {
            latency: Duration::from_secs_f64(ms / 1000.0),
            error,
            selected,
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
}
