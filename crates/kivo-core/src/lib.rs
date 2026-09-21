//! Core types: events, the session state machine, turns, the settings schema, identifiers and
//! secrets.
//! No OS or provider dependencies (ARCHITECTURE §7, §8).
//!
//! See `docs/architecture/` for the specification this crate implements.

pub mod bus;
pub mod config;
pub mod event;
pub mod ids;
pub mod secret;
pub mod session;
pub mod time;
pub mod turn;

pub use bus::{EventBus, Received, Subscription};
pub use config::{CONFIG_SCHEMA_VERSION, KivoConfig};
pub use event::{Event, EventKind, EventMeta};
pub use ids::{ProfileId, TaskId, TraceId, TurnId};
pub use secret::Secret;
pub use session::{InvalidTransition, Session, SessionInput, SessionState};
pub use time::Timestamp;
pub use turn::Turn;
