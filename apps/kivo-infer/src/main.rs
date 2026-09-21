//! The inference worker (ARCHITECTURE §1, ARCH-09): speech-to-text and text-to-speech run here, out
//! of the runtime, so a crash or a memory spike in a model never takes KIVO down. The runtime
//! starts it on demand and talks to it over stdin/stdout with the same framed JSON-RPC as the UI
//! pipe (ARCH-21, `kivo_ipc::infer`). Logs go to stderr, which the runtime copies into its log.
//!
//! Each engine slot has its own thread, so a long answer being spoken never delays recognition.
//! Cancel requests skip the queues: they fire the utterance's token directly (cancel → silence
//! ≤ 100 ms, ARCH-26).

// No console window when the runtime starts it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod stt;
mod tts;

use kivo_ipc::infer::{
    INFER_PROTOCOL, InferHello, InferSlot, InferWelcome, ModelLoad, ModelUnload, SttAudio,
    SttStart, TtsSpeak, UtteranceId, decode_pcm, method,
};
use kivo_ipc::{Incoming, Peer, RpcError};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

/// Cancel tokens for utterances and speech in progress, by id.
pub type Tokens = Arc<Mutex<HashMap<u64, CancellationToken>>>;

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() != Some("serve") {
        eprintln!(
            "kivo-infer {}: started by the KIVO runtime (`kivo-infer serve`)",
            env!("CARGO_PKG_VERSION")
        );
        return ExitCode::from(2);
    }
    #[cfg(windows)]
    if let Some(paths) = kivo_platform::Paths::user() {
        kivo_platform_windows::crash::install(&paths.crashes(), "kivo-infer");
    }
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_target(false)
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .init();
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!(%e, "couldn't start");
            return ExitCode::FAILURE;
        }
    };
    runtime.block_on(serve());
    ExitCode::SUCCESS
}

async fn serve() {
    let stdio = tokio::io::join(tokio::io::stdin(), tokio::io::stdout());
    let (peer, mut incoming) = Peer::spawn(stdio);
    let tokens: Tokens = Arc::default();
    let stt = stt::spawn(peer.clone(), Arc::clone(&tokens));
    let tts = tts::spawn(peer.clone(), Arc::clone(&tokens));
    tracing::info!(pid = std::process::id(), "kivo-infer ready");

    while let Some(message) = incoming.recv().await {
        match message {
            Incoming::Request(r) => {
                let reply = handle(&r.method, r.params, &stt, &tts, &tokens).await;
                let stop = r.method == method::SHUTDOWN;
                let _ = peer.respond(r.id, reply);
                if stop {
                    break;
                }
            }
            Incoming::Notification(n) => match n.method.as_str() {
                method::STT_AUDIO => {
                    if let Ok(audio) = serde_json::from_value::<SttAudio>(n.params)
                        && let Some(pcm) = decode_pcm(&audio.pcm)
                    {
                        stt.send(stt::Command::Audio { id: audio.id, pcm });
                    }
                }
                method::STT_CANCEL | method::TTS_CANCEL => {
                    if let Ok(UtteranceId { id }) = serde_json::from_value(n.params)
                        && let Some(token) =
                            tokens.lock().unwrap_or_else(|e| e.into_inner()).get(&id)
                    {
                        token.cancel();
                    }
                }
                other => tracing::warn!(method = other, "unknown notification"),
            },
        }
    }
    // The runtime closed the pipe or asked to stop: cancel everything and let the threads end.
    for token in tokens.lock().unwrap_or_else(|e| e.into_inner()).values() {
        token.cancel();
    }
    tracing::info!("kivo-infer stopping");
}

async fn handle(
    method: &str,
    params: Value,
    stt: &stt::Handle,
    tts: &tts::Handle,
    tokens: &Tokens,
) -> Result<Value, RpcError> {
    match method {
        method::HELLO => {
            let hello: InferHello = parse(params)?;
            if hello.protocol != INFER_PROTOCOL {
                return Err(RpcError::new(
                    RpcError::INCOMPATIBLE,
                    format!(
                        "kivo-infer speaks protocol {INFER_PROTOCOL}, not {}",
                        hello.protocol
                    ),
                ));
            }
            to_value(&InferWelcome {
                protocol: INFER_PROTOCOL,
                version: env!("CARGO_PKG_VERSION").into(),
                pid: std::process::id(),
            })
        }
        method::MODEL_LOAD => {
            let load: ModelLoad = parse(params)?;
            match load.slot {
                InferSlot::Stt => stt.call(|reply| stt::Command::Load { load, reply }).await,
                InferSlot::Tts => tts.call(|reply| tts::Command::Load { load, reply }).await,
            }
            .map(|()| Value::Null)
        }
        method::MODEL_UNLOAD => {
            let ModelUnload { slot } = parse(params)?;
            match slot {
                InferSlot::Stt => stt.call(|reply| stt::Command::Unload { reply }).await,
                InferSlot::Tts => tts.call(|reply| tts::Command::Unload { reply }).await,
            }
            .map(|()| Value::Null)
        }
        method::STT_START => {
            let start: SttStart = parse(params)?;
            let cancel = CancellationToken::new();
            tokens
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(start.id, cancel.clone());
            stt.call(|reply| stt::Command::Start {
                start,
                cancel,
                reply,
            })
            .await
            .map(|()| Value::Null)
        }
        method::STT_FINISH => {
            let UtteranceId { id } = parse(params)?;
            let result = stt.call(|reply| stt::Command::Finish { id, reply }).await;
            tokens.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
            result.and_then(|r| to_value(&r))
        }
        method::TTS_VOICES => tts
            .call(|reply| tts::Command::Voices { reply })
            .await
            .and_then(|v| to_value(&v)),
        method::TTS_SPEAK => {
            let speak: TtsSpeak = parse(params)?;
            let cancel = CancellationToken::new();
            tokens
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(speak.id, cancel.clone());
            tts.send(tts::Command::Speak { speak, cancel });
            Ok(Value::Null)
        }
        method::SHUTDOWN => Ok(json!(true)),
        other => Err(RpcError::method_not_found(other)),
    }
}

fn parse<T: DeserializeOwned>(params: Value) -> Result<T, RpcError> {
    serde_json::from_value(params).map_err(RpcError::invalid_params)
}

fn to_value<T: serde::Serialize>(value: &T) -> Result<Value, RpcError> {
    serde_json::to_value(value).map_err(|e| RpcError::new(RpcError::INTERNAL, e.to_string()))
}

/// A user-safe failure from an engine thread.
pub fn engine_error(message: impl Into<String>) -> RpcError {
    RpcError::new(RpcError::ENGINE, message)
}
