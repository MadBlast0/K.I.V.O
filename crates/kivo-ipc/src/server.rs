//! The runtime side of the IPC channel. Each connection must open with a valid `hello` (session
//! token + compatible protocol) within a few seconds, or it is dropped. After that it receives
//! every bus event as a notification, a full snapshot if it ever falls behind, and answers to its
//! requests (handled concurrently, so a slow call doesn't block the others).

use crate::frame::{Frames, frames};
use crate::protocol::{
    Hello, Message, Notification, PROTOCOL_VERSION, Request, RequestId, Response, RpcError,
    StateSnapshot, Welcome, method,
};
use crate::token::SessionToken;
use crate::transport;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use kivo_core::{EventBus, Received};
use serde_json::Value;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// What the runtime exposes over IPC.
pub trait Handler: Send + Sync + 'static {
    /// The current state, sent on connect and whenever a client may have missed events.
    fn snapshot(&self) -> StateSnapshot;
    /// Handles a request. `ping`, `state.get` and `hello` never reach here.
    fn call(&self, method: String, params: Value) -> BoxFuture<Result<Value, RpcError>>;
}

pub struct ServerConfig {
    pub endpoint: String,
    pub token: SessionToken,
    pub runtime_version: String,
    /// How long a new connection has to say `hello`.
    pub hello_timeout: Duration,
}

impl ServerConfig {
    pub fn new(endpoint: String, token: SessionToken, runtime_version: String) -> Self {
        Self {
            endpoint,
            token,
            runtime_version,
            hello_timeout: Duration::from_secs(3),
        }
    }
}

struct Shared {
    config: ServerConfig,
    handler: Arc<dyn Handler>,
    bus: EventBus,
}

/// A bound endpoint, ready to serve. Binding first means a second runtime (or a squatter)
/// fails immediately instead of after it starts accepting.
pub struct Server {
    #[cfg(windows)]
    first: transport::ServerStream,
    #[cfg(unix)]
    listener: tokio::net::UnixListener,
    config: ServerConfig,
}

impl Server {
    pub fn bind(config: ServerConfig) -> io::Result<Self> {
        #[cfg(windows)]
        let first = transport::create_server(&config.endpoint, true)?;
        #[cfg(unix)]
        let listener = transport::bind(&config.endpoint)?;
        Ok(Self {
            #[cfg(windows)]
            first,
            #[cfg(unix)]
            listener,
            config,
        })
    }

    /// Accepts connections until `shutdown` is cancelled.
    pub async fn run(
        self,
        handler: Arc<dyn Handler>,
        bus: EventBus,
        shutdown: CancellationToken,
    ) -> io::Result<()> {
        let endpoint = self.config.endpoint.clone();
        let shared = Arc::new(Shared {
            config: self.config,
            handler,
            bus,
        });

        #[cfg(windows)]
        {
            let mut pending = self.first;
            loop {
                tokio::select! {
                    connected = pending.connect() => {
                        connected?;
                        let next = transport::create_server(&endpoint, false)?;
                        let stream = std::mem::replace(&mut pending, next);
                        tokio::spawn(serve(stream, Arc::clone(&shared), shutdown.child_token()));
                    }
                    () = shutdown.cancelled() => return Ok(()),
                }
            }
        }
        #[cfg(unix)]
        {
            let _ = endpoint;
            loop {
                tokio::select! {
                    accepted = self.listener.accept() => {
                        let (stream, _) = accepted?;
                        tokio::spawn(serve(stream, Arc::clone(&shared), shutdown.child_token()));
                    }
                    () = shutdown.cancelled() => return Ok(()),
                }
            }
        }
    }
}

async fn send<S: AsyncRead + AsyncWrite + Unpin>(
    sink: &mut Frames<S>,
    message: &Message,
) -> io::Result<()> {
    let bytes = serde_json::to_vec(message).map_err(io::Error::other)?;
    sink.send(Bytes::from(bytes)).await
}

