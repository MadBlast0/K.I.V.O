//! The brains hub (BRAINS §3–5, §9; DISCOVERY §3): the brains the user connected, built from their
//! connections with keys read from Credential Manager here in the runtime and nowhere else
//! (SEC-17, SEC-19); their health (DISC-14, BRAIN-23); routing with a reason (BRAIN-21); usage and
//! cost metering (BRAIN-34, BRAIN-35), limits (BRAIN-36) and the weekly price refresh.
//!
//! A provider that fails is only marked unhealthy: nothing here can take the runtime down
//! (invariant 9).

use kivo_brain::anthropic::Anthropic;
use kivo_brain::catalog::{self, CatalogEntry};
use kivo_brain::cost::{self, Limit, LimitState, Price, PriceTable, Scope, Spend, TaskCaps};
use kivo_brain::gemini::Gemini;
use kivo_brain::http::Http;
use kivo_brain::openai::{OpenAi, OpenAiConfig};
use kivo_brain::routing::{self, Available, Profile, Route, RouteError, RouteRequest};
use kivo_brain::{BrainProvider, Health, ModelInfo, NormalizedError, PrivacyClass, ProviderKind};
use kivo_brain::{ProviderInfo, Usage};
use kivo_core::config::{BrainConnection, KivoConfig};
use kivo_core::{Capability, Secret};
use kivo_platform::{SecretHandle, Secrets};
use kivo_store::Database;
use kivo_store::brains::{UsageRow, now_ms};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// Health older than this is checked again before use and when the Brains page opens (DISC-14).
pub const HEALTH_TTL: Duration = Duration::from_secs(300);
/// A health check gives up after this long.
const HEALTH_TIMEOUT: Duration = Duration::from_secs(10);
/// The price table is refreshed at most weekly (BRAIN-35).
const PRICE_REFRESH_MS: i64 = 7 * 24 * 3_600 * 1_000;

const PROFILES_KEY: &str = "brains.profiles";
const LIMITS_KEY: &str = "brains.limits";
const OVERRIDES_KEY: &str = "brains.priceOverrides";
const CAPS_KEY: &str = "brains.taskCaps";
const PRICES_KEY: &str = "brains.prices";
const PRICES_AT_KEY: &str = "brains.pricesRefreshedAt";

/// `secret://kivo/<provider>/<name>` → a handle.
pub fn parse_handle(text: &str) -> Option<SecretHandle> {
    let rest = text.strip_prefix("secret://kivo/")?;
    let (provider, name) = rest.split_once('/')?;
    (!provider.is_empty() && !name.is_empty()).then(|| SecretHandle {
        provider: provider.to_owned(),
        name: name.to_owned(),
    })
}

/// Where a provider's API key is kept.
pub fn key_handle(provider: &str) -> SecretHandle {
    SecretHandle {
        provider: provider.to_owned(),
        name: "api-key".to_owned(),
    }
}

/// What KIVO knows about one provider's health.
#[derive(Clone, Debug)]
struct Known {
    health: Health,
    models: Vec<ModelInfo>,
    checked: Option<Instant>,
}

/// A CLI agent found on this PC (DISC-04).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundAgent {
    pub program: PathBuf,
    pub version: Option<String>,
    /// `None` when the CLI doesn't say.
    pub signed_in: Option<bool>,
}

/// One connected brain, as the Brains page shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrainView {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub privacy: PrivacyClass,
    /// The free option this is, if any (CONV-08).
    pub free: Option<String>,
    pub enabled: bool,
    pub health: Health,
    /// A key is stored (never the key itself: SEC-19).
    pub has_key: bool,
    pub models: Vec<String>,
    /// Seconds since the last health check.
    pub checked_ago: Option<u64>,
}

/// One metered call (BRAIN-34).
pub struct Meter<'a> {
    pub kind: &'a str,
    pub provider: &'a str,
    pub model: &'a str,
    pub profile: Option<&'a str>,
    pub usage: Usage,
    pub turn: Option<&'a str>,
    pub task: Option<&'a str>,
    pub routine: Option<&'a str>,
}

pub struct Brains {
    db: Arc<Mutex<Database>>,
    secrets: Arc<dyn Secrets>,
    http: Http,
    providers: RwLock<BTreeMap<String, Arc<dyn BrainProvider>>>,
    /// Providers put in directly (tests, the scripted rig); they survive a reload.
    injected: RwLock<BTreeMap<String, Arc<dyn BrainProvider>>>,
    connections: RwLock<Vec<BrainConnection>>,
    known: Mutex<HashMap<String, Known>>,
    agents: RwLock<BTreeMap<String, FoundAgent>>,
    /// What each CLI agent offers (its models and reasoning setting), from its last session.
    agent_options: RwLock<BTreeMap<String, kivo_brain::acp::AgentOptions>>,
    prices: RwLock<PriceTable>,
    /// Minutes to add to UTC for local time (limit periods).
    utc_offset: i32,
    /// When each provider's connection was last warmed (PLAN-10).
    warmed: Mutex<HashMap<String, Instant>>,
    /// A realtime provider put in directly (tests, the scripted rig); the capability and the
    /// privacy mode still decide whether it is used.
    realtime: RwLock<Option<Arc<dyn kivo_brain::realtime::RealtimeProvider>>>,
    /// A computer-use provider put in directly (tests, the scripted rig); cloud vision and the
    /// privacy mode still decide whether it is used.
    computer: RwLock<Option<Arc<dyn kivo_brain::computer::ComputerUseProvider>>>,
}

impl Brains {
    pub fn new(db: Arc<Mutex<Database>>, secrets: Arc<dyn Secrets>, utc_offset: i32) -> Self {
        let brains = Self {
            db,
            secrets,
            http: Http::new(),
            providers: RwLock::default(),
            injected: RwLock::default(),
            connections: RwLock::default(),
            known: Mutex::default(),
            agents: RwLock::default(),
            agent_options: RwLock::default(),
            prices: RwLock::new(PriceTable::bundled()),
            utc_offset,
            warmed: Mutex::default(),
            realtime: RwLock::default(),
            computer: RwLock::default(),
        };
        brains.load_prices();
        brains
    }

