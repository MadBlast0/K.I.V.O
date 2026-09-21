//! Measured values and their summary (BENCHMARKS §1: at least 5 runs, p50 and p95).

use serde::{Deserialize, Serialize};

/// One measured quantity over all runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metric {
    pub name: String,
    pub unit: String,
    /// Lower is better (latency, CPU) or higher is better (throughput).
    pub lower_is_better: bool,
    pub samples: Vec<f64>,
    pub p50: f64,
    pub p95: f64,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
}

impl Metric {
    /// Summarizes `samples` (which must not be empty).
    pub fn new(name: &str, unit: &str, lower_is_better: bool, samples: Vec<f64>) -> Self {
        assert!(!samples.is_empty(), "a metric needs at least one sample");
        let mut sorted = samples.clone();
        sorted.sort_by(f64::total_cmp);
        #[allow(clippy::cast_precision_loss, reason = "sample counts are small")]
        let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
        Self {
            name: name.to_owned(),
            unit: unit.to_owned(),
            lower_is_better,
            p50: percentile(&sorted, 50.0),
            p95: percentile(&sorted, 95.0),
            min: sorted[0],
            max: sorted[sorted.len() - 1],
            mean,
            samples,
        }
    }
}

/// Nearest-rank percentile of sorted values: the smallest value with at least `p`% of the values
/// at or below it.
pub fn percentile(sorted: &[f64], p: f64) -> f64 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "a rank within a small slice"
    )]
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_rank_percentiles() {
        let v: Vec<f64> = (1..=20).map(f64::from).collect();
        assert_eq!(percentile(&v, 50.0), 10.0);
        assert_eq!(percentile(&v, 95.0), 19.0);
        assert_eq!(percentile(&v, 100.0), 20.0);
        assert_eq!(percentile(&[7.0], 95.0), 7.0);
    }

    #[test]
    fn metrics_summarize_unsorted_samples() {
        let m = Metric::new("x", "ms", true, vec![5.0, 1.0, 3.0, 2.0, 4.0]);
        assert_eq!(
            (m.min, m.p50, m.p95, m.max, m.mean),
            (1.0, 3.0, 5.0, 5.0, 3.0)
        );
        assert_eq!(
            m.samples,
            [5.0, 1.0, 3.0, 2.0, 4.0],
            "raw samples keep their order"
        );
    }
}