/// Runs one connection to completion. Any protocol violation closes it.
async fn serve<S>(stream: S, shared: Arc<Shared>, shutdown: CancellationToken)
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let mut io = frames(stream);
    let Some((client, hello_id)) = handshake(&mut io, &shared).await else {
        return;
    };
    tracing::info!(%client, "IPC client connected");

    // Subscribed before the welcome is sent, so nothing between the snapshot and the first
    // notification can be lost.
    let mut events = shared.bus.subscribe();
    let welcome = Welcome {
        protocol_version: PROTOCOL_VERSION,
        runtime_version: shared.config.runtime_version.clone(),
        snapshot: shared.handler.snapshot(),
    };
    let reply = serde_json::to_value(welcome).map_or_else(
        |e| Response::err(hello_id, RpcError::new(RpcError::INTERNAL, e.to_string())),
        |v| Response::ok(hello_id, v),
    );
    if send(&mut io, &Message::Response(reply)).await.is_err() {
        return;
    }

    let (replies_tx, mut replies) = mpsc::channel::<Response>(64);
    loop {
        tokio::select! {
            frame = io.next() => {
                let Some(Ok(frame)) = frame else { break }; // closed, I/O error or oversized frame
                let Ok(Message::Request(request)) = serde_json::from_slice::<Message>(&frame) else {
                    tracing::warn!(%client, "invalid IPC message; closing the connection");
                    break;
                };
                let (shared, replies_tx) = (Arc::clone(&shared), replies_tx.clone());
                tokio::spawn(async move {
                    let _ = replies_tx.send(dispatch(&shared, request).await).await;
                });
            }
            Some(reply) = replies.recv() => {
                if send(&mut io, &Message::Response(reply)).await.is_err() { break }
            }
            received = events.recv() => {
                let note = match received {
                    Received::Event(event) => Notification::event(&event),
                    Received::Missed(n) => {
                        tracing::debug!(%client, missed = n, "client fell behind; sending a snapshot");
                        serde_json::to_value(shared.handler.snapshot()).map(|s| Notification::new(method::SNAPSHOT, s))
                    }
                    Received::Closed => break,
                };
                match note {
                    Ok(note) => if send(&mut io, &Message::Notification(note)).await.is_err() { break },
                    Err(e) => tracing::error!(%e, "couldn't serialize a notification"),
                }
            }
            () = shutdown.cancelled() => break,
        }
    }
    tracing::info!(%client, "IPC client disconnected");
}

/// Reads and checks `hello`. Returns the client's name and the request id to answer, or `None`
/// (after replying with the reason where possible) if the connection must close.
async fn handshake<S: AsyncRead + AsyncWrite + Unpin>(
    io: &mut Frames<S>,
    shared: &Shared,
) -> Option<(String, RequestId)> {
    let first = tokio::time::timeout(shared.config.hello_timeout, io.next()).await;
    let Ok(Some(Ok(frame))) = first else {
        return None;
    };
    let refuse = |id, code, message: &str| {
        Message::Response(Response::err(id, RpcError::new(code, message)))
    };

    let request = match serde_json::from_slice::<Message>(&frame) {
        Ok(Message::Request(r)) => r,
        _ => return None,
    };
    let rejection = if request.method != method::HELLO {
        Some(refuse(
            request.id,
            RpcError::INVALID_REQUEST,
            "the first message must be hello",
        ))
    } else {
        match serde_json::from_value::<Hello>(request.params.clone()) {
            Err(e) => Some(Message::Response(Response::err(
                request.id,
                RpcError::invalid_params(e),
            ))),
            Ok(hello) if !shared.config.token.matches(&hello.token) => {
                tracing::warn!(client = %hello.client, "IPC client presented a wrong session token");
                Some(refuse(
                    request.id,
                    RpcError::UNAUTHORIZED,
                    "wrong session token",
                ))
            }
            Ok(hello) if hello.protocol_version.major != PROTOCOL_VERSION.major => Some(refuse(
                request.id,
                RpcError::INCOMPATIBLE,
                &format!(
                    "this KIVO speaks protocol {}.x but the client speaks {}.x; restart KIVO after updating",
                    PROTOCOL_VERSION.major, hello.protocol_version.major
                ),
            )),
            Ok(hello) => return Some((hello.client, request.id)),
        }
    };
    if let Some(reply) = rejection {
        let _ = send(io, &reply).await;
    }
    None
}

async fn dispatch(shared: &Shared, request: Request) -> Response {
    let Request {
        id,
        method: name,
        params,
        ..
    } = request;
    let outcome = match name.as_str() {
        method::PING => Ok(Value::from("pong")),
        method::STATE => serde_json::to_value(shared.handler.snapshot())
            .map_err(|e| RpcError::new(RpcError::INTERNAL, e.to_string())),
        method::HELLO => Err(RpcError::new(
            RpcError::INVALID_REQUEST,
            "already said hello",
        )),
        _ => shared.handler.call(name, params).await,
    };
    match outcome {
        Ok(v) => Response::ok(id, v),
        Err(e) => Response::err(id, e),
    }
}
