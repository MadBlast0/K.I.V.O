//! The API adapters' HTTP: one client, JSON requests, server-sent-event streams that stop the
//! moment the turn is cancelled, and HTTP failures mapped onto `NormalizedError`.

use crate::sse::{SseEvent, SseParser};
use crate::types::{BrainEvent, NormalizedError};
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// How long connecting may take.
const CONNECT: Duration = Duration::from_secs(10);
/// A stream that sends nothing for this long is taken as dead.
const IDLE: Duration = Duration::from_secs(90);

/// A header; `secret` ones are marked sensitive (never printed by the HTTP stack).
pub struct Header {
    pub name: &'static str,
    pub value: String,
    pub secret: bool,
}

impl Header {
    pub fn plain(name: &'static str, value: impl Into<String>) -> Self {
        Self {
            name,
            value: value.into(),
            secret: false,
        }
    }

    pub fn secret(name: &'static str, value: impl Into<String>) -> Self {
        Self {
            name,
            value: value.into(),
            secret: true,
        }
    }
}

#[derive(Clone)]
pub struct Http {
    client: reqwest::Client,
}

impl Default for Http {
    fn default() -> Self {
        Self::new()
    }
}

impl Http {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT)
            .read_timeout(IDLE)
            .user_agent(concat!("KIVO/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_default();
        Self { client }
    }

    fn headers(headers: &[Header]) -> Result<HeaderMap, NormalizedError> {
        let mut map = HeaderMap::new();
        for h in headers {
            let mut value = HeaderValue::from_str(&h.value)
                .map_err(|_| NormalizedError::Other(format!("bad header {}", h.name)))?;
            value.set_sensitive(h.secret);
            map.insert(HeaderName::from_static(h.name), value);
        }
        Ok(map)
    }

    /// GETs JSON (model lists, health).
    pub async fn get_json(
        &self,
        url: &str,
        headers: &[Header],
        cancel: &CancellationToken,
    ) -> Result<Value, NormalizedError> {
        let request = self.client.get(url).headers(Self::headers(headers)?).send();
        let response = tokio::select! {
            r = request => r.map_err(network)?,
            () = cancel.cancelled() => return Err(NormalizedError::Cancelled),
        };
        let response = check(response).await?;
        tokio::select! {
            body = response.json::<Value>() => body.map_err(|e| NormalizedError::Other(e.to_string())),
            () = cancel.cancelled() => Err(NormalizedError::Cancelled),
        }
    }

    /// POSTs JSON and reads a JSON answer (sign-in code exchanges).
    pub async fn post_json(
        &self,
        url: &str,
        headers: &[Header],
        body: &Value,
        cancel: &CancellationToken,
    ) -> Result<Value, NormalizedError> {
        let request = self
            .client
            .post(url)
            .headers(Self::headers(headers)?)
            .json(body)
            .send();
        let response = tokio::select! {
            r = request => r.map_err(network)?,
            () = cancel.cancelled() => return Err(NormalizedError::Cancelled),
        };
        let response = check(response).await?;
        tokio::select! {
            body = response.json::<Value>() => body.map_err(|e| NormalizedError::Other(e.to_string())),
            () = cancel.cancelled() => Err(NormalizedError::Cancelled),
        }
    }

    /// POSTs a chat request and streams the answer: each server-sent event goes through
    /// `decoder`, and its events are sent on `tx` (waiting while the consumer is busy). Ends
    /// with the decoder's `finish`, or with one `Error` event: a failure, or `Cancelled` the
    /// moment `cancel` fires (the connection is dropped). Stops if the consumer goes away.
    pub async fn stream_chat(
        &self,
        url: &str,
        headers: &[Header],
        body: &Value,
        cancel: &CancellationToken,
        decoder: &mut dyn Decode,
        tx: &mpsc::Sender<BrainEvent>,
    ) {
        if let Err(e) = self
            .stream_inner(url, headers, body, cancel, decoder, tx)
            .await
        {
            let _ = tx.send(BrainEvent::Error(e)).await;
        }
    }

    async fn stream_inner(
        &self,
        url: &str,
        headers: &[Header],
        body: &Value,
        cancel: &CancellationToken,
        decoder: &mut dyn Decode,
        tx: &mpsc::Sender<BrainEvent>,
    ) -> Result<(), NormalizedError> {
        let request = self
            .client
            .post(url)
            .headers(Self::headers(headers)?)
            .header("accept", "text/event-stream")
            .json(body)
            .send();
        let response = tokio::select! {
            r = request => r.map_err(network)?,
            () = cancel.cancelled() => return Err(NormalizedError::Cancelled),
        };
        let response = check(response).await?;
        let mut bytes = response.bytes_stream();
        let mut parser = SseParser::default();
        let mut out = Vec::new();
        loop {
            let chunk = tokio::select! {
                c = bytes.next() => c,
                () = cancel.cancelled() => return Err(NormalizedError::Cancelled),
            };
            let events = match chunk {
                Some(Ok(chunk)) => parser.feed(&chunk),
                Some(Err(e)) => return Err(network(e)),
                None => {
                    if let Some(event) = parser.finish() {
                        decoder.decode(&event, &mut out);
                    }
                    decoder.finish(&mut out);
                    send_all(&mut out, tx, cancel).await?;
                    return Ok(());
                }
            };
            for event in events {
                decoder.decode(&event, &mut out);
            }
            if let Some(i) = out.iter().position(|e| matches!(e, BrainEvent::Error(_))) {
                // A failure inside the stream ends it: pass on what came before, then the error.
                out.truncate(i + 1);
                send_all(&mut out, tx, cancel).await?;
                return Ok(());
            }
            send_all(&mut out, tx, cancel).await?;
        }
    }
}