    /// The store, for the conversation history the turn engine keeps.
    pub fn database(&self) -> Arc<Mutex<Database>> {
        Arc::clone(&self.db)
    }

    /// Minutes to add to UTC for local time.
    pub fn utc_offset(&self) -> i32 {
        self.utc_offset
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Database> {
        lock(&self.db)
    }

    fn meta<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        let raw = self.db().meta(key).ok().flatten()?;
        serde_json::from_str(&raw).ok()
    }

    fn set_meta<T: Serialize>(&self, key: &str, value: &T) {
        let Ok(raw) = serde_json::to_string(value) else {
            return;
        };
        if let Err(e) = self.db().set_meta(key, &raw) {
            tracing::warn!(%e, key, "couldn't save brain settings");
        }
    }

    fn load_prices(&self) {
        let mut table = PriceTable::bundled();
        if let Some(stored) = self.meta::<serde_json::Value>(PRICES_KEY) {
            table.import(&stored);
        }
        for (model, price) in self.price_overrides() {
            table.set_override(&model, price);
        }
        *write(&self.prices) = table;
    }

    /// Builds the providers from the user's connections (after start and every settings change).
    pub fn reload(&self, config: &KivoConfig) {
        let mut built = BTreeMap::new();
        let mut known = lock(&self.known);
        for connection in config.brains.connections.iter().filter(|c| c.enabled) {
            match self.build(connection) {
                Ok(Some(provider)) => {
                    built.insert(connection.id.clone(), provider);
                }
                // A CLI agent: sessions are started per request (agents.rs).
                Ok(None) => {}
                Err(health) => {
                    known.insert(
                        connection.id.clone(),
                        Known {
                            health,
                            models: Vec::new(),
                            checked: Some(Instant::now()),
                        },
                    );
                }
            }
        }
        // Forget the health of what was disconnected or rebuilt with a new key.
        known.retain(|id, k| {
            config
                .brains
                .connections
                .iter()
                .any(|c| &c.id == id && c.enabled)
                && (built.contains_key(id) || k.health == Health::NeedsSignIn)
        });
        drop(known);
        *write(&self.providers) = built;
        write(&self.connections).clone_from(&config.brains.connections);
    }

    /// One connection's provider; `Ok(None)` for CLI agents; `Err` when it can't be used yet.
    fn build(&self, c: &BrainConnection) -> Result<Option<Arc<dyn BrainProvider>>, Health> {
        let entry = catalog::entry(&c.id);
        let key = if c.key.is_empty() {
            None
        } else {
            let handle = parse_handle(&c.key).ok_or(Health::NeedsSignIn)?;
            // Read here, handed to the adapter, never logged or sent to the UI (SEC-19).
            self.secrets.get(&handle).map_err(|e| Health::Unreachable {
                reason: e.to_string(),
            })?
        };
        let base = |entry: Option<&CatalogEntry>| {
            if c.base_url.is_empty() {
                entry.map_or_else(String::new, |e| e.base_url.to_owned())
            } else {
                c.base_url.trim_end_matches('/').to_owned()
            }
        };
        let http = self.http.clone();
        let needs_key = |key: Option<Secret<String>>| key.ok_or(Health::NeedsSignIn);
        let provider: Arc<dyn BrainProvider> = match entry {
            Some(e) if e.kind == ProviderKind::Cli => return Ok(None),
            Some(e) if e.kind == ProviderKind::Local => Arc::new(OpenAi::new(
                OpenAiConfig::local(e.id, e.name, base(Some(e))),
                key,
                http,
            )),
            Some(e) if e.id == "anthropic" => {
                Arc::new(Anthropic::new(base(Some(e)), needs_key(key)?, http))
            }
            Some(e) if e.id == "gemini" => {
                Arc::new(Gemini::new(base(Some(e)), needs_key(key)?, http))
            }
            Some(e) if e.id == "openai" => {
                let mut config = OpenAiConfig::openai();
                config.base_url = base(Some(e));
                Arc::new(OpenAi::new(config, Some(needs_key(key)?), http))
            }
            Some(e) if e.id == "openrouter" => {
                let mut config = OpenAiConfig::openrouter();
                config.base_url = base(Some(e));
                Arc::new(OpenAi::new(config, Some(needs_key(key)?), http))
            }
            Some(e) => {
                let mut config = OpenAiConfig::compatible(e.id, e.name, base(Some(e)));
                config.free = e.free.is_some();
                Arc::new(OpenAi::new(config, Some(needs_key(key)?), http))
            }
            None => {
                let name = if c.name.is_empty() { &c.id } else { &c.name };
                if base(None).is_empty() {
                    return Err(Health::Unreachable {
                        reason: "no address".into(),
                    });
                }
                let config = if c.local {
                    OpenAiConfig::local(&c.id, name, base(None))
                } else {
                    OpenAiConfig::compatible(&c.id, name, base(None))
                };
                Arc::new(OpenAi::new(config, key, http))
            }
        };
        Ok(Some(provider))
    }

    /// Adds a provider directly (tests and the scripted rig).
    pub fn insert(&self, provider: Arc<dyn BrainProvider>) {
        write(&self.injected).insert(provider.info().id, provider);
    }

    /// Puts in a computer-use provider (tests, the scripted rig), used instead of the connections'.
    pub fn set_computer(
        &self,
        provider: Option<Arc<dyn kivo_brain::computer::ComputerUseProvider>>,
    ) {
        *write(&self.computer) = provider;
    }

    /// Puts in a realtime provider (tests, the scripted rig), used instead of the connections'.
    pub fn set_realtime(&self, provider: Option<Arc<dyn kivo_brain::realtime::RealtimeProvider>>) {
        *write(&self.realtime) = provider;
    }

    pub fn provider(&self, id: &str) -> Option<Arc<dyn BrainProvider>> {
        let injected = read(&self.injected).get(id).cloned();
        injected.or_else(|| read(&self.providers).get(id).cloned())
    }

