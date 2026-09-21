//! Core types: events, the session state machine, turns, identifiers and secrets.
//! No OS or provider dependencies (ARCHITECTURE §7, §8).
//!
//! See `docs/architecture/` for the specification this crate implements.

pub mod bus;
pub mod event;
pub mod ids;
pub mod secret;
pub mod session;
pub mod time;
pub mod turn;

pub use bus::{EventBus, Received, Subscription};
pub use event::{Event, EventKind, EventMeta};
pub use ids::{ProfileId, TaskId, TraceId, TurnId};
pub use secret::Secret;
pub use session::{InvalidTransition, Session, SessionInput, SessionState};
pub use time::Timestamp;
pub use turn::Turn;
