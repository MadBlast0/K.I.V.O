//! KIVO's own updates (DISTRIBUTION §2; DIST-07/08/09, SEC-32).
//!
//! The runtime runs the update, because it holds the executables' locks and must wind down turns
//! first; the webview never runs an installer (SECURITY §9). It reads the manifest format of
//! `tauri-plugin-updater` (`latest.json`, published per channel as `<channel>.json` on the
//! `updates` release) and runs the installers the way that plugin does:
//!
//! 1. **Check** the channel's manifest (the setting, or the installed build's channel) once a day,
//!    or when asked; never in Strictly private.
//! 2. **Download and verify:** the installer's minisign signature against the key built into this
//!    runtime, then its Authenticode signature and publisher (Windows' `WinVerifyTrust`). Anything
//!    that fails is deleted and never run.
//! 3. **Ask** with a notification (Install now / When idle), or install when KIVO and the PC are
//!    idle if the user chose that.
//! 4. **Install:** keep the current version's installer for a rollback, let a running turn finish
//!    (10 s at most, then it is cancelled), record the pending update, start the installer
//!    (`/P /R /UPDATE`: passive, relaunch) and quit. The installer's hooks stop what is left.
//! 5. After the restart the runtime counts its starts and, once it has run for a moment, marks
//!    the update healthy (`kivo_core::update`); the app rolls back if it can't start twice.

use crate::core::Core;
use kivo_core::config::{PrivacyMode, UpdateChannel, UpdateInstall};
use kivo_core::event::{EventKind, SystemEvent};
use kivo_core::update::{self as record, Pending};
use kivo_core::{Event, text};
use kivo_ipc::protocol::{UpdateState, UpdateView, WhatsNew};
use kivo_platform::{CodeTrust, Notification, NotificationAction, Notifications, SystemInfo};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Where releases are downloaded from.
pub const DOWNLOADS: &str = "https://github.com/MadBlast0/K.I.V.O/releases/download";
/// Notification buttons.
pub const INSTALL_NOW: &str = "update.install";
pub const WHEN_IDLE: &str = "update.idle";
/// How often KIVO looks, when it may.
const CHECK_EVERY: Duration = Duration::from_secs(24 * 3600);
/// A running turn gets this long to finish before the update cancels it.
const WIND_DOWN: Duration = Duration::from_secs(10);
/// "When idle": nobody has touched the PC for this long.
const IDLE_FOR: u32 = 10 * 60;
/// The new runtime counts as healthy after serving this long.
pub const HEALTHY_AFTER: Duration = Duration::from_secs(15);

/// The keys this build was made with (the release workflow sets them; a local build has none and
/// can't update itself).
#[derive(Clone, Debug, Default)]
pub struct Keys {
    /// The minisign public key, as `tauri signer generate` prints it (base64).
    pub updater: Option<String>,
    /// The Authenticode publisher an installer must be signed by.
    pub publisher: Option<String>,
}

impl Keys {
    pub fn built_in() -> Self {
        let non_empty = |v: Option<&'static str>| v.map(str::trim).filter(|v| !v.is_empty());
        Self {
            updater: non_empty(option_env!("KIVO_UPDATER_PUBKEY")).map(str::to_owned),
            publisher: non_empty(option_env!("KIVO_SIGNING_PUBLISHER")).map(str::to_owned),
        }
    }
}

/// Starts an installer outside KIVO's process tree, so it outlives the runtime.
pub trait Launch: Send + Sync {
    fn launch(&self, program: &Path, args: &[String]) -> Result<(), String>;
}

/// The real launcher: a detached process that breaks away from any job KIVO runs in.
pub struct Detached;

impl Launch for Detached {
    fn launch(&self, program: &Path, args: &[String]) -> Result<(), String> {
        let mut command = std::process::Command::new(program);
        command.args(args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
            command.creation_flags(
                DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB,
            );
            if command.spawn().is_ok() {
                return Ok(());
            }
            // Not allowed to break away: start it anyway (the installer's hooks cope).
            command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
        }
        command.spawn().map(drop).map_err(|e| e.to_string())
    }
}

#[derive(Clone, Debug, Deserialize)]
struct Manifest {
    version: String,
    #[serde(default)]
    notes: String,
    platforms: HashMap<String, Asset>,
}

#[derive(Clone, Debug, Deserialize)]
struct Asset {
    signature: String,
    url: String,
}

#[derive(Clone, Debug)]
struct Offer {
    version: String,
    notes: String,
    asset: Asset,
}

