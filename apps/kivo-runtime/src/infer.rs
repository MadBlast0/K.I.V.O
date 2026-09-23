//! The inference worker's supervisor (ARCHITECTURE §1, ARCH-09/21): starts `kivo-infer` on demand,
//! keeps the speech models warm per the residency policy (VOICE §8, PLAN-02), restarts the worker
//! if it crashes, and fails the turn in progress with a spoken message instead of taking KIVO down.

use kivo_ipc::infer::{
    INFER_PROTOCOL, InferHello, InferSlot, InferWelcome, ModelLoad, ModelState, ModelUnload,
    Residency, SttAudio, SttFinal, SttPartial, SttStart, TtsAudio, TtsDone, TtsSpeak, UtteranceId,
    encode_pcm, method,
};
use kivo_ipc::{Incoming, Peer};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

/// Backoff between restarts after a crash.
const FIRST_RETRY: Duration = Duration::from_millis(500);
const MAX_RETRY: Duration = Duration::from_secs(30);

/// What the worker sends back while it works.
#[derive(Clone, Debug, PartialEq)]
pub enum InferEvent {
    Partial {
        id: u64,
        text: String,
        stable: bool,
    },
    /// Synthesized speech for utterance `id`.
    Speech {
        id: u64,
        rate: u32,
        pcm: Vec<f32>,
    },
    SpeakDone {
        id: u64,
        error: Option<String>,
        cancelled: bool,
    },
    Residency {
        slot: InferSlot,
        engine: String,
        state: Residency,
    },
    /// A chosen engine failed to load, so the worker uses `to` instead for the rest of the
    /// session (VOICE-47); `None` when nothing could replace it. The saved choice is unchanged.
    Fallback {
        slot: InferSlot,
        from: String,
        to: Option<String>,
        error: String,
    },
    /// The worker stopped; the current turn can't continue.
    Lost,
}

/// Which engines the worker should hold.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Engines {
    /// Speech-to-text engine id and its model folder.
    pub stt: Option<(String, PathBuf)>,
    /// Another installed speech-to-text engine, used if `stt` fails to load (VOICE-47).
    pub stt_fallback: Option<(String, PathBuf)>,
    /// Text-to-speech engine id ("system", "kokoro-82m") and its model folder, if it has one.
    pub tts: Option<(String, Option<PathBuf>)>,
    pub threads: usize,
    /// The language KIVO speaks and hears.
    pub language: String,
}

#[derive(Debug, thiserror::Error)]
pub enum InferError {
    #[error("{0}")]
    Engine(String),
    #[error("KIVO's speech engine is restarting")]
    NotReady,
}

/// A handle to the worker. Cloning is cheap; every call goes to the live connection.
///
/// Residency (VOICE §8, PLAN-02): the engines the settings choose are only *configured* here.
/// They are loaded when a request starts (`warm`: prewarm on turn start) and unloaded after
/// `warm_for` without use; with nothing loaded the worker process exits, so an idle KIVO holds no
/// model at all (plan §128).
#[derive(Clone)]
pub struct Infer {
    peer: Arc<Mutex<Option<Peer>>>,
    ready: watch::Sender<bool>,
    engines: watch::Sender<Engines>,
    configured: Arc<Mutex<Engines>>,
    /// Bumped on every use; the cool-down timer only unloads if nothing used the models since.
    used: watch::Sender<u64>,
    warm_for: Arc<Mutex<Duration>>,
    next_id: Arc<AtomicU64>,
    /// The running worker's process id (0 while none runs).
    pid: Arc<std::sync::atomic::AtomicU32>,
    program: PathBuf,
    /// Engines that failed to load this session; their fallbacks stand in (VOICE-47).
    failed: Arc<Mutex<std::collections::HashSet<String>>>,
}