    fn all_providers(&self) -> Vec<Arc<dyn BrainProvider>> {
        let mut all: BTreeMap<String, Arc<dyn BrainProvider>> = read(&self.providers).clone();
        all.extend(read(&self.injected).clone());
        all.into_values().collect()
    }

    /// CLI agents found by discovery (DISC-04).
    pub fn set_agents(&self, agents: BTreeMap<String, FoundAgent>) {
        *write(&self.agents) = agents;
    }

    pub fn agent(&self, id: &str) -> Option<FoundAgent> {
        read(&self.agents).get(id).cloned()
    }

    /// Cloud brains are allowed at all: the privacy mode and the Cloud AI capability (invariant
    /// 11).
    pub fn cloud_allowed(config: &KivoConfig) -> bool {
        kivo_security::privacy::cloud_brains(&config.privacy, &config.capabilities)
    }

    /// Every connected brain as routing sees it.
    pub fn available(&self, config: &KivoConfig) -> Vec<Available> {
        let known = lock(&self.known).clone();
        let agents_on = config.capabilities.enabled(Capability::CliAgents);
        let mut out: Vec<Available> = Vec::new();
        for provider in self.all_providers() {
            let info = provider.info();
            let k = known.get(&info.id);
            out.push(Available {
                free: info.free || info.privacy == PrivacyClass::Local,
                healthy: k.is_none_or(|k| k.health.is_ready()),
                models: k
                    .map(|k| k.models.iter().map(|m| m.id.clone()).collect())
                    .unwrap_or_default(),
                id: info.id,
                name: info.name,
                kind: info.kind,
                privacy: info.privacy,
            });
        }
        if agents_on {
            let agents = read(&self.agents).clone();
            for c in read(&self.connections).iter().filter(|c| c.enabled) {
                let Some(entry) = catalog::entry(&c.id).filter(|e| e.kind == ProviderKind::Cli)
                else {
                    continue;
                };
                let found = agents.get(&c.id);
                out.push(Available {
                    id: entry.id.into(),
                    name: entry.name.into(),
                    kind: ProviderKind::Cli,
                    privacy: entry.privacy,
                    free: entry.free.is_some(),
                    healthy: found.is_some_and(|f| f.signed_in != Some(false))
                        && known.get(entry.id).is_none_or(|k| k.health.is_ready()),
                    models: Vec::new(),
                });
            }
        }
        out
    }

    /// The built-in profiles with the user's changes, then the user's own (BRAIN-20).
    pub fn profiles(&self) -> Vec<Profile> {
        let user: Vec<Profile> = self.meta(PROFILES_KEY).unwrap_or_default();
        let mut all = routing::built_in_profiles();
        for p in user {
            match all.iter_mut().find(|b| b.id == p.id) {
                Some(existing) => {
                    *existing = Profile {
                        built_in: true,
                        ..p
                    }
                }
                None => all.push(Profile {
                    built_in: false,
                    ..p
                }),
            }
        }
        all
    }

    /// The reasoning levels `provider`'s `model` can take: what its API offers for that model
    /// (`kivo_brain::reasoning`), or a CLI agent's own reasoning setting; none for local servers.
    pub fn reasoning_levels(
        &self,
        provider: &str,
        model: &str,
    ) -> Vec<kivo_brain::reasoning::Effort> {
        match read(&self.agent_options).get(provider) {
            Some(options) => options.levels(),
            None => kivo_brain::reasoning::levels(provider, model),
        }
    }

    /// What CLI agent `id` offered in its latest session (its models and reasoning setting).
    pub fn note_agent(&self, id: &str, options: kivo_brain::acp::AgentOptions) {
        write(&self.agent_options).insert(id.to_owned(), options);
    }

    /// Whether KIVO knows what CLI agent `id` offers yet.
    pub fn knows_agent(&self, id: &str) -> bool {
        read(&self.agent_options).contains_key(id)
    }

    /// Saves a profile (a changed built-in, or one of the user's own).
    pub fn save_profile(&self, profile: Profile) {
        let mut user: Vec<Profile> = self.meta(PROFILES_KEY).unwrap_or_default();
        user.retain(|p| p.id != profile.id);
        user.push(profile);
        self.set_meta(PROFILES_KEY, &user);
    }

    /// Deletes one of the user's profiles, or resets a built-in one.
    pub fn delete_profile(&self, id: &str) {
        let mut user: Vec<Profile> = self.meta(PROFILES_KEY).unwrap_or_default();
        user.retain(|p| p.id != id);
        self.set_meta(PROFILES_KEY, &user);
    }

    /// Picks the brain for a request, with the reason the card shows (BRAIN-21).
    pub fn route(
        &self,
        config: &KivoConfig,
        text: &str,
        profile: Option<&str>,
        sensitive: bool,
    ) -> Result<Route, RouteError> {
        self.route_in(config, text, profile, sensitive, None)
    }

    /// [`Self::route`] in a workspace whose chosen agent takes coding requests first.
    pub fn route_in(
        &self,
        config: &KivoConfig,
        text: &str,
        profile: Option<&str>,
        sensitive: bool,
        workspace_agent: Option<&str>,
    ) -> Result<Route, RouteError> {
        let available = self.available(config);
        let request = RouteRequest {
            text,
            profile,
            default_profile: &config.brains.default_profile,
            sensitive,
            offline: self.offline(),
            cloud_allowed: Self::cloud_allowed(config),
            workspace_agent,
        };
        let cloud_ok = request.cloud_allowed;
        // With cloud brains off, only local ones are candidates at all.
        let usable: Vec<Available> = available
            .into_iter()
            .filter(|a| cloud_ok || a.privacy == PrivacyClass::Local)
            .collect();
        routing::route(&request, &self.profiles(), &usable)
    }

    /// The cloud looks unreachable: every cloud brain's last check failed on the network, and at
    /// least one was checked (PLAN-05, the Offline rule).
    pub fn offline(&self) -> bool {
        let known = lock(&self.known);
        let cloud: Vec<&Known> = self
            .all_providers()
            .iter()
            .filter(|p| p.info().privacy == PrivacyClass::Cloud)
            .filter_map(|p| known.get(&p.info().id))
            .collect();
        !cloud.is_empty()
            && cloud.iter().all(|k| {
                matches!(&k.health, Health::Unreachable { reason } if reason.starts_with("network"))
            })
    }

