//! Settings → Performance and Diagnostics (UX-31): how much of the PC KIVO uses, how quickly it
//! answers, which models are loaded, and a set of checks that everything works.
//!
//! KIVO's processes are the runtime, the app and its WebView2 helpers, and the speech worker. Their
//! CPU is sampled over one second; memory is their private working sets. Response times are
//! medians over the latest requests' own timings (ARCH-28, T0–T10).

use crate::engine::Engine;
use crate::models::Models;
use kivo_core::text;
use kivo_ipc::RpcError;
use kivo_ipc::protocol::method;
use kivo_platform::{Processes, SystemInfo};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::Arc;

/// Program names that are KIVO.
const KIVO_PROGRAMS: &[&str] = &[
    "kivo-runtime.exe",
    "kivo-app.exe",
    "kivo.exe",
    "kivo-infer.exe",
];
/// The latest requests the medians are taken over.
const RECENT_TURNS: u32 = 200;

/// Opens an https page in the default browser.
pub type OpenUrl = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

pub struct SystemRpc {
    pub engine: Arc<Engine>,
    pub models: Arc<Models>,
    pub system: Arc<dyn SystemInfo>,
    pub processes: Arc<dyn Processes>,
    pub browser: Option<Arc<dyn kivo_tools::Browser>>,
    /// What's installed, for setup's recommendations (UX-36).
    pub discovery: Option<Arc<crate::discovery::Discovery>>,
    /// Opens an https page in the default browser.
    pub open_url: OpenUrl,
}

/// One check on the Diagnostics tab.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub id: String,
    pub ok: bool,
    pub title: String,
    pub detail: String,
    /// The page that fixes it, when one does.
    pub fix: Option<String>,
}

/// The pids of KIVO's processes: KIVO's own programs and everything they started (the app's
/// WebView2 processes), found through parent links.
pub fn kivo_pids(list: &[kivo_platform::ProcessInfo], own: u32) -> Vec<u32> {
    let mut ours: HashSet<u32> = list
        .iter()
        .filter(|p| KIVO_PROGRAMS.contains(&p.name.to_ascii_lowercase().as_str()))
        .map(|p| p.pid)
        .collect();
    ours.insert(own);
    // Children of KIVO's processes, repeatedly (WebView2 has a browser process and its helpers).
    loop {
        let before = ours.len();
        for p in list {
            if ours.contains(&p.parent) && p.pid != 0 && p.pid != p.parent {
                ours.insert(p.pid);
            }
        }
        if ours.len() == before {
            break;
        }
    }
    let mut out: Vec<u32> = ours.into_iter().collect();
    out.sort_unstable();
    out
}

/// The median of `values`, when there are any.
pub fn median(mut values: Vec<u64>) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    Some(values[values.len() / 2])
}

/// Response-time medians in ms: wake → chime, end of speech → text, simple command → done,
/// question → first spoken word.
pub fn latencies(turns: &[(Option<String>, String, Value)]) -> Value {
    let at = |spans: &Value, key: &str| spans.get(key).and_then(Value::as_u64);
    let mut wake = Vec::new();
    let mut stt = Vec::new();
    let mut command = Vec::new();
    let mut answer = Vec::new();
    for (route, source, spans) in turns {
        let spoke = at(spans, "t4EndOfSpeech");
        let from = spoke.unwrap_or(0);
        if source == "wake"
            && let Some(t1) = at(spans, "t1Listening")
        {
            wake.push(t1);
        }
        if let (Some(t4), Some(t5)) = (spoke, at(spans, "t5FinalTranscript")) {
            stt.push(t5.saturating_sub(t4));
        }
        match route {
            None => {
                if let Some(done) = at(spans, "t8ToolDone") {
                    command.push(done.saturating_sub(from));
                }
            }
            Some(_) => {
                if let Some(first) = at(spans, "t9FirstAudio") {
                    answer.push(first.saturating_sub(from));
                }
            }
        }
    }
    json!({
        "wakeToChime": median(wake),
        "speechToText": median(stt),
        "commandDone": median(command),
        "firstWord": median(answer),
    })
}

impl SystemRpc {
    /// Handles `name` if it is one of these requests.
    pub async fn call(&self, name: &str, params: Value) -> Option<Result<Value, RpcError>> {
        Some(match name {
            method::PERFORMANCE_STATUS => Ok(self.performance().await),
            method::DIAGNOSTICS_RUN => Ok(json!(self.diagnostics().await)),
            method::SETUP_RECOMMEND => self.recommend().await,
            method::SYSTEM_OPEN_URL => {
                let url = params["url"].as_str().unwrap_or_default();
                // Web pages only, never a file or another program.
                if url.starts_with("https://") && !url.contains(['"', ' ', '\n']) {
                    (self.open_url)(url)
                        .map(|()| Value::Null)
                        .map_err(|e| RpcError::new(RpcError::REFUSED, e))
                } else {
                    Err(RpcError::new(RpcError::REFUSED, "only https links"))
                }
            }
            _ => return None,
        })
    }

