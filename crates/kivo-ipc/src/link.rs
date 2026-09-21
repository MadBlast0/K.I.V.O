//! What an IPC client shows about its connection to the runtime. The desktop app keeps one `Link`
//! and hands it to its UI, so the Rust side stays the source of truth for the TypeScript type.

use crate::protocol::StateSnapshot;
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkStatus {
    /// First connection attempt (starting the runtime if it isn't running).
    Connecting,
    Connected,
    /// The runtime went away; retrying until it is back.
    Reconnecting,
    /// The runtime speaks another protocol version (a partial update); KIVO must be restarted.
    Incompatible,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    pub status: LinkStatus,
    /// The connected runtime's version.
    pub runtime_version: Option<String>,
    /// The last state received; kept while reconnecting so the UI can show what it last knew.
    pub snapshot: Option<StateSnapshot>,
    /// Why the link is not connected, when there is something to say.
    pub message: Option<String>,
}

impl Link {
    pub fn connecting() -> Self {
        Self {
            status: LinkStatus::Connecting,
            runtime_version: None,
            snapshot: None,
            message: None,
        }
    }
}