impl Infer {
    /// Creates the handle and the event stream; nothing starts until `ensure` is called.
    pub fn new(
        program: PathBuf,
    ) -> (
        Self,
        mpsc::UnboundedReceiver<InferEvent>,
        mpsc::UnboundedSender<InferEvent>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Self {
                peer: Arc::default(),
                ready: watch::Sender::new(false),
                engines: watch::Sender::new(Engines::default()),
                configured: Arc::default(),
                used: watch::Sender::new(0),
                warm_for: Arc::new(Mutex::new(Duration::from_secs(10 * 60))),
                next_id: Arc::new(AtomicU64::new(1)),
                pid: Arc::default(),
                program,
                failed: Arc::default(),
            },
            rx,
            tx,
        )
    }

    /// The worker program, for a second worker that tries an engine out (VOICE-45).
    pub fn program(&self) -> &std::path::Path {
        &self.program
    }

    /// Lets an engine that failed earlier in the session be tried again (it was reinstalled, or
    /// passed a test in another worker).
    pub fn forgive(&self, engine: &str) {
        lock(&self.failed).remove(engine);
    }

    pub fn is_ready(&self) -> bool {
        *self.ready.borrow()
    }

    /// Waits until the worker is connected and the models are loaded.
    pub async fn wait_ready(&self, within: Duration) -> bool {
        if self.is_ready() {
            return true;
        }
        let mut ready = self.ready.subscribe();
        tokio::time::timeout(within, async move {
            while !*ready.borrow_and_update() {
                if ready.changed().await.is_err() {
                    return false;
                }
            }
            true
        })
        .await
        .unwrap_or(false)
    }

    /// Sets which engines to keep loaded now (the supervisor loads them, and reloads after a
    /// restart). Most callers use `configure` and `warm` instead.
    pub fn set_engines(&self, engines: Engines) {
        self.engines.send_replace(engines);
    }

    /// The engines the settings choose. If they are loaded, they are reloaded as configured.
    pub fn configure(&self, engines: Engines, warm_for: Duration) {
        *lock(&self.warm_for) = warm_for;
        *lock(&self.configured) = engines.clone();
        let loaded = {
            let current = self.engines.borrow();
            current.stt.is_some() || current.tts.is_some()
        };
        if loaded {
            self.engines.send_replace(engines);
        }
    }

    /// A request is starting: load the configured engines now if they aren't (prewarm on turn
    /// start, VOICE-34) and restart the cool-down.
    pub fn warm(&self) {
        let configured = lock(&self.configured).clone();
        self.engines.send_if_modified(|current| {
            let changed = *current != configured;
            if changed {
                current.clone_from(&configured);
            }
            changed
        });
        self.used.send_modify(|n| *n += 1);
    }

    /// Keeps the models warm (called when a request finishes).
    pub fn touch(&self) {
        self.used.send_modify(|n| *n += 1);
    }

    /// The residency of each slot, as the worker last reported it.
    pub fn loaded(&self) -> bool {
        let e = self.engines.borrow();
        e.stt.is_some() || e.tts.is_some()
    }

    /// The worker's process id, if one is running (diagnostics and tests).
    pub fn worker_pid(&self) -> Option<u32> {
        Some(self.pid.load(Ordering::Relaxed)).filter(|&p| p != 0)
    }

    pub fn next_utterance(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Unloads the models once they have gone `warm_for` without use (VOICE §8). Runs until
    /// shutdown.
    pub async fn cool_down(self, shutdown: CancellationToken) {
        let mut used = self.used.subscribe();
        loop {
            let seen = *used.borrow_and_update();
            let warm_for = *lock(&self.warm_for);
            tokio::select! {
                () = tokio::time::sleep(warm_for) => {
                    if *self.used.borrow() == seen && self.loaded() {
                        tracing::info!(minutes = warm_for.as_secs() / 60, "speech models unloaded after going unused");
                        self.engines.send_replace(Engines::default());
                    }
                    // Cold now: wait for the next use (or shutdown) before timing again.
                    tokio::select! {
                        changed = used.changed() => if changed.is_err() { return },
                        () = shutdown.cancelled() => return,
                    }
                }
                changed = used.changed() => if changed.is_err() { return },
                () = shutdown.cancelled() => return,
            }
        }
    }

    fn peer(&self) -> Result<Peer, InferError> {
        self.peer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .filter(|p| !p.is_closed())
            .ok_or(InferError::NotReady)
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, InferError> {
        self.peer()?
            .request(method, params)
            .await
            .map_err(|e| InferError::Engine(e.to_string()))
    }

    fn notify(&self, method: &str, params: Value) -> Result<(), InferError> {
        self.peer()?
            .notify(method, params)
            .map_err(|_| InferError::NotReady)
    }

    /// Starts transcribing utterance `id`.
    pub async fn start_stt(
        &self,
        id: u64,
        language: &str,
        vocabulary: Vec<String>,
    ) -> Result<(), InferError> {
        let params = json!(SttStart {
            id,
            language: language.to_owned(),
            vocabulary
        });
        self.request(method::STT_START, params).await.map(|_| ())
    }

    /// Sends 16 kHz mono audio for an utterance.
    pub fn send_audio(&self, id: u64, pcm: &[f32]) -> Result<(), InferError> {
        self.notify(
            method::STT_AUDIO,
            json!(SttAudio {
                id,
                pcm: encode_pcm(pcm)
            }),
        )
    }

    /// Ends the utterance and returns the final transcript.
    pub async fn finish_stt(&self, id: u64) -> Result<SttFinal, InferError> {
        let value = self
            .request(method::STT_FINISH, json!(UtteranceId { id }))
            .await?;
        serde_json::from_value(value).map_err(|e| InferError::Engine(e.to_string()))
    }

    pub fn cancel_stt(&self, id: u64) -> Result<(), InferError> {
        self.notify(method::STT_CANCEL, json!(UtteranceId { id }))
    }

    /// Speaks `text`; the audio arrives as `InferEvent::Speech`, then `SpeakDone`.
    pub async fn speak(&self, id: u64, text: &str, voice: Option<&str>) -> Result<(), InferError> {
        let params = json!(TtsSpeak {
            id,
            text: text.to_owned(),
            voice: voice.map(str::to_owned)
        });
        self.request(method::TTS_SPEAK, params).await.map(|_| ())
    }

    pub fn cancel_speech(&self, id: u64) -> Result<(), InferError> {
        self.notify(method::TTS_CANCEL, json!(UtteranceId { id }))
    }

    /// Asks the worker to stop (shutdown order, plan §127).
    pub async fn shutdown(&self) {
        if let Ok(peer) = self.peer() {
            let _ = peer.request(method::SHUTDOWN, Value::Null).await;
        }
    }
}

/// Starts `kivo-infer` and keeps it running until shutdown.
pub async fn supervise(
    infer: Infer,
    events: mpsc::UnboundedSender<InferEvent>,
    shutdown: CancellationToken,
) {
    let mut backoff = FIRST_RETRY;
    let mut engines_rx = infer.engines.subscribe();
    loop {
        if shutdown.is_cancelled() {
            return;
        }
        // Nothing to run until an engine is wanted (no models loaded at idle, plan §128).
        while {
            let engines = engines_rx.borrow_and_update();
            engines.stt.is_none() && engines.tts.is_none()
        } {
            tokio::select! {
                changed = engines_rx.changed() => if changed.is_err() { return },
                () = shutdown.cancelled() => return,
            }
        }
        match run_worker(&infer, &events, &shutdown, &mut engines_rx).await {
            Ok(()) => backoff = FIRST_RETRY,
            Err(e) => {
                tracing::error!(%e, "the speech worker stopped");
                infer.ready.send_replace(false);
                let _ = events.send(InferEvent::Lost);
                tokio::select! {
                    () = tokio::time::sleep(backoff) => {}
                    () = shutdown.cancelled() => return,
                }
                backoff = (backoff * 2).min(MAX_RETRY);
            }
        }
        if shutdown.is_cancelled() {
            return;
        }
    }
}

/// One worker lifetime: start, handshake, load models, forward notifications.
async fn run_worker(
    infer: &Infer,
    events: &mpsc::UnboundedSender<InferEvent>,
    shutdown: &CancellationToken,
    engines_rx: &mut watch::Receiver<Engines>,
) -> Result<(), String> {
    let mut child = spawn(&infer.program)?;
    let stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    if let Some(stderr) = child.stderr.take() {
        // The worker's log lines join the runtime's log.
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::debug!(target: "kivo_infer", "{line}");
            }
        });
    }
    let (peer, mut incoming) = Peer::spawn(tokio::io::join(stdout, stdin));
    let welcome: InferWelcome = serde_json::from_value(
        peer.request(
            method::HELLO,
            json!(InferHello {
                protocol: INFER_PROTOCOL
            }),
        )
        .await
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    tracing::info!(
        version = welcome.version,
        pid = welcome.pid,
        "speech worker started"
    );
    infer.pid.store(welcome.pid, Ordering::Relaxed);
    *infer
        .peer
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(peer.clone());
    let wanted = engines_rx.borrow_and_update().clone();
    load_engines(&peer, &wanted, &infer.failed, events).await?;
    infer.ready.send_replace(true);

    let result = loop {
        tokio::select! {
            message = incoming.recv() => match message {
                Some(Incoming::Notification(n)) => forward(&n.method, n.params, events),
                Some(Incoming::Request(_)) => {}
                None => break Err("the speech worker closed the connection".to_owned()),
            },
            changed = engines_rx.changed() => {
                if changed.is_err() {
                    break Ok(());
                }
                let engines = engines_rx.borrow_and_update().clone();
                if engines.stt.is_none() && engines.tts.is_none() {
                    // Nothing to hold: the worker process goes away until it is needed again.
                    let _ = peer.request(method::SHUTDOWN, Value::Null).await;
                    break Ok(());
                }
                if let Err(e) = load_engines(&peer, &engines, &infer.failed, events).await {
                    break Err(e);
                }
            }
            status = child.wait() => break Err(format!("the speech worker exited: {status:?}")),
            () = shutdown.cancelled() => {
                let _ = peer.request(method::SHUTDOWN, Value::Null).await;
                break Ok(());
            }
        }
    };
    infer.ready.send_replace(false);
    infer.pid.store(0, Ordering::Relaxed);
    *infer
        .peer
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    let _ = child.start_kill();
    result
}

