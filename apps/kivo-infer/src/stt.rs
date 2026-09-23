//! The speech-to-text thread: one engine, one utterance at a time. Audio for the utterance streams
//! in as notifications; partial transcripts stream back the same way; `stt.finish` returns the
//! final text.

use crate::{Tokens, engine_error};
use kivo_ipc::infer::{
    InferSlot, ModelLoad, ModelState, Residency, SttFinal, SttPartial, SttStart, method,
};
use kivo_ipc::{Peer, RpcError};
use kivo_voice::moonshine::{self, Moonshine};
use kivo_voice::{SttEngine, SttEvent, SttOptions, VoiceError};
use std::path::Path;
use std::sync::mpsc;
use std::time::Instant;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

pub type Reply<T> = oneshot::Sender<Result<T, RpcError>>;

pub enum Command {
    Load {
        load: ModelLoad,
        reply: Reply<()>,
    },
    Unload {
        reply: Reply<()>,
    },
    Start {
        start: SttStart,
        cancel: CancellationToken,
        reply: Reply<()>,
    },
    Audio {
        id: u64,
        pcm: Vec<f32>,
    },
    Finish {
        id: u64,
        reply: Reply<SttFinal>,
    },
    /// An utterance was cancelled: wakes the loop so it stops at once.
    Cancelled,
}

pub struct Handle(mpsc::Sender<Command>);

impl Handle {
    pub fn send(&self, command: Command) {
        let _ = self.0.send(command);
    }

    pub async fn call<T>(&self, make: impl FnOnce(Reply<T>) -> Command) -> Result<T, RpcError> {
        let (tx, rx) = oneshot::channel();
        self.send(make(tx));
        rx.await
            .unwrap_or_else(|_| Err(engine_error("the speech recognizer stopped")))
    }
}

pub fn spawn(peer: Peer, tokens: Tokens) -> Handle {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("kivo-stt".into())
        .spawn(move || run(&rx, &peer, &tokens))
        .expect("thread spawn");
    Handle(tx)
}

fn report(peer: &Peer, engine: &str, state: Residency) {
    let _ = peer.notify(
        method::MODEL_STATE,
        serde_json::to_value(ModelState {
            slot: InferSlot::Stt,
            engine: engine.into(),
            state,
        })
        .unwrap_or_default(),
    );
}

fn user_error(e: &VoiceError) -> RpcError {
    tracing::error!(detail = e.detail(), "speech recognition failed");
    engine_error(e.to_string())
}

fn run(rx: &mpsc::Receiver<Command>, peer: &Peer, tokens: &Tokens) {
    let mut engine: Option<Moonshine> = None;
    while let Ok(command) = rx.recv() {
        match command {
            Command::Load { load, reply } => {
                if engine.is_some() {
                    let _ = reply.send(Ok(()));
                    continue;
                }
                report(peer, &load.engine, Residency::Warming);
                let started = Instant::now();
                let loaded = match (load.engine.as_str(), &load.dir) {
                    (moonshine::MODEL_ID, Some(dir)) => {
                        Moonshine::load(Path::new(dir), load.threads.max(1))
                    }
                    (other, _) => Err(VoiceError::Unavailable(format!(
                        "the {other} speech recognizer"
                    ))),
                };
                match loaded {
                    Ok(e) => {
                        tracing::info!(
                            engine = load.engine,
                            ms = started.elapsed().as_millis(),
                            "speech recognizer loaded"
                        );
                        engine = Some(e);
                        report(peer, &load.engine, Residency::Warm);
                        let _ = reply.send(Ok(()));
                    }
                    Err(e) => {
                        report(peer, &load.engine, Residency::Unloaded);
                        let _ = reply.send(Err(user_error(&e)));
                    }
                }
            }
            Command::Unload { reply } => {
                if let Some(e) = engine.take() {
                    let id = e.info().id.clone();
                    report(peer, &id, Residency::Unloading);
                    drop(e);
                    report(peer, &id, Residency::Unloaded);
                }
                let _ = reply.send(Ok(()));
            }
            Command::Start {
                start,
                cancel,
                reply,
            } => match &mut engine {
                Some(e) => {
                    let _ = reply.send(Ok(()));
                    utterance(e, &start, cancel, rx, peer);
                    tokens
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .remove(&start.id);
                }
                None => {
                    let _ = reply.send(Err(engine_error("the speech recognizer isn't loaded")));
                }
            },
            // For an utterance that already ended.
            Command::Audio { .. } | Command::Cancelled => {}
            Command::Finish { reply, .. } => {
                let _ = reply.send(Err(engine_error("nothing is being transcribed")));
            }
        }
    }
}

/// Runs one utterance until it is finished or cancelled.
fn utterance(
    engine: &mut Moonshine,
    start: &SttStart,
    cancel: CancellationToken,
    rx: &mpsc::Receiver<Command>,
    peer: &Peer,
) {
    let id = engine.info().id.clone();
    report(peer, &id, Residency::Active);
    let options = SttOptions {
        language: start.language.clone(),
        vocabulary: start.vocabulary.clone(),
    };
    let mut stream = engine.start(&options, cancel.clone());
    let mut heard = 0usize;
    // A cancel sends `Cancelled`, so the loop sleeps until something arrives.
    while !cancel.is_cancelled() {
        let Ok(command) = rx.recv() else { break };
        match command {
            Command::Cancelled => {}
            Command::Audio { id: utterance, pcm } if utterance == start.id => {
                heard += pcm.len();
                match stream.accept(&pcm) {
                    Ok(events) => {
                        for event in events {
                            let (text, stable) = match event {
                                SttEvent::Partial(t) | SttEvent::Final(t) => (t, false),
                                SttEvent::Stable(t) => (t, true),
                            };
                            let partial = SttPartial {
                                id: start.id,
                                text,
                                stable,
                            };
                            let _ = peer.notify(
                                method::STT_PARTIAL,
                                serde_json::to_value(partial).unwrap_or_default(),
                            );
                        }
                    }
                    Err(VoiceError::Cancelled) => break,
                    Err(e) => tracing::warn!(detail = e.detail(), "partial transcript failed"),
                }
            }
            Command::Finish {
                id: utterance,
                reply,
            } if utterance == start.id => {
                let started = Instant::now();
                let result = stream
                    .finish()
                    .map_err(|e| user_error(&e))
                    .inspect(|text| {
                        // Lengths only: transcripts never reach the log (ARCH-32).
                        tracing::info!(
                            samples = heard,
                            chars = text.len(),
                            ms = started.elapsed().as_millis(),
                            "utterance transcribed"
                        );
                    })
                    .map(|text| SttFinal {
                        text,
                        millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                    });
                let _ = reply.send(result);
                break;
            }
            Command::Audio { .. } => {}
            Command::Finish { reply, .. } => {
                let _ = reply.send(Err(engine_error("that utterance isn't being transcribed")));
            }
            Command::Load { reply, .. } | Command::Unload { reply } => {
                let _ = reply.send(Err(engine_error("the speech recognizer is busy")));
            }
            Command::Start { reply, .. } => {
                let _ = reply.send(Err(engine_error("the speech recognizer is busy")));
            }
        }
    }
    report(peer, &id, Residency::Idle);
}
