//! M7 memory, end to end through the real runtime pieces on fakes: "remember …" said to KIVO
//! becomes a note in the vault in the user's own words, a later brain request carries it as
//! user-provided memory, "Why did you say that?" names it, a newer fact supersedes it and
//! "forget that …" removes it (MEM-05/08/10, CONV-17/18); a compaction suggests something worth
//! remembering and the Island asks (CONV-20); and the Memory page's requests (UX-29, MEM-06).

#![cfg(windows)]

use kivo_brain::PrivacyClass;
use kivo_brain::testing::{Script, ScriptedBrain};
use kivo_core::{Capability, SessionState};
use kivo_ipc::protocol::{MemoryNoteDetail, MemoryOverview};
use kivo_runtime::scripted::{self, Rig};
use serde_json::{Value, json};
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
        tokio::time::sleep(Duration::from_millis(20)).await;
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

    async fn rpc(&self, method: &str, params: Value) -> Result<Value, String> {
        self.rig
            .memory_rpc
            .call(method, params)
            .await
            .expect("a memory request")
            .map_err(|e| e.message)
    }

    async fn overview(&self) -> MemoryOverview {
        serde_json::from_value(self.rpc("memory.overview", json!({})).await.unwrap()).unwrap()
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
        c.capabilities.set(Capability::Memory, true);
        c.voice.speak_typed_replies = false;
    });
    Running { rig, worker, pump }
}