fn spawn(program: &std::path::Path) -> Result<Child, String> {
    let mut command = Command::new(program);
    command
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // CREATE_NO_WINDOW: no console flashes when the worker starts.
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    command
        .spawn()
        .map_err(|e| format!("couldn't start {}: {e}", program.display()))
}

/// Loads the wanted engines. An engine that fails to load doesn't take the worker down: the STT
/// fallback or the Windows voices stand in for the session, and the runtime is told so it can
/// say so (VOICE-47). Only a broken connection is an error.
async fn load_engines(
    peer: &Peer,
    engines: &Engines,
    failed: &Mutex<std::collections::HashSet<String>>,
    events: &mpsc::UnboundedSender<InferEvent>,
) -> Result<(), String> {
    let threads = engines.threads.max(1);
    let language = &engines.language;
    let load = |slot: InferSlot, engine: &str, dir: Option<&std::path::Path>| {
        let params = json!(ModelLoad {
            slot,
            engine: engine.to_owned(),
            dir: dir.map(|d| d.to_string_lossy().into_owned()),
            threads,
            language: Some(language.clone()),
        });
        async move { peer.request(method::MODEL_LOAD, params).await }
    };
    let unload = |slot: InferSlot| async move {
        let _ = peer
            .request(method::MODEL_UNLOAD, json!(ModelUnload { slot }))
            .await;
    };
    // Tries `candidates` in order, skipping ones that failed before; reports a stand-in.
    let fill = |slot: InferSlot, candidates: Vec<(String, Option<PathBuf>)>| async move {
        let Some(chosen) = candidates.first().map(|(id, _)| id.clone()) else {
            unload(slot).await;
            return Ok(());
        };
        let mut error = None;
        for (id, dir) in &candidates {
            if lock(failed).contains(id) {
                continue;
            }
            match load(slot, id, dir.as_deref()).await {
                Ok(_) => {
                    if *id != chosen {
                        let _ = events.send(InferEvent::Fallback {
                            slot,
                            from: chosen.clone(),
                            to: Some(id.clone()),
                            error: error.clone().unwrap_or_default(),
                        });
                    }
                    return Ok(());
                }
                Err(_) if peer.is_closed() => {
                    return Err("the speech worker closed the connection".to_owned());
                }
                Err(e) => {
                    tracing::error!(%e, engine = id, ?slot, "a speech engine failed to load");
                    lock(failed).insert(id.clone());
                    error = Some(e.to_string());
                }
            }
        }
        unload(slot).await;
        let _ = events.send(InferEvent::Fallback {
            slot,
            from: chosen,
            to: None,
            error: error.unwrap_or_default(),
        });
        Ok(())
    };
    let stt: Vec<(String, Option<PathBuf>)> = engines
        .stt
        .iter()
        .chain(engines.stt_fallback.iter())
        .map(|(id, dir)| (id.clone(), Some(dir.clone())))
        .collect();
    fill(InferSlot::Stt, stt).await?;
    let mut tts: Vec<(String, Option<PathBuf>)> = engines.tts.iter().cloned().collect();
    if tts
        .first()
        .is_some_and(|(id, _)| id != kivo_voice::system_tts::ENGINE_ID)
    {
        tts.push((kivo_voice::system_tts::ENGINE_ID.to_owned(), None));
    }
    fill(InferSlot::Tts, tts).await
}

