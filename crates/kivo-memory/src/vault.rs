//! The vault folder: listing, reading and writing notes safely.
//!
//! Paths are vault-relative with `/` (`people/maya.md`). A path that would leave the vault (`..`,
//! a drive, an absolute path), or that isn't a `.md` file, is refused. Writes go to a temporary
//! file first and are then renamed over the note, so a crash never leaves half a note. Folders
//! starting with `.` (`.kivo/`, `.obsidian/`, `.trash/`) are not notes.

use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VaultError {
    #[error("not a note in the memory folder: {0}")]
    BadPath(String),
    #[error("no note at {0}")]
    Missing(String),
    #[error("couldn't write {0}: {1}")]
    Io(String, String),
}

#[derive(Clone, Debug)]
pub struct Vault {
    root: PathBuf,
}

/// A note file as found on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub text: String,
    /// sha256 of the text, to tell whether it changed since it was indexed.
    pub hash: String,
}

pub fn hash(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// A file-name-safe, lower-case slug: `K.I.V.O` → `k-i-v-o`, `Maya Singh` → `maya-singh`.
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.chars().count() >= 60 {
            break;
        }
    }
    let out = out.trim_end_matches('-').to_owned();
    if out.is_empty() { "note".into() } else { out }
}

impl Vault {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The file for a vault-relative note path, if the path is a note inside the vault.
    pub fn file(&self, path: &str) -> Result<PathBuf, VaultError> {
        let bad = || VaultError::BadPath(path.to_owned());
        let rel = Path::new(path);
        if path.is_empty()
            || !path.to_ascii_lowercase().ends_with(".md")
            || rel.is_absolute()
            || path.contains(':')
            || path.contains('\\')
        {
            return Err(bad());
        }
        for c in rel.components() {
            match c {
                Component::Normal(part) => {
                    if part.to_string_lossy().starts_with('.') {
                        return Err(bad());
                    }
                }
                _ => return Err(bad()),
            }
        }
        Ok(self.root.join(rel))
    }

