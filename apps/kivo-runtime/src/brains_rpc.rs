//! The Control Center's brain requests (UX-21/22, BRAINS §4–5 and §9, CONVERSATION §0–1): the
//! Brains page (catalog, connected brains, what was found on this PC, sign-in, write-only keys,
//! profiles), Usage, Chat and stated preferences. Like every request they carry no authority
//! beyond a click in the UI, and no key ever travels back to it (SEC-19).

use crate::activity::Recorder;
use crate::brains::{Brains, parse_handle};
use crate::core::Core;
use crate::discovery::{self, Discovery};
use crate::engine::Engine;
use crate::signin::{self, OpenRouterOAuth};
use kivo_brain::catalog::{self, CATALOG, CLI_TOOLS};
use kivo_brain::cost::{Limit, Price, TaskCaps};
use kivo_brain::routing::Profile;
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
        }
    }

    /// Uses other OpenRouter endpoints (tests).
    #[must_use]
    pub fn with_oauth(mut self, oauth: OpenRouterOAuth) -> Self {
        self.oauth = oauth;
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
    fn list(self: &Arc<Self>) -> Value {
        let config = self.core.config();
        let me = Arc::clone(self);
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

    fn usage(&self, days: u32) -> Value {
        let since = now_ms() - i64::from(days) * 24 * 3_600 * 1_000;
        let rows = lock(&self.brains().database())
            .usage_since(since)
            .unwrap_or_default();
        let mut by_day: BTreeMap<i64, f64> = BTreeMap::new();
        let mut by_provider: BTreeMap<String, (f64, u64)> = BTreeMap::new();
        let mut by_feature: BTreeMap<String, f64> = BTreeMap::new();
        let mut by_turn: BTreeMap<String, f64> = BTreeMap::new();
        let offset = i64::from(self.brains().utc_offset()) * 60_000;
        let mut unknown = 0u32;
        for u in &rows {
            let cost = u.cost.unwrap_or(0.0);
            if u.cost.is_none() {
                unknown += 1;
            }
            let day = (u.ts + offset).div_euclid(86_400_000) * 86_400_000 - offset;
            *by_day.entry(day).or_default() += cost;
            let p = by_provider.entry(u.provider.clone()).or_default();
            p.0 += cost;
            p.1 += u.input_tokens + u.output_tokens;
            *by_feature.entry(u.kind.clone()).or_default() += cost;
            if let Some(turn) = &u.turn_id {
                *by_turn.entry(turn.clone()).or_default() += cost;
            }
        }
        let mut top: Vec<(String, f64)> = by_turn.into_iter().collect();
        top.sort_by(|a, b| b.1.total_cmp(&a.1));
        top.truncate(10);
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
            "byDay": by_day.into_iter().map(|(d, c)| json!({"day": d, "cost": c})).collect::<Vec<_>>(),
            "byProvider": by_provider.into_iter().map(|(p, (c, t))| json!({"provider": p, "cost": c, "tokens": t})).collect::<Vec<_>>(),
            "byFeature": by_feature.into_iter().map(|(f, c)| json!({"feature": f, "cost": c})).collect::<Vec<_>>(),
            "topTurns": top.into_iter().map(|(t, c)| json!({"turn": t, "cost": c})).collect::<Vec<_>>(),
            "limits": limits,
            "caps": brains.task_caps(),
            "overrides": brains.price_overrides(),
            "refreshPrices": self.core.config().brains.refresh_prices,
        })
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
                    self.connect(BrainConnection {
                        id: p.id,
                        name: p.name,
                        base_url: p.base_url,
                        local: p.local,
                        ..BrainConnection::default()
                    })
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
            method::USAGE_EXPORT => {
                let days = params["days"]
                    .as_u64()
                    .map_or(90, |d| u32::try_from(d).unwrap_or(90));
                Ok(json!({ "csv": self.usage_csv(days) }))
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

/// Does a connection's key handle point into KIVO's store (never a raw key)?
pub fn is_handle(text: &str) -> bool {
    parse_handle(text).is_some()
}
