//! The text-to-speech thread: speaks one request at a time and streams the audio back sentence by
//! sentence (`tts.audio`), then `tts.done`.

use crate::{Tokens, engine_error};
use kivo_ipc::infer::{
    InferSlot, ModelLoad, ModelState, Residency, TtsAudio, TtsDone, TtsSpeak, encode_pcm, method,
};
use kivo_ipc::{Peer, RpcError};
#[cfg(windows)]
use kivo_voice::system_tts::{self, SystemTts};
use kivo_voice::{TtsEngine, VoiceError, VoiceInfo};
#[cfg(windows)]
use std::sync::Arc;
use std::sync::mpsc;
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
    Voices {
        reply: Reply<Vec<VoiceInfo>>,
    },
    Speak {
        speak: TtsSpeak,
        cancel: CancellationToken,
    },
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
            .unwrap_or_else(|_| Err(engine_error("the voice stopped")))
    }
}

pub fn spawn(peer: Peer, tokens: Tokens) -> Handle {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("kivo-tts".into())
        .spawn(move || run(&rx, &peer, &tokens))
        .expect("thread spawn");
    Handle(tx)
}

fn report(peer: &Peer, engine: &str, state: Residency) {
    let state = ModelState {
        slot: InferSlot::Tts,
        engine: engine.into(),
        state,
    };
    let _ = peer.notify(
        method::MODEL_STATE,
        serde_json::to_value(state).unwrap_or_default(),
    );
}

fn load(engine: &str) -> Result<Box<dyn TtsEngine>, VoiceError> {
    match engine {
        #[cfg(windows)]
        system_tts::ENGINE_ID => Ok(Box::new(SystemTts::new(Arc::new(
            kivo_platform_windows::WindowsSpeech,
        )))),
        other => Err(VoiceError::Unavailable(format!("the {other} voice"))),
    }
}

fn run(rx: &mpsc::Receiver<Command>, peer: &Peer, tokens: &Tokens) {
    let mut engine: Option<Box<dyn TtsEngine>> = None;
    while let Ok(command) = rx.recv() {
        match command {
            Command::Load {
                load: request,
                reply,
            } => {
                if engine
                    .as_ref()
                    .is_some_and(|e| e.info().id == request.engine)
                {
                    let _ = reply.send(Ok(()));
                    continue;
                }
                report(peer, &request.engine, Residency::Warming);
                match load(&request.engine) {
                    Ok(e) => {
                        engine = Some(e);
                        report(peer, &request.engine, Residency::Warm);
                        let _ = reply.send(Ok(()));
                    }
                    Err(e) => {
                        report(peer, &request.engine, Residency::Unloaded);
                        tracing::error!(detail = e.detail(), "couldn't load the voice");
                        let _ = reply.send(Err(engine_error(e.to_string())));
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
            Command::Voices { reply } => {
                let voices = engine.as_ref().map(|e| e.voices()).unwrap_or_default();
                let _ = reply.send(Ok(voices));
            }
            Command::Speak { speak, cancel } => {
                let done = match &mut engine {
                    Some(e) => speak_one(e.as_mut(), &speak, &cancel, peer),
                    None => TtsDone {
                        id: speak.id,
                        error: Some("the voice isn't loaded".into()),
                        cancelled: false,
                    },
                };
                tokens
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(&speak.id);
                let _ = peer.notify(
                    method::TTS_DONE,
                    serde_json::to_value(done).unwrap_or_default(),
                );
            }
        }
    }
}

fn speak_one(
    engine: &mut dyn TtsEngine,
    speak: &TtsSpeak,
    cancel: &CancellationToken,
    peer: &Peer,
) -> TtsDone {
    let id = engine.info().id.clone();
    report(peer, &id, Residency::Active);
    let result = engine.speak(
        &speak.text,
        speak.voice.as_deref(),
        cancel,
        &mut |samples, rate| {
            let audio = TtsAudio {
                id: speak.id,
                rate,
                pcm: encode_pcm(samples),
            };
            peer.notify(
                method::TTS_AUDIO,
                serde_json::to_value(audio).unwrap_or_default(),
            )
            .map_err(|_| VoiceError::Cancelled)
        },
    );
    report(peer, &id, Residency::Idle);
    match result {
        Ok(()) => TtsDone {
            id: speak.id,
            error: None,
            cancelled: false,
        },
        Err(VoiceError::Cancelled) => TtsDone {
            id: speak.id,
            error: None,
            cancelled: true,
        },
        Err(e) => {
            tracing::error!(detail = e.detail(), "speaking failed");
            TtsDone {
                id: speak.id,
                error: Some(e.to_string()),
                cancelled: false,
            }
        }
    }
}
