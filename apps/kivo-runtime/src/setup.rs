//! Setup's recommendations (UX-36, plan §130): the defaults first launch preselects, chosen from
//! the hardware (RAM, GPU, battery), what is installed (CLI agents and local model servers
//! discovery found), the network (offline, metered) and the brains already connected. Every one is
//! only preselected: the user can pick something else on the same screen.
//!
//! The speech engines have their own recommendation (VOICE-43, `voice.recommend`); this covers the
//! rest of setup.

use kivo_core::text;
use kivo_ipc::protocol::SetupAdvice;
use kivo_platform::{Network, SystemSnapshot};
use serde_json::Value;

/// A PC with this much memory and a GPU with this much of its own runs a local model well.
const LOCAL_RAM_MB: u64 = 16 * 1024;
const LOCAL_VRAM_MB: u64 = 6 * 1024;
/// Below this the lighter profile keeps KIVO out of the way.
const LOW_RAM_MB: u64 = 8 * 1024;

/// What the recommendation is made from.
pub struct Inputs<'a> {
    pub machine: &'a SystemSnapshot,
    /// `None` when Windows can't tell: taken as online and unmetered.
    pub network: Option<Network>,
    /// Discovery's `cli` section (id, data).
    pub agents: &'a [(String, Value)],
    /// Discovery's `local` section.
    pub servers: &'a [(String, Value)],
    /// Brains connected already.
    pub connected: usize,
}

