//! M5 routines, end to end (ROUTINES): custom commands and routines said aloud or typed, their
//! grants (ROUT-03), phrase and hotkey collisions (ROUT-07), AI steps metered to the routine
//! (ROUT-08), the starters (ROUT-10), and runs as tasks (ROUT-02) — on fakes.

#![cfg(windows)]

use kivo_brain::PrivacyClass;
use kivo_brain::testing::{Script, ScriptedBrain};
use kivo_core::routine::{Routine, RoutineEvent, RoutineStep, Trigger, VarDef, VarKind};
use kivo_core::task::{OnError, StepAction, TaskStatus};
use kivo_core::{Capability, SessionState};
use kivo_runtime::scripted::{self, Rig};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn worker() -> PathBuf {
    let mut dir = std::env::current_exe().expect("test binary");
    dir.pop();
    dir.pop();
    dir.join("kivo-infer.exe")
}

static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn until(what: &str, timeout: Duration, mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

struct Running {
    rig: Rig,
    worker: tokio::task::JoinHandle<()>,
    pump: tokio::task::JoinHandle<()>,
}

impl Running {
    async fn stop(self) {
        self.rig.core.quit();
        let _ = tokio::time::timeout(Duration::from_secs(5), self.worker).await;
        self.pump.abort();
    }

    async fn task_done(&self, id: &str) -> kivo_ipc::protocol::TaskView {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let t = self.rig.tasks.view(id).unwrap();
            if t.status.is_final() {
                return t;
            }
            assert!(Instant::now() < deadline, "{t:#?}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

fn start() -> Running {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(std::env::var("KIVO_TEST_LOG").unwrap_or_else(|_| "warn".into()))
        .with_test_writer()
        .try_init();
    let (rig, worker, pump) = scripted::rig(worker(), Vec::new(), PathBuf::from("no-model"));
    rig.core.update_config(|c| {
        c.capabilities.set(Capability::SpeakResponses, false);
        c.voice.speak_typed_replies = false;
    });
    Running { rig, worker, pump }
}

async fn answered(rig: &Rig) -> kivo_ipc::protocol::TurnView {
    until("the turn to end", Duration::from_secs(20), || {
        rig.core.state().borrow().session == SessionState::Idle
            && rig
                .core
                .turn_view()
                .is_some_and(|t| t.answer.is_some() || t.error.is_some())
    })
    .await;
    rig.core.turn_view().expect("the card still shows the turn")
}

fn step(id: &str, action: StepAction) -> RoutineStep {
    RoutineStep {
        id: id.into(),
        action,
        on_error: OnError::Stop,
        delay_ms: None,
        parallel_group: None,
        confirm: false,
    }
}

fn routine(name: &str, phrases: &[&str], steps: Vec<RoutineStep>) -> Routine {
    Routine {
        id: String::new(),
        name: name.into(),
        description: String::new(),
        enabled: true,
        triggers: vec![Trigger::Phrase {
            phrases: phrases.iter().map(|p| (*p).to_owned()).collect(),
            lang: "en".into(),
        }],
        steps,
        variables: Vec::new(),
        grants: Vec::new(),
        starter: None,
    }
}

fn volume(level: u64) -> StepAction {
    StepAction::Tool {
        tool: "audio.volume_set".into(),
        args: json!({ "number": level }),
    }
}

/// ROUT-06, ROUT-05: a custom command is said and runs its one action at once, with no brain; a
/// routine is said with a variable and runs as a task.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn custom_commands_and_routines_run_from_their_phrases() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    let brain = Arc::new(ScriptedBrain::new("anthropic", PrivacyClass::Cloud, vec![]));
    r.rig.brains.insert(brain.clone());
    let cinema = r
        .rig
        .routines
        .save(
            routine(
                "Cinema",
                &["cinema", "movie time"],
                vec![step("v", volume(30))],
            ),
            true,
        )
        .unwrap();
    assert!(cinema.custom_command && cinema.granted);
    r.rig.engine.say("Kivo, cinema.").await.unwrap();
    answered(&r.rig).await;
    assert!((r.rig.control.volume.lock().unwrap().level - 0.3).abs() < 0.01);
    assert!(brain.requests.lock().unwrap().is_empty(), "no AI");

    // A routine with a number variable: "night mode for {minutes}" → a task.
    let mut night = routine(
        "Night",
        &["night mode for {minutes} minutes"],
        vec![
            step("v", volume(10)),
            step(
                "wait",
                StepAction::Tool {
                    tool: "tasks.remind".into(),
                    args: json!({ "number": "{minutes}", "unit": "minutes", "text": "Night mode ends" }),
                },
            ),
        ],
    );
    night.variables = vec![VarDef {
        name: "minutes".into(),
        kind: VarKind::Number,
    }];
    let night = r.rig.routines.save(night, true).unwrap();
    r.rig.core.clear_turn();
    r.rig
        .engine
        .say("night mode for 45 minutes please")
        .await
        .unwrap();
    let view = answered(&r.rig).await;
    assert_eq!(view.answer.as_deref(), Some("Running Night."));
    let task = view.task_id.expect("the routine's task");
    let done = r.task_done(&task).await;
    assert_eq!(done.status, TaskStatus::Done, "{done:#?}");
    assert!((r.rig.control.volume.lock().unwrap().level - 0.1).abs() < 0.01);
    // The reminder it set is for 45 minutes from now.
    let reminder = r
        .rig
        .tasks
        .list(false, 10)
        .into_iter()
        .find(|t| t.title == "Night mode ends")
        .expect("the reminder task");
    assert_eq!(reminder.kind, kivo_core::task::TaskKind::Reminder);
    assert!(
        r.rig
            .routines
            .list()
            .iter()
            .any(|v| v.routine.id == night.routine.id && v.last_run.is_some()),
        "last run recorded"
    );
    r.stop().await;
}

/// ROUT-03: the routine is granted exactly its steps when saved; an edited step needs granting
/// again, and until then it doesn't run.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn routines_run_only_what_was_granted() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    let saved = r
        .rig
        .routines
        .save(
            routine("Quiet", &["quiet please now"], vec![step("v", volume(20))]),
            true,
        )
        .unwrap();
    let check = r.rig.routines.check(&saved.routine);
    assert_eq!(check.grants.len(), 1);
    assert_eq!(check.grants[0].tool, "audio.volume_set");
    // Edited without granting: saved, but not granted, and it refuses to run.
    let mut edited = saved.routine.clone();
    edited.steps[0] = step("v", volume(90));
    let view = r.rig.routines.save(edited.clone(), false).unwrap();
    assert!(!view.granted);
    let e = r
        .rig
        .routines
        .run(&edited.id, &serde_json::Map::new())
        .unwrap_err();
    assert!(
        e.contains("changed since its permissions were allowed"),
        "{e}"
    );
    assert!(r.rig.routines.enable(&edited.id, true).is_err());
    // Granted again: runs.
    r.rig.routines.save(edited.clone(), true).unwrap();
    let task = r
        .rig
        .routines
        .run(&edited.id, &serde_json::Map::new())
        .unwrap();
    r.task_done(&task).await;
    assert!((r.rig.control.volume.lock().unwrap().level - 0.9).abs() < 0.01);
    r.stop().await;
}

/// ROUT-07: phrases that a built-in command, another routine or a wake word already answer
/// to, and hotkeys KIVO or another routine uses, are refused; sound-alikes warn.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn clashing_phrases_and_hotkeys_are_caught() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    r.rig
        .routines
        .save(
            routine("Work", &["work mode"], vec![step("v", volume(40))]),
            true,
        )
        .unwrap();
    let check = |phrase: &str| {
        r.rig
            .routines
            .check(&routine("Draft", &[phrase], vec![step("v", volume(40))]))
            .collisions
    };
    let mute = check("mute");
    assert!(
        mute.iter().any(|c| c.kind == "command" && c.blocking),
        "{mute:?}"
    );
    let work = check("work mode");
    assert!(
        work.iter().any(|c| c.kind == "routine" && c.blocking),
        "{work:?}"
    );
    let wake = check("hey kivo");
    assert!(
        wake.iter().any(|c| c.kind == "wakeWord" && c.blocking),
        "{wake:?}"
    );
    let alike = check("werk mode");
    assert!(
        alike.iter().any(|c| c.kind == "routine" && !c.blocking),
        "sounds alike: a warning {alike:?}"
    );
    assert!(check("breakfast playlist").is_empty());
    let e = r
        .rig
        .routines
        .save(
            routine("Mute clone", &["mute"], vec![step("v", volume(0))]),
            true,
        )
        .unwrap_err();
    assert!(e.contains("clashes"), "{e}");
    // Hotkeys.
    let mut keys = routine("Keys", &[], vec![step("v", volume(5))]);
    keys.triggers = vec![Trigger::Hotkey {
        chord: "shift+ctrl+m".into(),
    }];
    let c = r.rig.routines.check(&keys).collisions;
    assert!(c.iter().any(|c| c.kind == "hotkey" && c.blocking), "{c:?}");
    r.stop().await;
}

/// ROUT-08: an AI step asks a brain with the chosen profile, its answer is the step's result,
/// and the cost is metered to the routine; the routine is badged.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ai_steps_ask_a_brain_and_count_toward_its_cost() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    let brain = Arc::new(ScriptedBrain::new(
        "anthropic",
        PrivacyClass::Cloud,
        vec![Script::text("Three meetings today; the first at 10.")],
    ));
    r.rig.brains.insert(brain.clone());
    let saved = r
        .rig
        .routines
        .save(
            routine(
                "Morning",
                &["good morning kivo"],
                vec![step(
                    "brief",
                    StepAction::Ask {
                        prompt: "Summarize my day in one sentence".into(),
                        profile: None,
                    },
                )],
            ),
            true,
        )
        .unwrap();
    assert!(saved.contains_ai);
    let task = r
        .rig
        .routines
        .run(&saved.routine.id, &serde_json::Map::new())
        .unwrap();
    let done = r.task_done(&task).await;
    assert_eq!(done.status, TaskStatus::Done, "{done:#?}");
    assert_eq!(
        done.result.as_deref(),
        Some("Three meetings today; the first at 10.")
    );
    let asked = brain.requests.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    let db = r.rig.recorder.database();
    let usage = db.lock().unwrap().usage_since(0).unwrap();
    assert!(
        usage.iter().any(
            |u| u.routine_id.as_deref() == Some(saved.routine.id.as_str())
                && u.task_id.as_deref() == Some(task.as_str())
        ),
        "metered to the routine and its task: {usage:?}"
    );
    r.stop().await;
}

