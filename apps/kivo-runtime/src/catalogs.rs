//! KIVO's catalogs (DISCOVERY §3, DISC-17): the CLI install catalog, the connector directory, the
//! plugin index and the model catalog. Each ships inside KIVO; a newer one may be fetched once a
//! day from KIVO's own repository and is used only if it is signed with one of KIVO's catalog keys
//! (ed25519; the public keys are built in from `catalogs/keys.txt`, the private key never leaves
//! the owner's machine: `pnpm catalog:sign`). A verified catalog is cached, so KIVO works offline,
//! and an older version never replaces a newer one (no rollback). Fetching sends nothing but the
//! request, is off in Strictly private, and can be turned off (Settings → Privacy).

use base64::Engine as _;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// The catalogs KIVO knows.
pub const KINDS: [&str; 4] = ["cli", "connectors", "plugins", "models"];
/// Where newer catalogs are published: `<kind>.json` and its signature `<kind>.json.sig`.
pub const BASE_URL: &str = "https://raw.githubusercontent.com/MadBlast0/K.I.V.O/main/catalogs";
/// The public keys that may sign catalogs, one base64 key per line (`#` comments).
const KEYS: &str = include_str!("../../../catalogs/keys.txt");
/// A catalog bigger than this isn't one.
const MAX_BYTES: usize = 2 * 1024 * 1024;
/// How often KIVO looks for newer catalogs, and how long after starting it first does.
const EVERY: Duration = Duration::from_secs(24 * 60 * 60);
const FIRST_AFTER: Duration = Duration::from_secs(5 * 60);

/// The built-in keys.
pub fn keys() -> Vec<VerifyingKey> {
    parse_keys(KEYS)
}

pub fn parse_keys(text: &str) -> Vec<VerifyingKey> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| base64::engine::general_purpose::STANDARD.decode(l).ok())
        .filter_map(|b| <[u8; 32]>::try_from(b.as_slice()).ok())
        .filter_map(|b| VerifyingKey::from_bytes(&b).ok())
        .collect()
}

/// Whether `signature` (base64) is one of `keys` signing exactly `body`.
pub fn verify(body: &[u8], signature: &str, keys: &[VerifyingKey]) -> bool {
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(signature.trim()) else {
        return false;
    };
    let Ok(sig) = Signature::from_slice(&bytes) else {
        return false;
    };
    keys.iter().any(|k| k.verify(body, &sig).is_ok())
}

/// Why a fetched catalog wasn't taken.
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    Fetch(String),
    TooBig,
    BadSignature,
    NotACatalog,
    /// Older than (or the same as) the one KIVO has.
    NotNewer,
}

/// Reads one URL (a GET with nothing else sent).
pub type Fetch = Arc<dyn Fn(&str) -> Result<Vec<u8>, String> + Send + Sync>;

pub struct Catalogs {
    dir: PathBuf,
    keys: Vec<VerifyingKey>,
    fetch: Fetch,
}

impl Catalogs {
    pub fn new(dir: PathBuf, keys: Vec<VerifyingKey>, fetch: Fetch) -> Self {
        Self { dir, keys, fetch }
    }

    fn cached(&self, kind: &str) -> (PathBuf, PathBuf) {
        (
            self.dir.join(format!("{kind}.json")),
            self.dir.join(format!("{kind}.json.sig")),
        )
    }

    /// The newest verified catalog of `kind` on this PC (checked again when read, so a file
    /// changed on disk isn't trusted), else `None`: the built-in one applies.
    pub fn get(&self, kind: &str) -> Option<Value> {
        let (body, sig) = self.cached(kind);
        let bytes = std::fs::read(body).ok()?;
        let signature = std::fs::read_to_string(sig).ok()?;
        verify(&bytes, &signature, &self.keys)
            .then(|| serde_json::from_slice::<Value>(&bytes).ok())
            .flatten()
            .filter(|v| v["kind"] == kind)
    }

