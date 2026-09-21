//! The model manager (DISTRIBUTION §4, DIST-12): speech models are downloaded on demand, never
//! bundled. Each model has a manifest of files with sizes and sha256 hashes. Downloads resume with
//! HTTP ranges, every file is verified, and a model appears in `%LOCALAPPDATA%\KIVO\models\<id>`
//! only once all of it is there and verified (the folder is renamed into place).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

/// What a model is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelKind {
    Stt,
    Tts,
    Vad,
    Wake,
    Embedding,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFile {
    /// File name inside the model folder.
    pub name: String,
    pub url: String,
    pub size: u64,
    /// Lower-case hex sha256.
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelManifest {
    pub id: String,
    pub name: String,
    pub kind: ModelKind,
    /// SPDX id or named licence, shown before download (DIST-13).
    pub license: String,
    /// Required attribution (CC-BY) or use restrictions (OpenRAIL), shown before download.
    pub attribution: String,
    /// Where it comes from, for the licence screen.
    pub source: String,
    pub languages: Vec<String>,
    pub files: Vec<ModelFile>,
}

impl ModelManifest {
    pub fn download_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

/// The models KIVO knows how to download.
pub fn catalog() -> Vec<ModelManifest> {
    const MOONSHINE: &str = "https://huggingface.co/csukuangfj2/sherpa-onnx-moonshine-base-en-quantized-2026-02-27/resolve/main";
    vec![ModelManifest {
        id: "moonshine-base-en".into(),
        name: "Moonshine Base (English)".into(),
        kind: ModelKind::Stt,
        license: "MIT".into(),
        attribution: "Moonshine by Useful Sensors (Moonshine AI), MIT License.".into(),
        source: "https://github.com/moonshine-ai/moonshine".into(),
        languages: vec!["en".into()],
        files: vec![
            ModelFile {
                name: "encoder_model.ort".into(),
                url: format!("{MOONSHINE}/encoder_model.ort"),
                size: 31_326_816,
                sha256: "7c66495948d0d08ec1af454cd4b5514862ae6511e94712a60e6d83eaec8dc8cf".into(),
            },
            ModelFile {
                name: "decoder_model_merged.ort".into(),
                url: format!("{MOONSHINE}/decoder_model_merged.ort"),
                size: 109_424_400,
                sha256: "d9d7b333af34bc552580576ddcf248a1c6c839e0d3b43b09afb9376ed009899d".into(),
            },
            ModelFile {
                name: "tokens.txt".into(),
                url: format!("{MOONSHINE}/tokens.txt"),
                size: 549_350,
                sha256: "2870d843e14c1e187bf1913a521562a63b53933814bd7f2145120468f494a049".into(),
            },
        ],
    }]
}

/// A model on disk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledModel {
    pub manifest: ModelManifest,
    pub dir: PathBuf,
    pub bytes: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("KIVO doesn't know a model called {0}")]
    Unknown(String),
    #[error("the download failed; check the internet connection and try again")]
    Network(String),
    #[error("a downloaded file was damaged; try again")]
    Corrupt { file: String },
    #[error("there isn't enough space or the models folder can't be written")]
    Disk(#[from] io::Error),
    #[error("the download was cancelled")]
    Cancelled,
}

/// Where downloads come from. The real one speaks HTTP; tests use an in-memory one.
pub trait Fetcher: Send + Sync {
    /// Opens `url` starting at byte `from` (an HTTP range request when `from > 0`). Returns the
    /// reader and whether the server honoured the range (if not, it starts from byte 0).
    fn open(&self, url: &str, from: u64) -> Result<(Box<dyn Read + Send>, bool), ModelError>;
}

/// Progress of one model download.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub done: u64,
    pub total: u64,
}

pub struct ModelStore {
    root: PathBuf,
}

impl ModelStore {
    /// Models under `root` (`Paths::models()`).
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    /// The installed model, if it is complete.
    pub fn installed(&self, id: &str) -> Option<InstalledModel> {
        let dir = self.dir(id);
        let manifest: ModelManifest =
            serde_json::from_slice(&fs::read(dir.join("manifest.json")).ok()?).ok()?;
        let bytes = manifest
            .files
            .iter()
            .map(|f| fs::metadata(dir.join(&f.name)).map(|m| m.len()).ok())
            .sum::<Option<u64>>()?;
        Some(InstalledModel {
            manifest,
            dir,
            bytes,
        })
    }

    /// Every complete model on this PC (Voice → Models on this PC, DIST-13).
    pub fn list(&self) -> Vec<InstalledModel> {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut models: Vec<_> = entries
            .filter_map(Result::ok)
            .filter_map(|e| self.installed(e.file_name().to_str()?))
            .collect();
        models.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
        models
    }

    /// Deletes a model (and any unfinished download of it).
    pub fn remove(&self, id: &str) -> Result<(), ModelError> {
        for dir in [self.dir(id), self.partial_dir(id)] {
            if dir.exists() {
                fs::remove_dir_all(&dir)?;
            }
        }
        Ok(())
    }