#[derive(Clone, Debug)]
struct Ready {
    version: String,
    notes: String,
    path: PathBuf,
}

/// What the parts the updater needs.
pub struct Parts {
    pub core: Arc<Core>,
    pub dir: PathBuf,
    pub keys: Keys,
    pub trust: Arc<dyn CodeTrust>,
    pub launch: Arc<dyn Launch>,
    pub notifications: Arc<dyn Notifications>,
    pub system: Arc<dyn SystemInfo>,
}

pub struct Updater {
    core: Arc<Core>,
    dir: PathBuf,
    keys: Keys,
    trust: Arc<dyn CodeTrust>,
    launch: Arc<dyn Launch>,
    notifications: Arc<dyn Notifications>,
    system: Arc<dyn SystemInfo>,
    http: reqwest::Client,
    base: Mutex<String>,
    version: Mutex<String>,
    arch: &'static str,
    kind: Mutex<String>,
    state: Mutex<UpdateState>,
    offer: Mutex<Option<Offer>>,
    ready: Mutex<Option<Ready>>,
    when_idle: AtomicBool,
    busy: tokio::sync::Mutex<()>,
    last_check: Mutex<Option<Instant>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// How this copy was installed: the per-user NSIS installer (the default) or the MSI for IT
/// (per machine, under Program Files).
fn installed_kind() -> String {
    let exe = std::env::current_exe().unwrap_or_default();
    let in_program_files = ["ProgramFiles", "ProgramW6432"]
        .into_iter()
        .filter_map(std::env::var_os)
        .any(|p| exe.starts_with(p));
    if in_program_files { "msi" } else { "nsis" }.into()
}

fn now_ms() -> u64 {
    u64::try_from(kivo_store::brains::now_ms()).unwrap_or_default()
}

/// The installer's file name from its download address.
fn file_name(url: &str) -> String {
    let last = url.rsplit('/').next().unwrap_or("KIVO-setup.exe");
    let decoded = url::form_urlencoded::parse(format!("x={last}").as_bytes())
        .next()
        .map_or_else(|| last.to_owned(), |(_, v)| v.into_owned());
    decoded.replace(['\\', '/', ':'], "_")
}

/// Checks `bytes` against a minisign signature, both as Tauri writes them (base64 of the key and
/// signature files).
pub fn verify_minisign(bytes: &[u8], signature_b64: &str, pubkey_b64: &str) -> Result<(), String> {
    use base64::Engine as _;
    let decode = |b64: &str| {
        base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .ok()
            .and_then(|raw| String::from_utf8(raw).ok())
    };
    let key_text = decode(pubkey_b64).ok_or("the update key is malformed")?;
    let sig_text = decode(signature_b64).ok_or("the update's signature is malformed")?;
    let key = minisign_verify::PublicKey::decode(&key_text)
        .map_err(|_| "the update key is malformed".to_owned())?;
    let signature = minisign_verify::Signature::decode(&sig_text)
        .map_err(|_| "the update's signature is malformed".to_owned())?;
    key.verify(bytes, &signature, true)
        .map_err(|_| text::t("updates.badSignature"))
}

impl Updater {
    pub fn new(parts: Parts) -> Arc<Self> {
        Arc::new(Self {
            core: parts.core,
            dir: parts.dir,
            keys: parts.keys,
            trust: parts.trust,
            launch: parts.launch,
            notifications: parts.notifications,
            system: parts.system,
            http: reqwest::Client::new(),
            base: Mutex::new(DOWNLOADS.into()),
            version: Mutex::new(env!("CARGO_PKG_VERSION").into()),
            arch: if cfg!(target_arch = "aarch64") {
                "aarch64"
            } else {
                "x86_64"
            },
            kind: Mutex::new(installed_kind()),
            state: Mutex::new(UpdateState::Idle),
            offer: Mutex::new(None),
            ready: Mutex::new(None),
            when_idle: AtomicBool::new(false),
            busy: tokio::sync::Mutex::new(()),
            last_check: Mutex::new(None),
        })
    }

    /// Another download address (tests: a local server).
    pub fn set_base(&self, base: &str) {
        *lock(&self.base) = base.trim_end_matches('/').to_owned();
    }

    /// Another running version and install kind (tests).
    pub fn set_version(&self, version: &str, kind: &str) {
        *lock(&self.version) = version.to_owned();
        *lock(&self.kind) = kind.to_owned();
    }