    fn version_of(doc: Option<&Value>) -> u64 {
        doc.and_then(|v| v["version"].as_u64()).unwrap_or(0)
    }

    /// Fetches `kind` and keeps it if it's signed, well-formed and newer. Returns its version.
    pub fn refresh(&self, kind: &str) -> Result<u64, Refused> {
        let url = format!("{BASE_URL}/{kind}.json");
        let body = (self.fetch)(&url).map_err(Refused::Fetch)?;
        if body.len() > MAX_BYTES {
            return Err(Refused::TooBig);
        }
        let signature = (self.fetch)(&format!("{url}.sig")).map_err(Refused::Fetch)?;
        let signature = String::from_utf8_lossy(&signature).into_owned();
        if !verify(&body, &signature, &self.keys) {
            return Err(Refused::BadSignature);
        }
        let doc: Value = serde_json::from_slice(&body).map_err(|_| Refused::NotACatalog)?;
        if doc["kind"] != kind || doc["version"].as_u64().is_none() {
            return Err(Refused::NotACatalog);
        }
        let version = Self::version_of(Some(&doc));
        if version <= Self::version_of(self.get(kind).as_ref()) {
            return Err(Refused::NotNewer);
        }
        std::fs::create_dir_all(&self.dir).map_err(|e| Refused::Fetch(e.to_string()))?;
        let (body_file, sig_file) = self.cached(kind);
        write_atomic(&body_file, &body).map_err(Refused::Fetch)?;
        write_atomic(&sig_file, signature.as_bytes()).map_err(Refused::Fetch)?;
        Ok(version)
    }