    /// Opens the connection to the brain a request will most likely use, while the user is still
    /// speaking (PLAN-10): DNS, TLS and HTTP/2 are ready when the request comes. It is a plain GET
    /// of the service's address with no key and no side effects; the answer is ignored. At most
    /// once a minute per provider; local brains need nothing.
    pub async fn prewarm(&self, config: &KivoConfig) {
        let Ok(route) = self.route(config, "", None, false) else {
            return;
        };
        if route.privacy == PrivacyClass::Local || route.kind == ProviderKind::Cli {
            return;
        }
        let base = read(&self.connections)
            .iter()
            .find(|c| c.id == route.target.provider && !c.base_url.is_empty())
            .map(|c| c.base_url.clone())
            .or_else(|| catalog::entry(&route.target.provider).map(|e| e.base_url.to_owned()))
            .unwrap_or_default();
        if base.is_empty() {
            return;
        }
        {
            let mut warmed = lock(&self.warmed);
            if warmed
                .get(&route.target.provider)
                .is_some_and(|t| t.elapsed() < Duration::from_secs(60))
            {
                return;
            }
            warmed.insert(route.target.provider.clone(), Instant::now());
        }
        let cancel = CancellationToken::new();
        let _ = tokio::time::timeout(
            Duration::from_secs(3),
            self.http.get_json(&base, &[], &cancel),
        )
        .await;
    }

    /// Checks one provider now (the Brains page's Test, and lazily before use when stale).
    pub async fn check(&self, id: &str) -> Health {
        let Some(provider) = self.provider(id) else {
            return lock(&self.known).get(id).map_or(
                Health::Unreachable {
                    reason: "not connected".into(),
                },
                |k| k.health.clone(),
            );
        };
        let cancel = CancellationToken::new();
        let (health, models) = match tokio::time::timeout(HEALTH_TIMEOUT, provider.models()).await {
            Ok(Ok(models)) => (Health::Ready, models),
            Ok(Err(e)) => (Health::from_error(&e), Vec::new()),
            Err(_) => {
                cancel.cancel();
                (
                    Health::Unreachable {
                        reason: "network: no answer".into(),
                    },
                    Vec::new(),
                )
            }
        };
        let mut known = lock(&self.known);
        let models = if models.is_empty() {
            known.get(id).map(|k| k.models.clone()).unwrap_or_default()
        } else {
            models
        };
        known.insert(
            id.to_owned(),
            Known {
                health: health.clone(),
                models,
                checked: Some(Instant::now()),
            },
        );
        health
    }

    /// Checks every provider whose health is older than `max_age`, concurrently (DISC-14).
    pub async fn refresh_health(&self, max_age: Duration) {
        let stale: Vec<String> = {
            let known = lock(&self.known);
            self.all_providers()
                .iter()
                .map(|p| p.info().id)
                .filter(|id| {
                    known
                        .get(id)
                        .and_then(|k| k.checked)
                        .is_none_or(|t| t.elapsed() >= max_age)
                })
                .collect()
        };
        let checks = stale.iter().map(|id| self.check(id));
        futures_join_all(checks).await;
    }

    /// Checks again every provider whose last check failed (before telling the user that no
    /// brain is available: the outage may have passed).
    pub async fn recheck_unhealthy(&self) {
        let failing: Vec<String> = lock(&self.known)
            .iter()
            .filter(|(_, k)| !k.health.is_ready() && k.health != Health::NeedsSignIn)
            .map(|(id, _)| id.clone())
            .collect();
        futures_join_all(failing.iter().map(|id| self.check(id))).await;
    }

    /// A provider failed during a request: its health changes at once (DISC-14 "after errors").
    pub fn failed(&self, id: &str, error: &NormalizedError) {
        if matches!(
            error,
            NormalizedError::Cancelled | NormalizedError::Other(_)
        ) {
            return;
        }
        let mut known = lock(&self.known);
        let models = known.get(id).map(|k| k.models.clone()).unwrap_or_default();
        known.insert(
            id.to_owned(),
            Known {
                health: Health::from_error(error),
                models,
                checked: Some(Instant::now()),
            },
        );
    }

    /// The provider answered: whatever it had is over.
    pub fn succeeded(&self, id: &str) {
        let mut known = lock(&self.known);
        if let Some(k) = known.get_mut(id) {
            k.health = Health::Ready;
        }
    }

    /// Is the health of `id` older than the TTL (so it is checked before use)?
    pub fn stale(&self, id: &str) -> bool {
        lock(&self.known)
            .get(id)
            .and_then(|k| k.checked)
            .is_none_or(|t| t.elapsed() >= HEALTH_TTL)
    }

    /// Whether `model` at `provider` can look at images: what the provider reported, else the
    /// model families known to (CAP-08).
    pub fn sees(&self, provider: &str, model: &str) -> bool {
        let listed = lock(&self.known)
            .get(provider)
            .and_then(|k| k.models.iter().find(|m| m.id == model))
            .map(|m| m.vision);
        listed.unwrap_or_else(|| sees_by_name(model))
    }

    /// The context window of `model` at `provider`, when known.
    pub fn window(&self, provider: &str, model: &str) -> Option<u32> {
        let from_list = lock(&self.known)
            .get(provider)
            .and_then(|k| k.models.iter().find(|m| m.id == model))
            .and_then(|m| m.context_window);
        from_list.or_else(|| {
            read(&self.prices)
                .price(provider, model)
                .and_then(|p| p.window)
                .and_then(|w| u32::try_from(w).ok())
        })
    }