    pub fn version(&self) -> String {
        lock(&self.version).clone()
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// This build can update itself.
    pub fn enabled(&self) -> bool {
        self.keys.updater.is_some()
    }

    fn channel(&self) -> UpdateChannel {
        self.core
            .config()
            .updates
            .channel
            .unwrap_or_else(|| UpdateChannel::of_version(&self.version()))
    }

    fn set_state(&self, state: UpdateState) {
        *lock(&self.state) = state;
        self.core
            .bus
            .publish(Event::new(EventKind::System(SystemEvent::UpdateChanged)));
    }

    pub fn status(&self) -> UpdateView {
        let whats_new = record::installed(&self.dir)
            .filter(|i| !i.seen && i.version == self.version())
            .map(|i| WhatsNew {
                version: i.version,
                from: i.from,
                notes: i.notes,
            });
        UpdateView {
            current: self.version(),
            channel: self.channel(),
            enabled: self.enabled(),
            state: lock(&self.state).clone(),
            whats_new,
        }
    }

    /// "Got it" on What's new (UX-59).
    pub fn seen(&self) {
        if let Some(mut i) = record::installed(&self.dir) {
            i.seen = true;
            let _ = record::save_installed(&self.dir, &i);
            let state = lock(&self.state).clone();
            self.set_state(state);
        }
    }

    /// Looks for a newer version on the channel. `Ok(None)` when up to date.
    pub async fn check(&self) -> Result<Option<String>, String> {
        let _one = self.busy.lock().await;
        self.check_now().await
    }

    async fn check_now(&self) -> Result<Option<String>, String> {
        if !self.enabled() {
            return Err(text::t("updates.notThisBuild"));
        }
        self.set_state(UpdateState::Checking);
        *lock(&self.last_check) = Some(Instant::now());
        let url = format!("{}/updates/{}.json", lock(&self.base), self.channel().id());
        let result = async {
            let reply = self
                .http
                .get(&url)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !reply.status().is_success() {
                return Err(format!("{} for {url}", reply.status()));
            }
            reply.json::<Manifest>().await.map_err(|e| e.to_string())
        }
        .await;
        let manifest = match result {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(error = e, "couldn't check for updates");
                let message = text::t("updates.checkFailed");
                self.set_state(UpdateState::Failed {
                    message: message.clone(),
                });
                return Err(message);
            }
        };
        if !record::is_newer(&manifest.version, &self.version()) {
            self.set_state(UpdateState::UpToDate {
                checked_at: now_ms(),
            });
            return Ok(None);
        }
        let kind = lock(&self.kind).clone();
        let asset = [
            format!("windows-{}-{kind}", self.arch),
            format!("windows-{}", self.arch),
        ]
        .iter()
        .find_map(|k| manifest.platforms.get(k).cloned());
        let Some(asset) = asset else {
            tracing::info!(
                version = manifest.version,
                "no installer for this PC in the update"
            );
            self.set_state(UpdateState::UpToDate {
                checked_at: now_ms(),
            });
            return Ok(None);
        };
        *lock(&self.offer) = Some(Offer {
            version: manifest.version.clone(),
            notes: manifest.notes,
            asset,
        });
        Ok(Some(manifest.version))
    }

    /// Downloads and verifies the offered update. Returns the verified installer.
    pub async fn download(&self) -> Result<PathBuf, String> {
        let _one = self.busy.lock().await;
        self.download_now().await
    }

    async fn download_now(&self) -> Result<PathBuf, String> {
        let offer = lock(&self.offer)
            .clone()
            .ok_or(text::t("updates.nothing"))?;
        if let Some(ready) = lock(&self.ready)
            .as_ref()
            .filter(|r| r.version == offer.version)
        {
            return Ok(ready.path.clone());
        }
        let folder = self.dir.join(&offer.version);
        let path = folder.join(file_name(&offer.asset.url));
        let result = self
            .fetch_verified(
                &offer.version,
                &offer.asset.url,
                &offer.asset.signature,
                &path,
            )
            .await;
        match result {
            Ok(()) => {
                *lock(&self.ready) = Some(Ready {
                    version: offer.version.clone(),
                    notes: offer.notes.clone(),
                    path: path.clone(),
                });
                self.set_state(UpdateState::Ready {
                    version: offer.version,
                    notes: offer.notes,
                    when_idle: self.installs_when_idle(),
                });
                Ok(path)
            }
            Err(message) => {
                self.set_state(UpdateState::Failed {
                    message: message.clone(),
                });
                Err(message)
            }
        }
    }

