//! The permission engine (SECURITY §1–2): permission modes, hard limits, the default policy table,
//! confirmations and the `Permit` that every tool execution requires. The types the UI also sees
//! (`ConfirmSpec`, `Strength`, `ConfirmedBy`) live in `kivo_core::tool`.

pub mod policy;

pub use kivo_core::tool::{ConfirmSpec, ConfirmedBy, Strength};
pub use policy::{
    Answer, Context, Decision, Denial, DenyCode, Grant, HardLimits, Permit, SessionKind, Taint,
    authorize, confirmed, render_title,
};