    /// The Brains page's list: connected brains with their state.
    pub fn views(&self, config: &KivoConfig) -> Vec<BrainView> {
        let known = lock(&self.known).clone();
        let agents = read(&self.agents).clone();
        let mut out = Vec::new();
        for c in &config.brains.connections {
            let entry = catalog::entry(&c.id);
            let info: Option<ProviderInfo> = self.provider(&c.id).map(|p| p.info());
            let k = known.get(&c.id);
            let kind = entry.map_or_else(
                || {
                    if c.local {
                        ProviderKind::Local
                    } else {
                        ProviderKind::Api
                    }
                },
                |e| e.kind,
            );
            let health = if kind == ProviderKind::Cli {
                match agents.get(&c.id) {
                    None => Health::Unreachable {
                        reason: "not found on this PC".into(),
                    },
                    Some(f) if f.signed_in == Some(false) => Health::NeedsSignIn,
                    Some(_) => k.map_or(Health::Ready, |k| k.health.clone()),
                }
            } else if !c.enabled {
                Health::Unreachable {
                    reason: "turned off".into(),
                }
            } else {
                k.map_or_else(
                    || {
                        if info.is_some() {
                            Health::Ready
                        } else {
                            Health::NeedsSignIn
                        }
                    },
                    |k| k.health.clone(),
                )
            };
            out.push(BrainView {
                id: c.id.clone(),
                name: entry.map_or_else(
                    || {
                        if c.name.is_empty() {
                            c.id.clone()
                        } else {
                            c.name.clone()
                        }
                    },
                    |e| e.name.to_owned(),
                ),
                kind,
                privacy: entry.map_or(
                    if c.local {
                        PrivacyClass::Local
                    } else {
                        PrivacyClass::Cloud
                    },
                    |e| e.privacy,
                ),
                free: entry.and_then(|e| e.free).map(str::to_owned),
                enabled: c.enabled,
                health,
                has_key: parse_handle(&c.key)
                    .and_then(|h| self.secrets.get(&h).ok().flatten())
                    .is_some(),
                // A CLI agent's models are the ones it offered in its last session.
                models: if kind == ProviderKind::Cli {
                    read(&self.agent_options)
                        .get(&c.id)
                        .map(|o| o.models.iter().map(|m| m.id.clone()).collect())
                        .unwrap_or_default()
                } else {
                    k.map(|k| k.models.iter().map(|m| m.id.clone()).collect())
                        .unwrap_or_default()
                },
                checked_ago: k.and_then(|k| k.checked).map(|t| t.elapsed().as_secs()),
            });
        }
        out
    }

    /// A computer-use model (CAP-09): an enabled Anthropic, OpenAI or Gemini connection with a
    /// saved key, in that order, when screenshots may go to the cloud (privacy and the screen
    /// option). The key is read here and handed to the adapter only.
    pub fn computer_provider(
        &self,
        config: &KivoConfig,
    ) -> Option<Arc<dyn kivo_brain::computer::ComputerUseProvider>> {
        use kivo_brain::computer::{AnthropicComputer, GeminiComputer, OpenAiComputer};
        if !kivo_security::privacy::cloud_vision(&config.privacy) || !config.tools.cloud_vision {
            return None;
        }
        if let Some(injected) = read(&self.computer).clone() {
            return Some(injected);
        }
        for id in ["anthropic", "openai", "gemini"] {
            let Some(c) = config
                .brains
                .connections
                .iter()
                .find(|c| c.id == id && c.enabled && !c.key.is_empty())
            else {
                continue;
            };
            let Some(key) = parse_handle(&c.key).and_then(|h| self.secrets.get(&h).ok().flatten())
            else {
                continue;
            };
            let base = c.base_url.trim_end_matches('/').to_owned();
            let http = self.http.clone();
            let provider: Arc<dyn kivo_brain::computer::ComputerUseProvider> = match id {
                "anthropic" => {
                    let mut p = AnthropicComputer::new(key, http);
                    if !base.is_empty() {
                        p.base_url = base;
                    }
                    Arc::new(p)
                }
                "openai" => {
                    let mut p = OpenAiComputer::new(key, http);
                    if !base.is_empty() {
                        p.base_url = base;
                    }
                    Arc::new(p)
                }
                _ => {
                    let mut p = GeminiComputer::new(key, http);
                    if !base.is_empty() {
                        p.base_url = format!("{base}/v1beta");
                    }
                    Arc::new(p)
                }
            };
            return Some(provider);
        }
        None
    }

    /// The realtime conversation brain (BRAINS §8, BRAIN-33): OpenAI Realtime or Gemini Live on
    /// a connected key (the settings' choice first), only with the Realtime voice capability on
    /// and cloud brains allowed by the privacy mode.
    pub fn realtime_provider(
        &self,
        config: &KivoConfig,
    ) -> Option<Arc<dyn kivo_brain::realtime::RealtimeProvider>> {
        use kivo_brain::realtime::{GeminiLive, OpenAiRealtime};
        if !config
            .capabilities
            .enabled(kivo_core::Capability::RealtimeVoice)
            || !kivo_security::privacy::cloud_brains(&config.privacy, &config.capabilities)
        {
            return None;
        }
        if let Some(injected) = read(&self.realtime).clone() {
            return Some(injected);
        }
        let chosen = config.brains.realtime.provider.as_str();
        let order: Vec<&str> = if chosen.is_empty() {
            vec!["openai", "gemini"]
        } else {
            vec![chosen]
        };
        for id in order {
            let Some(c) = config
                .brains
                .connections
                .iter()
                .find(|c| c.id == id && c.enabled && !c.key.is_empty())
            else {
                continue;
            };
            let Some(key) = parse_handle(&c.key).and_then(|h| self.secrets.get(&h).ok().flatten())
            else {
                continue;
            };
            // A custom address (a proxy) keeps its host; the scheme becomes WebSocket.
            let ws_base = c
                .base_url
                .trim_end_matches('/')
                .trim_end_matches("/v1beta")
                .trim_end_matches("/v1")
                .replacen("https://", "wss://", 1)
                .replacen("http://", "ws://", 1);
            let provider: Arc<dyn kivo_brain::realtime::RealtimeProvider> = match id {
                "openai" => {
                    let mut p = OpenAiRealtime::new(key);
                    if !ws_base.is_empty() {
                        p.ws_base = ws_base;
                    }
                    Arc::new(p)
                }
                "gemini" => {
                    let mut p = GeminiLive::new(key);
                    if !ws_base.is_empty() {
                        p.ws_base = ws_base;
                    }
                    Arc::new(p)
                }
                _ => continue,
            };
            return Some(provider);
        }
        None
    }

