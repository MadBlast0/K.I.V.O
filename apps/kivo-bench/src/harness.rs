//! Running a suite: warmup rounds (discarded), then measured runs, each producing one sample per
//! metric (BENCHMARKS §1: at least 5 runs, warmup excluded from cold metrics).

use crate::report::{SuiteResult, now_ms};
use crate::stats::Metric;

/// One measured value from one run.
#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    pub metric: &'static str,
    pub unit: &'static str,
    pub lower_is_better: bool,
    pub value: f64,
}

impl Sample {
    /// A value where lower is better (latency, CPU, memory).
    pub fn cost(metric: &'static str, unit: &'static str, value: f64) -> Self {
        Self {
            metric,
            unit,
            lower_is_better: true,
            value,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Plan {
    pub runs: u32,
    pub warmup: u32,
}

/// A benchmark suite: `run` is called `warmup + runs` times.
pub trait Suite {
    fn name(&self) -> &'static str;
    /// Measures once. The first `warmup` calls are discarded.
    fn run(&mut self) -> Result<Vec<Sample>, String>;
    /// How it was measured, for the report.
    fn notes(&self) -> Vec<String>;
}

pub fn execute(suite: &mut dyn Suite, plan: Plan) -> Result<SuiteResult, String> {
    let started_at = now_ms();
    for i in 0..plan.warmup {
        suite
            .run()
            .map_err(|e| format!("{} warmup {}: {e}", suite.name(), i + 1))?;
    }
    let mut metrics: Vec<(Sample, Vec<f64>)> = Vec::new();
    for i in 0..plan.runs {
        let samples = suite
            .run()
            .map_err(|e| format!("{} run {}: {e}", suite.name(), i + 1))?;
        for s in samples {
            match metrics.iter_mut().find(|(m, _)| m.metric == s.metric) {
                Some((_, values)) => values.push(s.value),
                None => metrics.push((s.clone(), vec![s.value])),
            }
        }
    }
    Ok(SuiteResult {
        suite: suite.name().to_owned(),
        started_at,
        runs: plan.runs,
        warmup: plan.warmup,
        metrics: metrics
            .into_iter()
            .map(|(s, values)| Metric::new(s.metric, s.unit, s.lower_is_better, values))
            .collect(),
        notes: suite.notes(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counting(u32);

    impl Suite for Counting {
        fn name(&self) -> &'static str {
            "counting"
        }
        fn run(&mut self) -> Result<Vec<Sample>, String> {
            self.0 += 1;
            Ok(vec![Sample::cost("call", "n", f64::from(self.0))])
        }
        fn notes(&self) -> Vec<String> {
            vec!["counts calls".into()]
        }
    }

    #[test]
    fn warmup_runs_are_discarded() {
        let result = execute(&mut Counting(0), Plan { runs: 5, warmup: 2 }).unwrap();
        assert_eq!(result.metrics.len(), 1);
        assert_eq!(result.metrics[0].samples, [3.0, 4.0, 5.0, 6.0, 7.0]);
        assert_eq!((result.runs, result.warmup), (5, 2));
        assert_eq!(result.notes, ["counts calls"]);
    }
}