    /// Every catalog, once a day while `allowed()` says so.
    pub async fn run_daily(
        self: Arc<Self>,
        allowed: Arc<dyn Fn() -> bool + Send + Sync>,
        shutdown: tokio_util::sync::CancellationToken,
    ) {
        let mut wait = FIRST_AFTER;
        loop {
            tokio::select! {
                () = tokio::time::sleep(wait) => {}
                () = shutdown.cancelled() => return,
            }
            wait = EVERY;
            if !allowed() || self.keys.is_empty() {
                continue;
            }
            let me = Arc::clone(&self);
            let _ = tokio::task::spawn_blocking(move || {
                for kind in KINDS {
                    match me.refresh(kind) {
                        Ok(v) => tracing::info!(kind, version = v, "a newer catalog"),
                        Err(Refused::NotNewer) => {}
                        Err(e) => tracing::debug!(kind, ?e, "catalog not updated"),
                    }
                }
            })
            .await;
        }
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// The CLI install catalog's commands for `id`, when a verified newer catalog has them:
/// (install, adapter install).
pub fn cli_commands(catalog: Option<&Value>, id: &str) -> Option<(String, String)> {
    let entry = catalog?["entries"]
        .as_array()?
        .iter()
        .find(|e| e["id"] == id)?;
    Some((
        entry["install"].as_str()?.to_owned(),
        entry["adapterInstall"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::collections::HashMap;
    use std::sync::Mutex;

    fn signed(key: &SigningKey, doc: &Value) -> (Vec<u8>, String) {
        let body = serde_json::to_vec(doc).unwrap();
        let sig = base64::engine::general_purpose::STANDARD.encode(key.sign(&body).to_bytes());
        (body, sig)
    }

    type Files = Arc<Mutex<HashMap<String, Vec<u8>>>>;

    fn server(files: HashMap<String, Vec<u8>>) -> (Fetch, Files) {
        let files = Arc::new(Mutex::new(files));
        let f = Arc::clone(&files);
        (
            Arc::new(move |url: &str| {
                f.lock()
                    .unwrap()
                    .get(url)
                    .cloned()
                    .ok_or_else(|| "404".to_owned())
            }),
            files,
        )
    }

    #[test]
    fn only_signed_newer_catalogs_are_kept_and_they_work_offline() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let other = SigningKey::from_bytes(&[9u8; 32]);
        let keys_text = format!(
            "# KIVO catalog keys\n{}\n",
            base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes())
        );
        let keys = parse_keys(&keys_text);
        assert_eq!(keys.len(), 1);
        let url = format!("{BASE_URL}/cli.json");
        let v2 = serde_json::json!({ "kind": "cli", "version": 2, "entries": [
            { "id": "codex", "install": "npm install -g @openai/codex@next", "adapterInstall": "" }
        ]});
        let (body, sig) = signed(&key, &v2);
        let (fetch, files) = server(HashMap::from([
            (url.clone(), body.clone()),
            (format!("{url}.sig"), sig.clone().into_bytes()),
        ]));
        let dir = tempfile::tempdir().unwrap();
        let catalogs = Catalogs::new(dir.path().to_path_buf(), keys, fetch);
        assert_eq!(catalogs.get("cli"), None, "only the built-in one at first");
        assert_eq!(catalogs.refresh("cli"), Ok(2));
        let got = catalogs.get("cli").unwrap();
        assert_eq!(
            cli_commands(Some(&got), "codex"),
            Some(("npm install -g @openai/codex@next".into(), String::new()))
        );
        // The same again, or an older one: kept as is (no rollback).
        assert_eq!(catalogs.refresh("cli"), Err(Refused::NotNewer));
        let v1 = serde_json::json!({ "kind": "cli", "version": 1, "entries": [] });
        let (old, old_sig) = signed(&key, &v1);
        files.lock().unwrap().insert(url.clone(), old);
        files
            .lock()
            .unwrap()
            .insert(format!("{url}.sig"), old_sig.into_bytes());
        assert_eq!(catalogs.refresh("cli"), Err(Refused::NotNewer));
        // Signed by someone else, or changed after signing: refused.
        let v3 = serde_json::json!({ "kind": "cli", "version": 3, "entries": [] });
        let (forged, forged_sig) = signed(&other, &v3);
        files.lock().unwrap().insert(url.clone(), forged);
        files
            .lock()
            .unwrap()
            .insert(format!("{url}.sig"), forged_sig.into_bytes());
        assert_eq!(catalogs.refresh("cli"), Err(Refused::BadSignature));
        let (mut tampered, good_sig) = signed(&key, &v3);
        tampered.extend_from_slice(b" ");
        files.lock().unwrap().insert(url.clone(), tampered);
        files
            .lock()
            .unwrap()
            .insert(format!("{url}.sig"), good_sig.into_bytes());
        assert_eq!(catalogs.refresh("cli"), Err(Refused::BadSignature));
        // Offline: the cached, verified one still applies; edited on disk, it doesn't.
        files.lock().unwrap().clear();
        assert!(matches!(catalogs.refresh("cli"), Err(Refused::Fetch(_))));
        assert_eq!(catalogs.get("cli").unwrap()["version"], 2);
        std::fs::write(
            dir.path().join("cli.json"),
            b"{\"kind\":\"cli\",\"version\":99}",
        )
        .unwrap();
        assert_eq!(catalogs.get("cli"), None);
    }

    #[test]
    fn a_catalog_of_the_wrong_kind_is_refused() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let url = format!("{BASE_URL}/models.json");
        let (body, sig) = signed(&key, &serde_json::json!({ "kind": "cli", "version": 5 }));
        let (fetch, _) = server(HashMap::from([
            (url.clone(), body),
            (format!("{url}.sig"), sig.into_bytes()),
        ]));
        let dir = tempfile::tempdir().unwrap();
        let catalogs = Catalogs::new(dir.path().to_path_buf(), vec![key.verifying_key()], fetch);
        assert_eq!(catalogs.refresh("models"), Err(Refused::NotACatalog));
    }

    #[test]
    fn the_built_in_keys_parse() {
        // Until the owner makes the catalog key (`pnpm catalog:sign keygen`), there are none,
        // and no fetched catalog is ever trusted.
        let _ = keys();
    }
}
