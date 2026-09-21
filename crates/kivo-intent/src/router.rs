//! The intent router (BRAINS §2): the first confident stage wins. M1 has the grammar stage; the
//! semantic stage and the brain join in M3 (BRAIN-03, BRAIN-04). Every decision is timed, and the
//! router keeps the `fast_path_ratio` and per-stage p95 latency (BRAIN-05).

use crate::grammar::{Context, Grammar, Match};
use serde::Serialize;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Latencies kept per stage for the p95.
const WINDOW: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Grammar,
    Semantic,
    Brain,
}

/// Where a request went.
#[derive(Clone, Debug, PartialEq)]
pub enum Route {
    /// The fast path: run this tool directly, no AI.
    FastPath(Match),
    /// No stage handled it.
    Unhandled,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Decision {
    pub route: Route,
    /// Time spent in each stage that ran.
    pub stages: Vec<(Stage, Duration)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouterMetrics {
    pub requests: u64,
    pub fast_path: u64,
    /// Share of requests handled without AI (the invariant "deterministic actions never require
    /// heavy AI", plan §160).
    pub fast_path_ratio: f64,
    pub grammar_p95_micros: u64,
}

pub struct IntentRouter {
    grammar: Grammar,
    requests: u64,
    fast_path: u64,
    grammar_times: VecDeque<Duration>,
}

impl IntentRouter {
    pub fn new(grammar: Grammar) -> Self {
        Self {
            grammar,
            requests: 0,
            fast_path: 0,
            grammar_times: VecDeque::with_capacity(WINDOW),
        }
    }

    pub fn grammar(&self) -> &Grammar {
        &self.grammar
    }

    pub fn route(&mut self, text: &str, cx: &Context<'_>) -> Decision {
        self.requests += 1;
        let started = Instant::now();
        let matched = self.grammar.match_text(text, cx);
        let took = started.elapsed();
        if self.grammar_times.len() == WINDOW {
            self.grammar_times.pop_front();
        }
        self.grammar_times.push_back(took);
        let route = match matched {
            Some(m) => {
                self.fast_path += 1;
                Route::FastPath(m)
            }
            None => Route::Unhandled,
        };
        Decision {
            route,
            stages: vec![(Stage::Grammar, took)],
        }
    }

    pub fn metrics(&self) -> RouterMetrics {
        #[allow(clippy::cast_precision_loss, reason = "request counts")]
        let ratio = if self.requests == 0 {
            0.0
        } else {
            self.fast_path as f64 / self.requests as f64
        };
        RouterMetrics {
            requests: self.requests,
            fast_path: self.fast_path,
            fast_path_ratio: ratio,
            grammar_p95_micros: u64::try_from(p95(&self.grammar_times).as_micros())
                .unwrap_or(u64::MAX),
        }
    }
}

/// Nearest-rank 95th percentile.
fn p95(times: &VecDeque<Duration>) -> Duration {
    if times.is_empty() {
        return Duration::ZERO;
    }
    let mut sorted: Vec<Duration> = times.iter().copied().collect();
    sorted.sort_unstable();
    let rank = (sorted.len() * 95).div_ceil(100).max(1);
    sorted[rank - 1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{Index, IndexEntry};

    #[test]
    fn the_fast_path_ratio_and_grammar_latency_are_tracked() {
        let mut router = IntentRouter::new(Grammar::bundled("en").unwrap());
        let apps = Index::new(vec![IndexEntry {
            id: "c".into(),
            name: "Chrome".into(),
            aliases: Vec::new(),
        }]);
        let windows = Index::default();
        let cx = Context {
            apps: &apps,
            windows: &windows,
        };
        assert!(matches!(
            router.route("open chrome", &cx).route,
            Route::FastPath(_)
        ));
        assert!(matches!(
            router.route("mute", &cx).route,
            Route::FastPath(_)
        ));
        assert!(matches!(
            router.route("tell me a joke", &cx).route,
            Route::Unhandled
        ));
        let decision = router.route("take a screenshot", &cx);
        assert_eq!(decision.stages[0].0, Stage::Grammar);
        let m = router.metrics();
        assert_eq!((m.requests, m.fast_path), (4, 3));
        assert!((m.fast_path_ratio - 0.75).abs() < 1e-9);
        // The grammar stage is microseconds, far inside the 500 ms fast-path budget (VOICE §10).
        assert!(m.grammar_p95_micros < 20_000, "{} µs", m.grammar_p95_micros);
    }

    #[test]
    fn p95_uses_the_nearest_rank() {
        let times: VecDeque<Duration> = (1..=100).map(Duration::from_millis).collect();
        assert_eq!(p95(&times), Duration::from_millis(95));
        assert_eq!(p95(&VecDeque::new()), Duration::ZERO);
    }
}