    // Keys (BRAIN-18): write-only from the UI, tested by the runtime.

    /// Stores a key and returns the handle to put in the connection.
    pub fn set_key(&self, provider: &str, key: &str) -> Result<String, String> {
        let handle = key_handle(provider);
        self.secrets
            .set(&handle, Secret::new(key.trim().to_owned()))
            .map_err(|e| e.to_string())?;
        Ok(handle.to_string())
    }

    pub fn delete_key(&self, provider: &str) {
        let _ = self.secrets.delete(&key_handle(provider));
    }

    /// Tries a key without saving it: builds the provider and lists its models.
    pub async fn test_key(&self, provider: &str, key: &str, base_url: &str) -> Health {
        let temp = FakeKeyStore(Secret::new(key.trim().to_owned()));
        let tester = Brains {
            db: Arc::clone(&self.db),
            secrets: Arc::new(temp),
            http: self.http.clone(),
            providers: RwLock::default(),
            injected: RwLock::default(),
            connections: RwLock::default(),
            agent_options: RwLock::default(),
            known: Mutex::default(),
            agents: RwLock::default(),
            prices: RwLock::new(PriceTable::default()),
            utc_offset: 0,
            warmed: Mutex::default(),
            realtime: RwLock::default(),
            computer: RwLock::default(),
        };
        let connection = BrainConnection {
            id: provider.to_owned(),
            base_url: base_url.to_owned(),
            key: key_handle(provider).to_string(),
            enabled: true,
            ..BrainConnection::default()
        };
        match tester.build(&connection) {
            Ok(Some(p)) => match tokio::time::timeout(HEALTH_TIMEOUT, p.models()).await {
                Ok(Ok(_)) => Health::Ready,
                Ok(Err(e)) => Health::from_error(&e),
                Err(_) => Health::Unreachable {
                    reason: "network: no answer".into(),
                },
            },
            Ok(None) => Health::Ready,
            Err(h) => h,
        }
    }

    // Money (BRAINS §9).

    /// What `usage` would cost on `provider`'s `model`, recording nothing (Settings → Context's
    /// estimate). Local brains cost nothing; `None` when the price isn't known.
    pub fn estimate(&self, provider: &str, model: &str, usage: Usage) -> Option<f64> {
        let local = self
            .provider(provider)
            .is_some_and(|p| p.info().privacy == PrivacyClass::Local);
        read(&self.prices).cost(provider, model, usage, local)
    }

    /// Records one call's usage with its estimated cost; returns the cost.
    pub fn meter(&self, m: &Meter<'_>) -> Option<f64> {
        let local = self
            .provider(m.provider)
            .is_some_and(|p| p.info().privacy == PrivacyClass::Local);
        let cost = read(&self.prices).cost(m.provider, m.model, m.usage, local);
        let row = UsageRow {
            ts: now_ms(),
            kind: m.kind.to_owned(),
            provider: m.provider.to_owned(),
            model: m.model.to_owned(),
            brain_profile: m.profile.map(str::to_owned),
            input_tokens: m.usage.input_tokens,
            output_tokens: m.usage.output_tokens,
            cached_tokens: m.usage.cached_tokens,
            audio_seconds: 0.0,
            images: 0,
            requests: 1,
            cost,
            turn_id: m.turn.map(str::to_owned),
            task_id: m.task.map(str::to_owned),
            routine_id: m.routine.map(str::to_owned),
        };
        if let Err(e) = self.db().record_usage(&row) {
            tracing::warn!(%e, "couldn't record usage");
        }
        cost
    }

    /// Spending since `since` (ms), for limits and the Usage page.
    pub fn spend_since(&self, since: i64) -> Vec<Spend> {
        self.db()
            .usage_since(since)
            .unwrap_or_default()
            .into_iter()
            .map(|u| Spend {
                at: u.ts,
                feature: match u.kind.as_str() {
                    "agent" => "agents".to_owned(),
                    other => other.to_owned(),
                },
                provider: u.provider,
                profile: u.brain_profile.unwrap_or_default(),
                cost: u.cost.unwrap_or(0.0),
            })
            .collect()
    }

    pub fn limits(&self) -> Vec<Limit> {
        self.meta(LIMITS_KEY).unwrap_or_default()
    }

    pub fn set_limits(&self, limits: &[Limit]) {
        self.set_meta(LIMITS_KEY, &limits);
    }

    pub fn task_caps(&self) -> TaskCaps {
        self.meta(CAPS_KEY).unwrap_or_default()
    }

    pub fn set_task_caps(&self, caps: &TaskCaps) {
        self.set_meta(CAPS_KEY, caps);
    }

    /// Every limit that applies to a request to `provider` under `profile`, with where it stands.
    /// The default is no limits: track only (BRAIN-36).
    pub fn limit_states(
        &self,
        provider: &str,
        profile: &str,
        feature: &str,
    ) -> Vec<(Limit, LimitState)> {
        let limits: Vec<Limit> = self
            .limits()
            .into_iter()
            .filter(|l| match &l.scope {
                Scope::Overall => true,
                Scope::Provider(p) => p == provider,
                Scope::Profile(p) => p == profile,
                Scope::Feature(f) => f == feature,
            })
            .collect();
        if limits.is_empty() {
            return Vec::new();
        }
        let now = now_ms();
        // A monthly period is at most 31 days back.
        let spend = self.spend_since(now - 32 * 24 * 3_600 * 1_000);
        limits
            .into_iter()
            .map(|l| {
                let state = cost::check(&l, &spend, now, self.utc_offset);
                (l, state)
            })
            .collect()
    }

    pub fn price_overrides(&self) -> BTreeMap<String, Price> {
        self.meta(OVERRIDES_KEY).unwrap_or_default()
    }

    pub fn set_price_override(&self, model: &str, price: Option<Price>) {
        let mut all = self.price_overrides();
        match price {
            Some(p) => {
                all.insert(model.to_owned(), p);
            }
            None => {
                all.remove(model);
            }
        }
        self.set_meta(OVERRIDES_KEY, &all);
        self.load_prices();
    }

