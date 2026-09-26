//! The Control Center's brain requests (UX-21/22, BRAINS §4–5 and §9, CONVERSATION §0–1): the
//! Brains page (catalog, connected brains, what was found on this PC, sign-in, write-only keys,
//! profiles), Usage, Chat and stated preferences. Like every request they carry no authority
//! beyond a click in the UI, and no key ever travels back to it (SEC-19).

use crate::activity::Recorder;
use crate::brains::Brains;
use crate::core::Core;
use crate::discovery::{self, Discovery};
use crate::engine::Engine;
use crate::signin::{self, OpenRouterOAuth};
use kivo_brain::catalog::{self, CATALOG, CLI_TOOLS};
use kivo_brain::cost::{Limit, Price, TaskCaps};
use kivo_brain::routing::{ModelRef, Profile};
use kivo_brain::{Health, ProviderKind};
use kivo_core::config::BrainConnection;
use kivo_core::event::{EventKind, ProviderEvent};
use kivo_core::{Capability, Event};
use kivo_ipc::{RpcError, method};
use kivo_platform::SystemControl;
use kivo_store::brains::now_ms;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Opens a command in a visible terminal (a CLI's own sign-in).
pub type Terminal = Arc<dyn Fn(&Path, &[String]) -> Result<(), String> + Send + Sync>;

pub struct BrainsRpc {
    core: Arc<Core>,
    engine: Arc<Engine>,
    discovery: Arc<Discovery>,
    recorder: Recorder,
    control: Arc<dyn SystemControl>,
    terminal: Terminal,
    oauth: OpenRouterOAuth,
    /// Where "Export CSV" saves (the Downloads folder).
    exports: Option<std::path::PathBuf>,
}

/// A thread as Markdown: its title, the summary of what was compacted, then each message with who
/// said it and when (CONV-03 export).
fn thread_markdown(
    thread: &kivo_store::brains::Conversation,
    messages: &[kivo_store::brains::StoredMessage],
    utc_offset: i32,
) -> String {
    let title = if thread.title.trim().is_empty() {
        "Conversation"
    } else {
        thread.title.trim()
    };
    let mut out = format!(
        "# {title}

_{} · {} messages_
",
        kivo_memory::date::format(thread.created_at, utc_offset),
        messages.len()
    );
    if !thread.summary.trim().is_empty() {
        out.push_str(&format!(
            "
> Earlier: {}
",
            thread.summary.trim()
        ));
    }
    for m in messages {
        let who = match m.role.as_str() {
            "user" => "You".to_owned(),
            "assistant" => m
                .brain
                .as_deref()
                .map_or_else(|| "KIVO".to_owned(), |b| format!("KIVO ({b})")),
            other => other.to_owned(),
        };
        out.push_str(&format!(
            "
**{who}** · {}

{}
",
            kivo_memory::date::format(m.ts, utc_offset),
            m.text.trim()
        ));
    }
    out
}

/// `2026-09-23` for a day number (days since 1970-01-01).
fn chrono_date(day: i64) -> String {
    kivo_memory::date::format(day * 86_400_000, 0)
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RpcError> {
    serde_json::from_value(params).map_err(RpcError::invalid_params)
}

fn ok<T: serde::Serialize>(value: &T) -> Result<Value, RpcError> {
    serde_json::to_value(value).map_err(|e| RpcError::new(RpcError::INTERNAL, e.to_string()))
}

fn refuse(message: impl Into<String>) -> RpcError {
    RpcError::new(RpcError::REFUSED, message)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Id {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SectionParam {
    section: String,
}

impl BrainsRpc {
    pub fn new(
        core: Arc<Core>,
        engine: Arc<Engine>,
        discovery: Arc<Discovery>,
        recorder: Recorder,
        control: Arc<dyn SystemControl>,
        terminal: Terminal,
    ) -> Self {
        Self {
            core,
            engine,
            discovery,
            recorder,
            control,
            terminal,
            oauth: OpenRouterOAuth::default(),
            exports: None,
        }
    }

    /// Where the Usage page's CSV export is saved.
    #[must_use]
    pub fn with_exports(mut self, dir: std::path::PathBuf) -> Self {
        self.exports = Some(dir);
        self
    }

    fn brains(&self) -> &Arc<Brains> {
        &self.engine.brains
    }

    fn publish(&self, event: ProviderEvent) {
        self.core
            .bus
            .publish(Event::new(EventKind::Provider(event)));
    }

    /// What KIVO can connect, no-key options first, free ones labelled (BRAIN-17, CONV-08).
    fn catalog(&self) -> Value {
        let mut entries: Vec<&catalog::CatalogEntry> = CATALOG.iter().collect();
        entries.sort_by_key(|e| e.sign_in as u8);
        json!({
            "brains": entries,
            "cli": CLI_TOOLS,
        })
    }

    /// The Brains page: connected brains, profiles and the default (UX-22). Stale health is
    /// checked in the background and pushed (DISC-14); stale discovery likewise (DISC-15).
    /// The brain KIVO uses: the default profile's choice, one of the connected brains (a model
    /// it offers, when it lists them), at a level the model takes; `None` is Automatic.
    fn set_active(
        self: &Arc<Self>,
        provider: Option<String>,
        model: String,
        reasoning: Option<kivo_brain::reasoning::Effort>,
    ) -> Result<Value, RpcError> {
        let config = self.core.config();
        let chosen = match provider {
            None => None,
            Some(provider) => {
                if !config.brains.connections.iter().any(|c| c.id == provider) {
                    return Err(refuse(kivo_core::text::t("brain.chooseConnected")));
                }
                let levels = self.brains().reasoning_levels(&provider, &model);
                Some(ModelRef {
                    reasoning: reasoning.filter(|r| levels.contains(r)),
                    provider,
                    model,
                })
            }
        };
        let Some(mut profile) = self
            .brains()
            .profiles()
            .into_iter()
            .find(|p| p.id == config.brains.default_profile)
        else {
            return Err(refuse("no such profile"));
        };
        profile.primary = chosen;
        self.brains().save_profile(profile);
        ok(&self.list())
    }

    fn list(self: &Arc<Self>) -> Value {
        let config = self.core.config();
        let me = Arc::clone(self);
        // Connected CLI agents KIVO hasn't asked yet: what they offer.
        for c in &config.brains.connections {
            if catalog::entry(&c.id).is_some_and(|e| e.kind == ProviderKind::Cli)
                && config.capabilities.enabled(Capability::CliAgents)
            {
                self.probe_agent(&c.id);
            }
        }
        tokio::spawn(async move {
            me.brains().refresh_health(crate::brains::HEALTH_TTL).await;
            for view in me.brains().views(&me.core.config()) {
                me.publish(ProviderEvent::HealthChanged {
                    provider: view.id,
                    healthy: view.health.is_ready(),
                });
            }
            for section in [discovery::CLI, discovery::LOCAL] {
                if me
                    .discovery
                    .refresh_if_older(section, discovery::STALE_AFTER)
                    .await
                {
                    me.publish(ProviderEvent::DiscoveryChanged {
                        section: section.to_owned(),
                    });
                }
            }
        });
        json!({
            "connected": self.brains().views(&config),
            "profiles": self.brains().profiles(),
            "defaultProfile": config.brains.default_profile,
            // The brain KIVO uses: the default profile's choice (null: Automatic).
            "active": self
                .brains()
                .profiles()
                .into_iter()
                .find(|p| p.id == config.brains.default_profile)
                .and_then(|p| p.primary),
            "persona": config.brains.persona,
            "customPersona": config.brains.custom_persona,
            "cliAgentsOn": config.capabilities.enabled(Capability::CliAgents),
            "cloudOn": Brains::cloud_allowed(&config),
            "workspace": self.engine.agents.workspace(),
        })
    }

    /// Adds (or turns back on) a connection. Connecting a CLI agent is the user's consent to CLI
    /// agents, so that capability goes on too (audited, CAP-03).
    fn connect(&self, connection: BrainConnection) -> Result<Value, RpcError> {
        if connection.id.is_empty() {
            return Err(refuse("a brain needs an id"));
        }
        let entry = catalog::entry(&connection.id);
        if entry.is_none() && connection.base_url.is_empty() {
            return Err(refuse("a custom brain needs its address"));
        }
        let cli = entry.is_some_and(|e| e.kind == ProviderKind::Cli);
        let mut turned_on = false;
        let saved = self.core.update_config(|c| {
            let mut connection = connection.clone();
            connection.enabled = true;
            // A key handle can only be one KIVO stored for this connection.
            let existing_key = c
                .brains
                .connections
                .iter()
                .find(|x| x.id == connection.id)
                .map(|x| x.key.clone());
            connection.key = existing_key.unwrap_or_default();
            c.brains.connections.retain(|x| x.id != connection.id);
            c.brains.connections.push(connection);
            if cli {
                turned_on = c.capabilities.set(Capability::CliAgents, true);
            }
        });
        if turned_on {
            self.recorder
                .capability_changed(Capability::CliAgents, true);
        }
        self.engine.settings_changed(&saved);
        Ok(json!({ "connected": self.brains().views(&saved) }))
    }

    /// Learns what connected CLI agent `id` offers (its models and reasoning setting) by
    /// starting its session in the default folder, once: no prompt is sent, so it costs no
    /// quota. Not for agents that aren't here or need a sign-in first.
    fn probe_agent(self: &Arc<Self>, id: &str) {
        if self.brains().knows_agent(id) {
            return;
        }
        let Some(found) = self.brains().agent(id) else {
            return;
        };
        if found.signed_in == Some(false) {
            return;
        }
        let me = Arc::clone(self);
        let id = id.to_owned();
        tokio::spawn(async move {
            let cwd = me.engine.agents.workspace();
            match me
                .engine
                .agents
                .session(&id, Some(found.program.as_path()), &cwd)
                .await
            {
                Ok(session) => {
                    me.brains().note_agent(&id, session.options());
                    me.publish(ProviderEvent::HealthChanged {
                        provider: id,
                        healthy: true,
                    });
                }
                Err(e) => tracing::info!(%e, agent = id, "couldn't ask the agent what it offers"),
            }
        });
    }

    async fn disconnect(&self, id: &str) -> Value {
        let saved = self.core.update_config(|c| {
            c.brains.connections.retain(|x| x.id != id);
        });
        self.brains().delete_key(id);
        let cwd = self.engine.agents.workspace();
        self.engine.agents.forget(id, &cwd).await;
        self.engine.settings_changed(&saved);
        json!({ "connected": self.brains().views(&saved) })
    }

    /// Stores a key after the runtime has tested it (BRAIN-18). The key is never sent back.
    async fn set_key(&self, id: &str, key: &str) -> Result<Value, RpcError> {
        let base_url = self
            .core
            .config()
            .brains
            .connections
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.base_url.clone())
            .unwrap_or_default();
        let health = self.brains().test_key(id, key, &base_url).await;
        if health == Health::NeedsSignIn {
            return Err(refuse(kivo_core::text::tf(
                "brain.auth",
                &[("name", &catalog::entry(id).map_or(id, |e| e.name))],
            )));
        }
        let handle = self.brains().set_key(id, key).map_err(refuse)?;
        self.store_handle(id, &handle);
        Ok(json!({ "health": health }))
    }

    fn store_handle(&self, id: &str, handle: &str) {
        let saved = self.core.update_config(|c| {
            match c.brains.connections.iter_mut().find(|x| x.id == id) {
                Some(x) => {
                    x.key = handle.to_owned();
                    x.enabled = true;
                }
                None => c.brains.connections.push(BrainConnection {
                    id: id.to_owned(),
                    key: handle.to_owned(),
                    enabled: true,
                    ..BrainConnection::default()
                }),
            }
        });
        self.engine.settings_changed(&saved);
    }

    /// Sign-in without a key (BRAIN-17): OpenRouter OAuth, or a CLI's own login in a terminal.
    async fn sign_in(&self, id: &str) -> Result<Value, RpcError> {
        if id == "openrouter" {
            let handle = signin::connect_openrouter(
                self.brains(),
                &self.control,
                &self.oauth,
                &CancellationToken::new(),
            )
            .await
            .map_err(refuse)?;
            self.store_handle(id, &handle);
            let health = self.brains().check(id).await;
            return Ok(json!({ "health": health }));
        }
        // The CLI itself, not its ACP adapter, runs the login.
        let cli = self
            .discovery
            .section(discovery::CLI)
            .items
            .into_iter()
            .find(|i| i.id == id)
            .and_then(|i| i.data["cli"].as_str().map(PathBuf::from));
        signin::cli_login(id, cli.as_deref(), &*self.terminal).map_err(refuse)?;
        Ok(json!({ "started": true }))
    }

    /// Today's date, `2026-09-23`, in the user's time zone.
    fn today(&self) -> String {
        let offset = i64::from(self.brains().utc_offset()) * 60_000;
        chrono_date((now_ms() + offset).div_euclid(86_400_000))
    }

    /// Writes `content` to the Downloads folder as `<stem>.<ext>`, numbered if taken; returns the
    /// file (UX-30, CONV-03).
    fn save_download(&self, stem: &str, ext: &str, content: &str) -> Result<String, String> {
        let dir = self
            .exports
            .as_ref()
            .ok_or_else(|| "there's no Downloads folder".to_owned())?;
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let file = (1..)
            .map(|n: u32| {
                dir.join(if n < 2 {
                    format!("{stem}.{ext}")
                } else {
                    format!("{stem} ({n}).{ext}")
                })
            })
            .find(|f| !f.exists())
            .unwrap_or_else(|| dir.join(format!("{stem}.{ext}")));
        std::fs::write(&file, content).map_err(|e| e.to_string())?;
        Ok(file.display().to_string())
    }

    /// A thread as Markdown (headings, who said what, the running summary) or JSON; with `save`,
    /// written to Downloads (CONV-03).
    fn export_thread(&self, id: &str, as_json: bool, save: bool) -> Result<Value, RpcError> {
        let db = self.brains().database();
        let (thread, messages) = {
            let db = lock(&db);
            match db.conversation(id) {
                Ok(Some(c)) => (c, db.messages(id).unwrap_or_default()),
                _ => return Err(refuse("no such conversation")),
            }
        };
        let offset = self.brains().utc_offset();
        let text = if as_json {
            serde_json::to_string_pretty(&json!({ "thread": thread, "messages": messages }))
                .unwrap_or_default()
        } else {
            thread_markdown(&thread, &messages, offset)
        };
        if !save {
            return Ok(json!({ "text": text }));
        }
        let title: String = thread
            .title
            .chars()
            .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
            .take(60)
            .collect();
        let stem = if title.trim().is_empty() {
            format!("KIVO conversation {}", self.today())
        } else {
            format!("KIVO - {}", title.trim())
        };
        self.save_download(&stem, if as_json { "json" } else { "md" }, &text)
            .map(|file| json!({ "text": text, "file": file }))
            .map_err(refuse)
    }

    fn usage(&self, days: u32) -> Value {
        let since = now_ms() - i64::from(days) * 24 * 3_600 * 1_000;
        let rows = lock(&self.brains().database())
            .usage_since(since)
            .unwrap_or_default();
        // Per day: AI (brains, agents, computer use) and speech (cloud STT/TTS, realtime).
        let mut by_day: BTreeMap<i64, (f64, f64)> = BTreeMap::new();
        let mut by_provider: BTreeMap<String, (f64, u64)> = BTreeMap::new();
        let mut by_feature: BTreeMap<String, f64> = BTreeMap::new();
        let mut by_routine: BTreeMap<String, f64> = BTreeMap::new();
        let mut by_turn: BTreeMap<String, f64> = BTreeMap::new();
        let mut by_task: BTreeMap<String, f64> = BTreeMap::new();
        let offset = i64::from(self.brains().utc_offset()) * 60_000;
        let today_start = (now_ms() + offset).div_euclid(86_400_000) * 86_400_000 - offset;
        let mut today = 0.0;
        let mut unknown = 0u32;
        let mut paid_turns: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for u in &rows {
            let cost = u.cost.unwrap_or(0.0);
            if u.cost.is_none() {
                unknown += 1;
            }
            if u.ts >= today_start {
                today += cost;
            }
            let day = (u.ts + offset).div_euclid(86_400_000) * 86_400_000 - offset;
            let d = by_day.entry(day).or_default();
            if matches!(u.kind.as_str(), "stt" | "tts" | "realtime") {
                d.1 += cost;
            } else {
                d.0 += cost;
            }
            let p = by_provider.entry(u.provider.clone()).or_default();
            p.0 += cost;
            p.1 += u.input_tokens + u.output_tokens;
            *by_feature.entry(u.kind.clone()).or_default() += cost;
            if let Some(routine) = &u.routine_id {
                *by_routine.entry(routine.clone()).or_default() += cost;
            }
            if let Some(task) = &u.task_id {
                *by_task.entry(task.clone()).or_default() += cost;
            } else if let Some(turn) = &u.turn_id {
                *by_turn.entry(turn.clone()).or_default() += cost;
            }
            if cost > 0.0
                && let Some(turn) = &u.turn_id
            {
                paid_turns.insert(turn.clone());
            }
        }
        // The most expensive tasks and requests, named (a task by its title, a request by what
        // was said).
        let db = self.brains().database();
        let mut top: Vec<Value> = {
            let db = lock(&db);
            by_task
                .into_iter()
                .map(|(id, cost)| {
                    let title = db
                        .task(&id)
                        .ok()
                        .flatten()
                        .map(|t| t.title)
                        .unwrap_or_default();
                    json!({ "kind": "task", "id": id, "title": title, "cost": cost })
                })
                .chain(by_turn.into_iter().map(|(id, cost)| {
                    let title = db
                        .turn(&id)
                        .ok()
                        .flatten()
                        .and_then(|t| t.transcript)
                        .unwrap_or_default();
                    json!({ "kind": "turn", "id": id, "title": title, "cost": cost })
                }))
                .collect()
        };
        top.sort_by(|a, b| {
            b["cost"]
                .as_f64()
                .unwrap_or(0.0)
                .total_cmp(&a["cost"].as_f64().unwrap_or(0.0))
        });
        top.truncate(10);
        let routines: Vec<Value> = {
            let db = lock(&db);
            by_routine
                .into_iter()
                .map(|(id, cost)| {
                    let name = db
                        .routine(&id)
                        .ok()
                        .flatten()
                        .map(|r| r.name)
                        .unwrap_or_default();
                    json!({ "routine": id, "name": name, "cost": cost })
                })
                .collect()
        };
        let turns = lock(&db).turns_since(since).unwrap_or(0);
        let paid = u64::try_from(paid_turns.len()).unwrap_or(0);
        let brains = self.brains();
        let limits: Vec<Value> = brains
            .limits()
            .into_iter()
            .map(|l| {
                let (provider, profile, feature) = match &l.scope {
                    kivo_brain::cost::Scope::Provider(p) => {
                        (p.clone(), String::new(), String::new())
                    }
                    kivo_brain::cost::Scope::Profile(p) => {
                        (String::new(), p.clone(), String::new())
                    }
                    kivo_brain::cost::Scope::Feature(f) => {
                        (String::new(), String::new(), f.clone())
                    }
                    kivo_brain::cost::Scope::Overall => Default::default(),
                };
                let state = brains
                    .limit_states(&provider, &profile, &feature)
                    .into_iter()
                    .find(|(x, _)| *x == l)
                    .map(|(_, s)| s);
                json!({ "limit": l, "state": state })
            })
            .collect();
        json!({
            "total": rows.iter().filter_map(|u| u.cost).sum::<f64>(),
            "requests": rows.len(),
            "unpriced": unknown,
            "today": today,
            "turns": turns,
            // Requests that cost nothing: the fast path, local brains and local speech.
            "freeTurns": turns.saturating_sub(paid),
            "byDay": by_day.into_iter().map(|(d, (ai, speech))| json!({"day": d, "cost": ai + speech, "ai": ai, "speech": speech})).collect::<Vec<_>>(),
            "byRoutine": routines,
            "byProvider": by_provider.into_iter().map(|(p, (c, t))| json!({"provider": p, "cost": c, "tokens": t})).collect::<Vec<_>>(),
            "byFeature": by_feature.into_iter().map(|(f, c)| json!({"feature": f, "cost": c})).collect::<Vec<_>>(),
            "top": top,
            "limits": limits,
            "caps": brains.task_caps(),
            "overrides": brains.price_overrides(),
            "refreshPrices": self.core.config().brains.refresh_prices,
        })
    }

    /// The whole Activity timeline, newest first, as CSV (UX-20's Export).
    fn activity_csv(&self) -> String {
        let cell = |s: &str| {
            if s.contains([',', '"', '\n']) {
                format!("\"{}\"", s.replace('"', "\"\""))
            } else {
                s.to_owned()
            }
        };
        let mut out = String::from("time,kind,title,detail,status,turn\n");
        let mut before = None;
        // At most 20 000 rows: the timeline is kept for a while, not for ever.
        for _ in 0..100 {
            let page = self.recorder.recent(before, 200);
            let Some(last) = page.last() else {
                break;
            };
            before = Some(last.id);
            for a in &page {
                out.push_str(&format!(
                    "{},{},{},{},{},{}\n",
                    a.ts,
                    cell(&a.kind),
                    cell(&a.title),
                    cell(a.detail.as_deref().unwrap_or_default()),
                    cell(&a.status),
                    cell(a.turn_id.as_deref().unwrap_or_default()),
                ));
            }
        }
        out
    }

    fn usage_csv(&self, days: u32) -> String {
        let since = now_ms() - i64::from(days) * 24 * 3_600 * 1_000;
        let rows = lock(&self.brains().database())
            .usage_since(since)
            .unwrap_or_default();
        let mut out = String::from(
            "time,kind,provider,model,profile,input_tokens,output_tokens,cached_tokens,cost_estimate,turn,task,routine\n",
        );
        for u in rows {
            let cell = |s: &str| {
                if s.contains([',', '"', '\n']) {
                    format!("\"{}\"", s.replace('"', "\"\""))
                } else {
                    s.to_owned()
                }
            };
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{},{}\n",
                u.ts,
                cell(&u.kind),
                cell(&u.provider),
                cell(&u.model),
                cell(u.brain_profile.as_deref().unwrap_or_default()),
                u.input_tokens,
                u.output_tokens,
                u.cached_tokens,
                u.cost.map(|c| format!("{c:.6}")).unwrap_or_default(),
                cell(u.turn_id.as_deref().unwrap_or_default()),
                cell(u.task_id.as_deref().unwrap_or_default()),
                cell(u.routine_id.as_deref().unwrap_or_default()),
            ));
        }
        out
    }

    /// Answers the requests this module owns; `None` for the rest.
    #[allow(clippy::too_many_lines, reason = "one table of the brain requests")]
    pub async fn call(
        self: &Arc<Self>,
        name: &str,
        params: Value,
    ) -> Option<Result<Value, RpcError>> {
        let db = self.brains().database();
        let result = match name {
            method::BRAINS_CATALOG => Ok(self.catalog()),
            method::BRAINS_LIST => Ok(self.list()),
            method::BRAINS_CHECK => match parse::<Id>(params) {
                Ok(Id { id }) => {
                    let health = self.brains().check(&id).await;
                    self.publish(ProviderEvent::HealthChanged {
                        provider: id,
                        healthy: health.is_ready(),
                    });
                    Ok(json!({ "connected": self.brains().views(&self.core.config()) }))
                }
                Err(e) => Err(e),
            },
            method::BRAINS_CONNECT => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase", deny_unknown_fields)]
                struct P {
                    id: String,
                    #[serde(default)]
                    name: String,
                    #[serde(default)]
                    base_url: String,
                    #[serde(default)]
                    local: bool,
                }
                parse::<P>(params).and_then(|p| {
                    let id = p.id.clone();
                    let connected = self.connect(BrainConnection {
                        id: p.id,
                        name: p.name,
                        base_url: p.base_url,
                        local: p.local,
                        ..BrainConnection::default()
                    });
                    // A CLI agent: learn what it offers (its models, its reasoning setting).
                    if connected.is_ok() {
                        self.probe_agent(&id);
                    }
                    connected
                })
            }
            method::BRAINS_DISCONNECT => match parse::<Id>(params) {
                Ok(Id { id }) => Ok(self.disconnect(&id).await),
                Err(e) => Err(e),
            },
            method::BRAINS_SET_KEY => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    id: String,
                    key: String,
                }
                match parse::<P>(params) {
                    Ok(P { id, key }) => self.set_key(&id, &key).await,
                    Err(e) => Err(e),
                }
            }
            method::BRAINS_TEST_KEY => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase", deny_unknown_fields)]
                struct P {
                    id: String,
                    key: String,
                    #[serde(default)]
                    base_url: String,
                }
                match parse::<P>(params) {
                    Ok(p) => Ok(json!({
                        "health": self.brains().test_key(&p.id, &p.key, &p.base_url).await
                    })),
                    Err(e) => Err(e),
                }
            }
            method::BRAINS_SIGN_IN => match parse::<Id>(params) {
                Ok(Id { id }) => self.sign_in(&id).await,
                Err(e) => Err(e),
            },
            method::BRAINS_SET_DEFAULT => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase", deny_unknown_fields)]
                struct P {
                    #[serde(default)]
                    profile: Option<String>,
                    #[serde(default)]
                    persona: Option<String>,
                    #[serde(default)]
                    custom_persona: Option<String>,
                }
                match parse::<P>(params) {
                    Ok(p) => {
                        if let Some(profile) = &p.profile
                            && !self.brains().profiles().iter().any(|x| &x.id == profile)
                        {
                            return Some(Err(refuse("no such profile")));
                        }
                        let saved = self.core.update_config(|c| {
                            if let Some(v) = p.profile {
                                c.brains.default_profile = v;
                            }
                            if let Some(v) = p.persona {
                                c.brains.persona = v;
                            }
                            if let Some(v) = p.custom_persona {
                                c.brains.custom_persona = v.chars().take(600).collect();
                            }
                        });
                        ok(&saved.brains)
                    }
                    Err(e) => Err(e),
                }
            }
            method::BRAINS_SET_ACTIVE => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase", deny_unknown_fields)]
                struct P {
                    provider: Option<String>,
                    #[serde(default)]
                    model: String,
                    #[serde(default)]
                    reasoning: Option<kivo_brain::reasoning::Effort>,
                }
                match parse::<P>(params) {
                    Ok(p) => self.set_active(p.provider, p.model, p.reasoning),
                    Err(e) => Err(e),
                }
            }
            method::BRAINS_REASONING => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    provider: String,
                    model: String,
                }
                match parse::<P>(params) {
                    Ok(p) => ok(&self.brains().reasoning_levels(&p.provider, &p.model)),
                    Err(e) => Err(e),
                }
            }
            method::BRAINS_SAVE_PROFILE => match parse::<Profile>(params) {
                Ok(profile) if profile.id.trim().is_empty() || profile.name.trim().is_empty() => {
                    Err(refuse("a profile needs an id and a name"))
                }
                Ok(profile) => {
                    self.brains().save_profile(profile);
                    ok(&self.brains().profiles())
                }
                Err(e) => Err(e),
            },
            method::BRAINS_DELETE_PROFILE => match parse::<Id>(params) {
                Ok(Id { id }) => {
                    self.brains().delete_profile(&id);
                    ok(&self.brains().profiles())
                }
                Err(e) => Err(e),
            },
            method::BRAINS_DISCOVERY => match parse::<SectionParam>(params) {
                Ok(p) => ok(&self.discovery.section(&p.section)),
                Err(e) => Err(e),
            },
            method::BRAINS_REFRESH => match parse::<SectionParam>(params) {
                Ok(p) => {
                    self.discovery.refresh(&p.section).await;
                    ok(&self.discovery.section(&p.section))
                }
                Err(e) => Err(e),
            },
            method::BRAINS_VIEWED => match parse::<SectionParam>(params) {
                Ok(p) => {
                    self.discovery.viewed(&p.section);
                    Ok(Value::Null)
                }
                Err(e) => Err(e),
            },
            method::BRAINS_SET_WORKSPACE => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    folder: PathBuf,
                }
                match parse::<P>(params) {
                    Ok(P { folder }) if folder.is_dir() => {
                        self.engine.agents.set_workspace(folder);
                        Ok(Value::Null)
                    }
                    Ok(_) => Err(refuse("that folder doesn't exist")),
                    Err(e) => Err(e),
                }
            }
            method::BRAINS_CONTEXT => {
                #[derive(Deserialize, Default)]
                #[serde(deny_unknown_fields)]
                struct P {
                    #[serde(default)]
                    thread: Option<String>,
                }
                let p: P = if params.is_null() {
                    P::default()
                } else {
                    match parse(params) {
                        Ok(p) => p,
                        Err(e) => return Some(Err(e)),
                    }
                };
                ok(&self.engine.context_preview(p.thread.as_deref()))
            }
            method::USAGE_SUMMARY => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    #[serde(default = "thirty")]
                    days: u32,
                }
                fn thirty() -> u32 {
                    30
                }
                let p: P = parse(params).unwrap_or(P { days: 30 });
                Ok(self.usage(p.days.clamp(1, 366)))
            }
            method::ACTIVITY_EXPORT => {
                let csv = self.activity_csv();
                let date = self.today();
                self.save_download(&format!("KIVO activity {date}"), "csv", &csv)
                    .map(|file| json!({ "file": file }))
                    .map_err(refuse)
            }
            method::USAGE_EXPORT => {
                let days = params["days"]
                    .as_u64()
                    .map_or(90, |d| u32::try_from(d).unwrap_or(90));
                let csv = self.usage_csv(days);
                // `save`: into the Downloads folder, and the file is returned (UX-30).
                if params["save"].as_bool().unwrap_or(false) {
                    let date = self.today();
                    self.save_download(&format!("KIVO usage {date}"), "csv", &csv)
                        .map(|file| json!({ "csv": csv, "file": file }))
                        .map_err(refuse)
                } else {
                    Ok(json!({ "csv": csv }))
                }
            }
            method::USAGE_SET_LIMITS => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    limits: Vec<Limit>,
                }
                match parse::<P>(params) {
                    Ok(P { limits }) if limits.iter().all(|l| l.amount >= 0.0) => {
                        self.brains().set_limits(&limits);
                        Ok(self.usage(30))
                    }
                    Ok(_) => Err(refuse("limits can't be negative")),
                    Err(e) => Err(e),
                }
            }
            method::USAGE_SET_CAPS => match parse::<TaskCaps>(params) {
                Ok(caps) => {
                    self.brains().set_task_caps(&caps);
                    Ok(self.usage(30))
                }
                Err(e) => Err(e),
            },
            method::USAGE_SET_PRICE => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    model: String,
                    #[serde(default)]
                    price: Option<Price>,
                }
                match parse::<P>(params) {
                    Ok(P { model, price }) => {
                        self.brains().set_price_override(&model, price);
                        Ok(self.usage(30))
                    }
                    Err(e) => Err(e),
                }
            }
            method::CHAT_THREADS => {
                let limit = params["limit"]
                    .as_u64()
                    .map_or(100, |l| u32::try_from(l).unwrap_or(100));
                ok(&lock(&db).conversations(limit).unwrap_or_default())
            }
            method::CHAT_THREAD => match parse::<Id>(params) {
                Ok(Id { id }) => {
                    let db = lock(&db);
                    match db.conversation(&id) {
                        Ok(Some(c)) => Ok(json!({
                            "thread": c,
                            "messages": db.messages(&id).unwrap_or_default(),
                        })),
                        _ => Err(refuse("no such conversation")),
                    }
                }
                Err(e) => Err(e),
            },
            method::CHAT_NEW => {
                let title = params["title"].as_str().unwrap_or_default().to_owned();
                let id = format!("c-{}", uuid::Uuid::now_v7().simple());
                let created = lock(&db).create_conversation(&id, "chat", &title);
                match created {
                    Ok(c) => {
                        self.publish(ProviderEvent::ThreadChanged { thread: id });
                        ok(&c)
                    }
                    Err(e) => Err(refuse(e.to_string())),
                }
            }
            method::CHAT_UPDATE => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    id: String,
                    #[serde(default)]
                    title: Option<String>,
                    #[serde(default)]
                    pinned: Option<bool>,
                }
                match parse::<P>(params) {
                    Ok(p) => {
                        let r = lock(&db).update_conversation(
                            &p.id,
                            p.title.as_deref(),
                            None,
                            p.pinned,
                        );
                        self.publish(ProviderEvent::ThreadChanged { thread: p.id });
                        r.map(|()| Value::Null).map_err(|e| refuse(e.to_string()))
                    }
                    Err(e) => Err(e),
                }
            }
            method::CHAT_DELETE => match parse::<Id>(params) {
                Ok(Id { id }) => {
                    let r = lock(&db).delete_conversation(&id);
                    self.publish(ProviderEvent::ThreadChanged { thread: id });
                    r.map(|()| Value::Null).map_err(|e| refuse(e.to_string()))
                }
                Err(e) => Err(e),
            },
            method::CHAT_SEND => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Attachment {
                    name: String,
                    text: String,
                }
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    thread: String,
                    text: String,
                    #[serde(default)]
                    profile: Option<String>,
                    #[serde(default)]
                    attachments: Vec<Attachment>,
                }
                match parse::<P>(params) {
                    Ok(p)
                        if p.attachments.iter().map(|a| a.text.len()).sum::<usize>() > 200_000 =>
                    {
                        Err(refuse("attachments are limited to 200 KB of text"))
                    }
                    Ok(p) => self
                        .engine
                        .say_in(
                            &p.text,
                            Some(p.thread),
                            p.profile,
                            p.attachments
                                .into_iter()
                                .map(|a| (a.name, a.text))
                                .collect(),
                        )
                        .await
                        .map(|()| Value::Null)
                        .map_err(refuse),
                    Err(e) => Err(e),
                }
            }
            method::CHAT_COMPACT => match parse::<Id>(params) {
                Ok(Id { id }) => {
                    let r = self.engine.compact(&id, None).await;
                    self.publish(ProviderEvent::ThreadChanged { thread: id });
                    r.map(|()| Value::Null).map_err(refuse)
                }
                Err(e) => Err(e),
            },
            method::CHAT_CONTINUE => match parse::<Id>(params) {
                Ok(Id { id }) if lock(&db).conversation(&id).ok().flatten().is_some() => {
                    self.engine.continue_thread(&id);
                    Ok(Value::Null)
                }
                Ok(_) => Err(refuse("no such conversation")),
                Err(e) => Err(e),
            },
            method::CHAT_BRANCH => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    id: String,
                    /// The last message to keep; all when absent.
                    #[serde(default)]
                    message: Option<i64>,
                }
                match parse::<P>(params) {
                    Ok(p) => {
                        let id = format!("c-{}", uuid::Uuid::now_v7().simple());
                        let db = lock(&db);
                        let title = db
                            .conversation(&p.id)
                            .ok()
                            .flatten()
                            .map(|c| {
                                kivo_core::text::tf("chat.branchTitle", &[("title", &c.title)])
                            })
                            .unwrap_or_default();
                        let made = db.branch_conversation(&p.id, &id, &title, p.message);
                        drop(db);
                        match made {
                            Ok(c) => {
                                self.publish(ProviderEvent::ThreadChanged { thread: id });
                                ok(&c)
                            }
                            Err(e) => Err(refuse(e.to_string())),
                        }
                    }
                    Err(e) => Err(e),
                }
            }
            method::CHAT_EXPORT => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    id: String,
                    #[serde(default)]
                    json: bool,
                    #[serde(default)]
                    save: bool,
                }
                match parse::<P>(params) {
                    Ok(p) => self.export_thread(&p.id, p.json, p.save),
                    Err(e) => Err(e),
                }
            }
            method::CHAT_SEARCH => {
                let query = params["query"].as_str().unwrap_or_default().to_owned();
                ok(&lock(&db).search_messages(&query, 50).unwrap_or_default())
            }
            method::CHAT_MISROUTE => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase", deny_unknown_fields)]
                struct P {
                    turn_id: String,
                    #[serde(default)]
                    note: String,
                }
                match parse::<P>(params) {
                    Ok(p) => Ok(self.engine.misroute(&p.turn_id, &p.note)),
                    Err(e) => Err(e),
                }
            }
            method::VOICE_VOCABULARY => ok(&lock(&db).vocabulary(500).unwrap_or_default()),
            method::VOICE_ADD_WORD | method::VOICE_REMOVE_WORD => {
                let word = params["word"]
                    .as_str()
                    .unwrap_or_default()
                    .trim()
                    .to_owned();
                if word.is_empty() || word.chars().count() > 60 {
                    Err(refuse("a word of 1–60 characters"))
                } else {
                    let db = lock(&db);
                    let r = if name == method::VOICE_ADD_WORD {
                        db.add_vocabulary(&word)
                    } else {
                        db.delete_vocabulary(&word)
                    };
                    r.map_err(|e| refuse(e.to_string()))
                        .and_then(|()| ok(&db.vocabulary(500).unwrap_or_default()))
                }
            }
            method::MEMORY_PREFERENCES => ok(&lock(&db)
                .preferences()
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| json!({ "key": k, "value": v }))
                .collect::<Vec<_>>()),
            method::MEMORY_SET_PREFERENCE => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    key: String,
                    value: String,
                }
                match parse::<P>(params) {
                    Ok(p) if p.key.trim().is_empty() => Err(refuse("a preference needs a name")),
                    Ok(p) => lock(&db)
                        .set_preference(p.key.trim(), p.value.trim(), "settings")
                        .map(|()| Value::Null)
                        .map_err(|e| refuse(e.to_string())),
                    Err(e) => Err(e),
                }
            }
            method::MEMORY_DELETE_PREFERENCE => {
                let key = params["key"].as_str().unwrap_or_default().to_owned();
                lock(&db)
                    .delete_preference(&key)
                    .map(|()| Value::Null)
                    .map_err(|e| refuse(e.to_string()))
            }
            _ => return None,
        };
        Some(result)
    }

    /// Discovery at app start, 30 s in and at low priority (DISC-15), then the weekly price
    /// refresh (BRAIN-35) and a health check of the connected brains.
    pub fn start_background(self: &Arc<Self>, shutdown: CancellationToken) {
        let me = Arc::clone(self);
        tokio::spawn(async move {
            tokio::select! {
                () = tokio::time::sleep(Duration::from_secs(30)) => {}
                () = shutdown.cancelled() => return,
            }
            for section in [discovery::CLI, discovery::LOCAL] {
                if me.discovery.refresh(section).await {
                    me.publish(ProviderEvent::DiscoveryChanged {
                        section: section.to_owned(),
                    });
                }
            }
            if me.core.config().brains.refresh_prices {
                match me.brains().refresh_prices(false).await {
                    Ok(n) if n > 0 => tracing::info!(n, "prices refreshed"),
                    Ok(_) => {}
                    Err(e) => tracing::debug!(%e, "price refresh skipped"),
                }
            }
            me.brains().refresh_health(crate::brains::HEALTH_TTL).await;
        });
    }

    /// Conversations older than the retention setting are deleted at start and daily (MEM-01).
    pub fn start_pruning(self: &Arc<Self>, shutdown: CancellationToken) {
        let me = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                let days = me.core.config().privacy.retention_days;
                let pruned = lock(&me.brains().database()).prune_conversations(days, now_ms());
                match pruned {
                    Ok(n) if n > 0 => tracing::info!(n, days, "old conversations deleted"),
                    Ok(_) => {}
                    Err(e) => tracing::warn!(%e, "couldn't prune conversations"),
                }
                tokio::select! {
                    () = tokio::time::sleep(Duration::from_secs(24 * 3_600)) => {}
                    () = shutdown.cancelled() => return,
                }
            }
        });
    }

    /// `PATH` changed (WM_SETTINGCHANGE): look for CLIs again (DISC-15).
    pub fn environment_changed(self: &Arc<Self>) {
        let me = Arc::clone(self);
        tokio::spawn(async move {
            if me.discovery.refresh(discovery::CLI).await {
                me.publish(ProviderEvent::DiscoveryChanged {
                    section: discovery::CLI.to_owned(),
                });
            }
        });
    }
}

fn lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