async fn answered(rig: &Rig) -> kivo_ipc::protocol::TurnView {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        let done = rig.core.state().borrow().session == SessionState::Idle
            && rig
                .core
                .turn_view()
                .is_some_and(|t| t.answer.is_some() || t.error.is_some());
        if done {
            return rig.core.turn_view().unwrap();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the turn didn't end: {:#?}", rig.core.turn_view());
}

/// Says `text` and waits for the answer (a new turn replaces the last one's view).
async fn say(rig: &Rig, text: &str) -> kivo_ipc::protocol::TurnView {
    let before = rig.core.turn_view().map(|t| t.id);
    rig.engine.say(text).await.unwrap();
    until("the new turn", Duration::from_secs(10), || {
        rig.core.turn_view().map(|t| t.id) != before
    })
    .await;
    answered(rig).await
}

fn local_brain(script: Vec<Script>) -> Arc<ScriptedBrain> {
    let mut b = ScriptedBrain::new("ollama", PrivacyClass::Local, script);
    b.info.name = "Ollama".into();
    Arc::new(b)
}

/// MEM-05, MEM-08, MEM-10, CONV-17/18: remembered in the user's own words, used in a later
/// request, named by "Why did you say that?", superseded, and forgotten on request.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn remembering_by_voice_then_answers_use_it() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();

    // The grammar handles it: no brain, and the path keeps its case and backslash.
    let turn = say(&r.rig, "Kivo, remember that my project is in D:\\Work.").await;
    assert_eq!(turn.answer.as_deref(), Some("I'll remember that."));
    let notes = r.overview().await.notes;
    let fact = notes
        .iter()
        .find(|n| n.kind == "fact")
        .expect("a fact note");
    let detail: MemoryNoteDetail = serde_json::from_value(
        r.rpc("memory.note", json!({ "path": fact.path }))
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(
        detail.markdown.contains("my project is in D:\\Work."),
        "{}",
        detail.markdown
    );
    assert!(detail.markdown.starts_with("---\ntype: fact"));

    // A later question: the brain's request carries it as KIVO's own memory.
    let b = local_brain(vec![Script::text("It's in D:\\Work.")]);
    r.rig.brains.insert(b.clone());
    let turn = say(&r.rig, "where is my project folder?").await;
    assert_eq!(turn.answer.as_deref(), Some("It's in D:\\Work."));
    let request = format!("{:?}", b.requests.lock().unwrap()[0]);
    assert!(
        request.contains("my project is in D:\\\\Work."),
        "{request}"
    );
    assert!(request.contains("remember"), "{request}");
    // "Why did you say that?"
    let why = r
        .rpc("memory.why", json!({ "turn": turn.id }))
        .await
        .unwrap();
    assert_eq!(why[0]["path"], fact.path.as_str());
    assert_eq!(why[0]["exists"], true);

    // A newer fact about the same thing replaces it; the old one stays as history.
    say(&r.rig, "remember that my project is in E:\\Code now").await;
    let notes = r.overview().await.notes;
    let old = notes.iter().find(|n| n.path == fact.path).unwrap();
    assert!(old.valid_until.is_some(), "superseded");
    let b2 = local_brain(vec![Script::text("E:\\Code.")]);
    r.rig.brains.insert(b2.clone());
    say(&r.rig, "where is my project folder now?").await;
    // The memory block has only the current fact (the thread still has the earlier answer).
    let system = format!("{:?}", b2.requests.lock().unwrap()[0].system);
    assert!(
        system.contains("E:\\\\Code") && !system.contains("D:\\\\Work"),
        "{system}"
    );

    // The Island's "Remember this" (MEM-05): that turn's own answer becomes a fact.
    let b3 = local_brain(vec![Script::text("Your dentist is Dr. Rao on Park Street.")]);
    r.rig.brains.insert(b3.clone());
    let turn = say(&r.rig, "who is my dentist again?").await;
    let kept = r
        .rpc("memory.rememberTurn", json!({ "turnId": turn.id }))
        .await
        .unwrap();
    let detail: MemoryNoteDetail = serde_json::from_value(
        r.rpc("memory.note", json!({ "path": kept["path"] }))
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(detail.markdown.contains("Dr. Rao on Park Street"), "{}", detail.markdown);
    assert!(
        r.rpc("memory.rememberTurn", json!({ "turnId": "no-such-turn" }))
            .await
            .is_err()
    );

    // "Forget that …": gone from the vault.
    let turn = say(&r.rig, "forget that my project is in E:\\Code").await;
    assert_eq!(turn.answer.as_deref(), Some("Forgotten."));
    assert!(
        r.overview()
            .await
            .notes
            .iter()
            .all(|n| !n.excerpt.contains("E:\\Code"))
    );

    // A password is never kept.
    let turn = say(&r.rig, "remember that the wifi password is hunter22").await;
    assert!(turn.error.is_some() || turn.answer.as_deref() != Some("I'll remember that."));
    assert!(
        r.overview()
            .await
            .notes
            .iter()
            .all(|n| !n.excerpt.contains("hunter22"))
    );
    r.stop().await;
}

/// CONV-20: compacting a conversation suggests what's worth remembering; the Island asks, and a
/// spoken "yes" keeps it. Nothing is kept without that yes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compaction_suggests_and_the_island_asks() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    let thread = "t-release";
    {
        let db = r.rig.db.lock().unwrap();
        db.create_conversation(thread, "chat", "Release").unwrap();
        for i in 0..6 {
            let (role, text) = if i % 2 == 0 {
                (
                    "user",
                    format!("About the release, point {i}. We always ship on Fridays."),
                )
            } else {
                ("assistant", format!("Noted, point {i}."))
            };
            db.add_message(thread, role, &text, None, None).unwrap();
        }
    }
    let b = local_brain(vec![Script::text(
        "Sam is planning the release.\nREMEMBER: Sam ships releases on Fridays",
    )]);
    r.rig.brains.insert(b.clone());
    r.rig.engine.compact(thread, None).await.unwrap();
    let summary = r
        .rig
        .db
        .lock()
        .unwrap()
        .conversation(thread)
        .unwrap()
        .unwrap()
        .summary;
    assert_eq!(summary, "Sam is planning the release.");
    // Suggested, not kept: the Island asks.
    let offer = r.rig.core.offer().expect("the Island asks");
    assert!(
        offer.text.contains("Sam ships releases on Fridays"),
        "{}",
        offer.text
    );
    let overview = r.overview().await;
    assert_eq!(overview.suggestions.len(), 1);
    assert!(overview.notes.iter().all(|n| n.kind != "fact"));
    // A spoken yes keeps it.
    let turn = say(&r.rig, "yes").await;
    assert_eq!(turn.answer.as_deref(), Some("I'll remember that."));
    let overview = r.overview().await;
    assert!(overview.suggestions.is_empty());
    assert!(
        overview
            .notes
            .iter()
            .any(|n| n.kind == "fact" && n.excerpt.contains("Fridays"))
    );
    r.stop().await;
}

/// UX-29, MEM-06: the Memory page — remember with tags, read, edit, label sensitive, open in
/// Obsidian, export to JSON, tidy, and Forget everything (audited).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_memory_page() {
    let _one = ONE_AT_A_TIME.lock().await;
    let r = start();
    let added = r
        .rpc(
            "memory.remember",
            json!({ "text": "Design reviews with [[Maya]] are on Thursdays", "tags": ["design"] }),
        )
        .await
        .unwrap();
    let path = added["path"].as_str().unwrap().to_owned();
    let overview = r.overview().await;
    assert!(overview.tags.iter().any(|t| t.tag == "design"));
    assert!(
        overview
            .folders
            .iter()
            .any(|f| f.path == "notes" && f.count == 1)
    );

    // Edit the Markdown, as in the page's editor.
    let detail: MemoryNoteDetail =
        serde_json::from_value(r.rpc("memory.note", json!({ "path": path })).await.unwrap())
            .unwrap();
    assert_eq!(detail.links, ["Maya"]);
    let edited = detail.markdown.replace("Thursdays", "Thursdays at 3 pm");
    r.rpc("memory.save", json!({ "path": path, "markdown": edited }))
        .await
        .unwrap();
    r.rpc(
        "memory.meta",
        json!({ "path": path, "sensitivity": "sensitive" }),
    )
    .await
    .unwrap();
    let overview = r.overview().await;
    let note = overview.notes.iter().find(|n| n.path == path).unwrap();
    assert!(note.excerpt.contains("3 pm"));
    assert_eq!(note.sensitivity, "sensitive");
    assert!(overview.tags.iter().any(|t| t.tag == "sensitive"));
    // A person note links back to it.
    std::fs::create_dir_all(r.rig.memory.root().join("people")).unwrap();
    std::fs::write(
        r.rig.memory.root().join("people").join("maya.md"),
        "# Maya\n- Designer\n",
    )
    .unwrap();
    r.rig.memory.sync();
    let maya: MemoryNoteDetail = serde_json::from_value(
        r.rpc("memory.note", json!({ "path": "people/maya.md" }))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(maya.backlinks, std::slice::from_ref(&path));
    assert_eq!(maya.note.kind, "person");

    // Open in Obsidian and in Explorer.
    r.rpc("memory.open", json!({ "in": "obsidian" }))
        .await
        .unwrap();
    r.rpc("memory.open", json!({ "in": "folder", "path": path }))
        .await
        .unwrap();
    assert!(
        r.rpc("memory.open", json!({ "in": "folder", "path": "../x.md" }))
            .await
            .is_err()
    );
    let opened = r.rig.opened.lock().unwrap().clone();
    assert!(
        opened
            .iter()
            .any(|o| o.starts_with("obsidian://open?path=")),
        "{opened:?}"
    );
    assert!(
        opened
            .iter()
            .any(|o| o.starts_with("folder:") && o.ends_with("notes")),
        "{opened:?}"
    );

    // Export, tidy, forget.
    let export = r.rpc("memory.export", json!({})).await.unwrap();
    let file = export["file"].as_str().unwrap();
    let body: Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    assert_eq!(body["notes"].as_array().unwrap().len(), 2);
    r.rpc("memory.tidy", json!({})).await.unwrap();
    let forgot = r.rpc("memory.forget", json!({})).await.unwrap();
    assert_eq!(forgot["forgotten"], 2);
    assert!(r.overview().await.notes.is_empty());
    assert!(
        r.rig
            .recorder
            .audit_rows(10)
            .iter()
            .any(|a| a.tool == "memory.forget"),
        "forgetting is audited"
    );
    r.stop().await;
}
