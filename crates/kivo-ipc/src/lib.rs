//! The local IPC channel between the runtime and its UIs (ARCHITECTURE §3): a per-user named pipe
//! (Unix socket on other platforms), a session token, length-prefixed JSON-RPC 2.0 frames and
//! protocol versioning. UIs are untrusted clients: they authenticate, are validated and size-limited,
//! and get nothing the protocol doesn't expose (INTEGRATIONS_AND_PLUGINS §4). The same framing and
//! JSON-RPC connect the runtime to its `kivo-infer` worker (`peer`, `infer`).

pub mod client;
mod frame;
pub mod infer;
pub mod link;
pub mod peer;
pub mod protocol;
pub mod server;
pub mod token;
pub mod transport;

pub use client::{Client, ClientError, Connection, connect, connect_with_backoff, health};
pub use frame::MAX_FRAME;
pub use link::{Link, LinkStatus};
pub use peer::{Incoming, Peer, PeerError};
pub use protocol::{
    APP_CLIENT, Notification, PROTOCOL_VERSION, RpcError, StateSnapshot, Welcome, method,
};
pub use server::{BoxFuture, Handler, Server, ServerConfig};
pub use token::SessionToken;

#[cfg(test)]
mod tests;