    /// The vault-relative path of a file inside the vault (for watcher events).
    pub fn relative(&self, file: &Path) -> Option<String> {
        let rel = file.strip_prefix(&self.root).ok()?;
        let parts: Vec<String> = rel
            .components()
            .map(|c| match c {
                Component::Normal(p) => Some(p.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect::<Option<_>>()?;
        let path = parts.join("/");
        self.file(&path).ok().map(|_| path)
    }

    /// Every note, by path.
    pub fn list(&self) -> Vec<Entry> {
        let mut out = Vec::new();
        let mut stack = vec![self.root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    continue;
                }
                let p = e.path();
                let Ok(kind) = e.file_type() else { continue };
                if kind.is_dir() {
                    stack.push(p);
                } else if kind.is_file()
                    && let Some(path) = self.relative(&p)
                    && let Ok(text) = std::fs::read_to_string(&p)
                {
                    out.push(Entry {
                        hash: hash(&text),
                        path,
                        text,
                    });
                }
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }

    pub fn read(&self, path: &str) -> Result<String, VaultError> {
        let file = self.file(path)?;
        std::fs::read_to_string(&file).map_err(|_| VaultError::Missing(path.to_owned()))
    }

    pub fn exists(&self, path: &str) -> bool {
        self.file(path).is_ok_and(|f| f.is_file())
    }

    /// Writes a note (creating its folders), through a temporary file.
    pub fn write(&self, path: &str, text: &str) -> Result<(), VaultError> {
        let file = self.file(path)?;
        let io = |e: std::io::Error| VaultError::Io(path.to_owned(), e.to_string());
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        let tmp_dir = self.root.join(".kivo").join("tmp");
        std::fs::create_dir_all(&tmp_dir).map_err(io)?;
        let tmp = tmp_dir.join(format!("{}.tmp", hash(path)));
        std::fs::write(&tmp, text).map_err(io)?;
        std::fs::rename(&tmp, &file).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            io(e)
        })
    }

    /// Removes a note, and its folder if that's left empty.
    pub fn delete(&self, path: &str) -> Result<(), VaultError> {
        let file = self.file(path)?;
        std::fs::remove_file(&file).map_err(|_| VaultError::Missing(path.to_owned()))?;
        self.prune(&file);
        Ok(())
    }

    /// Removes the folders above `file` that are left empty (never the vault itself).
    fn prune(&self, file: &Path) {
        let mut dir = file.parent().map(Path::to_path_buf);
        while let Some(d) = dir {
            if d == self.root || std::fs::remove_dir(&d).is_err() {
                break;
            }
            dir = d.parent().map(Path::to_path_buf);
        }
    }

    /// Moves a note (for archiving); the target must not exist.
    pub fn rename(&self, from: &str, to: &str) -> Result<(), VaultError> {
        let a = self.file(from)?;
        let b = self.file(to)?;
        if b.exists() {
            return Err(VaultError::Io(to.to_owned(), "already exists".into()));
        }
        if let Some(parent) = b.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| VaultError::Io(to.to_owned(), e.to_string()))?;
        }
        std::fs::rename(&a, &b).map_err(|e| VaultError::Io(from.to_owned(), e.to_string()))?;
        self.prune(&a);
        Ok(())
    }

    /// `folder/<slug>.md`, or `<slug>-2.md` … when that's taken.
    pub fn free_path(&self, folder: &str, title: &str) -> String {
        let base = slug(title);
        let make = |n: u32| {
            let file = if n < 2 {
                format!("{base}.md")
            } else {
                format!("{base}-{n}.md")
            };
            if folder.is_empty() {
                file
            } else {
                format!("{}/{file}", folder.trim_end_matches('/'))
            }
        };
        (1..)
            .map(make)
            .find(|p| !self.exists(p))
            .unwrap_or_else(|| make(1))
    }

    /// Deletes every note (Forget everything). KIVO's own `.kivo/` cache goes too; other dot
    /// folders (`.obsidian/`) are the user's and stay.
    pub fn forget_everything(&self) -> usize {
        let notes = self.list();
        for n in &notes {
            let _ = self.delete(&n.path);
        }
        let _ = std::fs::remove_dir_all(self.root.join(".kivo"));
        notes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_inside_the_vault() {
        let v = Vault::new("C:/vault");
        assert!(v.file("people/maya.md").is_ok());
        for bad in [
            "../secret.md",
            "people/../../x.md",
            "C:/Windows/x.md",
            "/etc/x.md",
            "people\\maya.md",
            ".kivo/index.md",
            "people/.hidden.md",
            "notes/x.txt",
            "",
        ] {
            assert!(v.file(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn notes_are_written_listed_moved_and_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let v = Vault::new(dir.path());
        v.write("people/maya.md", "# Maya\n").unwrap();
        v.write("notes/a.md", "A").unwrap();
        std::fs::create_dir_all(dir.path().join(".obsidian")).unwrap();
        std::fs::write(dir.path().join(".obsidian").join("x.md"), "no").unwrap();
        std::fs::write(dir.path().join("image.png"), "no").unwrap();
        let listed: Vec<String> = v.list().into_iter().map(|e| e.path).collect();
        assert_eq!(listed, ["notes/a.md", "people/maya.md"]);
        assert_eq!(v.free_path("notes", "A"), "notes/a-2.md");
        v.rename("notes/a.md", "archive/a.md").unwrap();
        assert!(v.exists("archive/a.md") && !v.exists("notes/a.md"));
        assert!(!dir.path().join("notes").exists(), "the empty folder went");
        v.delete("people/maya.md").unwrap();
        assert_eq!(
            v.delete("people/maya.md"),
            Err(VaultError::Missing("people/maya.md".into()))
        );
        assert_eq!(v.forget_everything(), 1);
        assert!(v.list().is_empty());
        assert!(dir.path().join(".obsidian").join("x.md").exists());
    }

    #[test]
    fn slugs_are_safe_file_names() {
        assert_eq!(slug("K.I.V.O"), "k-i-v-o");
        assert_eq!(slug("  Maya Singh!! "), "maya-singh");
        assert_eq!(slug("Café in Zürich"), "café-in-zürich");
        assert_eq!(slug("..."), "note");
        assert!(slug(&"word ".repeat(40)).chars().count() <= 60);
    }
}