    /// Setup's recommended defaults (UX-36). Looks for agents and local servers first if it
    /// never has.
    async fn recommend(&self) -> Result<Value, RpcError> {
        let system = Arc::clone(&self.system);
        let (machine, network) =
            tokio::task::spawn_blocking(move || (system.snapshot(), system.network()))
                .await
                .map_err(|e| RpcError::new(RpcError::INTERNAL, e.to_string()))?;
        let machine = machine.map_err(|e| RpcError::new(RpcError::REFUSED, e.to_string()))?;
        let mut sections = [Vec::new(), Vec::new()];
        if let Some(d) = &self.discovery {
            for (slot, name) in sections.iter_mut().zip(["cli", "local"]) {
                if d.section(name).checked_at.is_none() {
                    d.refresh(name).await;
                }
                *slot = d
                    .section(name)
                    .items
                    .into_iter()
                    .map(|i| (i.id, i.data))
                    .collect();
            }
        }
        let connected = self.engine.brains.views(&self.engine.core.config()).len();
        let advice = crate::setup::recommend(&crate::setup::Inputs {
            machine: &machine,
            network,
            agents: &sections[0],
            servers: &sections[1],
            connected,
        });
        Ok(json!(advice))
    }

    async fn performance(&self) -> Value {
        let processes = Arc::clone(&self.processes);
        let system = Arc::clone(&self.system);
        // CPU over one second, off the async threads.
        let sampled = tokio::task::spawn_blocking(move || {
            let list = processes.list().unwrap_or_default();
            let pids = kivo_pids(&list, std::process::id());
            let usage = |pids: &[u32]| -> (u64, u64) {
                pids.iter()
                    .filter_map(|p| system.process_usage(*p))
                    .fold((0, 0), |(c, m), u| (c + u.cpu_ms, m + u.memory_bytes))
            };
            let (cpu_before, _) = usage(&pids);
            std::thread::sleep(std::time::Duration::from_secs(1));
            let (cpu_after, memory) = usage(&pids);
            let cpus = system.snapshot().map_or(1, |s| s.logical_cpus.max(1));
            let busy_ms = cpu_after.saturating_sub(cpu_before);
            #[allow(clippy::cast_precision_loss, reason = "milliseconds in a second")]
            let percent = busy_ms as f64 / (1_000.0 * f64::from(cpus)) * 100.0;
            (percent, memory, pids.len())
        })
        .await
        .unwrap_or((0.0, 0, 0));
        let timings = self
            .engine
            .recorder
            .database()
            .lock()
            .map(|db| db.recent_turn_timings(RECENT_TURNS).unwrap_or_default())
            .unwrap_or_default();
        let models: Vec<Value> = self
            .models
            .list()
            .into_iter()
            .filter(|m| m.installed)
            .map(|m| {
                json!({
                    "id": m.id, "name": m.name, "kind": m.kind,
                    "residency": m.residency, "diskBytes": m.disk_bytes,
                })
            })
            .collect();
        json!({
            "cpuPercent": (sampled.0 * 10.0).round() / 10.0,
            "memoryMb": sampled.1 / (1024 * 1024),
            "processes": sampled.2,
            // Speech runs on the CPU (ONNX Runtime's CPU provider): KIVO uses no graphics card.
            "gpu": false,
            "latency": latencies(&timings),
            "models": models,
            "profile": self.engine.core.config().performance.profile,
        })
    }

