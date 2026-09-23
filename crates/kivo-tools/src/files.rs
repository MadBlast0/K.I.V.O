//! File tools (TOOL-16): search (the Windows Search index, else a bounded walk), read, open,
//! reveal, create, rename, move, copy and delete. Delete always goes to the Recycle Bin.
//!
//! Every path passes the guard: absolute, no `..`, no device or network namespaces, no alternate
//! data streams. Paths in protected places (the OS, program files, app data, the profile root, a
//! drive root) and operations over more than 20 files are High risk (SECURITY §3).

use crate::builtin::{Def, object, platform_error};
use crate::controls::{Builder, Controls, missing, str_arg};
use crate::registry::{Output, Tool};
use kivo_core::capability::Capability;
use kivo_core::text;
use kivo_core::tool::{
    CapabilityTier, Initiator, Reversibility, Risk, SideEffect, Target, ToolError, ToolErrorCode,
};
use kivo_platform::{FileHit, PlatformError};
use serde_json::{Value, json};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

/// More files than this in one operation is a bulk operation: High risk.
pub const BULK: usize = 20;
/// The fallback walk's limits.
const WALK_ENTRIES: usize = 50_000;
const WALK_DEPTH: usize = 10;
/// Longest text returned by `files.read`.
const READ_CHARS: usize = 12_000;

fn def(
    id: &'static str,
    description: &'static str,
    params: Value,
    risk: Risk,
    effects: &'static [SideEffect],
    capability: Capability,
    reversibility: Reversibility,
) -> Def {
    Def {
        id,
        description,
        params,
        risk,
        effects,
        capability,
        reversibility,
        egress: false,
        timeout_ms: 30_000,
        tier: CapabilityTier::OsApi,
    }
}

fn refused(key: &str) -> ToolError {
    ToolError::new(ToolErrorCode::InvalidArgs, text::t(key))
}

/// The path guard: an absolute, local, plain path, normalized.
pub fn safe_path(raw: &str) -> Result<PathBuf, ToolError> {
    let raw = raw.trim().trim_matches('"');
    if raw.is_empty() || raw.contains('\0') {
        return Err(missing("path"));
    }
    // Device (\\?\, \\.\) and network (\\server) namespaces, and forward-slash variants.
    #[cfg(windows)]
    let unified = raw.replace('/', "\\");
    #[cfg(not(windows))]
    let unified = raw.to_owned();
    if unified.starts_with("\\\\") || unified.starts_with("//") {
        return Err(refused("error.files.network"));
    }
    let path = PathBuf::from(&unified);
    if !path.is_absolute() {
        return Err(refused("error.files.relative"));
    }
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => return Err(refused("error.files.traversal")),
            Component::CurDir => {}
            Component::Normal(part) => {
                // An alternate data stream (`file.txt:hidden`).
                if part.to_string_lossy().contains(':') {
                    return Err(refused("error.files.stream"));
                }
                out.push(part);
            }
            other => out.push(other.as_os_str()),
        }
    }
    Ok(out)
}

fn lower(p: &Path) -> String {
    p.to_string_lossy()
        .to_lowercase()
        .trim_end_matches(['\\', '/'])
        .to_owned()
}

/// Whether `path` is somewhere a change is High risk.
pub fn is_protected(path: &Path, roots: &[PathBuf], home: Option<&Path>) -> bool {
    let p = lower(path);
    // A drive root, or the folder holding every user's profile.
    if path.parent().is_none() || path.components().count() <= 1 {
        return true;
    }
    if p.ends_with(":\\users") || p.ends_with(":") {
        return true;
    }
    if home.is_some_and(|h| lower(h) == p) {
        return true;
    }
    roots.iter().any(|r| {
        let r = lower(r);
        !r.is_empty()
            && (p == r || p.starts_with(&format!("{r}\\")) || p.starts_with(&format!("{r}/")))
    })
}

fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// Every path a call names (`path`, `paths`, `to`), guarded.
fn paths_in(args: &Value) -> Vec<PathBuf> {
    let mut raw: Vec<&str> = Vec::new();
    for key in ["path", "to", "folder"] {
        if let Some(s) = args[key].as_str() {
            raw.push(s);
        }
    }
    if let Some(list) = args["paths"].as_array() {
        raw.extend(list.iter().filter_map(Value::as_str));
    }
    raw.into_iter().filter_map(|r| safe_path(r).ok()).collect()
}

/// SECURITY §3 for files: protected places or more than 20 files → High.
fn assess(spec_risk: Risk) -> Box<crate::controls::Assess> {
    Box::new(move |args: &Value, _: Initiator, c: &Controls| {
        let roots = c.files.protected_roots();
        let home = home();
        let paths = paths_in(args);
        let bulk = args["paths"].as_array().is_some_and(|a| a.len() > BULK);
        if bulk
            || paths
                .iter()
                .any(|p| is_protected(p, &roots, home.as_deref()))
        {
            Risk::High
        } else {
            spec_risk
        }
    })
}

fn targets(args: &Value, _: &Controls) -> Vec<Target> {
    paths_in(args)
        .into_iter()
        .map(|p| Target::Window {
            title: p.display().to_string(),
            app_id: String::new(),
        })
        .take(5)
        .collect()
}

fn io(e: &std::io::Error, path: &Path) -> ToolError {
    let code = match e.kind() {
        std::io::ErrorKind::NotFound => ToolErrorCode::NotFound,
        std::io::ErrorKind::PermissionDenied => ToolErrorCode::AccessDenied,
        std::io::ErrorKind::AlreadyExists => ToolErrorCode::InvalidArgs,
        _ => ToolErrorCode::Failed,
    };
    let message = match code {
        ToolErrorCode::NotFound => text::tf("error.notFound", &[("what", &name(path))]),
        ToolErrorCode::AccessDenied => text::t("error.accessDenied"),
        ToolErrorCode::InvalidArgs => text::tf("error.files.exists", &[("name", &name(path))]),
        _ => text::t("error.failed"),
    };
    ToolError::new(code, message).with_detail(e.to_string())
}

fn name(p: &Path) -> String {
    p.file_name().map_or_else(
        || p.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

fn under(path: &Path, root: &Path) -> bool {
    let (p, r) = (lower(path), lower(root));
    !r.is_empty() && (p == r || p.starts_with(&format!("{r}\\")) || p.starts_with(&format!("{r}/")))
}

/// The folders file tools may use: the user's list, else the user's own folders and the
/// workspace (CAPABILITIES §1 "Allowed folders").
fn allowed_roots(c: &Controls) -> Vec<PathBuf> {
    let options = c.options();
    if options.allowed_folders.is_empty() {
        let mut roots = c.files.user_folders();
        roots.push(c.shell_home.clone());
        roots
    } else {
        options
            .allowed_folders
            .iter()
            .filter_map(|f| safe_path(f).ok())
            .collect()
    }
}

/// A guarded path inside the allowed folders and outside the private ones.
fn scoped(c: &Controls, path: PathBuf) -> Result<PathBuf, ToolError> {
    let options = c.options();
    if options
        .private_folders
        .iter()
        .filter_map(|f| safe_path(f).ok())
        .any(|private| under(&path, &private))
    {
        return Err(ToolError::new(
            ToolErrorCode::AccessDenied,
            text::t("error.files.private"),
        ));
    }
    if !allowed_roots(c).iter().any(|root| under(&path, root)) {
        return Err(ToolError::new(
            ToolErrorCode::AccessDenied,
            text::t("error.files.outside"),
        ));
    }
    Ok(path)
}

fn path_arg(args: &Value, key: &str, c: &Controls) -> Result<PathBuf, ToolError> {
    scoped(c, safe_path(str_arg(args, key)?)?)
}

fn paths_arg(args: &Value, c: &Controls) -> Result<Vec<PathBuf>, ToolError> {
    let list: Vec<&str> = match (args["paths"].as_array(), args["path"].as_str()) {
        (Some(a), _) => a.iter().filter_map(Value::as_str).collect(),
        (None, Some(p)) => vec![p],
        (None, None) => return Err(missing("paths")),
    };
    if list.is_empty() {
        return Err(missing("paths"));
    }
    list.into_iter().map(|p| scoped(c, safe_path(p)?)).collect()
}

/// A destination that must not exist yet.
fn fresh(dest: &Path) -> Result<(), ToolError> {
    if dest.exists() {
        Err(ToolError::new(
            ToolErrorCode::InvalidArgs,
            text::tf("error.files.exists", &[("name", &name(dest))]),
        ))
    } else {
        Ok(())
    }
}

fn copy_all(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        std::fs::create_dir(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy_all(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(|_| ())
    }
}

/// Moves, across drives too (copy, then remove the original only once the copy is whole).
fn move_one(from: &Path, to: &Path) -> std::io::Result<()> {
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) if from.exists() && !to.exists() => {
            copy_all(from, to)?;
            if from.is_dir() {
                std::fs::remove_dir_all(from)
            } else {
                std::fs::remove_file(from)
            }
        }
        Err(e) => Err(e),
    }
}

fn skip_dir(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name.to_ascii_lowercase().as_str(),
            "appdata" | "node_modules" | "target" | "$recycle.bin" | "windows" | "program files"
        )
}

