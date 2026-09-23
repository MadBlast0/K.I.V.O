//! The Memory page's requests (UX-29, MEM-06, MEM-10): the vault's notes, tags and folders,
//! reading and editing a note, remembering, suggestions, Forget everything, export, the tidy job,
//! opening the vault in Explorer or Obsidian, and "Why did you say that?".

use crate::memory::{Memory, Remembered};
use kivo_core::text;
use kivo_ipc::RpcError;
use kivo_ipc::protocol::method;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

/// Opens a folder in Explorer, or a URI (`obsidian://…`) with its app.
pub type Open = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

pub struct MemoryRpc {
    pub memory: Arc<Memory>,
    /// Minutes from UTC, for the export's date.
    pub utc_offset: i32,
    pub recorder: crate::activity::Recorder,
    /// Where exports go (the Downloads folder).
    pub exports: PathBuf,
    pub reveal: Open,
    pub open_uri: Open,
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RpcError> {
    serde_json::from_value(params).map_err(RpcError::invalid_params)
}

fn ok<T: serde::Serialize>(value: &T) -> Result<Value, RpcError> {
    serde_json::to_value(value).map_err(|e| RpcError::new(RpcError::INTERNAL, e.to_string()))
}

fn refuse(message: String) -> RpcError {
    RpcError::new(RpcError::REFUSED, message)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PathArg {
    path: String,
}

/// `obsidian://open?path=…` for a folder or note (percent-encoded).
pub fn obsidian_uri(path: &std::path::Path) -> String {
    let raw = path.display().to_string();
    let mut out = String::from("obsidian://open?path=");
    for b in raw.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

impl MemoryRpc {
    /// Handles `name` if it is one of these requests.
    #[allow(clippy::too_many_lines, reason = "one table of requests")]
    pub async fn call(&self, name: &str, params: Value) -> Option<Result<Value, RpcError>> {
        let memory = Arc::clone(&self.memory);
        // File work happens off the async threads.
        let blocking = |f: Box<dyn FnOnce() -> Result<Value, RpcError> + Send>| async move {
            tokio::task::spawn_blocking(f)
                .await
                .unwrap_or_else(|e| Err(RpcError::new(RpcError::INTERNAL, e.to_string())))
        };
        Some(match name {
            method::MEMORY_OVERVIEW => blocking(Box::new(move || ok(&memory.overview()))).await,
            method::MEMORY_NOTE => match parse::<PathArg>(params) {
                Ok(p) => {
                    blocking(Box::new(move || {
                        memory.note(&p.path).map_err(refuse).and_then(|n| ok(&n))
                    }))
                    .await
                }
                Err(e) => Err(e),
            },
            method::MEMORY_SAVE => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    path: String,
                    markdown: String,
                }
                match parse::<P>(params) {
                    Ok(p) => {
                        blocking(Box::new(move || {
                            memory
                                .save(&p.path, &p.markdown)
                                .map_err(refuse)
                                .map(|()| Value::Null)
                        }))
                        .await
                    }
                    Err(e) => Err(e),
                }
            }
            method::MEMORY_REMEMBER => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    text: String,
                    #[serde(default)]
                    tags: Vec<String>,
                }
                match parse::<P>(params) {
                    Ok(p) => blocking(Box::new(move || {
                        match memory.remember(&p.text, &p.tags, Some("user"), false) {
                            Ok(Remembered::New { path, superseded }) => Ok(
                                json!({ "path": path, "superseded": superseded, "already": false }),
                            ),
                            Ok(Remembered::Already { path }) => {
                                Ok(json!({ "path": path, "superseded": [], "already": true }))
                            }
                            Err(e) => Err(refuse(e)),
                        }
                    }))
                    .await,
                    Err(e) => Err(e),
                }
            }
            method::MEMORY_REMEMBER_TURN => {
                let turn = params["turnId"].as_str().unwrap_or_default().to_owned();
                blocking(Box::new(move || match memory.remember_turn(&turn) {
                    Ok(Remembered::New { path, .. } | Remembered::Already { path }) => {
                        Ok(json!({ "path": path }))
                    }
                    Err(e) => Err(refuse(e)),
                }))
                .await
            }
            method::MEMORY_META => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields, rename_all = "camelCase")]
                struct P {
                    path: String,
                    #[serde(default)]
                    tags: Option<Vec<String>>,
                    #[serde(default)]
                    sensitivity: Option<String>,
                    #[serde(default)]
                    share_cloud: Option<bool>,
                }
                match parse::<P>(params) {
                    Ok(p) => {
                        blocking(Box::new(move || {
                            memory
                                .set_meta(&p.path, p.tags, p.sensitivity, p.share_cloud)
                                .map_err(refuse)
                                .map(|()| Value::Null)
                        }))
                        .await
                    }
                    Err(e) => Err(e),
                }
            }
            method::MEMORY_DELETE => match parse::<PathArg>(params) {
                Ok(p) => {
                    blocking(Box::new(move || {
                        memory.delete(&p.path).map_err(refuse).map(|()| Value::Null)
                    }))
                    .await
                }
                Err(e) => Err(e),
            },
            method::MEMORY_FORGET => {
                let recorder = self.recorder.clone();
                blocking(Box::new(move || {
                    let n = memory.forget_everything();
                    // Forgetting is recorded (not what was forgotten).
                    recorder.user_action(
                        "memory.forget",
                        &text::tf("activity.memoryForgotten", &[("count", &n)]),
                    );
                    Ok(json!({ "forgotten": n }))
                }))
                .await
            }
            method::MEMORY_EXPORT => {
                let dir = self.exports.clone();
                let utc = self.utc_offset;
                blocking(Box::new(move || {
                    let date = kivo_memory::date::format(kivo_store::brains::now_ms(), utc);
                    let _ = std::fs::create_dir_all(&dir);
                    let file = (1..)
                        .map(|n| {
                            dir.join(if n < 2 {
                                format!("KIVO memory {date}.json")
                            } else {
                                format!("KIVO memory {date} ({n}).json")
                            })
                        })
                        .find(|f| !f.exists())
                        .unwrap_or_else(|| dir.join("KIVO memory.json"));
                    let body = serde_json::to_string_pretty(&memory.export())
                        .map_err(|e| RpcError::new(RpcError::INTERNAL, e.to_string()))?;
                    std::fs::write(&file, body).map_err(|e| refuse(e.to_string()))?;
                    Ok(json!({ "file": file.display().to_string() }))
                }))
                .await
            }
            method::MEMORY_SUGGESTION => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    id: i64,
                    accept: bool,
                    #[serde(default)]
                    text: Option<String>,
                }
                match parse::<P>(params) {
                    Ok(p) => blocking(Box::new(move || {
                        memory
                            .answer(p.id, p.accept, p.text.as_deref())
                            .map_err(refuse)
                            .map(|r| {
                                json!({ "path": match r {
                                    Some(Remembered::New { path, .. } | Remembered::Already { path }) => Some(path),
                                    None => None,
                                } })
                            })
                    }))
                    .await,
                    Err(e) => Err(e),
                }
            }
            method::MEMORY_TIDY => {
                let recorder = self.recorder.clone();
                blocking(Box::new(move || {
                    let report = memory.tidy();
                    if !report.is_empty() {
                        recorder.background("memory", &report.summary(), None);
                    }
                    Ok(json!({
                        "merged": report.merged,
                        "superseded": report.superseded,
                        "condensedLogs": report.condensed_logs,
                        "trimmed": report.trimmed,
                        "secretsRemoved": report.secrets_removed,
                    }))
                }))
                .await
            }
            method::MEMORY_OPEN => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    r#in: String,
                    #[serde(default)]
                    path: Option<String>,
                }
                match parse::<P>(params) {
                    Ok(p) => {
                        let root = self.memory.root().to_path_buf();
                        // A note inside the vault, or the vault itself.
                        let target = match p.path.as_deref() {
                            Some(rel) => match kivo_memory::Vault::new(&root).file(rel) {
                                Ok(f) => f,
                                Err(e) => return Some(Err(refuse(e.to_string()))),
                            },
                            None => root.clone(),
                        };
                        let result = if p.r#in == "obsidian" {
                            (self.open_uri)(&obsidian_uri(&target))
                        } else {
                            // Explorer opens folders; for a note, its folder.
                            let folder = if target.is_dir() {
                                target
                            } else {
                                target.parent().map_or(root, std::path::Path::to_path_buf)
                            };
                            (self.reveal)(&folder.display().to_string())
                        };
                        result.map(|()| Value::Null).map_err(refuse)
                    }
                    Err(e) => Err(e),
                }
            }
            method::MEMORY_WHY => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    turn: String,
                }
                parse::<P>(params).map(|p| {
                    let used: Vec<Value> = self
                        .memory
                        .used_by(&p.turn)
                        .into_iter()
                        .map(|(path, title)| {
                            json!({ "path": path, "title": title, "exists": self.memory_exists(&path) })
                        })
                        .collect();
                    Value::Array(used)
                })
            }
            _ => return None,
        })
    }

    fn memory_exists(&self, path: &str) -> bool {
        kivo_memory::Vault::new(self.memory.root()).exists(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obsidian_links_are_encoded() {
        assert_eq!(
            obsidian_uri(std::path::Path::new(
                "C:\\Users\\Sam\\AppData\\Roaming\\KIVO\\memory"
            )),
            "obsidian://open?path=C%3A%5CUsers%5CSam%5CAppData%5CRoaming%5CKIVO%5Cmemory"
        );
    }
}