/// The recommended defaults, each with its reason.
pub fn recommend(i: &Inputs<'_>) -> SetupAdvice {
    let network = i.network.unwrap_or(Network {
        online: true,
        metered: false,
    });
    let m = i.machine;
    let vram = m.gpus.iter().map(|g| g.vram_mb).max().unwrap_or(0);
    let gpu = m
        .gpus
        .iter()
        .max_by_key(|g| g.vram_mb)
        .map(|g| g.name.clone());
    let strong = m.ram_mb >= LOCAL_RAM_MB && vram >= LOCAL_VRAM_MB;
    let mut reasons = Vec::new();

    // A brain: none if one is connected; offline, only a local server can answer; a strong PC
    // prefers its own server; else an agent already signed in; else OpenRouter's free models.
    let server = i.servers.first();
    let agent = i
        .agents
        .iter()
        .find(|(_, d)| d["signedIn"] != Value::Bool(false) && d["needsAdapter"] != true);
    let name = |(id, d): &(String, Value)| d["name"].as_str().unwrap_or(id).to_owned();
    let (brain, brain_url) = if i.connected > 0 {
        reasons.push(text::t("setup.reason.brainConnected"));
        (None, None)
    } else if let Some(s) = server.filter(|_| !network.online || strong) {
        let key = if network.online {
            "setup.reason.localStrong"
        } else {
            "setup.reason.localOffline"
        };
        reasons.push(text::tf(key, &[("name", &name(s))]));
        (Some(s.0.clone()), s.1["url"].as_str().map(str::to_owned))
    } else if let Some(a) = agent.filter(|_| network.online) {
        reasons.push(text::tf("setup.reason.agent", &[("name", &name(a))]));
        (Some(a.0.clone()), None)
    } else if let Some(s) = server {
        reasons.push(text::tf("setup.reason.localStrong", &[("name", &name(s))]));
        (Some(s.0.clone()), s.1["url"].as_str().map(str::to_owned))
    } else if network.online {
        reasons.push(text::t("setup.reason.openRouter"));
        (Some("openrouter".to_owned()), None)
    } else {
        reasons.push(text::t("setup.reason.noBrain"));
        (None, None)
    };

    // Offline, cloud brains can't answer anyway: keep everything on the PC until the user says.
    let privacy = if network.online { "cloud" } else { "local" };
    if !network.online {
        reasons.push(text::t("setup.reason.offline"));
    }
    let performance = if m.ram_mb < LOW_RAM_MB {
        reasons.push(text::t("setup.reason.lowMemory"));
        "battery"
    } else {
        "auto"
    };
    let download_now = network.online && !network.metered;
    if network.metered {
        reasons.push(text::t("setup.reason.metered"));
    }

    SetupAdvice {
        online: network.online,
        metered: network.metered,
        ram_mb: m.ram_mb,
        gpu,
        on_battery: m.on_battery,
        // The default (SECURITY §1.1): routine actions run, high-risk ones ask.
        mode: "auto".to_owned(),
        performance: performance.to_owned(),
        privacy: privacy.to_owned(),
        brain,
        brain_url,
        download_now,
        reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivo_platform::GpuInfo;
    use serde_json::json;

    fn machine(ram_gb: u64, vram_gb: u64) -> SystemSnapshot {
        SystemSnapshot {
            cpu_name: "CPU".into(),
            logical_cpus: 8,
            ram_mb: ram_gb * 1024,
            ram_free_mb: 1024,
            cpu_load_percent: 5,
            gpus: if vram_gb > 0 {
                vec![GpuInfo {
                    name: "GPU".into(),
                    vram_mb: vram_gb * 1024,
                }]
            } else {
                Vec::new()
            },
            on_battery: false,
            battery_percent: None,
            fullscreen_app: false,
            focus_mode: false,
        }
    }

    fn online(metered: bool) -> Option<Network> {
        Some(Network {
            online: true,
            metered,
        })
    }

    #[test]
    fn a_strong_pc_with_a_local_server_uses_it() {
        let m = machine(32, 12);
        let servers = [(
            "ollama".to_owned(),
            json!({ "name": "Ollama", "url": "http://127.0.0.1:11434/v1" }),
        )];
        let agents = [(
            "gemini-cli".to_owned(),
            json!({ "name": "Gemini CLI", "signedIn": true }),
        )];
        let a = recommend(&Inputs {
            machine: &m,
            network: online(false),
            agents: &agents,
            servers: &servers,
            connected: 0,
        });
        assert_eq!(a.brain.as_deref(), Some("ollama"));
        assert_eq!(a.brain_url.as_deref(), Some("http://127.0.0.1:11434/v1"));
        assert_eq!(a.gpu.as_deref(), Some("GPU"));
        assert_eq!(
            (a.mode.as_str(), a.privacy.as_str(), a.performance.as_str()),
            ("auto", "cloud", "auto")
        );
        assert!(a.download_now);
    }

    #[test]
    fn a_modest_pc_prefers_a_signed_in_agent_then_openrouter() {
        let m = machine(8, 0);
        let servers = [(
            "ollama".to_owned(),
            json!({ "url": "http://127.0.0.1:11434/v1" }),
        )];
        let agents = [
            ("claude-code".to_owned(), json!({ "signedIn": false })),
            (
                "gemini-cli".to_owned(),
                json!({ "name": "Gemini CLI", "signedIn": true }),
            ),
        ];
        let with = |agents: &[(String, Value)]| {
            recommend(&Inputs {
                machine: &m,
                network: online(false),
                agents,
                servers: &servers,
                connected: 0,
            })
        };
        assert_eq!(with(&agents).brain.as_deref(), Some("gemini-cli"));
        // No agent: the server it has is still better than nothing.
        assert_eq!(with(&[]).brain.as_deref(), Some("ollama"));
        let bare = recommend(&Inputs {
            machine: &m,
            network: online(false),
            agents: &[],
            servers: &[],
            connected: 0,
        });
        assert_eq!(bare.brain.as_deref(), Some("openrouter"));
    }

    #[test]
    fn offline_or_metered_or_small_changes_the_defaults() {
        let m = machine(4, 0);
        let offline = recommend(&Inputs {
            machine: &m,
            network: Some(Network {
                online: false,
                metered: false,
            }),
            agents: &[("gemini-cli".to_owned(), json!({ "signedIn": true }))],
            servers: &[],
            connected: 0,
        });
        assert_eq!(offline.brain, None, "nothing online can answer");
        assert_eq!(offline.privacy, "local");
        assert_eq!(offline.performance, "battery");
        assert!(!offline.download_now);
        let metered = recommend(&Inputs {
            machine: &m,
            network: online(true),
            agents: &[],
            servers: &[],
            connected: 1,
        });
        assert_eq!(metered.brain, None, "one is connected already");
        assert!(!metered.download_now);
        assert_eq!(metered.reasons.len(), 3, "{:?}", metered.reasons);
    }
}
