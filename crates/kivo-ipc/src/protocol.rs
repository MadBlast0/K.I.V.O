//! The wire protocol (ARCHITECTURE §3): JSON-RPC 2.0 messages in length-prefixed frames.
//!
//! A connection starts with `hello` (carrying the session token and protocol version). The
//! reply is a full `StateSnapshot`; after that the runtime pushes `event` notifications, and a
//! fresh `snapshot` notification whenever the client may have missed events.

use kivo_core::{Event, SessionState};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Bumped on breaking changes (major) and additions (minor). Clients with another major
/// version are refused with a clear error (for example after a partial update).
pub const PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion { major: 1, minor: 0 };

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
}

pub mod method {
    /// Client → runtime, first message: authenticate and get the initial state.
    pub const HELLO: &str = "hello";
    /// Client → runtime: a liveness check.
    pub const PING: &str = "ping";
    /// Client → runtime: the current state snapshot.
    pub const STATE: &str = "state.get";
    /// Runtime → client notification: one event from the bus.
    pub const EVENT: &str = "event";
    /// Runtime → client notification: a full snapshot (after events were missed).
    pub const SNAPSHOT: &str = "snapshot";
}

/// `hello` parameters.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hello {
    pub protocol_version: ProtocolVersion,
    /// Who is connecting, e.g. "kivo-app 0.1.0".
    pub client: String,
    /// The contents of the session token file.
    pub token: String,
}

/// `hello` result.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Welcome {
    pub protocol_version: ProtocolVersion,
    pub runtime_version: String,
    pub snapshot: StateSnapshot,
}

/// Everything a UI needs to render from scratch (ARCHITECTURE §3: full snapshot, then deltas).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateSnapshot {
    pub session: SessionState,
    /// Increases with every state change, so a client can tell whether its view is current.
    pub revision: u64,
}

/// A JSON-RPC 2.0 request id (numbers only; KIVO's clients never send strings).
pub type RequestId = u64;

/// One JSON-RPC message on the wire.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Message {
    Request(Request),
    Response(Response),
    Notification(Notification),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub jsonrpc: JsonRpcV2,
    pub id: RequestId,
    pub method: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notification {
    pub jsonrpc: JsonRpcV2,
    pub method: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub jsonrpc: JsonRpcV2,
    pub id: RequestId,
    #[serde(flatten)]
    pub outcome: Outcome,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Result(Value),
    Error(RpcError),
}

/// The literal `"2.0"`; anything else is rejected while parsing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JsonRpcV2;

impl Serialize for JsonRpcV2 {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str("2.0")
    }
}

impl<'de> Deserialize<'de> for JsonRpcV2 {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = String::deserialize(d)?;
        if v == "2.0" {
            Ok(Self)
        } else {
            Err(serde::de::Error::custom("jsonrpc must be \"2.0\""))
        }
    }
}

/// A JSON-RPC error. `message` is safe to show the user.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{message} ({code})")]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

impl RpcError {
    pub const PARSE: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL: i32 = -32603;
    /// Wrong or missing session token.
    pub const UNAUTHORIZED: i32 = -32001;
    /// Different protocol major version.
    pub const INCOMPATIBLE: i32 = -32002;

    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn method_not_found(method: &str) -> Self {
        Self::new(Self::METHOD_NOT_FOUND, format!("unknown method {method}"))
    }

    pub fn invalid_params(detail: impl std::fmt::Display) -> Self {
        Self::new(
            Self::INVALID_PARAMS,
            format!("invalid parameters: {detail}"),
        )
    }
}

impl Request {
    pub fn new(id: RequestId, method: &str, params: Value) -> Self {
        Self {
            jsonrpc: JsonRpcV2,
            id,
            method: method.to_owned(),
            params,
        }
    }
}

impl Notification {
    pub fn new(method: &str, params: Value) -> Self {
        Self {
            jsonrpc: JsonRpcV2,
            method: method.to_owned(),
            params,
        }
    }

    /// An `event` notification carrying one bus event.
    pub fn event(event: &Event) -> Result<Self, serde_json::Error> {
        Ok(Self::new(method::EVENT, serde_json::to_value(event)?))
    }
}

impl Response {
    pub fn ok(id: RequestId, result: Value) -> Self {
        Self {
            jsonrpc: JsonRpcV2,
            id,
            outcome: Outcome::Result(result),
        }
    }

    pub fn err(id: RequestId, error: RpcError) -> Self {
        Self {
            jsonrpc: JsonRpcV2,
            id,
            outcome: Outcome::Error(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(v: Value) -> Result<Message, serde_json::Error> {
        serde_json::from_value(v)
    }

    #[test]
    fn parses_each_message_kind() {
        let req = parse(json!({"jsonrpc":"2.0","id":7,"method":"ping"})).unwrap();
        assert_eq!(req, Message::Request(Request::new(7, "ping", Value::Null)));
        let note = parse(json!({"jsonrpc":"2.0","method":"event","params":{"x":1}})).unwrap();
        assert!(matches!(note, Message::Notification(n) if n.method == "event"));
        let ok = parse(json!({"jsonrpc":"2.0","id":7,"result":"pong"})).unwrap();
        assert_eq!(ok, Message::Response(Response::ok(7, json!("pong"))));
        let err = parse(
            json!({"jsonrpc":"2.0","id":7,"error":{"code":-32601,"message":"unknown method x"}}),
        )
        .unwrap();
        assert_eq!(
            err,
            Message::Response(Response::err(7, RpcError::method_not_found("x")))
        );
    }

    #[test]
    fn rejects_malformed_messages() {
        for bad in [
            json!({"jsonrpc":"1.0","id":1,"method":"ping"}),
            json!({"id":1,"method":"ping"}),
            json!({"jsonrpc":"2.0","id":"a","method":"ping"}),
            json!({"jsonrpc":"2.0","id":1,"method":"ping","extra":true}),
            json!({"jsonrpc":"2.0","id":1}),
        ] {
            assert!(parse(bad.clone()).is_err(), "{bad}");
        }
    }

    #[test]
    fn serializes_responses_in_the_standard_shape() {
        let v = serde_json::to_value(Response::err(
            3,
            RpcError::new(RpcError::UNAUTHORIZED, "wrong token"),
        ))
        .unwrap();
        assert_eq!(
            v,
            json!({"jsonrpc":"2.0","id":3,"error":{"code":-32001,"message":"wrong token"}})
        );
    }
}
