//! The permission engine (SECURITY §1–2): permission modes, hard limits, the default policy table,
//! confirmations and the `Permit` that every tool execution requires.

pub mod policy;

pub use policy::{
    Answer, ConfirmSpec, ConfirmedBy, Context, Decision, Denial, DenyCode, Grant, HardLimits,
    Permit, SessionKind, Strength, Taint, authorize, confirmed, render_title,
};