/// The fallback search: a bounded walk for names containing every word of `query`.
fn walk(query: &str, roots: &[PathBuf], limit: usize, cancel: &dyn Fn() -> bool) -> Vec<FileHit> {
    let words: Vec<String> = query
        .to_lowercase()
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    let mut out = Vec::new();
    let mut seen = 0usize;
    let mut stack: Vec<(PathBuf, usize)> = roots.iter().map(|r| (r.clone(), 0)).collect();
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            seen += 1;
            if seen > WALK_ENTRIES || cancel() {
                return out;
            }
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
            let lower = file_name.to_lowercase();
            if !words.is_empty() && words.iter().all(|w| lower.contains(w.as_str())) {
                let meta = entry.metadata().ok();
                out.push(FileHit {
                    path: entry.path(),
                    name: file_name.clone(),
                    is_dir,
                    modified: meta
                        .as_ref()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs()),
                    size: meta.filter(std::fs::Metadata::is_file).map(|m| m.len()),
                });
                if out.len() >= limit {
                    return out;
                }
            }
            if is_dir && depth < WALK_DEPTH && !skip_dir(&file_name) {
                stack.push((entry.path(), depth + 1));
            }
        }
    }
    out
}

#[allow(clippy::too_many_lines, reason = "one table of tool definitions")]
pub(crate) fn tools(c: &Arc<Controls>) -> Vec<Arc<dyn Tool>> {
    use Capability::{FilesModify, FilesRead};
    use Reversibility::{Irreversible, NotApplicable, Undoable};
    use SideEffect::{Destructive, LocalRead, LocalWrite};
    let b = Builder::new(c);
    let path_param = |extra: Value, required: &[&str]| {
        let mut props = json!({ "path": { "type": "string", "description": "An absolute path" } });
        if let (Some(p), Some(e)) = (props.as_object_mut(), extra.as_object()) {
            p.extend(e.clone());
        }
        object(props, required)
    };
    let paths_param = |extra: Value, required: &[&str]| {
        let mut props = json!({ "paths": { "type": "array", "items": { "type": "string" }, "description": "Absolute paths" } });
        if let (Some(p), Some(e)) = (props.as_object_mut(), extra.as_object()) {
            p.extend(e.clone());
        }
        object(props, required)
    };
    vec![
        b.tool(
            &def(
                "files.search",
                "Find files and folders by name in the user's folders (or one folder).",
                object(
                    json!({
                        "query": { "type": "string" },
                        "folder": { "type": "string", "description": "An absolute folder; the user's folders by default" },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
                    }),
                    &["query"],
                ),
                Risk::Safe,
                &[LocalRead],
                FilesRead,
                NotApplicable,
            ),
            Box::new(|args, c, cancel| {
                let query = str_arg(args, "query")?;
                let limit = args["limit"].as_u64().map_or(20, |n| n.clamp(1, 100)) as usize;
                let roots = match args["folder"].as_str() {
                    Some(f) => vec![scoped(c, safe_path(f)?)?],
                    None => allowed_roots(c),
                };
                let private: Vec<PathBuf> = c
                    .options()
                    .private_folders
                    .iter()
                    .filter_map(|f| safe_path(f).ok())
                    .collect();
                let hits = match c.files.search_index(query, &roots, limit) {
                    Ok(hits) => hits,
                    Err(PlatformError::Unsupported) => walk(query, &roots, limit, &|| cancel.is_cancelled()),
                    Err(e) => return Err(platform_error(e)),
                };
                // Private folders never show up, even through the index.
                let hits: Vec<FileHit> = hits
                    .into_iter()
                    .filter(|h| !private.iter().any(|p| under(&h.path, p)))
                    .collect();
                Ok(Output::new(
                    text::plural("reply.files.found", hits.len() as u64, &[]),
                    json!({ "files": hits }),
                )
                .untrusted(text::t("source.fileNames")))
            }),
        )
        .assess(assess(Risk::Safe))
        .targets(Box::new(targets))
        .build(),
        b.tool(
            &def(
                "files.read",
                "Read a text file (the start of it, for long files).",
                path_param(json!({ "maxChars": { "type": "integer", "minimum": 100, "maximum": 12000 } }), &["path"]),
                Risk::Low,
                &[LocalRead],
                FilesRead,
                NotApplicable,
            ),
            Box::new(|args, c, _| {
                let path = path_arg(args, "path", c)?;
                let max = args["maxChars"].as_u64().map_or(READ_CHARS, |n| n.clamp(100, 12_000) as usize);
                let bytes = std::fs::read(&path).map_err(|e| io(&e, &path))?;
                if bytes.iter().take(8192).any(|&b| b == 0) {
                    return Err(ToolError::new(ToolErrorCode::Unsupported, text::t("error.files.binary")));
                }
                let content = String::from_utf8_lossy(&bytes);
                let truncated = content.chars().count() > max;
                let content: String = content.chars().take(max).collect();
                Ok(Output::new(
                    String::new(),
                    json!({ "path": path, "content": content, "truncated": truncated }),
                )
                .untrusted(name(&path)))
            }),
        )
        .assess(assess(Risk::Low))
        .targets(Box::new(targets))
        .build(),
        b.tool(
            &def(
                "files.open",
                "Open a file or folder with its default app.",
                path_param(json!({}), &["path"]),
                Risk::Low,
                &[LocalRead],
                FilesRead,
                NotApplicable,
            ),
            Box::new(|args, c, _| {
                let path = path_arg(args, "path", c)?;
                c.files.open(&path).map_err(platform_error)?;
                Ok(Output::new(
                    text::tf("reply.opening", &[("name", &name(&path))]),
                    json!({ "path": path }),
                ))
            }),
        )
        .assess(assess(Risk::Low))
        .targets(Box::new(targets))
        .build(),
        b.tool(
            &def(
                "files.reveal",
                "Show a file selected in File Explorer.",
                path_param(json!({}), &["path"]),
                Risk::Low,
                &[LocalRead],
                FilesRead,
                NotApplicable,
            ),
            Box::new(|args, c, _| {
                let path = path_arg(args, "path", c)?;
                c.files.reveal(&path).map_err(platform_error)?;
                Ok(Output::new(
                    text::tf("reply.files.revealed", &[("name", &name(&path))]),
                    json!({ "path": path }),
                ))
            }),
        )
        .assess(assess(Risk::Low))
        .targets(Box::new(targets))
        .build(),
        b.tool(
            &def(
                "files.create",
                "Create a text file (with optional content) or a folder.",
                path_param(
                    json!({
                        "content": { "type": "string" },
                        "folder": { "type": "boolean", "description": "true to create a folder" }
                    }),
                    &["path"],
                ),
                Risk::Low,
                &[LocalWrite],
                FilesModify,
                Undoable,
            ),
            Box::new(|args, c, _| {
                let path = path_arg(args, "path", c)?;
                fresh(&path)?;
                if args["folder"].as_bool().unwrap_or(false) {
                    std::fs::create_dir_all(&path).map_err(|e| io(&e, &path))?;
                } else {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| io(&e, parent))?;
                    }
                    let content = args["content"].as_str().unwrap_or_default();
                    std::fs::write(&path, content).map_err(|e| io(&e, &path))?;
                }
                Ok(Output::new(
                    text::tf("reply.files.created", &[("name", &name(&path))]),
                    json!({ "path": path }),
                ))
            }),
        )
        .undo(Box::new(|data, c| {
            let path = path_arg(data, "path", c)?;
            c.files.recycle(std::slice::from_ref(&path)).map_err(platform_error)?;
            Ok(Output::new(text::t("reply.undone"), json!({ "path": path })))
        }))
        .assess(assess(Risk::Low))
        .targets(Box::new(targets))
        .build(),
        b.tool(
            &def(
                "files.rename",
                "Rename a file or folder (a new name, not a path).",
                path_param(json!({ "name": { "type": "string" } }), &["path", "name"]),
                Risk::Low,
                &[LocalWrite],
                FilesModify,
                Undoable,
            ),
            Box::new(|args, c, _| {
                let path = path_arg(args, "path", c)?;
                let new_name = str_arg(args, "name")?;
                if new_name.contains(['\\', '/', ':']) || new_name == ".." || new_name == "." {
                    return Err(refused("error.files.badName"));
                }
                let to = path.with_file_name(new_name);
                fresh(&to)?;
                std::fs::rename(&path, &to).map_err(|e| io(&e, &path))?;
                Ok(Output::new(
                    text::tf("reply.files.renamed", &[("name", &new_name)]),
                    json!({ "path": to, "from": path }),
                ))
            }),
        )
        .undo(Box::new(|data, c| {
            let now = path_arg(data, "path", c)?;
            let was = path_arg(data, "from", c)?;
            fresh(&was)?;
            std::fs::rename(&now, &was).map_err(|e| io(&e, &now))?;
            Ok(Output::new(text::t("reply.undone"), json!({ "path": was })))
        }))
        .assess(assess(Risk::Low))
        .targets(Box::new(targets))
        .build(),
        b.tool(
            &def(
                "files.move",
                "Move files or folders into a folder.",
                paths_param(json!({ "to": { "type": "string", "description": "The destination folder" } }), &["paths", "to"]),
                Risk::Medium,
                &[LocalWrite],
                FilesModify,
                Undoable,
            ),
            Box::new(|args, c, cancel| {
                let paths = paths_arg(args, c)?;
                let to = path_arg(args, "to", c)?;
                if !to.is_dir() {
                    return Err(io(&std::io::Error::from(std::io::ErrorKind::NotFound), &to));
                }
                let mut moved = Vec::new();
                for p in &paths {
                    if cancel.is_cancelled() {
                        break;
                    }
                    let dest = to.join(p.file_name().unwrap_or_default());
                    fresh(&dest)?;
                    move_one(p, &dest).map_err(|e| io(&e, p))?;
                    moved.push(json!({ "from": p, "to": dest }));
                }
                Ok(Output::new(
                    text::plural("reply.files.moved", moved.len() as u64, &[("folder", &name(&to))]),
                    json!({ "moved": moved }),
                ))
            }),
        )
        .undo(Box::new(|data, c| {
            let moved = data["moved"].as_array().cloned().unwrap_or_default();
            for m in moved.iter().rev() {
                let (Some(from), Some(to)) = (m["from"].as_str(), m["to"].as_str()) else {
                    continue;
                };
                let (from, to) = (scoped(c, safe_path(from)?)?, scoped(c, safe_path(to)?)?);
                fresh(&from)?;
                move_one(&to, &from).map_err(|e| io(&e, &to))?;
            }
            Ok(Output::new(text::t("reply.undone"), json!({ "count": moved.len() })))
        }))
        .assess(assess(Risk::Medium))
        .targets(Box::new(targets))
        .build(),
        b.tool(
            &def(
                "files.copy",
                "Copy files or folders into a folder.",
                paths_param(json!({ "to": { "type": "string", "description": "The destination folder" } }), &["paths", "to"]),
                Risk::Low,
                &[LocalWrite],
                FilesModify,
                Undoable,
            ),
            Box::new(|args, c, cancel| {
                let paths = paths_arg(args, c)?;
                let to = path_arg(args, "to", c)?;
                if !to.is_dir() {
                    return Err(io(&std::io::Error::from(std::io::ErrorKind::NotFound), &to));
                }
                let mut copies = Vec::new();
                for p in &paths {
                    if cancel.is_cancelled() {
                        break;
                    }
                    let dest = to.join(p.file_name().unwrap_or_default());
                    fresh(&dest)?;
                    copy_all(p, &dest).map_err(|e| io(&e, p))?;
                    copies.push(dest);
                }
                Ok(Output::new(
                    text::plural("reply.files.copied", copies.len() as u64, &[("folder", &name(&to))]),
                    json!({ "copies": copies }),
                ))
            }),
        )
        .undo(Box::new(|data, c| {
            let copies: Vec<PathBuf> = data["copies"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_str).filter_map(|p| safe_path(p).ok()).collect())
                .unwrap_or_default();
            c.files.recycle(&copies).map_err(platform_error)?;
            Ok(Output::new(text::t("reply.undone"), json!({ "count": copies.len() })))
        }))
        .assess(assess(Risk::Low))
        .targets(Box::new(targets))
        .build(),
        b.tool(
            &def(
                "files.delete",
                "Move files or folders to the Recycle Bin (never deletes permanently).",
                paths_param(json!({}), &["paths"]),
                Risk::Medium,
                &[Destructive],
                FilesModify,
                Irreversible,
            ),
            Box::new(|args, c, _| {
                let paths = paths_arg(args, c)?;
                c.files.recycle(&paths).map_err(platform_error)?;
                Ok(Output::new(
                    text::plural("reply.files.recycled", paths.len() as u64, &[]),
                    json!({ "recycled": paths }),
                ))
            }),
        )
        .assess(assess(Risk::Medium))
        .targets(Box::new(targets))
        .build(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::rig;

    #[cfg(not(windows))]
    #[test]
    fn the_guard_refuses_traversal_and_network_paths() {
        assert!(safe_path("/home/me/notes.txt").is_ok());
        assert!(safe_path("/home/me/../other/x").is_err());
        assert!(safe_path("notes.txt").is_err());
        assert!(safe_path("//server/share/x").is_err());
        assert_eq!(safe_path("/a/./b").unwrap(), PathBuf::from("/a/b"));
        assert!(is_protected(Path::new("/"), &[], None));
        assert!(is_protected(
            Path::new("/usr/bin/ls"),
            &[PathBuf::from("/usr")],
            None
        ));
        assert!(!is_protected(
            Path::new("/usrx/y"),
            &[PathBuf::from("/usr")],
            None
        ));
    }

    #[cfg(windows)]
    #[test]
    fn the_guard_refuses_traversal_devices_networks_and_streams() {
        assert!(safe_path(r"C:\Users\me\notes.txt").is_ok());
        assert_eq!(
            safe_path(r"C:\Users\me\..\other\x").unwrap_err().code,
            ToolErrorCode::InvalidArgs
        );
        assert!(safe_path(r"notes.txt").is_err());
        assert!(safe_path(r"\\?\C:\Windows").is_err());
        assert!(safe_path(r"\\.\PhysicalDrive0").is_err());
        assert!(safe_path(r"\\server\share\x").is_err());
        assert!(safe_path("//server/share").is_err());
        assert!(safe_path(r"C:\x\file.txt:secret").is_err());
        assert_eq!(safe_path(r"C:\a\.\b").unwrap(), PathBuf::from(r"C:\a\b"));
    }

    #[cfg(windows)]
    #[test]
    fn protected_places_are_recognized() {
        let roots = vec![
            PathBuf::from(r"C:\Windows"),
            PathBuf::from(r"C:\Users\me\AppData"),
        ];
        let home = PathBuf::from(r"C:\Users\me");
        let p = |s: &str| is_protected(Path::new(s), &roots, Some(&home));
        assert!(p(r"C:\Windows\System32\drivers"));
        assert!(p(r"C:\"));
        assert!(p(r"C:\Users"));
        assert!(p(r"C:\Users\me"));
        assert!(p(r"C:\Users\me\AppData\Roaming\app\config.json"));
        assert!(!p(r"C:\Users\me\Documents\notes.txt"));
        assert!(
            !p(r"C:\WindowsApps-not-really\x"),
            "a prefix of a name isn't the folder"
        );
    }

    #[test]
    fn protected_paths_and_bulk_raise_the_risk_to_high() {
        let r = rig();
        let delete = r.tool("files.delete");
        let docs = r.dir.path().join("docs");
        let one = json!({ "paths": [docs.join("a.txt")] });
        assert_eq!(delete.assess(&one, Initiator::Brain), Risk::Medium);
        #[cfg(windows)]
        {
            let system = json!({ "paths": [r"C:\Windows\win.ini"] });
            assert_eq!(delete.assess(&system, Initiator::UserDirect), Risk::High);
            let reading = r.tool("files.read");
            assert_eq!(
                reading.assess(&json!({ "path": r"C:\Windows\win.ini" }), Initiator::Brain),
                Risk::High
            );
        }
        let many: Vec<String> = (0..21)
            .map(|i| docs.join(format!("{i}.txt")).display().to_string())
            .collect();
        assert_eq!(
            delete.assess(&json!({ "paths": many }), Initiator::UserDirect),
            Risk::High
        );
    }

    #[test]
    fn create_rename_move_copy_and_undo_on_the_synthetic_tree() {
        let r = rig();
        let root = r.tree();
        let alpha = root.join("Projects").join("alpha");
        let beta = root.join("Projects").join("beta");
        // Create, then undo (to the Recycle Bin).
        let create = r.tool("files.create");
        let made = create
            .run(&json!({ "path": alpha.join("todo.txt"), "content": "- ship" }))
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(alpha.join("todo.txt")).unwrap(),
            "- ship"
        );
        assert!(
            create
                .run(&json!({ "path": alpha.join("todo.txt") }))
                .is_err(),
            "never overwrites"
        );
        create.undo(&made.data).unwrap();
        assert!(!alpha.join("todo.txt").exists());
        // Rename and back.
        let rename = r.tool("files.rename");
        let renamed = rename
            .run(&json!({ "path": alpha.join("notes.txt"), "name": "notes-old.txt" }))
            .unwrap();
        assert!(alpha.join("notes-old.txt").exists());
        assert!(
            rename
                .run(&json!({ "path": alpha.join("notes-old.txt"), "name": r"..\escape.txt" }))
                .is_err()
        );
        rename.undo(&renamed.data).unwrap();
        assert!(alpha.join("notes.txt").exists());
        // Move a file into beta and back.
        let mv = r.tool("files.move");
        let moved = mv
            .run(&json!({ "paths": [alpha.join("main.rs")], "to": beta }))
            .unwrap();
        assert!(beta.join("main.rs").exists() && !alpha.join("main.rs").exists());
        mv.undo(&moved.data).unwrap();
        assert!(alpha.join("main.rs").exists() && !beta.join("main.rs").exists());
        // Copy a folder, then undo the copy.
        let copy = r.tool("files.copy");
        let copied = copy.run(&json!({ "paths": [alpha], "to": beta })).unwrap();
        assert!(beta.join("alpha").join("notes.txt").exists());
        copy.undo(&copied.data).unwrap();
        assert!(!beta.join("alpha").exists());
        assert!(
            alpha.join("notes.txt").exists(),
            "the original is untouched"
        );
    }

    #[test]
    fn files_stay_inside_the_allowed_folders_and_out_of_private_ones() {
        let r = rig();
        let root = r.tree();
        let outside = if cfg!(windows) {
            r"C:\Windows\win.ini"
        } else {
            "/etc/hosts"
        };
        let e = r
            .tool("files.read")
            .run(&json!({ "path": outside }))
            .unwrap_err();
        assert_eq!(e.message, "That’s outside the folders KIVO may use.");
        r.set_options(|o| o.private_folders = vec![root.join("Old stuff").display().to_string()]);
        let e = r
            .tool("files.read")
            .run(&json!({ "path": root.join("Old stuff").join("list.txt") }))
            .unwrap_err();
        assert_eq!(e.message, "That folder is private, so KIVO won’t touch it.");
        let found = r
            .tool("files.search")
            .run(&json!({ "query": "list", "folder": root }))
            .unwrap();
        assert!(
            found.data["files"].as_array().unwrap().is_empty(),
            "private files never show up"
        );
        r.set_options(|o| o.allowed_folders = vec![root.join("Projects").display().to_string()]);
        assert!(
            r.tool("files.read")
                .run(&json!({ "path": root.join("Projects").join("beta").join("plan.md") }))
                .is_ok()
        );
        assert!(
            r.tool("files.read")
                .run(&json!({ "path": root.join("README.md") }))
                .is_err()
        );
    }

    #[test]
    fn delete_goes_to_the_recycle_bin() {
        let r = rig();
        let root = r.tree();
        let list = root.join("Old stuff").join("list.txt");
        let out = r
            .tool("files.delete")
            .run(&json!({ "paths": [list] }))
            .unwrap();
        assert_eq!(out.say, "Moved 1 item to the Recycle Bin.");
        assert!(!list.exists());
        assert!(
            r.bin().join("list.txt").exists(),
            "recoverable from the bin"
        );
        assert_eq!(
            r.tool("files.delete").spec().reversibility,
            Reversibility::Irreversible
        );
    }

    #[test]
    fn search_walks_when_there_is_no_index_and_marks_names_untrusted() {
        let r = rig();
        let root = r.tree();
        let out = r
            .tool("files.search")
            .run(&json!({ "query": "plan", "folder": root }))
            .unwrap();
        let files = out.data["files"].as_array().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0]["path"].as_str().unwrap().ends_with("plan.md"));
        assert!(out.source.is_some());
    }

    #[test]
    fn reading_is_capped_untrusted_and_text_only() {
        let r = rig();
        let root = r.tree();
        let out = r
            .tool("files.read")
            .run(&json!({ "path": root.join("Projects").join("beta").join("plan.md") }))
            .unwrap();
        assert_eq!(out.data["content"], "beta plan\n");
        assert_eq!(out.source.as_deref(), Some("plan.md"));
        let bin = root.join("blob.bin");
        std::fs::write(&bin, [0u8, 1, 2, 3]).unwrap();
        assert_eq!(
            r.tool("files.read")
                .run(&json!({ "path": bin }))
                .unwrap_err()
                .code,
            ToolErrorCode::Unsupported
        );
    }
}