    fn partial_dir(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.partial"))
    }

    /// Downloads and installs `manifest`, resuming an earlier partial download. `progress` is
    /// called as bytes arrive.
    pub fn install(
        &self,
        manifest: &ModelManifest,
        fetcher: &dyn Fetcher,
        cancel: &CancellationToken,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<InstalledModel, ModelError> {
        if let Some(done) = self.installed(&manifest.id) {
            return Ok(done);
        }
        let partial = self.partial_dir(&manifest.id);
        fs::create_dir_all(&partial)?;
        let total = manifest.download_size();
        let mut done_before = 0;
        for file in &manifest.files {
            let target = partial.join(&file.name);
            if target.is_file() && fs::metadata(&target)?.len() == file.size {
                done_before += file.size;
                continue; // finished and verified in an earlier attempt
            }
            let part = partial.join(format!("{}.part", file.name));
            let mut attempts = 0;
            loop {
                attempts += 1;
                match download(file, &part, fetcher, cancel, &mut |n| {
                    progress(Progress {
                        done: done_before + n,
                        total,
                    });
                }) {
                    Ok(()) => break,
                    // A damaged file starts over once; a second failure is reported.
                    Err(ModelError::Corrupt { .. }) if attempts == 1 => {
                        fs::remove_file(&part)?;
                    }
                    Err(e) => return Err(e),
                }
            }
            fs::rename(&part, &target)?;
            done_before += file.size;
        }
        fs::write(
            partial.join("manifest.json"),
            serde_json::to_vec_pretty(manifest).map_err(io::Error::other)?,
        )?;
        let dir = self.dir(&manifest.id);
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        fs::rename(&partial, &dir)?;
        tracing::info!(model = manifest.id, "model installed");
        self.installed(&manifest.id)
            .ok_or_else(|| ModelError::Corrupt {
                file: "manifest.json".into(),
            })
    }
}

/// Downloads one file into `part`, resuming from its current length, then checks its hash.
fn download(
    file: &ModelFile,
    part: &Path,
    fetcher: &dyn Fetcher,
    cancel: &CancellationToken,
    progress: &mut dyn FnMut(u64),
) -> Result<(), ModelError> {
    let have = fs::metadata(part).map_or(0, |m| m.len()).min(file.size);
    let (mut body, resumed) = fetcher.open(&file.url, have)?;
    let mut out = if resumed && have > 0 {
        OpenOptions::new().append(true).open(part)?
    } else {
        File::create(part)?
    };
    let mut written = if resumed { have } else { 0 };
    let mut buf = vec![0; 256 * 1024];
    loop {
        if cancel.is_cancelled() {
            return Err(ModelError::Cancelled);
        }
        let n = body
            .read(&mut buf)
            .map_err(|e| ModelError::Network(e.to_string()))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        written += n as u64;
        progress(written);
        if written > file.size {
            break; // more than the manifest promised: the hash check will reject it
        }
    }
    out.sync_all()?;
    drop(out);
    if written != file.size || sha256_file(part)? != file.sha256 {
        return Err(ModelError::Corrupt {
            file: file.name.clone(),
        });
    }
    Ok(())
}

pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut f = File::open(path)?;
    let mut buf = vec![0; 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Downloads over HTTPS (Windows' own TLS), following redirects (Hugging Face → its CDN).
pub struct HttpFetcher {
    agent: ureq::Agent,
}

impl Default for HttpFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpFetcher {
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .tls_config(
                ureq::tls::TlsConfig::builder()
                    .provider(ureq::tls::TlsProvider::NativeTls)
                    .build(),
            )
            .timeout_connect(Some(std::time::Duration::from_secs(15)))
            .user_agent(concat!("KIVO/", env!("CARGO_PKG_VERSION")))
            .build();
        Self {
            agent: config.into(),
        }
    }
}

impl Fetcher for HttpFetcher {
    fn open(&self, url: &str, from: u64) -> Result<(Box<dyn Read + Send>, bool), ModelError> {
        let mut request = self.agent.get(url);
        if from > 0 {
            request = request.header("Range", format!("bytes={from}-"));
        }
        let response = request
            .call()
            .map_err(|e| ModelError::Network(e.to_string()))?;
        let resumed = response.status() == 206;
        Ok((Box::new(response.into_body().into_reader()), resumed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Serves files from memory; can cut a response short to simulate a dropped connection.
    struct FakeServer {
        files: HashMap<String, Vec<u8>>,
        cut_first_after: Mutex<Option<usize>>,
        range_requests: AtomicUsize,
    }

    impl Fetcher for FakeServer {
        fn open(&self, url: &str, from: u64) -> Result<(Box<dyn Read + Send>, bool), ModelError> {
            let data = self
                .files
                .get(url)
                .ok_or_else(|| ModelError::Network("404".into()))?;
            if from > 0 {
                self.range_requests.fetch_add(1, Ordering::Relaxed);
            }
            let from = usize::try_from(from).unwrap();
            let mut body = data[from..].to_vec();
            if let Some(cut) = self.cut_first_after.lock().unwrap().take() {
                body.truncate(cut);
            }
            Ok((Box::new(io::Cursor::new(body)), true))
        }
    }

    fn sha(data: &[u8]) -> String {
        hex(&Sha256::digest(data))
    }

    fn manifest(files: &[(&str, &[u8])]) -> ModelManifest {
        ModelManifest {
            id: "test-model".into(),
            name: "Test".into(),
            kind: ModelKind::Stt,
            license: "MIT".into(),
            attribution: String::new(),
            source: String::new(),
            languages: vec!["en".into()],
            files: files
                .iter()
                .map(|(name, data)| ModelFile {
                    name: (*name).into(),
                    url: format!("https://example/{name}"),
                    size: data.len() as u64,
                    sha256: sha(data),
                })
                .collect(),
        }
    }

    fn server(files: &[(&str, &[u8])], cut: Option<usize>) -> FakeServer {
        FakeServer {
            files: files
                .iter()
                .map(|(n, d)| (format!("https://example/{n}"), d.to_vec()))
                .collect(),
            cut_first_after: Mutex::new(cut),
            range_requests: AtomicUsize::new(0),
        }
    }

    #[test]
    fn a_model_installs_verified_and_atomically() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModelStore::new(tmp.path().to_path_buf());
        let files: [(&str, &[u8]); 2] = [("a.onnx", &[1; 5000]), ("tokens.txt", b"hello 0")];
        let m = manifest(&files);
        let mut last = Progress { done: 0, total: 0 };
        let installed = store
            .install(
                &m,
                &server(&files, None),
                &CancellationToken::new(),
                &mut |p| last = p,
            )
            .unwrap();
        assert_eq!(installed.bytes, 5007);
        assert_eq!(
            last,
            Progress {
                done: 5007,
                total: 5007
            }
        );
        assert!(!tmp.path().join("test-model.partial").exists());
        assert_eq!(store.list().len(), 1);
        store.remove("test-model").unwrap();
        assert!(store.installed("test-model").is_none());
    }

    #[test]
    fn an_interrupted_download_resumes_with_a_range_request() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModelStore::new(tmp.path().to_path_buf());
        let data: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
        let files: [(&str, &[u8]); 1] = [("big.onnx", &data)];
        let m = manifest(&files);
        // The connection drops after 4000 bytes: the file is short, so its hash fails once and it
        // restarts; a cut file left on disk (e.g. after a crash) resumes with a range instead.
        let part = tmp.path().join("test-model.partial").join("big.onnx.part");
        fs::create_dir_all(part.parent().unwrap()).unwrap();
        fs::write(&part, &data[..4000]).unwrap();
        let srv = server(&files, None);
        store
            .install(&m, &srv, &CancellationToken::new(), &mut |_| {})
            .unwrap();
        assert_eq!(srv.range_requests.load(Ordering::Relaxed), 1);
        assert_eq!(
            fs::read(store.dir("test-model").join("big.onnx")).unwrap(),
            data
        );
    }

    #[test]
    fn a_damaged_file_is_retried_then_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModelStore::new(tmp.path().to_path_buf());
        let files: [(&str, &[u8]); 1] = [("a.onnx", &[7; 100])];
        let mut m = manifest(&files);
        // One cut response: retried and installed.
        store
            .install(
                &m,
                &server(&files, Some(10)),
                &CancellationToken::new(),
                &mut |_| {},
            )
            .unwrap();
        // A wrong hash in the manifest can never verify.
        store.remove("test-model").unwrap();
        m.files[0].sha256 = "00".repeat(32);
        let err = store
            .install(
                &m,
                &server(&files, None),
                &CancellationToken::new(),
                &mut |_| {},
            )
            .unwrap_err();
        assert!(matches!(err, ModelError::Corrupt { .. }), "{err:?}");
        assert!(
            store.installed("test-model").is_none(),
            "nothing half-installed"
        );
    }

    #[test]
    fn cancelling_stops_the_download_and_keeps_the_part() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModelStore::new(tmp.path().to_path_buf());
        let files: [(&str, &[u8]); 1] = [("a.onnx", &[7; 100])];
        let cancel = CancellationToken::new();
        cancel.cancel();
        let err = store
            .install(
                &manifest(&files),
                &server(&files, None),
                &cancel,
                &mut |_| {},
            )
            .unwrap_err();
        assert!(matches!(err, ModelError::Cancelled));
    }

    #[test]
    fn the_catalog_has_complete_manifests() {
        for m in catalog() {
            assert!(!m.files.is_empty() && !m.license.is_empty() && !m.attribution.is_empty());
            for f in &m.files {
                assert_eq!(f.sha256.len(), 64, "{}", f.name);
                assert!(f.url.starts_with("https://") && f.size > 0);
            }
        }
    }
}