    /// Downloads `url` to `path` and keeps it only if both signatures check out (SEC-32).
    async fn fetch_verified(
        &self,
        version: &str,
        url: &str,
        signature: &str,
        path: &Path,
    ) -> Result<(), String> {
        use futures_util::StreamExt;
        let key = self
            .keys
            .updater
            .clone()
            .ok_or(text::t("updates.notThisBuild"))?;
        let failed = |e: String| {
            tracing::warn!(error = e, "update download failed");
            text::t("updates.downloadFailed")
        };
        std::fs::create_dir_all(path.parent().unwrap_or(&self.dir))
            .map_err(|e| failed(e.to_string()))?;
        let reply = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| failed(e.to_string()))?;
        if !reply.status().is_success() {
            return Err(failed(format!("{} for {url}", reply.status())));
        }
        let total = reply.content_length();
        let mut bytes = Vec::with_capacity(usize::try_from(total.unwrap_or(0)).unwrap_or(0));
        let mut stream = reply.bytes_stream();
        let mut shown = Instant::now();
        while let Some(chunk) = stream.next().await {
            bytes.extend_from_slice(&chunk.map_err(|e| failed(e.to_string()))?);
            if shown.elapsed() > Duration::from_millis(250) {
                shown = Instant::now();
                self.set_state(UpdateState::Downloading {
                    version: version.to_owned(),
                    received: bytes.len() as u64,
                    total,
                });
            }
        }
        // 1. The manifest's minisign signature, with the key built into this runtime.
        verify_minisign(&bytes, signature, &key)?;
        let part = path.with_extension("part");
        std::fs::write(&part, &bytes).map_err(|e| failed(e.to_string()))?;
        // 2. Authenticode: signed, trusted, by KIVO's publisher.
        if let Some(publisher) = &self.keys.publisher {
            let signer = self.trust.signer(&part).ok().flatten();
            if !signer
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case(publisher))
            {
                let _ = std::fs::remove_file(&part);
                tracing::warn!(signer = ?signer, "an update's installer isn't signed by KIVO");
                return Err(text::t("updates.badPublisher"));
            }
        } else {
            tracing::info!("this build isn't code-signed: only the update signature was checked");
        }
        std::fs::rename(&part, path).map_err(|e| failed(e.to_string()))?;
        Ok(())
    }

    /// The installer of the version running now, for a rollback: kept from its own update, or
    /// fetched and verified from its release. `None` when it can't be had.
    async fn keep_current(&self) -> Option<PathBuf> {
        let current = self.version();
        let folder = self.dir.join(&current);
        let kept = std::fs::read_dir(&folder).ok().and_then(|entries| {
            entries.flatten().map(|e| e.path()).find(|p| {
                p.extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("exe") || x.eq_ignore_ascii_case("msi"))
            })
        });
        if kept.is_some() {
            return kept;
        }
        let arch = if self.arch == "aarch64" {
            "arm64"
        } else {
            "x64"
        };
        let name = if *lock(&self.kind) == "msi" {
            format!("KIVO_{current}_{arch}_en-US.msi")
        } else {
            format!("KIVO_{current}_{arch}-setup.exe")
        };
        let url = format!("{}/v{current}/{name}", lock(&self.base));
        let signature = self
            .http
            .get(format!("{url}.sig"))
            .send()
            .await
            .ok()
            .filter(|r| r.status().is_success())?
            .text()
            .await
            .ok()?;
        let path = folder.join(&name);
        match self.fetch_verified(&current, &url, &signature, &path).await {
            Ok(()) => Some(path),
            Err(e) => {
                tracing::warn!(
                    error = e,
                    "the running version's installer couldn't be kept"
                );
                None
            }
        }
    }

    fn installs_when_idle(&self) -> bool {
        self.when_idle.load(Ordering::SeqCst)
            || self.core.config().updates.install == UpdateInstall::WhenIdle
    }

    /// Installs the downloaded update now, or (`when_idle`) when KIVO and the PC are idle.
    pub async fn install(&self, when_idle: bool) -> Result<(), String> {
        if when_idle {
            self.when_idle.store(true, Ordering::SeqCst);
            let state = lock(&self.state).clone();
            if let UpdateState::Ready { version, notes, .. } = state {
                self.set_state(UpdateState::Ready {
                    version,
                    notes,
                    when_idle: true,
                });
            }
            return Ok(());
        }
        let _one = self.busy.lock().await;
        let ready = lock(&self.ready)
            .clone()
            .ok_or(text::t("updates.nothing"))?;
        // A rollback needs the version running now.
        let previous = self.keep_current().await;
        // Let a turn finish, then cancel what is left; the rest is saved as KIVO shuts down.
        let started = Instant::now();
        while self.core.state().borrow().session.is_active() && started.elapsed() < WIND_DOWN {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let kind = lock(&self.kind).clone();
        let pending = Pending {
            from: self.version(),
            to: ready.version.clone(),
            installer: ready.path.clone(),
            previous,
            kind: kind.clone(),
            notes: ready.notes.clone(),
            at: kivo_store::brains::now_ms(),
            ..Pending::default()
        };
        record::save_pending(&self.dir, &pending).map_err(|e| e.to_string())?;
        let (program, args) = record::installer_args(&kind, &ready.path);
        if let Err(e) = self.launch.launch(&program, &args) {
            let _ = std::fs::remove_file(self.dir.join(record::PENDING));
            let message = text::tf("updates.launchFailed", &[("error", &e)]);
            self.set_state(UpdateState::Failed {
                message: message.clone(),
            });
            return Err(message);
        }
        tracing::info!(to = ready.version, "installing an update; KIVO restarts");
        self.set_state(UpdateState::Installing {
            version: ready.version,
        });
        self.core.quit();
        Ok(())
    }

    /// The Control Center's update requests; `None` for other methods.
    pub async fn call(
        &self,
        name: &str,
        params: serde_json::Value,
    ) -> Option<Result<serde_json::Value, kivo_ipc::RpcError>> {
        use kivo_ipc::{RpcError, method};
        let refuse = |message: String| RpcError::new(RpcError::REFUSED, message);
        let view = |me: &Self| {
            serde_json::to_value(me.status())
                .map_err(|e| RpcError::new(RpcError::INTERNAL, e.to_string()))
        };
        Some(match name {
            method::UPDATES_STATUS => view(self),
            method::UPDATES_CHECK => match self.check().await {
                Ok(Some(_)) => match self.download().await {
                    Ok(_) => view(self),
                    Err(e) => Err(refuse(e)),
                },
                Ok(None) => view(self),
                Err(e) => Err(refuse(e)),
            },
            method::UPDATES_INSTALL => {
                #[derive(Deserialize, Default)]
                #[serde(default, rename_all = "camelCase", deny_unknown_fields)]
                struct Params {
                    when_idle: bool,
                }
                let p: Params = if params.is_null() {
                    Params::default()
                } else {
                    match serde_json::from_value(params) {
                        Ok(p) => p,
                        Err(e) => return Some(Err(RpcError::invalid_params(e))),
                    }
                };
                self.install(p.when_idle)
                    .await
                    .map(|()| serde_json::Value::Null)
                    .map_err(refuse)
            }
            method::UPDATES_SEEN => {
                self.seen();
                Ok(serde_json::Value::Null)
            }
            _ => return None,
        })
    }

    /// A notification button (Install now / When idle).
    pub fn answered(self: &Arc<Self>, action: &str) {
        let when_idle = action == WHEN_IDLE;
        let me = Arc::clone(self);
        tokio::spawn(async move {
            if let Err(e) = me.install(when_idle).await {
                tracing::warn!(error = e, "update not installed");
            }
        });
    }

    fn may_check(&self) -> bool {
        let config = self.core.config();
        self.enabled() && config.updates.check && config.privacy.mode != PrivacyMode::StrictPrivate
    }

    /// Tells the user an update is ready (a notification with Install now / When idle).
    fn announce(&self, version: &str) {
        if !self.core.config().automation.toasts {
            return;
        }
        let action = |id: &str, key: &str| NotificationAction {
            id: id.into(),
            label: text::t(key),
        };
        let _ = self.notifications.show(&Notification {
            title: text::tf("updates.readyTitle", &[("version", &version)]),
            body: text::t("updates.readyBody"),
            actions: vec![
                action(INSTALL_NOW, "updates.installNow"),
                action(WHEN_IDLE, "updates.whenIdle"),
            ],
            reply: false,
            silent: !self.core.config().automation.notification_sound,
        });
    }

    fn idle_enough(&self) -> bool {
        let state = self.core.state().borrow().clone();
        !state.session.is_active()
            && state.tasks_active == 0
            && self.system.presence().idle_seconds >= IDLE_FOR
    }

    /// Checks once a day when allowed, downloads what it finds, says so, and installs at idle
    /// when the user chose that.
    pub async fn run(self: Arc<Self>) {
        let mut announced: Option<String> = None;
        loop {
            let due = lock(&self.last_check).is_none_or(|t| t.elapsed() >= CHECK_EVERY);
            if due && self.may_check() && matches!(self.check().await, Ok(Some(_))) {
                let _ = self.download().await;
            }
            let ready = lock(&self.ready).clone();
            if let Some(ready) = ready {
                if announced.as_deref() != Some(&ready.version) && !self.installs_when_idle() {
                    self.announce(&ready.version);
                    announced = Some(ready.version.clone());
                }
                if self.installs_when_idle()
                    && self.idle_enough()
                    && let Err(e) = self.install(false).await
                {
                    tracing::warn!(error = e, "update not installed at idle");
                }
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }

    /// After a restart: counts this start, and marks the update healthy once the runtime has run
    /// for a moment ("What's new" then shows once).
    pub fn watch_health(self: &Arc<Self>) {
        let version = self.version();
        if record::runtime_started(&self.dir, &version).is_none() {
            return;
        }
        let me = Arc::clone(self);
        tokio::spawn(async move {
            tokio::time::sleep(HEALTHY_AFTER).await;
            if record::healthy(&me.dir, &version).is_some() {
                tracing::info!(version, "the update runs");
                me.set_state(UpdateState::Idle);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use kivo_brain::testing::{MockServer, Reply};
    use kivo_testkit::{FakeNotifications, FakeSystemInfo};
    use serde_json::json;

    #[test]
    fn installer_names_come_from_the_address() {
        assert_eq!(
            file_name("https://x/releases/download/v1.2.0/KIVO_1.2.0_x64-setup.exe"),
            "KIVO_1.2.0_x64-setup.exe"
        );
        assert_eq!(file_name("https://x/v1/KIVO%201.2.0.msi"), "KIVO 1.2.0.msi");
        assert_eq!(file_name("https://x/v1/..%5Cevil.exe"), ".._evil.exe");
    }

    /// Signs like `tauri signer sign`: base64 of minisign's key and signature files.
    struct Signer {
        pk: minisign::PublicKey,
        sk: minisign::SecretKey,
    }

    impl Signer {
        fn new() -> Self {
            let pair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
            Self {
                pk: pair.pk,
                sk: pair.sk,
            }
        }
        fn public(&self) -> String {
            base64::engine::general_purpose::STANDARD.encode(self.pk.to_box().unwrap().to_string())
        }
        fn sign(&self, bytes: &[u8]) -> String {
            let sig = minisign::sign(
                Some(&self.pk),
                &self.sk,
                std::io::Cursor::new(bytes),
                None,
                None,
            )
            .unwrap();
            base64::engine::general_purpose::STANDARD.encode(sig.to_string())
        }
    }

    struct Trust(Mutex<Option<String>>);
    impl CodeTrust for Trust {
        fn signer(&self, path: &Path) -> kivo_platform::PlatformResult<Option<String>> {
            assert!(path.is_file(), "checked on disk");
            Ok(lock(&self.0).clone())
        }
    }

    #[derive(Default)]
    struct Launched(Mutex<Vec<(PathBuf, Vec<String>)>>);
    impl Launch for Launched {
        fn launch(&self, program: &Path, args: &[String]) -> Result<(), String> {
            lock(&self.0).push((program.to_path_buf(), args.to_vec()));
            Ok(())
        }
    }

    struct Rig {
        updater: Arc<Updater>,
        core: Arc<Core>,
        launched: Arc<Launched>,
        trust: Arc<Trust>,
        dir: tempfile::TempDir,
        server: MockServer,
        files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    }

    async fn rig(signer: &Signer, publisher: Option<&str>) -> Rig {
        let files: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::default();
        let server = {
            let files = Arc::clone(&files);
            MockServer::start(move |r| match lock(&files).get(&r.path) {
                Some(body) => Reply {
                    status: 200,
                    headers: vec![("content-type".into(), "application/octet-stream".into())],
                    chunks: vec![body.clone()],
                    pause: Duration::ZERO,
                    hang: false,
                },
                None => Reply::json(404, &json!({ "error": "not found" })),
            })
            .await
        };
        let dir = tempfile::tempdir().unwrap();
        let core = Arc::new(Core::default());
        let launched = Arc::new(Launched::default());
        let trust = Arc::new(Trust(Mutex::new(publisher.map(str::to_owned))));
        let updater = Updater::new(Parts {
            core: Arc::clone(&core),
            dir: dir.path().to_path_buf(),
            keys: Keys {
                updater: Some(signer.public()),
                publisher: Some("KIVO".into()),
            },
            trust: trust.clone(),
            launch: launched.clone(),
            notifications: Arc::new(FakeNotifications::default()),
            system: Arc::new(FakeSystemInfo::default()),
        });
        updater.set_base(&server.url);
        updater.set_version("0.1.0", "nsis");
        Rig {
            updater,
            core,
            launched,
            trust,
            dir,
            server,
            files,
        }
    }

    impl Rig {
        fn serve(&self, path: &str, body: Vec<u8>) {
            lock(&self.files).insert(path.to_owned(), body);
        }
        fn url(&self, path: &str) -> String {
            format!("{}{path}", self.server.url)
        }
    }

    fn manifest(url: &str, version: &str, signature: &str, key: &str) -> Vec<u8> {
        json!({
            "version": version,
            "notes": "- Say \"approve\" when KIVO asks\n- Faster wake word",
            "pub_date": "2026-10-01T09:30:00Z",
            "platforms": { key: { "signature": signature, "url": url } }
        })
        .to_string()
        .into_bytes()
    }

    #[tokio::test]
    async fn an_update_is_found_verified_installed_and_proves_itself() {
        let signer = Signer::new();
        let new = b"MZ KIVO 0.2.0 installer".to_vec();
        let old = b"MZ KIVO 0.1.0 installer".to_vec();
        let r = rig(&signer, Some("KIVO")).await;
        let installer = "/v0.2.0/KIVO_0.2.0_x64-setup.exe";
        r.serve(installer, new.clone());
        r.serve("/v0.1.0/KIVO_0.1.0_x64-setup.exe", old.clone());
        r.serve(
            "/v0.1.0/KIVO_0.1.0_x64-setup.exe.sig",
            signer.sign(&old).into_bytes(),
        );
        r.serve(
            "/updates/stable.json",
            manifest(
                &r.url(installer),
                "0.2.0",
                &signer.sign(&new),
                "windows-x86_64-nsis",
            ),
        );

        assert_eq!(r.updater.check().await.unwrap().as_deref(), Some("0.2.0"));
        let path = r.updater.download().await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), new);
        assert!(matches!(
            r.updater.status().state,
            UpdateState::Ready { ref version, .. } if version == "0.2.0"
        ));

        r.updater.install(false).await.unwrap();
        let launched = lock(&r.launched.0).clone();
        assert_eq!(
            launched,
            [(
                path.clone(),
                vec!["/P".to_owned(), "/R".into(), "/UPDATE".into()]
            )]
        );
        assert!(
            r.core.shutdown().is_cancelled(),
            "KIVO quits for the installer"
        );
        let pending = record::pending(r.dir.path()).expect("the pending update");
        assert_eq!(
            (pending.from.as_str(), pending.to.as_str()),
            ("0.1.0", "0.2.0")
        );
        // The running version's installer was fetched, verified and kept for a rollback.
        let previous = pending.previous.expect("a rollback installer");
        assert_eq!(std::fs::read(previous).unwrap(), old);

        // After the restart: the new version counts its start, proves itself, says what's new.
        assert!(record::runtime_started(r.dir.path(), "0.2.0").is_some());
        record::healthy(r.dir.path(), "0.2.0").unwrap();
        r.updater.set_version("0.2.0", "nsis");
        let whats_new = r.updater.status().whats_new.expect("what's new");
        assert!(whats_new.notes.contains("Faster wake word"));
        assert_eq!(whats_new.from, "0.1.0");
        r.updater.seen();
        assert!(r.updater.status().whats_new.is_none(), "shown once");
    }

    #[tokio::test]
    async fn a_tampered_or_foreign_installer_is_refused_and_deleted() {
        let signer = Signer::new();
        let good = b"MZ good".to_vec();
        let installer = "/v0.2.0/KIVO_0.2.0_x64-setup.exe";

        // The file doesn't match the signature in the manifest.
        let r = rig(&signer, Some("KIVO")).await;
        r.serve(installer, b"MZ tampered".to_vec());
        r.serve(
            "/updates/stable.json",
            manifest(
                &r.url(installer),
                "0.2.0",
                &signer.sign(&good),
                "windows-x86_64",
            ),
        );
        r.updater.check().await.unwrap();
        assert_eq!(
            r.updater.download().await.unwrap_err(),
            text::t("updates.badSignature")
        );
        assert!(
            std::fs::read_dir(r.dir.path().join("0.2.0"))
                .unwrap()
                .next()
                .is_none(),
            "nothing is kept"
        );
        assert!(matches!(
            r.updater.status().state,
            UpdateState::Failed { .. }
        ));

        // Signed with the update key, but the installer's publisher is someone else.
        let r = rig(&signer, Some("Contoso")).await;
        r.serve(installer, good.clone());
        r.serve(
            "/updates/stable.json",
            manifest(
                &r.url(installer),
                "0.2.0",
                &signer.sign(&good),
                "windows-x86_64",
            ),
        );
        r.updater.check().await.unwrap();
        assert_eq!(
            r.updater.download().await.unwrap_err(),
            text::t("updates.badPublisher")
        );
        assert!(
            std::fs::read_dir(r.dir.path().join("0.2.0"))
                .unwrap()
                .next()
                .is_none()
        );
        // KIVO's publisher: accepted.
        *lock(&r.trust.0) = Some("KIVO".into());
        assert!(r.updater.download().await.is_ok());

        // A different update key signed it: refused.
        let stranger = Signer::new();
        let r = rig(&signer, Some("KIVO")).await;
        r.serve(installer, good.clone());
        r.serve(
            "/updates/stable.json",
            manifest(
                &r.url(installer),
                "0.2.0",
                &stranger.sign(&good),
                "windows-x86_64",
            ),
        );
        r.updater.check().await.unwrap();
        assert_eq!(
            r.updater.download().await.unwrap_err(),
            text::t("updates.badSignature")
        );
    }

    #[tokio::test]
    async fn the_channel_and_install_kind_choose_the_manifest_and_installer() {
        let signer = Signer::new();
        let r = rig(&signer, Some("KIVO")).await;
        r.serve(
            "/updates/beta.json",
            manifest(
                "http://x/v0.3.0-beta.2/KIVO.msi",
                "0.3.0-beta.2",
                "c2ln",
                "windows-x86_64-msi",
            ),
        );
        r.serve(
            "/updates/stable.json",
            manifest("http://x/a.exe", "0.3.0-beta.1", "c2ln", "windows-x86_64"),
        );
        // A beta build follows Beta; the MSI install takes the MSI.
        r.updater.set_version("0.3.0-beta.1", "msi");
        assert_eq!(
            r.updater.check().await.unwrap().as_deref(),
            Some("0.3.0-beta.2")
        );
        assert!(
            lock(&r.updater.offer)
                .as_ref()
                .unwrap()
                .asset
                .url
                .ends_with("KIVO.msi")
        );
        // An NSIS install finds no installer of its kind there: up to date.
        r.updater.set_version("0.3.0-beta.1", "nsis");
        assert_eq!(r.updater.check().await.unwrap(), None);
        // Chosen Stable: the stable manifest, which isn't newer.
        r.core
            .update_config(|c| c.updates.channel = Some(UpdateChannel::Stable));
        assert_eq!(r.updater.check().await.unwrap(), None);
        assert!(matches!(
            r.updater.status().state,
            UpdateState::UpToDate { .. }
        ));
        assert_eq!(r.updater.status().channel, UpdateChannel::Stable);
        let requested: Vec<String> = r.server.requests().into_iter().map(|q| q.path).collect();
        assert_eq!(
            requested,
            [
                "/updates/beta.json",
                "/updates/beta.json",
                "/updates/stable.json"
            ]
        );
        // "When idle" marks it; nothing runs yet.
        r.updater.install(true).await.unwrap();
        assert!(r.updater.installs_when_idle());
        assert!(lock(&r.launched.0).is_empty());
    }

    #[tokio::test]
    async fn a_build_without_the_update_key_cant_update() {
        let signer = Signer::new();
        let r = rig(&signer, Some("KIVO")).await;
        let plain = Updater::new(Parts {
            core: Arc::clone(&r.core),
            dir: r.dir.path().to_path_buf(),
            keys: Keys::default(),
            trust: r.trust.clone(),
            launch: r.launched.clone(),
            notifications: Arc::new(FakeNotifications::default()),
            system: Arc::new(FakeSystemInfo::default()),
        });
        assert!(!plain.enabled());
        assert_eq!(
            plain.check().await.unwrap_err(),
            text::t("updates.notThisBuild")
        );
        assert!(r.server.requests().is_empty(), "nothing was fetched");
    }
}