    pub fn price(&self, provider: &str, model: &str) -> Option<Price> {
        read(&self.prices).price(provider, model)
    }

    /// Refreshes prices from LiteLLM when the last refresh is a week old (or `force`). Returns
    /// how many models were updated.
    pub async fn refresh_prices(&self, force: bool) -> Result<usize, NormalizedError> {
        let last: i64 = self.meta(PRICES_AT_KEY).unwrap_or(0);
        if !force && now_ms() - last < PRICE_REFRESH_MS {
            return Ok(0);
        }
        let raw = self
            .http
            .get_json(cost::REFRESH_URL, &[], &CancellationToken::new())
            .await?;
        let mut fresh = PriceTable::default();
        let n = fresh.refresh_from_litellm(&raw);
        if n > 0 {
            self.set_meta(PRICES_KEY, &fresh.export());
            self.set_meta(PRICES_AT_KEY, &now_ms());
            self.load_prices();
        }
        Ok(n)
    }
}

/// A one-key store for testing a key before it is saved.
struct FakeKeyStore(Secret<String>);

impl Secrets for FakeKeyStore {
    fn get(&self, _handle: &SecretHandle) -> kivo_platform::PlatformResult<Option<Secret<String>>> {
        Ok(Some(Secret::new(self.0.expose().clone())))
    }
    fn set(&self, _: &SecretHandle, _: Secret<String>) -> kivo_platform::PlatformResult<()> {
        Ok(())
    }
    fn delete(&self, _: &SecretHandle) -> kivo_platform::PlatformResult<()> {
        Ok(())
    }
    fn protect(&self, data: &[u8]) -> kivo_platform::PlatformResult<Vec<u8>> {
        Ok(data.to_vec())
    }
    fn unprotect(&self, data: &[u8]) -> kivo_platform::PlatformResult<Vec<u8>> {
        Ok(data.to_vec())
    }
}

