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
    /// The worker stopped; the current turn can't continue.
    Lost,
}

/// Which engines the worker should hold.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Engines {
    /// Speech-to-text engine id and its model folder.
    pub stt: Option<(String, PathBuf)>,
    /// Text-to-speech engine id ("system", "kokoro-82m").
    pub tts: Option<String>,
    pub threads: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum InferError {
    #[error("{0}")]
    Engine(String),
    #[error("KIVO's speech engine is restarting")]
    NotReady,
}

/// A handle to the worker. Cloning is cheap; every call goes to the live connection.
#[derive(Clone)]
pub struct Infer {
    peer: Arc<Mutex<Option<Peer>>>,
    ready: watch::Sender<bool>,
    engines: watch::Sender<Engines>,
    next_id: Arc<AtomicU64>,
    program: PathBuf,
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
                next_id: Arc::new(AtomicU64::new(1)),
                program,
            },
            rx,
            tx,
        )
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

    /// Sets which engines to keep loaded (the supervisor loads them, and reloads after a restart).
    pub fn set_engines(&self, engines: Engines) {
        self.engines.send_replace(engines);
    }

    pub fn next_utterance(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
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
    *infer
        .peer
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(peer.clone());
    let wanted = engines_rx.borrow_and_update().clone();
    load_engines(&peer, &wanted).await?;
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
                if let Err(e) = load_engines(&peer, &engines).await {
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

async fn load_engines(peer: &Peer, engines: &Engines) -> Result<(), String> {
    let threads = engines.threads.max(1);
    match &engines.stt {
        Some((engine, dir)) => {
            let params = json!(ModelLoad {
                slot: InferSlot::Stt,
                engine: engine.clone(),
                dir: Some(dir.to_string_lossy().into_owned()),
                threads,
            });
            peer.request(method::MODEL_LOAD, params)
                .await
                .map_err(|e| e.to_string())?;
        }
        None => {
            let _ = peer
                .request(
                    method::MODEL_UNLOAD,
                    json!(ModelUnload {
                        slot: InferSlot::Stt
                    }),
                )
                .await;
        }
    }
    match &engines.tts {
        Some(engine) => {
            let params = json!(ModelLoad {
                slot: InferSlot::Tts,
                engine: engine.clone(),
                dir: None,
                threads
            });
            peer.request(method::MODEL_LOAD, params)
                .await
                .map_err(|e| e.to_string())?;
        }
        None => {
            let _ = peer
                .request(
                    method::MODEL_UNLOAD,
                    json!(ModelUnload {
                        slot: InferSlot::Tts
                    }),
                )
                .await;
        }
    }
    Ok(())
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