    async fn diagnostics(&self) -> Vec<Check> {
        let config = self.engine.core.config();
        let mut out = Vec::new();
        let check = |id: &str, ok: bool, detail: String, fix: Option<&str>| Check {
            id: id.into(),
            ok,
            title: text::t(&format!("diagnostics.{id}")),
            detail,
            fix: fix.map(str::to_owned),
        };
        // The microphone: one to listen with.
        let audio = self.engine.speaker.audio();
        let inputs = audio.input_devices().unwrap_or_default();
        let mic = inputs.iter().find(|d| d.is_default).or(inputs.first());
        out.push(check(
            "microphone",
            mic.is_some(),
            mic.map_or_else(|| text::t("diagnostics.noMic"), |d| d.name.clone()),
            mic.is_none().then_some("voice"),
        ));
        // The wake word: the keyword model and at least one word.
        let spotter = self
            .models
            .installed_dir(kivo_store::models::KEYWORD_SPOTTER)
            .is_some();
        let words = self
            .engine
            .recorder
            .database()
            .lock()
            .map(|db| db.wake_words().map(|w| w.len()).unwrap_or(0))
            .unwrap_or(0);
        out.push(check(
            "wake",
            spotter && words > 0,
            if spotter {
                text::tf("diagnostics.wakeWords", &[("count", &words)])
            } else {
                text::t("diagnostics.noSpotter")
            },
            (!spotter || words == 0).then_some("voice"),
        ));
        // Speech in and out.
        let speech = self.engine.core.state().borrow().speech.clone();
        let stt_ready = matches!(speech, kivo_ipc::protocol::SpeechStatus::Ready);
        out.push(check(
            "speech",
            stt_ready,
            if stt_ready {
                text::tf(
                    "diagnostics.speechEngines",
                    &[
                        ("stt", &config.voice.stt_engine),
                        ("tts", &config.voice.tts_engine),
                    ],
                )
            } else {
                text::t("diagnostics.noSpeech")
            },
            (!stt_ready).then_some("voice"),
        ));
        // Echo cancellation: KIVO's own (AEC3), used hands-free.
        out.push(check("echo", true, text::t("diagnostics.echoReady"), None));
        // Brains: every one enabled can be reached.
        let views = self.engine.brains.views(&config);
        let enabled: Vec<_> = views.iter().filter(|v| v.enabled).collect();
        let broken: Vec<&str> = enabled
            .iter()
            .filter(|v| !matches!(v.health, kivo_brain::Health::Ready))
            .map(|v| v.name.as_str())
            .collect();
        out.push(check(
            "brains",
            broken.is_empty(),
            if enabled.is_empty() {
                text::t("diagnostics.noBrains")
            } else if broken.is_empty() {
                enabled
                    .iter()
                    .map(|v| v.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            } else {
                text::tf("diagnostics.brainsDown", &[("names", &broken.join(", "))])
            },
            (!broken.is_empty()).then_some("brains"),
        ));
        // The browser extension (TOOL-24).
        let extension = self.browser.as_ref().is_some_and(|b| b.connected());
        out.push(check(
            "extension",
            extension,
            text::t(if extension {
                "diagnostics.extensionOn"
            } else {
                "diagnostics.extensionOff"
            }),
            (!extension).then_some("permissions"),
        ));
        // The audit log's hash chain (SECURITY §7).
        let chain = self
            .engine
            .recorder
            .database()
            .lock()
            .ok()
            .and_then(|db| db.verify_audit().ok());
        out.push(match chain {
            Some(c) if c.broken_at.is_none() => check(
                "audit",
                true,
                text::tf("diagnostics.auditOk", &[("count", &c.rows)]),
                None,
            ),
            Some(c) => check(
                "audit",
                false,
                text::tf(
                    "diagnostics.auditBroken",
                    &[("row", &c.broken_at.unwrap_or_default())],
                ),
                Some("activity"),
            ),
            None => check("audit", false, text::t("diagnostics.auditUnread"), None),
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, name: &str, parent: u32) -> kivo_platform::ProcessInfo {
        kivo_platform::ProcessInfo {
            pid,
            name: name.into(),
            parent,
        }
    }

    #[test]
    fn kivos_processes_and_their_children_are_counted() {
        let list = [
            p(10, "explorer.exe", 1),
            p(20, "kivo-app.exe", 10),
            p(21, "msedgewebview2.exe", 20),
            p(22, "msedgewebview2.exe", 21),
            p(30, "kivo-infer.exe", 40),
            p(40, "kivo-runtime.exe", 10),
            p(50, "chrome.exe", 10),
            p(51, "msedgewebview2.exe", 50),
        ];
        assert_eq!(kivo_pids(&list, 40), [20, 21, 22, 30, 40]);
    }

    #[test]
    fn medians_come_from_each_kind_of_request() {
        let turns = vec![
            (
                None,
                "wake".into(),
                json!({"t1Listening": 140, "t4EndOfSpeech": 1000, "t5FinalTranscript": 1250, "t8ToolDone": 1180}),
            ),
            (
                None,
                "wake".into(),
                json!({"t1Listening": 150, "t4EndOfSpeech": 900, "t5FinalTranscript": 1170, "t8ToolDone": 1100}),
            ),
            (
                Some("Default · Claude".into()),
                "pushToTalk".into(),
                json!({"t4EndOfSpeech": 800, "t5FinalTranscript": 1070, "t9FirstAudio": 1900}),
            ),
            (None, "typed".into(), json!({"t8ToolDone": 120})),
        ];
        let l = latencies(&turns);
        assert_eq!(l["wakeToChime"], 150);
        assert_eq!(l["speechToText"], 270);
        assert_eq!(l["commandDone"], 180);
        assert_eq!(l["firstWord"], 1100);
        assert_eq!(latencies(&[])["firstWord"], Value::Null);
    }
}