/// Runs futures concurrently and waits for all of them.
async fn futures_join_all<F: std::future::Future>(futures: impl IntoIterator<Item = F>) {
    let mut set = Vec::new();
    for f in futures {
        set.push(f);
    }
    let mut pinned: Vec<std::pin::Pin<Box<F>>> = set.into_iter().map(Box::pin).collect();
    std::future::poll_fn(|cx| {
        pinned.retain_mut(|f| f.as_mut().poll(cx).is_pending());
        if pinned.is_empty() {
            std::task::Poll::Ready(())
        } else {
            std::task::Poll::Pending
        }
    })
    .await;
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn read<T>(l: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    l.read().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write<T>(l: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    l.write().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Model families that take images, for providers that don't say (CAP-08).
pub fn sees_by_name(model: &str) -> bool {
    let m = model.to_lowercase();
    [
        "claude-",
        "gpt-4o",
        "gpt-4.1",
        "gpt-5",
        "o3",
        "o4",
        "gemini-",
        "llava",
        "bakllava",
        "qwen2.5-vl",
        "qwen2-vl",
        "qwen3-vl",
        "llama3.2-vision",
        "llama-4",
        "minicpm-v",
        "pixtral",
        "gemma3",
        "moondream",
        "grok-4",
        "grok-2-vision",
    ]
    .iter()
    .any(|f| m.contains(f))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivo_brain::testing::{MockServer, Reply, Script, ScriptedBrain};
    use kivo_core::config::PrivacyMode;
    use kivo_testkit::FakeSecrets;
    use serde_json::json;

    fn hub() -> Brains {
        Brains::new(
            Arc::new(Mutex::new(Database::in_memory().unwrap())),
            Arc::new(FakeSecrets::default()),
            0,
        )
    }

    fn connect(config: &mut KivoConfig, id: &str, base_url: &str, key: &str) {
        config.brains.connections.push(BrainConnection {
            id: id.into(),
            base_url: base_url.into(),
            key: key.into(),
            enabled: true,
            ..BrainConnection::default()
        });
    }

    #[test]
    fn handles_parse_and_only_kivo_handles_count() {
        assert_eq!(
            parse_handle("secret://kivo/openai/api-key"),
            Some(key_handle("openai"))
        );
        assert_eq!(parse_handle("sk-live-123"), None);
        assert_eq!(parse_handle("secret://other/openai/api-key"), None);
    }

    #[tokio::test]
    async fn keys_are_stored_by_handle_and_providers_built_from_them() {
        let hub = hub();
        let mut config = KivoConfig::default();
        // Without a key, a key-only provider needs sign-in and isn't routed to.
        connect(&mut config, "anthropic", "", "");
        hub.reload(&config);
        assert!(hub.provider("anthropic").is_none());
        let views = hub.views(&config);
        assert_eq!(views[0].health, Health::NeedsSignIn);
        assert!(!views[0].has_key);

        let handle = hub.set_key("anthropic", " sk-ant-test ").unwrap();
        assert_eq!(handle, "secret://kivo/anthropic/api-key");
        config.brains.connections[0].key = handle;
        hub.reload(&config);
        assert!(hub.provider("anthropic").is_some());
        let views = hub.views(&config);
        assert!(views[0].has_key);
        // The view carries no key material (SEC-19).
        let json = serde_json::to_string(&views).unwrap();
        assert!(!json.contains("sk-ant-test"));
    }

    #[tokio::test]
    async fn local_servers_need_no_key_and_health_follows_the_model_list() {
        let server = MockServer::start(|r| {
            assert_eq!(r.path, "/v1/models");
            Reply::json(200, &json!({"data": [{"id": "llama3.2"}, {"id": "qwen3"}]}))
        })
        .await;
        let hub = hub();
        let mut config = KivoConfig::default();
        connect(&mut config, "ollama", &format!("{}/v1", server.url), "");
        hub.reload(&config);
        assert_eq!(hub.check("ollama").await, Health::Ready);
        let available = hub.available(&config);
        assert_eq!(available[0].models, ["llama3.2", "qwen3"]);
        assert!(available[0].free && available[0].healthy);
        assert!(!hub.stale("ollama"));
    }

    #[tokio::test]
    async fn a_refused_key_marks_the_brain_as_needing_sign_in_and_routing_skips_it() {
        let server =
            MockServer::start(|_| Reply::json(401, &json!({"error": {"message": "invalid key"}})))
                .await;
        let hub = hub();
        let mut config = KivoConfig::default();
        let handle = hub.set_key("openai", "sk-bad").unwrap();
        connect(
            &mut config,
            "openai",
            &format!("{}/v1", server.url),
            &handle,
        );
        hub.reload(&config);
        hub.insert(Arc::new(ScriptedBrain::new(
            "ollama",
            PrivacyClass::Local,
            vec![],
        )));
        hub.refresh_health(Duration::ZERO).await;
        assert_eq!(hub.views(&config)[0].health, Health::NeedsSignIn);
        let route = hub
            .route(&config, "what's the capital of Peru", None, false)
            .unwrap();
        assert_eq!(route.target.provider, "ollama", "{}", route.reason);
        assert!(route.reason.contains("isn't available"), "{}", route.reason);
    }

    #[tokio::test]
    async fn testing_a_key_does_not_save_it() {
        let server = MockServer::start(|r| {
            let ok = r.header("authorization") == Some("Bearer sk-good");
            if ok {
                Reply::json(200, &json!({"data": []}))
            } else {
                Reply::json(401, &json!({}))
            }
        })
        .await;
        let hub = hub();
        let url = format!("{}/v1", server.url);
        assert_eq!(hub.test_key("openai", "sk-good", &url).await, Health::Ready);
        assert_eq!(
            hub.test_key("openai", "sk-nope", &url).await,
            Health::NeedsSignIn
        );
        assert!(
            hub.secrets.get(&key_handle("openai")).unwrap().is_none(),
            "nothing stored"
        );
    }

    #[test]
    fn cloud_brains_are_skipped_when_cloud_processing_is_off() {
        let hub = hub();
        hub.insert(Arc::new(ScriptedBrain::new(
            "anthropic",
            PrivacyClass::Cloud,
            vec![Script::text("hi")],
        )));
        let mut config = KivoConfig::default();
        assert!(hub.route(&config, "hello there", None, false).is_ok());
        config.privacy.mode = PrivacyMode::Local;
        assert_eq!(
            hub.route(&config, "hello there", None, false),
            Err(RouteError::NoBrain)
        );
        config.privacy.mode = PrivacyMode::Cloud;
        config.capabilities.set(Capability::CloudBrains, false);
        assert!(hub.route(&config, "hello there", None, false).is_err());
    }

    #[test]
    fn usage_is_metered_with_a_cost_estimate_and_limits_add_up() {
        let hub = hub();
        let usage = Usage {
            input_tokens: 1_000_000,
            output_tokens: 0,
            cached_tokens: 0,
        };
        let cost = hub
            .meter(&Meter {
                kind: "brain",
                provider: "openai",
                model: "gpt-4o",
                profile: Some("default"),
                usage,
                turn: Some("t1"),
                task: None,
                routine: None,
            })
            .expect("gpt-4o is in the bundled table");
        assert!(cost > 0.0);
        assert!(hub.limit_states("openai", "default", "brain").is_empty());
        hub.set_limits(&[Limit {
            scope: Scope::Provider("openai".into()),
            period: cost::Period::Daily,
            amount: cost / 2.0,
            warnings: vec![50, 80],
            at_limit: cost::AtLimit::Ask,
        }]);
        let states = hub.limit_states("openai", "default", "brain");
        assert_eq!(states.len(), 1);
        assert!(states[0].1.reached);
        assert_eq!(states[0].1.warning, Some(80));
        assert!(hub.limit_states("anthropic", "default", "brain").is_empty());
        // A user price wins over the bundled one.
        hub.set_price_override(
            "gpt-4o",
            Some(Price {
                input: 0.0,
                output: 0.0,
                cached: None,
                window: None,
            }),
        );
        assert_eq!(
            hub.meter(&Meter {
                kind: "brain",
                provider: "openai",
                model: "gpt-4o",
                profile: None,
                usage,
                turn: None,
                task: None,
                routine: None,
            }),
            Some(0.0)
        );
    }

    #[tokio::test]
    async fn prewarming_opens_the_connection_without_a_key_or_a_request() {
        let server = MockServer::start(|_| Reply::json(404, &json!({}))).await;
        let hub = hub();
        let mut config = KivoConfig::default();
        let handle = hub.set_key("openai", "sk-secret").unwrap();
        connect(
            &mut config,
            "openai",
            &format!("{}/v1", server.url),
            &handle,
        );
        hub.reload(&config);
        hub.prewarm(&config).await;
        hub.prewarm(&config).await;
        let requests = server.requests();
        assert_eq!(requests.len(), 1, "once a minute at most");
        assert_eq!(requests[0].method, "GET");
        assert_eq!(requests[0].path, "/v1");
        assert!(
            requests[0].header("authorization").is_none(),
            "no key is sent"
        );
    }

    #[test]
    fn user_profiles_override_built_ins_and_add_their_own() {
        let hub = hub();
        let mut fast = hub.profiles().into_iter().find(|p| p.id == "fast").unwrap();
        fast.system_prompt_addendum = "Be very brief.".into();
        hub.save_profile(fast);
        let mut mine = routing::built_in_profiles().remove(0);
        mine.id = "research".into();
        mine.name = "Research".into();
        hub.save_profile(mine);
        let all = hub.profiles();
        assert_eq!(all.len(), 8);
        let fast = all.iter().find(|p| p.id == "fast").unwrap();
        assert!(fast.built_in && fast.system_prompt_addendum == "Be very brief.");
        assert!(!all.iter().find(|p| p.id == "research").unwrap().built_in);
        hub.delete_profile("fast");
        assert!(
            hub.profiles()
                .iter()
                .find(|p| p.id == "fast")
                .unwrap()
                .system_prompt_addendum
                .is_empty(),
            "reset"
        );
    }
}
