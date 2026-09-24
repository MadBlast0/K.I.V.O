//! `router`: the intent router's fast path (BENCHMARKS §4 smoke subset, BRAINS §2). The bundled
//! English grammar routes a fixed mix of commands and requests for a brain against app and
//! window indexes the size of a busy PC. No models, no runtime: it runs anywhere, including CI.

use crate::harness::{Sample, Suite};
use kivo_intent::{Context, Grammar, Index, IndexEntry, IntentRouter, Route};
use std::time::Instant;

/// Commands the grammar handles (the M1 journeys and their neighbours).
const COMMANDS: &[&str] = &[
    "Open Chrome.",
    "Hey Kivo, can you open Spotify for me please?",
    "Mute",
    "Set the volume to 30%",
    "Take a screenshot.",
    "Screenshot this window",
    "Restart Chrome",
    "whats playing",
    "Go to YouTube.com",
    "Search for rust tutorials",
    "switch to inbox",
    "close notepad",
];

/// Requests no grammar pattern handles: they go on to the next stage (a brain).
const REQUESTS: &[&str] = &[
    "Summarize the email Maya sent me yesterday about the budget",
    "Why is my build failing on the linker step?",
    "Write a polite reply saying I'll be ten minutes late",
    "What's the difference between a mutex and a semaphore?",
];

/// Routes per run; every phrase repeats until this many.
const ROUTES_PER_RUN: usize = 1_000;

/// An index of `n` entries with realistic names (a few real ones, then generated ones).
fn index(real: &[(&str, &str)], n: usize, prefix: &str) -> Index {
    let mut entries: Vec<IndexEntry> = real
        .iter()
        .map(|(id, name)| IndexEntry {
            id: (*id).into(),
            name: (*name).into(),
            aliases: Vec::new(),
        })
        .collect();
    let words = [
        "Studio", "Manager", "Player", "Editor", "Viewer", "Center", "Tools", "Sync",
    ];
    for i in entries.len()..n {
        entries.push(IndexEntry {
            id: format!("{prefix}{i}"),
            name: format!("{prefix} {} {i}", words[i % words.len()]),
            aliases: Vec::new(),
        });
    }
    Index::new(entries)
}

pub struct Router {
    router: IntentRouter,
    apps: Index,
    windows: Index,
}

impl Router {
    pub fn start() -> Result<Self, String> {
        let grammar = Grammar::bundled("en").map_err(|e| e.to_string())?;
        let apps = index(
            &[
                ("chrome", "Google Chrome"),
                ("spotify", "Spotify"),
                ("notepad", "Notepad"),
                ("code", "Visual Studio Code"),
                ("outlook", "Outlook"),
            ],
            200,
            "Contoso",
        );
        let windows = index(
            &[("100", "Inbox - Outlook"), ("200", "Spotify Premium")],
            30,
            "Window",
        );
        let mut router = Self {
            router: IntentRouter::new(grammar),
            apps,
            windows,
        };
        // Every command must take the fast path and every request must not, or the numbers
        // would describe the wrong work.
        for text in COMMANDS {
            if !matches!(router.route(text), Route::FastPath(_)) {
                return Err(format!("the grammar no longer handles \"{text}\""));
            }
        }
        for text in REQUESTS {
            if matches!(router.route(text), Route::FastPath(_)) {
                return Err(format!("the grammar unexpectedly handles \"{text}\""));
            }
        }
        Ok(router)
    }

    fn route(&mut self, text: &str) -> Route {
        let cx = Context {
            apps: &self.apps,
            windows: &self.windows,
        };
        self.router.route(text, &cx).route
    }
}

fn percentile(sorted: &[f64], p: usize) -> f64 {
    sorted[(sorted.len() * p / 100).min(sorted.len() - 1)]
}

impl Suite for Router {
    fn name(&self) -> &'static str {
        "router"
    }

    fn run(&mut self) -> Result<Vec<Sample>, String> {
        let mut commands = Vec::with_capacity(ROUTES_PER_RUN);
        let mut requests = Vec::with_capacity(ROUTES_PER_RUN / 3);
        let phrases: Vec<(&str, bool)> = COMMANDS
            .iter()
            .map(|t| (*t, true))
            .chain(REQUESTS.iter().map(|t| (*t, false)))
            .collect();
        for i in 0..ROUTES_PER_RUN {
            let (text, command) = phrases[i % phrases.len()];
            let started = Instant::now();
            let route = self.route(text);
            let micros = started.elapsed().as_secs_f64() * 1e6;
            if matches!(route, Route::FastPath(_)) != command {
                return Err(format!("\"{text}\" changed route during the run"));
            }
            if command {
                commands.push(micros);
            } else {
                requests.push(micros);
            }
        }
        commands.sort_by(f64::total_cmp);
        requests.sort_by(f64::total_cmp);
        Ok(vec![
            Sample::cost(
                "command → fast path (median)",
                "µs",
                percentile(&commands, 50),
            ),
            Sample::cost("command → fast path (p95)", "µs", percentile(&commands, 95)),
            Sample::cost(
                "request → on to a brain (median)",
                "µs",
                percentile(&requests, 50),
            ),
            Sample::cost(
                "request → on to a brain (p95)",
                "µs",
                percentile(&requests, 95),
            ),
        ])
    }

    fn notes(&self) -> Vec<String> {
        vec![format!(
            "The bundled English grammar alone (the semantic stage needs its optional model), \
             {ROUTES_PER_RUN} routes per run over {} commands and {} requests for a brain, against \
             200 apps and 30 windows.",
            COMMANDS.len(),
            REQUESTS.len()
        )]
    }
}
