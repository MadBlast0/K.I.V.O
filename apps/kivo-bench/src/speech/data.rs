//! Benchmark data: where models and test audio live, and the LibriSpeech test-clean subset.
//! Everything sits under `%LOCALAPPDATA%\KIVO\bench` (or `KIVO_BENCH_DATA`), outside the repo.

use std::path::{Path, PathBuf};

/// The benchmark data folder.
pub fn root() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os("KIVO_BENCH_DATA") {
        return Ok(PathBuf::from(dir));
    }
    let paths = kivo_platform::Paths::user().ok_or("couldn't find the AppData folders")?;
    Ok(paths.local.join("bench"))
}

/// A model folder, or a clear message on how to get it.
pub fn model(name: &str) -> Result<PathBuf, String> {
    let dir = root()?.join("models").join(name);
    if dir.is_dir() {
        Ok(dir)
    } else {
        Err(format!(
            "model {name} is missing: unpack it into {} (see docs/benchmarks/README.md)",
            dir.display()
        ))
    }
}

/// The first file in `dir` whose name contains every one of `parts`.
pub fn file(dir: &Path, parts: &[&str]) -> Result<String, String> {
    let mut names: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    names.sort();
    names
        .into_iter()
        .find(|p| {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            parts.iter().all(|part| name.contains(part))
        })
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| format!("no file matching {parts:?} in {}", dir.display()))
}

/// One test utterance: 16 kHz mono samples and the reference transcript.
pub struct Utterance {
    pub samples: Vec<f32>,
    pub text: String,
}

impl Utterance {
    pub fn seconds(&self) -> f64 {
        #[allow(clippy::cast_precision_loss, reason = "sample counts")]
        let n = self.samples.len() as f64;
        n / 16_000.0
    }
}

/// A LibriSpeech test-clean utterance on disk, decoded only when needed.
#[derive(Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub text: String,
}

impl Entry {
    /// The duration, from the FLAC header (nothing is decoded).
    pub fn seconds(&self) -> Result<f64, String> {
        let reader = claxon::FlacReader::open(&self.path)
            .map_err(|e| format!("{}: {e}", self.path.display()))?;
        let info = reader.streaminfo();
        #[allow(clippy::cast_precision_loss, reason = "sample counts")]
        Ok(info.samples.unwrap_or(0) as f64 / f64::from(info.sample_rate))
    }

    pub fn load(&self) -> Result<Utterance, String> {
        Ok(Utterance {
            samples: read_flac(&self.path)?,
            text: self.text.clone(),
        })
    }
}

/// Every utterance of LibriSpeech test-clean, sorted by id, so every machine uses the same order.
pub fn librispeech_index() -> Result<Vec<Entry>, String> {
    librispeech_set("test-clean")
}

/// test-clean, then test-other when `pnpm bench:data` has fetched it (10.7 h together).
pub fn librispeech_all() -> Result<Vec<Entry>, String> {
    let mut entries = librispeech_set("test-clean")?;
    if let Ok(other) = librispeech_set("test-other") {
        entries.extend(other);
    }
    Ok(entries)
}

fn librispeech_set(set: &str) -> Result<Vec<Entry>, String> {
    let base = root()?.join("data").join("LibriSpeech").join(set);
    if !base.is_dir() {
        return Err(format!(
            "LibriSpeech {set} is missing: run `pnpm bench:data` (it unpacks into {})",
            root()?.join("data").display()
        ));
    }
    let mut transcripts = Vec::new();
    collect(&base, &mut transcripts)?;
    transcripts.sort();
    let mut entries = Vec::new();
    for file in transcripts {
        let text = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
        let dir = file.parent().ok_or("bad path")?;
        for line in text.lines() {
            if let Some((id, words)) = line.split_once(' ') {
                entries.push(Entry {
                    path: dir.join(format!("{id}.flac")),
                    text: words.to_owned(),
                });
            }
        }
    }
    Ok(entries)
}

/// The first `count` utterances of LibriSpeech test-clean, decoded.
pub fn librispeech(count: usize) -> Result<Vec<Utterance>, String> {
    librispeech_index()?
        .iter()
        .take(count)
        .map(Entry::load)
        .collect()
}

fn collect(dir: &Path, found: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            collect(&path, found)?;
        } else if path.to_string_lossy().ends_with(".trans.txt") {
            found.push(path);
        }
    }
    Ok(())
}

/// Decodes a 16 kHz mono FLAC file to samples in [-1, 1].
pub fn read_flac(path: &Path) -> Result<Vec<f32>, String> {
    let mut reader =
        claxon::FlacReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let info = reader.streaminfo();
    if info.sample_rate != 16_000 || info.channels != 1 {
        return Err(format!(
            "{}: expected 16 kHz mono, got {} Hz × {}",
            path.display(),
            info.sample_rate,
            info.channels
        ));
    }
    #[allow(clippy::cast_precision_loss, reason = "sample scaling")]
    let scale = (1_i64 << (info.bits_per_sample - 1)) as f32;
    #[allow(clippy::cast_precision_loss, reason = "sample values")]
    reader
        .samples()
        .map(|s| s.map(|v| v as f32 / scale).map_err(|e| e.to_string()))
        .collect()
}