/// ROUT-10 and a hotkey run: the starters are added once, all off; a routine's hotkey runs it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn starters_are_added_once_and_hotkeys_run_routines() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    r.rig.routines.seed_starters();
    r.rig.routines.seed_starters();
    let starters: Vec<_> = r
        .rig
        .routines
        .list()
        .into_iter()
        .filter(|v| v.routine.starter.is_some())
        .collect();
    assert_eq!(starters.len(), 5);
    assert!(starters.iter().all(|v| !v.routine.enabled && !v.granted));
    // Deleted starters don't come back.
    r.rig.routines.delete(&starters[0].routine.id);
    r.rig.routines.seed_starters();
    assert_eq!(r.rig.routines.list().len(), 4);

    let mut keys = routine("Bright", &[], vec![step("v", volume(70))]);
    keys.triggers = vec![Trigger::Hotkey {
        chord: "Ctrl+Alt+B".into(),
    }];
    let saved = r.rig.routines.save(keys, true).unwrap();
    assert_eq!(
        r.rig.routines.hotkeys().borrow().clone(),
        [("Ctrl+Alt+B".to_owned(), saved.routine.id.clone())]
    );
    let task = r.rig.engine.run_routine(&saved.routine.id).unwrap();
    r.task_done(&task).await;
    assert!((r.rig.control.volume.lock().unwrap().level - 0.7).abs() < 0.01);
    r.stop().await;
}

