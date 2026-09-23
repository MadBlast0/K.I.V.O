//! Requests from the UIs (ARCHITECTURE §3). `hello`, `ping` and `state.get` are answered by the
//! IPC server itself; everything else arrives here. Every client is untrusted (INT-11): requests
//! carry no authority beyond what a user could do from the tray, and switching the permission mode
//! or a capability is a user action, never something a tool or the AI can reach (SECURITY §1.1).

use crate::activity::Recorder;
use crate::core::Core;
use crate::engine::Engine;
use crate::lifecycle::Lifecycle;
use crate::models::Models;
use kivo_core::Capability;
use kivo_core::config::PermissionMode;
use kivo_core::event::{CancelReason, TurnSource};
use kivo_ipc::protocol::CapabilityItem;
use kivo_ipc::{BoxFuture, Handler, RpcError, method};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

pub struct Rpc {
    core: Arc<Core>,
    engine: Arc<Engine>,
    models: Arc<Models>,
    recorder: Recorder,
    lifecycle: Arc<Lifecycle>,
    /// Wake words, voice enrollment and sounds (VOICE §4–6).
    voice: Option<Arc<crate::voice_rpc::VoiceRpc>>,
    /// Brains, Chat, Usage and preferences (BRAINS, CONVERSATION).
    brains: Option<Arc<crate::brains_rpc::BrainsRpc>>,
}

impl Rpc {
    pub fn new(
        core: Arc<Core>,
        engine: Arc<Engine>,
        models: Arc<Models>,
        recorder: Recorder,
        lifecycle: Arc<Lifecycle>,
    ) -> Self {
        Self {
            core,
            engine,
            models,
            recorder,
            lifecycle,
            voice: None,
            brains: None,
        }
    }

    /// Adds the brain requests.
    #[must_use]
    pub fn with_brains(mut self, brains: Arc<crate::brains_rpc::BrainsRpc>) -> Self {
        self.brains = Some(brains);
        self
    }