fn forward(method_name: &str, params: Value, events: &mpsc::UnboundedSender<InferEvent>) {
    let sent = match method_name {
        method::STT_PARTIAL => {
            serde_json::from_value::<SttPartial>(params).map(|p| InferEvent::Partial {
                id: p.id,
                text: p.text,
                stable: p.stable,
            })
        }
        method::TTS_AUDIO => {
            serde_json::from_value::<TtsAudio>(params).map(|a| InferEvent::Speech {
                id: a.id,
                rate: a.rate,
                pcm: kivo_ipc::infer::decode_pcm(&a.pcm).unwrap_or_default(),
            })
        }
        method::TTS_DONE => {
            serde_json::from_value::<TtsDone>(params).map(|d| InferEvent::SpeakDone {
                id: d.id,
                error: d.error,
                cancelled: d.cancelled,
            })
        }
        method::MODEL_STATE => {
            serde_json::from_value::<ModelState>(params).map(|m| InferEvent::Residency {
                slot: m.slot,
                engine: m.engine,
                state: m.state,
            })
        }
        other => {
            tracing::warn!(method = other, "unknown message from the speech worker");
            return;
        }
    };
    match sent {
        Ok(event) => {
            let _ = events.send(event);
        }
        Err(e) => tracing::warn!(%e, "unreadable message from the speech worker"),
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn calls_fail_clearly_while_the_worker_is_down() {
        let (infer, _events, _tx) = Infer::new(PathBuf::from("kivo-infer"));
        assert!(!infer.is_ready());
        let err = infer.start_stt(1, "en", Vec::new()).await.unwrap_err();
        assert_eq!(err.to_string(), "KIVO's speech engine is restarting");
        assert!(infer.send_audio(1, &[0.0]).is_err());
        assert!(!infer.wait_ready(Duration::from_millis(50)).await);
    }

    #[tokio::test]
    async fn utterance_ids_are_unique() {
        let (infer, _events, _tx) = Infer::new(PathBuf::from("kivo-infer"));
        let ids: Vec<u64> = (0..3).map(|_| infer.next_utterance()).collect();
        assert_eq!(ids, [1, 2, 3]);
    }

    #[tokio::test(start_paused = true)]
    async fn models_load_on_use_and_unload_after_the_warm_time() {
        let (infer, _events, _tx) = Infer::new(PathBuf::from("kivo-infer"));
        let engines = Engines {
            stt: None,
            stt_fallback: None,
            tts: Some(("system".into(), None)),
            threads: 1,
            language: "en-US".into(),
        };
        infer.configure(engines.clone(), Duration::from_secs(600));
        assert!(!infer.loaded(), "configuring loads nothing");
        let shutdown = CancellationToken::new();
        let cooling = tokio::spawn(infer.clone().cool_down(shutdown.clone()));
        infer.warm();
        assert!(infer.loaded(), "a request loads the engines");
        tokio::time::sleep(Duration::from_secs(300)).await;
        infer.touch();
        tokio::time::sleep(Duration::from_secs(400)).await;
        assert!(infer.loaded(), "use restarts the cool-down");
        tokio::time::sleep(Duration::from_secs(300)).await;
        assert!(!infer.loaded(), "unloaded after 10 minutes unused");
        shutdown.cancel();
        cooling.await.unwrap();
    }

    #[test]
    fn worker_messages_become_events() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        forward(
            method::STT_PARTIAL,
            json!({"id": 4, "text": "open chr", "stable": false}),
            &tx,
        );
        assert_eq!(
            rx.try_recv().unwrap(),
            InferEvent::Partial {
                id: 4,
                text: "open chr".into(),
                stable: false
            }
        );
        forward(
            method::TTS_DONE,
            json!({"id": 4, "error": null, "cancelled": true}),
            &tx,
        );
        assert_eq!(
            rx.try_recv().unwrap(),
            InferEvent::SpeakDone {
                id: 4,
                error: None,
                cancelled: true
            }
        );
        forward("nonsense", json!({}), &tx);
        assert!(
            rx.try_recv().is_err(),
            "unknown messages are ignored, not delivered"
        );
    }
}
