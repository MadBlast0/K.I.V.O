//! `ipc`: the runtime ↔ UI channel (BENCHMARKS §4 smoke subset). A real server and client over
//! the real transport (named pipe with its DACL and token), on a private pipe name so a running
//! KIVO is never touched.

use crate::harness::{Sample, Suite};
use kivo_core::config::PermissionMode;
use kivo_core::{EventBus, SessionState};
use kivo_ipc::{
    BoxFuture, Handler, RpcError, Server, ServerConfig, SessionToken, StateSnapshot, connect,
};
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

const PINGS_PER_RUN: usize = 200;

struct NoMethods;

impl Handler for NoMethods {
    fn call(&self, method: String, _params: Value) -> BoxFuture<Result<Value, RpcError>> {
        Box::pin(async move { Err(RpcError::method_not_found(&method)) })
    }
}

pub struct Ipc {
    rt: tokio::runtime::Runtime,
    endpoint: String,
    token: String,
    shutdown: CancellationToken,
    _state: watch::Sender<StateSnapshot>,
}

impl Ipc {
    pub fn start() -> Result<Self, String> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let token = SessionToken::generate().map_err(|e| e.to_string())?;
        let presented = token.as_str().to_owned();
        #[cfg(windows)]
        let endpoint = format!(r"\\.\pipe\kivo-bench-{}", std::process::id());
        #[cfg(unix)]
        let endpoint = format!("/tmp/kivo-bench-{}.sock", std::process::id());
        let state = watch::Sender::new(StateSnapshot {
            session: SessionState::Idle,
            mode: PermissionMode::Auto,
            island_hidden: false,
            turn: None,
            speech: kivo_ipc::protocol::SpeechStatus::Ready,
            hotkey_conflict: None,
            in_use: Vec::new(),
            island: Default::default(),
            revision: 0,
        });
        let shutdown = CancellationToken::new();
        let server = {
            let _guard = rt.enter();
            Server::bind(ServerConfig::new(endpoint.clone(), token, "bench".into()))
                .map_err(|e| e.to_string())?
        };
        rt.spawn(server.run(
            Arc::new(NoMethods),
            EventBus::new(),
            state.subscribe(),
            shutdown.clone(),
        ));
        Ok(Self {
            rt,
            endpoint,
            token: presented,
            shutdown,
            _state: state,
        })
    }
}

impl Drop for Ipc {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

fn micros(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1e6
}

impl Suite for Ipc {
    fn name(&self) -> &'static str {
        "ipc"
    }

    fn run(&mut self) -> Result<Vec<Sample>, String> {
        self.rt.block_on(async {
            let t = Instant::now();
            let conn = connect(&self.endpoint, &self.token, "bench")
                .await
                .map_err(|e| e.to_string())?;
            let connect_us = micros(t);

            let mut pings = Vec::with_capacity(PINGS_PER_RUN);
            for _ in 0..PINGS_PER_RUN {
                let t = Instant::now();
                conn.client
                    .request("ping", Value::Null)
                    .await
                    .map_err(|e| e.to_string())?;
                pings.push(micros(t));
            }
            pings.sort_by(f64::total_cmp);

            let t = Instant::now();
            conn.client
                .request("state.get", Value::Null)
                .await
                .map_err(|e| e.to_string())?;
            let state_us = micros(t);

            Ok(vec![
                Sample::cost("connect + hello", "µs", connect_us),
                Sample::cost(
                    "ping round trip (median of 200)",
                    "µs",
                    pings[pings.len() / 2],
                ),
                Sample::cost(
                    "ping round trip (p95 of 200)",
                    "µs",
                    pings[pings.len() * 95 / 100],
                ),
                Sample::cost("state.get round trip", "µs", state_us),
            ])
        })
    }

    fn notes(&self) -> Vec<String> {
        vec![
            "Real kivo-ipc server and client in one process over the real transport (named pipe \
             with the user-only DACL, session token, JSON-RPC frames), on a private pipe name."
                .into(),
            "Each run opens a fresh connection, then sends 200 pings and one state.get.".into(),
        ]
    }
}
