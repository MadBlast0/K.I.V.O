//! The UI side of the IPC channel: connect, say `hello`, then send requests and receive
//! notifications. `connect_with_backoff` retries until the runtime is reachable (ARCHITECTURE §3:
//! the UI reconnects with backoff and gets a fresh snapshot each time).

use crate::frame::{Frames, frames};
use crate::protocol::{
    Hello, Message, Notification, Outcome, PROTOCOL_VERSION, Request, RequestId, RpcError, Welcome,
    method,
};
use crate::transport;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
/// Notifications waiting for the app. If it stops reading, backpressure reaches the runtime,
/// which then sends a snapshot instead of the missed events.
const NOTIFICATION_BUFFER: usize = 256;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("couldn't reach KIVO: {0}")]
    Io(#[from] std::io::Error),
    #[error("KIVO refused the connection: {0}")]
    Rejected(RpcError),
    #[error("KIVO sent something unexpected: {0}")]
    Protocol(String),
    #[error("the connection to KIVO closed")]
    Closed,
    #[error("cancelled")]
    Cancelled,
}

type Pending = Arc<Mutex<HashMap<RequestId, oneshot::Sender<Outcome>>>>;

/// A live connection. Cloning shares it.
#[derive(Clone)]
pub struct Client {
    out: mpsc::Sender<Request>,
    pending: Pending,
    next_id: Arc<AtomicU64>,
}

pub struct Connection {
    pub client: Client,
    pub welcome: Welcome,
    /// Pushed by the runtime; ends when the connection closes.
    pub notifications: mpsc::Receiver<Notification>,
}

/// Connects once. `token` is the contents of the session token file.
pub async fn connect(
    endpoint: &str,
    token: &str,
    client_name: &str,
) -> Result<Connection, ClientError> {
    let stream = transport::connect(endpoint).await?;
    start(frames(stream), token, client_name).await
}

/// Connects, retrying with backoff (100 ms doubling to 2 s) until it works or `cancel` fires.
/// `read_token` runs before every attempt: a restarted runtime writes a new token.
/// Gives up immediately on an incompatible protocol, which retrying can't fix.
pub async fn connect_with_backoff(
    endpoint: &str,
    read_token: impl Fn() -> std::io::Result<String>,
    client_name: &str,
    cancel: &CancellationToken,
) -> Result<Connection, ClientError> {
    let mut delay = Duration::from_millis(100);
    loop {
        let attempt = async {
            let token = read_token()?;
            connect(endpoint, &token, client_name).await
        };
        match attempt.await {
            Ok(connection) => return Ok(connection),
            Err(ClientError::Rejected(e)) if e.code == RpcError::INCOMPATIBLE => {
                return Err(ClientError::Rejected(e));
            }
            Err(e) => {
                tracing::debug!(%e, retry_in_ms = delay.as_millis(), "runtime not reachable yet")
            }
        }
        tokio::select! {
            () = tokio::time::sleep(delay) => {}
            () = cancel.cancelled() => return Err(ClientError::Cancelled),
        }
        delay = (delay * 2).min(Duration::from_secs(2));
    }
}

async fn send<S: AsyncRead + AsyncWrite + Unpin>(
    io: &mut Frames<S>,
    message: &Message,
) -> Result<(), ClientError> {
    let bytes = serde_json::to_vec(message).map_err(|e| ClientError::Protocol(e.to_string()))?;
    Ok(io.send(Bytes::from(bytes)).await?)
}

async fn start<S>(
    mut io: Frames<S>,
    token: &str,
    client_name: &str,
) -> Result<Connection, ClientError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let hello = Hello {
        protocol_version: PROTOCOL_VERSION,
        client: client_name.to_owned(),
        token: token.to_owned(),
    };
    let params = serde_json::to_value(hello).map_err(|e| ClientError::Protocol(e.to_string()))?;
    send(
        &mut io,
        &Message::Request(Request::new(0, method::HELLO, params)),
    )
    .await?;

    let reply = tokio::time::timeout(HELLO_TIMEOUT, io.next())
        .await
        .map_err(|_| ClientError::Closed)?;
    let frame = reply.ok_or(ClientError::Closed)??;
    let welcome = match serde_json::from_slice::<Message>(&frame) {
        Ok(Message::Response(r)) if r.id == 0 => match r.outcome {
            Outcome::Result(v) => serde_json::from_value::<Welcome>(v)
                .map_err(|e| ClientError::Protocol(e.to_string()))?,
            Outcome::Error(e) => return Err(ClientError::Rejected(e)),
        },
        other => {
            return Err(ClientError::Protocol(format!(
                "expected the hello reply, got {other:?}"
            )));
        }
    };

    let (out_tx, out_rx) = mpsc::channel(64);
    let (note_tx, note_rx) = mpsc::channel(NOTIFICATION_BUFFER);
    let pending: Pending = Arc::default();
    tokio::spawn(pump(io, out_rx, note_tx, Arc::clone(&pending)));
    Ok(Connection {
        client: Client {
            out: out_tx,
            pending,
            next_id: Arc::new(AtomicU64::new(1)),
        },
        welcome,
        notifications: note_rx,
    })
}

/// Moves messages between the socket and the `Client` handles until either side closes.
async fn pump<S>(
    mut io: Frames<S>,
    mut out: mpsc::Receiver<Request>,
    notes: mpsc::Sender<Notification>,
    pending: Pending,
) where
    S: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        tokio::select! {
            frame = io.next() => {
                let Some(Ok(frame)) = frame else { break };
                match serde_json::from_slice::<Message>(&frame) {
                    Ok(Message::Response(r)) => {
                        let waiter = pending.lock().unwrap_or_else(PoisonError::into_inner).remove(&r.id);
                        if let Some(waiter) = waiter {
                            let _ = waiter.send(r.outcome);
                        }
                    }
                    Ok(Message::Notification(n)) => {
                        if notes.send(n).await.is_err() { break } // the app dropped the receiver
                    }
                    Ok(Message::Request(_)) | Err(_) => {
                        tracing::warn!("unexpected message from the runtime; closing");
                        break;
                    }
                }
            }
            request = out.recv() => {
                let Some(request) = request else { break }; // every Client handle dropped
                if send(&mut io, &Message::Request(request)).await.is_err() { break }
            }
        }
    }
    // Dropping the waiters makes every in-flight request fail with `Closed`.
    pending
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
}

impl Client {
    pub async fn request(&self, method: &str, params: Value) -> Result<Value, ClientError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, tx);
        if self
            .out
            .send(Request::new(id, method, params))
            .await
            .is_err()
        {
            self.pending
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&id);
            return Err(ClientError::Closed);
        }
        match rx.await.map_err(|_| ClientError::Closed)? {
            Outcome::Result(v) => Ok(v),
            Outcome::Error(e) => Err(ClientError::Rejected(e)),
        }
    }
}