/// ROUT-11, ROUT-12: an app launching starts a routine on its own; its High-risk step asks on
/// screen first even though the routine is granted (the user says no, so nothing shuts down);
/// the routine waiting on it runs when it finishes. KIVO starting up doesn't count as a launch.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn events_start_routines_unattended_and_high_risk_steps_ask_first() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    // Already running before the triggers start: no launch.
    r.rig.processes.start(4100, "code.exe");
    let mut on_app = routine(
        "Music time",
        &[],
        vec![
            step("v", volume(25)),
            step(
                "off",
                StepAction::Tool {
                    tool: "system.shutdown".into(),
                    args: json!({}),
                },
            ),
        ],
    );
    on_app.triggers = vec![
        Trigger::Event {
            event: RoutineEvent::AppLaunched {
                app: "Spotify".into(),
            },
        },
        Trigger::Event {
            event: RoutineEvent::AppLaunched { app: "code".into() },
        },
    ];
    let music = r.rig.routines.save(on_app, true).unwrap();
    let mut after = routine("After music", &[], vec![step("v2", volume(40))]);
    after.triggers = vec![Trigger::Event {
        event: RoutineEvent::AfterRoutine {
            routine: music.routine.id.clone(),
        },
    }];
    r.rig.routines.save(after, true).unwrap();
    // A schedule that doesn't parse is refused when saving.
    let mut bad = routine("Bad", &[], vec![step("v3", volume(10))]);
    bad.triggers = vec![Trigger::Schedule {
        cron: "every morning".into(),
        tz: None,
    }];
    assert!(r.rig.routines.save(bad, true).is_err());

    let runner = tokio::spawn(kivo_runtime::triggers::run(
        Arc::clone(&r.rig.core),
        Arc::clone(&r.rig.routines),
        r.rig.system.clone(),
        r.rig.processes.clone(),
        Arc::clone(&r.rig.db),
    ));
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(r.rig.tasks.list(false, 20).is_empty(), "nothing at start");
    r.rig.processes.start(4200, "Spotify.exe");

    let find = |title: &str| {
        r.rig
            .tasks
            .list(true, 50)
            .into_iter()
            .chain(r.rig.tasks.list(false, 50))
            .find(|t| t.title == title)
    };
    until("the routine to ask", Duration::from_secs(15), || {
        find("Music time").is_some_and(|t| t.question.is_some())
    })
    .await;
    let task = find("Music time").unwrap();
    assert!((r.rig.control.volume.lock().unwrap().level - 0.25).abs() < 0.01);
    let q = task.question.unwrap();
    assert_eq!(q.choices, ["allow", "deny"]);
    r.rig.tasks.answer(&task.id, "deny").unwrap();
    r.task_done(&task.id).await;
    assert!(
        r.rig.control.power.lock().unwrap().is_empty(),
        "nothing shut down"
    );

    until("the routine after it", Duration::from_secs(10), || {
        find("After music").is_some_and(|t| t.status.is_final())
    })
    .await;
    assert!((r.rig.control.volume.lock().unwrap().level - 0.40).abs() < 0.01);
    runner.abort();
    r.stop().await;
}

