//! The plan §102 journeys, end to end, for `kivo-bench e2e` (BENCH-08). Each run speaks the three
//! requests through a scripted microphone into a real KIVO turn engine with the real speech
//! worker and models; apps, windows and system controls are fakes, so "mute" never mutes this PC.
//!
//! Protocol: one line `run` on stdin runs the journeys and prints one JSON line to stdout,
//! `{"journeys": [{"name", "outcome", "spans": {"t4EndOfSpeech": ms, …}}]}`; EOF ends it. The
//! first run loads the models (cold); later runs are warm, as KIVO is right after a request.

#[cfg(windows)]
#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> std::process::ExitCode {
    windows_main::run().await
}

#[cfg(not(windows))]
fn main() {
    eprintln!("kivo-e2e runs on Windows only for now");
}

#[cfg(windows)]
mod windows_main {
    use kivo_core::SessionState;
    use kivo_core::event::TurnSource;
    use kivo_platform::Paths;
    use kivo_runtime::scripted;
    use kivo_store::models::ModelStore;
    use serde_json::{Value, json};
    use std::io::{BufRead, Write};
    use std::process::ExitCode;
    use std::time::{Duration, Instant};

    /// Plan §102: simple, medium and complex.
    const JOURNEYS: [(&str, &str); 3] = [
        ("simple", "Mute."),
        ("medium", "Open Chrome and search for cute cats."),
        (
            "complex",
            "Inspect my project, find the test failure, fix it, and verify.",
        ),
    ];
    const TURN_LIMIT: Duration = Duration::from_secs(90);

    fn fail(message: &str) -> ExitCode {
        println!("{}", json!({ "error": message }));
        ExitCode::FAILURE
    }

    pub async fn run() -> ExitCode {
        let Some(paths) = Paths::user() else {
            return fail("no per-user folders");
        };
        let Some(model) =
            ModelStore::new(paths.models()).installed(kivo_voice::moonshine::MODEL_ID)
        else {
            return fail("the speech model isn't installed (run KIVO once to download it)");
        };
        let worker = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|d| d.join("kivo-infer.exe")))
            .filter(|p| p.is_file());
        let Some(worker) = worker else {
            return fail("kivo-infer.exe isn't beside kivo-e2e.exe (cargo build -p kivo-infer)");
        };
        let clips: Vec<(&str, Vec<f32>)> = JOURNEYS
            .iter()
            .map(|(name, text)| (*name, scripted::spoken(text)))
            .collect();
        let (rig, _worker_task, _pump) = scripted::rig(worker, Vec::new(), model.dir);
        // The journeys measure KIVO, not the voice: replies are cues only.
        rig.core.update_config(|c| {
            c.capabilities
                .set(kivo_core::Capability::SpeakResponses, false);
        });
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            match line.trim() {
                "run" => {}
                "quit" => break,
                _ => continue,
            }
            let mut journeys = Vec::new();
            for (name, clip) in &clips {
                journeys.push(journey(&rig, name, clip.clone()).await);
            }
            println!("{}", json!({ "journeys": journeys }));
            let _ = std::io::stdout().flush();
        }
        rig.core.quit();
        ExitCode::SUCCESS
    }

    async fn journey(rig: &scripted::Rig, name: &str, clip: Vec<f32>) -> Value {
        rig.audio.say_next(clip);
        if let Err(e) = rig.engine.talk(TurnSource::PushToTalk).await {
            return json!({ "name": name, "outcome": "refused", "error": e });
        }
        let Some(turn) = rig.core.turn_view().map(|t| t.id) else {
            return json!({ "name": name, "outcome": "no-turn" });
        };
        let deadline = Instant::now() + TURN_LIMIT;
        loop {
            let finished = rig
                .db
                .lock()
                .ok()
                .and_then(|db| db.turn(&turn).ok().flatten())
                .and_then(|t| t.outcome);
            if let Some(outcome) = finished {
                // Wait for the session to settle before the next journey.
                while rig.core.state().borrow().session != SessionState::Idle
                    && Instant::now() < deadline
                {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                rig.core.clear_turn();
                let spans = rig
                    .db
                    .lock()
                    .ok()
                    .and_then(|db| db.turn_metrics(&turn).ok().flatten())
                    .unwrap_or(Value::Null);
                return json!({ "name": name, "outcome": outcome, "spans": spans });
            }
            if Instant::now() > deadline {
                rig.engine.cancel(kivo_core::event::CancelReason::Timeout);
                return json!({ "name": name, "outcome": "timeout" });
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}