/// Turns a provider's server-sent events into `BrainEvent`s.
pub trait Decode: Send {
    fn decode(&mut self, event: &SseEvent, out: &mut Vec<BrainEvent>);
    /// The stream ended cleanly: finished tool calls, then `Done`.
    fn finish(&mut self, out: &mut Vec<BrainEvent>);
}

async fn send_all(
    out: &mut Vec<BrainEvent>,
    tx: &mpsc::Sender<BrainEvent>,
    cancel: &CancellationToken,
) -> Result<(), NormalizedError> {
    for event in out.drain(..) {
        tokio::select! {
            sent = tx.send(event) => if sent.is_err() {
                // Nobody is listening any more: stop quietly.
                return Err(NormalizedError::Cancelled);
            },
            () = cancel.cancelled() => return Err(NormalizedError::Cancelled),
        }
    }
    Ok(())
}

fn network(e: reqwest::Error) -> NormalizedError {
    if e.is_timeout() || e.is_connect() || e.is_request() {
        NormalizedError::Network(e.without_url().to_string())
    } else {
        NormalizedError::Other(e.without_url().to_string())
    }
}

/// Passes a successful response on; turns a failed one into a `NormalizedError`.
async fn check(response: reqwest::Response) -> Result<reqwest::Response, NormalizedError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_secs);
    let body = response.text().await.unwrap_or_default();
    Err(error_for(status.as_u16(), retry_after, &body))
}

/// The normalized error for an HTTP failure, using the provider's message where it helps.
pub fn error_for(status: u16, retry_after: Option<Duration>, body: &str) -> NormalizedError {
    let lower = body.to_lowercase();
    let message = || {
        serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|v| {
                v.pointer("/error/message")
                    .or_else(|| v.get("message"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| format!("HTTP {status}"))
    };
    let too_long = [
        "context length",
        "context_length",
        "too long",
        "maximum context",
        "prompt is too long",
        "token limit",
    ]
    .iter()
    .any(|w| lower.contains(w));
    match status {
        401 | 403 => NormalizedError::Auth,
        402 => NormalizedError::Quota,
        429 if lower.contains("quota") || lower.contains("credit") || lower.contains("billing") => {
            NormalizedError::Quota
        }
        429 => NormalizedError::RateLimited { retry_after },
        400 | 413 if too_long => NormalizedError::ContextTooLong,
        400 if lower.contains("safety")
            || lower.contains("content_filter")
            || lower.contains("moderation") =>
        {
            NormalizedError::ContentFiltered
        }
        500..=599 => NormalizedError::ProviderDown(message()),
        _ => NormalizedError::Other(message()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_failures_map_onto_the_normalized_errors() {
        assert_eq!(error_for(401, None, ""), NormalizedError::Auth);
        assert_eq!(
            error_for(429, Some(Duration::from_secs(3)), "{}"),
            NormalizedError::RateLimited {
                retry_after: Some(Duration::from_secs(3))
            }
        );
        assert_eq!(
            error_for(
                429,
                None,
                r#"{"error":{"message":"You exceeded your current quota"}}"#
            ),
            NormalizedError::Quota
        );
        assert_eq!(
            error_for(
                400,
                None,
                r#"{"error":{"message":"prompt is too long: 250000 tokens"}}"#
            ),
            NormalizedError::ContextTooLong
        );
        assert_eq!(
            error_for(529, None, r#"{"error":{"message":"Overloaded"}}"#),
            NormalizedError::ProviderDown("Overloaded".into())
        );
        assert_eq!(error_for(402, None, ""), NormalizedError::Quota);
    }
}
