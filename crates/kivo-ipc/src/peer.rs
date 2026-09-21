//! A two-way JSON-RPC 2.0 connection over any byte stream, in the same frames as the UI pipe
//! (ARCHITECTURE §3). The runtime and `kivo-infer` talk this way over the worker's stdin/stdout:
//! either side can send requests (answered by id) and notifications.

use crate::frame::frames;
use crate::protocol::{Message, Notification, Outcome, Request, RequestId, Response, RpcError};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, oneshot};

/// Something the other side sent that this side must handle.
#[derive(Debug)]
pub enum Incoming {
    Request(Request),
    Notification(Notification),
}

#[derive(Debug, thiserror::Error)]
pub enum PeerError {
    #[error("the connection closed")]
    Closed,
    #[error("{}", .0.message)]
    Remote(RpcError),
}

type Pending = Arc<Mutex<HashMap<RequestId, oneshot::Sender<Outcome>>>>;

/// Sends to the other side. Cheap to clone.
#[derive(Clone)]
pub struct Peer {
    out: mpsc::UnboundedSender<Message>,
    pending: Pending,
    next_id: Arc<AtomicU64>,
}

impl Peer {
    /// Starts reading and writing `stream`. Incoming requests and notifications arrive on the
    /// returned receiver; it ends when the connection closes.
    pub fn spawn<S>(stream: S) -> (Self, mpsc::UnboundedReceiver<Incoming>)
    where
        S: AsyncRead + AsyncWrite + Send + 'static,
    {
        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();
        let (in_tx, in_rx) = mpsc::unbounded_channel();
        let pending: Pending = Arc::default();
        let (mut sink, mut source) = frames(stream).split();

        tokio::spawn(async move {
            while let Some(message) = out_rx.recv().await {
                let Ok(bytes) = serde_json::to_vec(&message) else {
                    continue;
                };
                if sink.send(Bytes::from(bytes)).await.is_err() {
                    break;
                }
            }
        });

        let reader_pending = Arc::clone(&pending);
        tokio::spawn(async move {
            while let Some(Ok(frame)) = source.next().await {
                let message: Message = match serde_json::from_slice(&frame) {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::warn!(%e, "unreadable message from the peer");
                        break;
                    }
                };
                match message {
                    Message::Response(r) => {
                        let waiter = lock(&reader_pending).remove(&r.id);
                        if let Some(waiter) = waiter {
                            let _ = waiter.send(r.outcome);
                        }
                    }
                    Message::Request(r) => {
                        if in_tx.send(Incoming::Request(r)).is_err() {
                            break;
                        }
                    }
                    Message::Notification(n) => {
                        if in_tx.send(Incoming::Notification(n)).is_err() {
                            break;
                        }
                    }
                }
            }
            // Wake everyone still waiting: the connection is gone.
            lock(&reader_pending).clear();
        });

        (
            Self {
                out: out_tx,
                pending,
                next_id: Arc::new(AtomicU64::new(1)),
            },
            in_rx,
        )
    }

    /// Sends a request and waits for its result.
    pub async fn request(&self, method: &str, params: Value) -> Result<Value, PeerError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        lock(&self.pending).insert(id, tx);
        if self
            .out
            .send(Message::Request(Request::new(id, method, params)))
            .is_err()
        {
            lock(&self.pending).remove(&id);
            return Err(PeerError::Closed);
        }
        match rx.await {
            Ok(Outcome::Result(v)) => Ok(v),
            Ok(Outcome::Error(e)) => Err(PeerError::Remote(e)),
            Err(_) => Err(PeerError::Closed),
        }
    }

    /// Sends a notification (no reply).
    pub fn notify(&self, method: &str, params: Value) -> Result<(), PeerError> {
        self.out
            .send(Message::Notification(Notification::new(method, params)))
            .map_err(|_| PeerError::Closed)
    }

    /// Answers a request this side received.
    pub fn respond(&self, id: RequestId, result: Result<Value, RpcError>) -> Result<(), PeerError> {
        let response = match result {
            Ok(v) => Response::ok(id, v),
            Err(e) => Response::err(id, e),
        };
        self.out
            .send(Message::Response(response))
            .map_err(|_| PeerError::Closed)
    }

    /// True once the writer has stopped (the connection closed).
    pub fn is_closed(&self) -> bool {
        self.out.is_closed()
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn requests_notifications_and_errors_flow_both_ways() {
        let (a, b) = tokio::io::duplex(64 * 1024);
        let (left, _left_in) = Peer::spawn(a);
        let (right, mut right_in) = Peer::spawn(b);
        tokio::spawn(async move {
            while let Some(incoming) = right_in.recv().await {
                match incoming {
                    Incoming::Request(r) if r.method == "echo" => {
                        right.respond(r.id, Ok(r.params)).unwrap();
                    }
                    Incoming::Request(r) => {
                        right
                            .respond(r.id, Err(RpcError::method_not_found(&r.method)))
                            .unwrap();
                    }
                    Incoming::Notification(n) => {
                        right.notify("seen", json!({ "method": n.method })).unwrap();
                    }
                }
            }
        });
        assert_eq!(
            left.request("echo", json!({"x": 1})).await.unwrap(),
            json!({"x": 1})
        );
        let err = left.request("nope", Value::Null).await.unwrap_err();
        assert!(err.to_string().contains("nope"), "{err}");
    }

    #[tokio::test]
    async fn waiting_requests_fail_when_the_other_side_goes_away() {
        let (a, b) = tokio::io::duplex(1024);
        let (left, _in) = Peer::spawn(a);
        let pending = tokio::spawn(async move { left.request("slow", Value::Null).await });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        drop(b);
        assert!(matches!(pending.await.unwrap(), Err(PeerError::Closed)));
    }
}