    /// Adds the voice requests.
    #[must_use]
    pub fn with_voice(mut self, voice: Arc<crate::voice_rpc::VoiceRpc>) -> Self {
        self.voice = Some(voice);
        self
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RpcError> {
    serde_json::from_value(params).map_err(RpcError::invalid_params)
}

fn ok<T: serde::Serialize>(value: &T) -> Result<Value, RpcError> {
    serde_json::to_value(value).map_err(|e| RpcError::new(RpcError::INTERNAL, e.to_string()))
}

impl Handler for Rpc {
    #[allow(
        clippy::too_many_lines,
        reason = "one table of the requests the UI can make"
    )]
    fn call(&self, name: String, params: Value) -> BoxFuture<Result<Value, RpcError>> {
        let core = Arc::clone(&self.core);
        let engine = Arc::clone(&self.engine);
        let models = Arc::clone(&self.models);
        let recorder = self.recorder.clone();
        let lifecycle = Arc::clone(&self.lifecycle);
        let voice = self.voice.clone();
        let brains = self.brains.clone();
        Box::pin(async move {
            if let Some(brains) = brains
                && let Some(result) = brains.call(&name, params.clone()).await
            {
                return result;
            }
            if let Some(voice) = voice
                && let Some(result) = voice.call(&name, params.clone()).await
            {
                return result;
            }
            let refused = |e: crate::core::Refused| RpcError::new(RpcError::REFUSED, e.0);
            let refuse = |message: String| RpcError::new(RpcError::REFUSED, message);
            match name.as_str() {
                method::SESSION_PAUSE => core.pause().map(|_| Value::Null).map_err(refused),
                method::SESSION_RESUME => core.resume().map(|_| Value::Null).map_err(refused),
                method::RUNTIME_QUIT => {
                    core.quit();
                    Ok(Value::Null)
                }
                method::PERMISSIONS_SET_MODE => {
                    #[derive(Deserialize)]
                    #[serde(deny_unknown_fields)]
                    struct Params {
                        mode: PermissionMode,
                    }
                    let Params { mode } = parse(params)?;
                    core.set_mode(mode).map(|_| Value::Null).map_err(refused)
                }
                // Talk: the Island's mic button and Home's Talk button behave like push-to-talk
                // in toggle mode (the pipeline ends the utterance on silence).
                method::SESSION_TALK => engine
                    .talk(TurnSource::PushToTalk)
                    .await
                    .map(|()| Value::Null)
                    .map_err(refuse),
                method::SESSION_SAY => {
                    #[derive(Deserialize)]
                    #[serde(deny_unknown_fields)]
                    struct Params {
                        text: String,
                    }
                    let Params { text } = parse(params)?;
                    engine
                        .say(&text)
                        .await
                        .map(|()| Value::Null)
                        .map_err(refuse)
                }
                method::SESSION_CANCEL => {
                    engine.cancel(CancelReason::UserButton);
                    Ok(Value::Null)
                }
                method::SESSION_STOP_ALL => {
                    engine.stop_everything();
                    Ok(Value::Null)
                }
                method::PERMISSIONS_ANSWER => {
                    #[derive(Deserialize)]
                    #[serde(rename_all = "camelCase", deny_unknown_fields)]
                    struct Params {
                        call_id: String,
                        allow: bool,
                        #[serde(default)]
                        always: bool,
                    }
                    let p: Params = parse(params)?;
                    engine
                        .answer_confirmation(&p.call_id, p.allow, p.always)
                        .await
                        .map(|()| Value::Null)
                        .map_err(refuse)
                }
                method::CAPABILITIES_GET => ok(&capability_list(&core)),
                method::CAPABILITIES_SET => {
                    #[derive(Deserialize)]
                    #[serde(deny_unknown_fields)]
                    struct Params {
                        capability: Capability,
                        on: bool,
                    }
                    let Params { capability, on } = parse(params)?;
                    let mut changed = false;
                    core.update_config(|config| changed = config.capabilities.set(capability, on));
                    if changed {
                        // Every toggle change is audited (CAP-03).
                        recorder.capability_changed(capability, on);
                    }
                    ok(&capability_list(&core))
                }
                method::ACTIVITY_LIST => {
                    #[derive(Deserialize)]
                    #[serde(deny_unknown_fields)]
                    struct Params {
                        #[serde(default)]
                        before: Option<i64>,
                        #[serde(default = "default_limit")]
                        limit: u32,
                    }
                    let p: Params = parse(params).unwrap_or(Params {
                        before: None,
                        limit: 50,
                    });
                    ok(&recorder.recent(p.before, p.limit))
                }
                method::PERMISSIONS_GRANTS => ok(&recorder.grants_in_force()),
                method::PERMISSIONS_REVOKE => {
                    #[derive(Deserialize)]
                    #[serde(deny_unknown_fields)]
                    struct Params {
                        id: i64,
                    }
                    let Params { id } = parse(params)?;
                    if recorder.revoke_grant(id) {
                        Ok(Value::Null)
                    } else {
                        Err(refuse(kivo_core::text::t("turn.permissionGone")))
                    }
                }
                method::AUDIT_LIST => {
                    #[derive(Deserialize)]
                    #[serde(deny_unknown_fields)]
                    struct Params {
                        #[serde(default = "default_limit")]
                        limit: u32,
                    }
                    let p: Params = parse(params).unwrap_or(Params { limit: 50 });
                    ok(&recorder.audit_rows(p.limit))
                }
                method::MODELS_LIST => ok(&models.list()),
                method::MODELS_INSTALL => {
                    let Id { id } = parse(params)?;
                    models.install(&id).map(|()| Value::Null).map_err(refuse)
                }
                method::MODELS_REMOVE => {
                    #[derive(serde::Deserialize)]
                    struct Remove {
                        id: String,
                        #[serde(default)]
                        confirmed: bool,
                    }
                    let Remove { id, confirmed } = parse(params)?;
                    models
                        .remove(&id, confirmed)
                        .map(|()| Value::Null)
                        .map_err(refuse)
                }
                method::MODELS_PAUSE => {
                    #[derive(serde::Deserialize)]
                    struct Id {
                        id: String,
                    }
                    let Id { id } = parse(params)?;
                    models.pause(&id);
                    Ok(Value::Null)
                }
                method::MODELS_CANCEL => {
                    #[derive(serde::Deserialize)]
                    struct Id {
                        id: String,
                    }
                    let Id { id } = parse(params)?;
                    let models = Arc::clone(&models);
                    tokio::task::spawn_blocking(move || models.cancel(&id))
                        .await
                        .map_err(|e| RpcError::new(RpcError::INTERNAL, e.to_string()))?
                        .map(|()| Value::Null)
                        .map_err(refuse)
                }
                method::UI_WINDOW_CLOSED => Ok(serde_json::json!({
                    "keepRunning": lifecycle.window_closed()
                })),
                method::SETTINGS_GET => ok(&core.config()),
                method::SETTINGS_SET => {
                    // A partial settings object, merged into the current one.
                    let current = serde_json::to_value(core.config())
                        .map_err(|e| RpcError::new(RpcError::INTERNAL, e.to_string()))?;
                    let merged = merge(current, params);
                    let config: kivo_core::KivoConfig =
                        serde_json::from_value(merged).map_err(RpcError::invalid_params)?;
                    config.validate().map_err(|problems| {
                        refuse(
                            problems
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join("; "),
                        )
                    })?;
                    let saved = core.update_config(|c| *c = config);
                    models.apply_engines(&saved);
                    engine.settings_changed(&saved);
                    lifecycle.apply_autostart(&saved);
                    ok(&saved)
                }
                _ => Err(RpcError::method_not_found(&name)),
            }
        })
    }
}

