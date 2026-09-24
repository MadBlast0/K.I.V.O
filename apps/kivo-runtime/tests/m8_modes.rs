//! M8 product modes (PLAN-06), on the real runtime pieces: a mode changes what it maps to and is
//! remembered across the switch; Normal brings back exactly the user's own settings, even after a
//! restart in between (what Normal looked like is kept in the database).

#![cfg(windows)]

use kivo_core::config::{IslandSize, PerformanceProfile, PrivacyMode, ProductMode, SpeakMode};
use kivo_runtime::scripted;
use std::path::PathBuf;
use std::time::Duration;

fn worker() -> PathBuf {
    let mut dir = std::env::current_exe().expect("test binary");
    dir.pop();
    dir.pop();
    dir.join("kivo-infer.exe")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_mode_changes_its_part_and_normal_restores_the_rest() {
    let (rig, worker_task, pump) = scripted::rig(worker(), Vec::new(), PathBuf::from("no-model"));
    rig.core.update_config(|c| {
        c.overlay.size = IslandSize::Compact;
        c.performance.profile = PerformanceProfile::Balanced;
    });
    let mine = rig.core.config();

    let presenting = kivo_runtime::modes::apply(&rig.core, &rig.engine, ProductMode::Presentation);
    assert_eq!(presenting.general.product_mode, ProductMode::Presentation);
    assert_eq!(presenting.overlay.size, IslandSize::Large);
    assert!(!presenting.automation.toasts);
    assert_eq!(presenting.automation.speak, SpeakMode::Never);
    // The Island reads its size from the live state: it follows at once.
    assert_eq!(rig.core.state().borrow().island.size, IslandSize::Large);

    let offline = kivo_runtime::modes::apply(&rig.core, &rig.engine, ProductMode::Offline);
    assert_eq!(offline.privacy.mode, PrivacyMode::StrictPrivate);
    assert_eq!(offline.overlay.size, IslandSize::Compact, "no stacking");

    // What Normal looked like survives in the database (a restart in between).
    let kept = rig
        .db
        .lock()
        .unwrap()
        .meta(kivo_runtime::modes::BEFORE_KEY)
        .unwrap()
        .unwrap();
    assert!(kept.contains("\"privacy\":\"cloud\""), "{kept}");

    let normal = kivo_runtime::modes::apply(&rig.core, &rig.engine, ProductMode::Normal);
    let mut expected = mine.clone();
    expected.general.product_mode = ProductMode::Normal;
    assert_eq!(normal, expected);
    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}

/// Waits for the turn that answers `text` and returns what KIVO said.
async fn say(rig: &scripted::Rig, text: &str) -> String {
    let before = rig.core.turn_view().map(|t| t.id);
    rig.engine.say(text).await.unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        if let Some(view) = rig
            .core
            .turn_view()
            .filter(|t| t.id.as_str() != before.as_deref().unwrap_or_default())
            && rig.core.state().borrow().session == kivo_core::SessionState::Idle
            && let Some(answer) = view.answer.clone()
        {
            return answer;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("no answer to {text:?}: {:#?}", rig.core.turn_view());
}

/// "Presentation mode" and "back to normal" by voice: the grammar handles them, no brain, and the
/// speech engines are applied again for the new settings.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn modes_switch_by_voice() {
    let (rig, worker_task, pump) = scripted::rig(worker(), Vec::new(), PathBuf::from("no-model"));
    rig.core.update_config(|c| {
        c.capabilities
            .set(kivo_core::capability::Capability::SpeakResponses, false);
        c.voice.speak_typed_replies = false;
    });
    let applied = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    {
        let applied = std::sync::Arc::clone(&applied);
        rig.engine.set_engines_hook(std::sync::Arc::new(move |_| {
            applied.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }));
    }

    let answer = say(&rig, "presentation mode").await;
    assert_eq!(answer, "Presentation mode is on.");
    let config = rig.core.config();
    assert_eq!(config.general.product_mode, ProductMode::Presentation);
    assert_eq!(config.overlay.size, IslandSize::Large);

    let answer = say(&rig, "turn off presentation mode").await;
    assert_eq!(answer, "Normal mode is on.");
    let config = rig.core.config();
    assert_eq!(config.general.product_mode, ProductMode::Normal);
    assert_ne!(config.overlay.size, IslandSize::Large);
    assert_eq!(applied.load(std::sync::atomic::Ordering::SeqCst), 2);

    rig.core.quit();
    let _ = tokio::time::timeout(Duration::from_secs(5), worker_task).await;
    pump.abort();
}