/// ROUT-13, ROUT-14: "save that as a routine" turns the last request into a draft; a brain asked
/// to create a routine drafts one through `routines.draft`; neither is saved until the user saves
/// it. A routine exports to a file and comes back in as an off, ungranted draft; a file naming a
/// tool KIVO doesn't have, or that isn't a routine file, is refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn routines_are_drafted_by_voice_and_chat_and_move_as_files() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    let call = |method: &'static str, params: serde_json::Value| {
        let rpc = Arc::clone(&r.rig.rpc);
        async move { rpc.call(method, params).await.expect("handled") }
    };
    let before = r.rig.routines.list().len();

    // What I just did.
    r.rig.engine.say("set the volume to 30").await.unwrap();
    answered(&r.rig).await;
    r.rig.engine.say("save that as a routine").await.unwrap();
    let t = answered(&r.rig).await;
    assert_eq!(
        t.answer.as_deref(),
        Some(kivo_core::text::t("routine.fromLastTurn").as_str())
    );
    let draft = call("routines.draft", serde_json::Value::Null)
        .await
        .unwrap();
    assert_eq!(draft["name"], "Set the volume to 30");
    assert_eq!(draft["enabled"], false);
    assert_eq!(draft["steps"][0]["action"]["tool"], "audio.volume_set");
    assert_eq!(
        call("routines.draft", serde_json::Value::Null)
            .await
            .unwrap(),
        serde_json::Value::Null,
        "taken once"
    );
    assert_eq!(r.rig.routines.list().len(), before, "nothing saved");

    // By chat or voice, through a brain.
    let brain = Arc::new(ScriptedBrain::new(
        "anthropic",
        PrivacyClass::Cloud,
        vec![
            Script::tool(
                "routines__draft",
                json!({
                    "name": "Work mode",
                    "phrases": ["work mode"],
                    "steps": [
                        { "tool": "apps.launch", "args": { "app": { "id": "Slack", "name": "Slack" } } },
                        { "say": "Ready." }
                    ]
                }),
            ),
            Script::text("I've drafted Work mode for you to check."),
        ],
    ));
    r.rig.brains.insert(brain.clone());
    r.rig
        .engine
        .say("create a routine called work mode that opens slack and says ready")
        .await
        .unwrap();
    answered(&r.rig).await;
    let draft = call("routines.draft", serde_json::Value::Null)
        .await
        .unwrap();
    assert_eq!(draft["name"], "Work mode", "{draft}");
    assert_eq!(draft["triggers"][0]["phrases"][0], "work mode");
    assert_eq!(r.rig.routines.list().len(), before, "still nothing saved");

    // Export and import.
    let routine: kivo_core::routine::Routine = serde_json::from_value(draft).unwrap();
    let saved = r.rig.routines.save(routine, true).unwrap();
    let file = call("routines.export", json!({ "id": saved.routine.id }))
        .await
        .unwrap();
    let path = std::path::PathBuf::from(file["file"].as_str().unwrap());
    assert!(path.to_string_lossy().ends_with(".kivo-routine.json"));
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(!content.contains("grants") && !content.contains(&saved.routine.id));
    let back = call("routines.import", json!({ "content": content }))
        .await
        .unwrap();
    assert_eq!(back["name"], "Work mode");
    assert_eq!(back["enabled"], false);
    assert_eq!(back["grants"], json!([]));
    let evil = content.replace("apps.launch", "shell.format_everything");
    assert!(
        call("routines.import", json!({ "content": evil }))
            .await
            .is_err()
    );
    assert!(
        call("routines.import", json!({ "content": "{}" }))
            .await
            .is_err()
    );
    std::fs::remove_file(path).ok();
    r.stop().await;
}