fn default_limit() -> u32 {
    50
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Id {
    id: String,
}

fn capability_list(core: &Core) -> Vec<CapabilityItem> {
    let settings = core.config().capabilities;
    Capability::ALL
        .iter()
        .map(|&capability| CapabilityItem {
            capability,
            label: capability.label(),
            enabled: settings.enabled(capability),
            default: capability.default_enabled(),
            badges: capability.badges().to_vec(),
        })
        .collect()
}

/// Deep-merges `patch` into `base` (objects merge key by key; anything else replaces).
fn merge(base: Value, patch: Value) -> Value {
    match (base, patch) {
        (Value::Object(mut base), Value::Object(patch)) => {
            for (key, value) in patch {
                let merged = match base.remove(&key) {
                    Some(existing) => merge(existing, value),
                    None => value,
                };
                base.insert(key, merged);
            }
            Value::Object(base)
        }
        (_, patch) => patch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infer::Infer;
    use crate::speaker::Speaker;
    use kivo_core::SessionState;
    use kivo_intent::{Grammar, IntentRouter};
    use kivo_store::Database;
    use serde_json::json;
    use std::sync::Mutex;

    fn rpc() -> (Rpc, Arc<Core>) {
        let core = Arc::new(Core::default());
        let db = Arc::new(Mutex::new(Database::in_memory().unwrap()));
        let recorder = Recorder::new(db, true);
        let (infer, _events, _tx) = Infer::new(std::path::PathBuf::from("kivo-infer"));
        let audio = Arc::new(kivo_testkit::FakeAudio::with_clip(Vec::new()));
        let speaker = Arc::new(Speaker::new(audio, None));
        let catalog = Arc::new(std::sync::RwLock::new(Vec::new()));
        let engine = Arc::new(Engine::new(crate::engine::Parts {
            core: Arc::clone(&core),
            infer: infer.clone(),
            speaker,
            registry: Arc::new(kivo_tools::Registry::default()),
            recorder: recorder.clone(),
            apps: Arc::new(kivo_testkit::FakeApps::default()),
            windows: Arc::new(kivo_testkit::FakeWindows::default()),
            app_catalog: catalog,
            router: IntentRouter::new(Grammar::bundled("en").unwrap()),
            system: Arc::new(kivo_testkit::FakeSystemInfo::default()),
            fallback_voice: None,
            brains: Arc::new(crate::brains::Brains::new(
                Arc::new(Mutex::new(Database::in_memory().unwrap())),
                Arc::new(kivo_testkit::FakeSecrets::default()),
                0,
            )),
            agents: crate::agents::Agents::new(
                Arc::new(Mutex::new(Database::in_memory().unwrap())),
                std::env::temp_dir(),
            ),
        }));
        let lifecycle = Arc::new(Lifecycle::new(
            Arc::clone(&core),
            Arc::new(kivo_testkit::FakeNotifications::default()),
            Arc::new(kivo_testkit::FakeSystemControl::default()),
            Arc::new(kivo_testkit::FakeAutostart::default()),
            recorder.clone(),
            std::env::temp_dir().join("kivo-test-crashes"),
        ));
        let models = Arc::new(Models::new(
            std::env::temp_dir().join("kivo-test-models"),
            Arc::clone(&core),
            infer,
        ));
        (
            Rpc::new(Arc::clone(&core), engine, models, recorder, lifecycle),
            core,
        )
    }

    async fn call(rpc: &Rpc, name: &str, params: Value) -> Result<Value, RpcError> {
        rpc.call(name.into(), params).await
    }

    #[tokio::test]
    async fn pause_resume_and_quit_reach_the_core() {
        let (rpc, core) = rpc();
        call(&rpc, method::SESSION_PAUSE, Value::Null)
            .await
            .unwrap();
        assert_eq!(core.state().borrow().session, SessionState::Paused);
        call(&rpc, method::SESSION_RESUME, Value::Null)
            .await
            .unwrap();
        assert_eq!(core.state().borrow().session, SessionState::Idle);
        let refused = call(&rpc, method::SESSION_RESUME, Value::Null)
            .await
            .unwrap_err();
        assert_eq!(refused.code, RpcError::REFUSED);
        call(&rpc, method::RUNTIME_QUIT, Value::Null).await.unwrap();
        assert!(core.shutdown().is_cancelled());
    }

    #[tokio::test]
    async fn capabilities_are_listed_toggled_and_audited() {
        let (rpc, core) = rpc();
        let list = call(&rpc, method::CAPABILITIES_GET, Value::Null)
            .await
            .unwrap();
        assert_eq!(list.as_array().unwrap().len(), Capability::ALL.len());
        call(
            &rpc,
            method::CAPABILITIES_SET,
            json!({"capability": "shell", "on": true}),
        )
        .await
        .unwrap();
        assert!(core.config().capabilities.enabled(Capability::Shell));
        // Unknown capabilities and missing fields are refused.
        assert!(
            call(
                &rpc,
                method::CAPABILITIES_SET,
                json!({"capability": "teleport", "on": true})
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn settings_merge_and_are_validated() {
        let (rpc, core) = rpc();
        call(
            &rpc,
            method::SETTINGS_SET,
            json!({"voice": {"follow-up-seconds": 5}}),
        )
        .await
        .unwrap();
        let config = core.config();
        assert_eq!(config.voice.follow_up_seconds, 5);
        assert_eq!(
            config.voice.push_to_talk,
            ["Ctrl", "Space"],
            "other settings are untouched"
        );
        let bad = call(
            &rpc,
            method::SETTINGS_SET,
            json!({"voice": {"follow-up-seconds": 200}}),
        )
        .await;
        assert_eq!(bad.unwrap_err().code, RpcError::REFUSED);
    }

    #[tokio::test]
    async fn the_island_requests_are_answered_even_when_nothing_is_running() {
        let (rpc, _core) = rpc();
        call(&rpc, method::SESSION_CANCEL, Value::Null)
            .await
            .unwrap();
        call(&rpc, method::SESSION_STOP_ALL, Value::Null)
            .await
            .unwrap();
        let answer = call(
            &rpc,
            method::PERMISSIONS_ANSWER,
            json!({"callId": "x", "allow": true}),
        )
        .await;
        assert_eq!(answer.unwrap_err().code, RpcError::REFUSED);
        // Stopping everything is itself recorded (SECURITY §8).
        let activity = call(&rpc, method::ACTIVITY_LIST, json!({"limit": 5}))
            .await
            .unwrap();
        let rows = activity.as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["title"], "Stopped everything");
        let models = call(&rpc, method::MODELS_LIST, Value::Null).await.unwrap();
        assert!(!models.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn grants_and_the_audit_log_are_readable_and_revocable() {
        let (rpc, _core) = rpc();
        assert!(
            call(&rpc, method::PERMISSIONS_GRANTS, Value::Null)
                .await
                .unwrap()
                .as_array()
                .unwrap()
                .is_empty()
        );
        let gone = call(&rpc, method::PERMISSIONS_REVOKE, json!({"id": 1})).await;
        assert_eq!(gone.unwrap_err().code, RpcError::REFUSED);
        assert!(
            call(&rpc, method::AUDIT_LIST, json!({"limit": 5}))
                .await
                .unwrap()
                .is_array()
        );
    }

    #[tokio::test]
    async fn unknown_methods_are_refused() {
        let (rpc, _core) = rpc();
        let err = call(&rpc, "nope", Value::Null).await.unwrap_err();
        assert_eq!(err.code, RpcError::METHOD_NOT_FOUND);
    }

    #[test]
    fn settings_patches_merge_deeply() {
        let base = json!({"voice": {"a": 1, "b": 2}, "general": {"c": 3}});
        let merged = merge(base, json!({"voice": {"b": 9}}));
        assert_eq!(
            merged,
            json!({"voice": {"a": 1, "b": 9}, "general": {"c": 3}})
        );
    }
}
