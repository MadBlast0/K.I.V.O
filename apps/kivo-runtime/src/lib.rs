//! The runtime's parts, as a library so `main.rs` and the end-to-end tests assemble the same
//! pieces (ARCHITECTURE §1): the core state, the turn engine, the voice pipeline, the speech
//! worker's supervisor, the speaker, the records, the IPC handler, the tray and the hotkeys.

pub mod activity;
pub mod app;
pub mod args;
pub mod core;
pub mod engine;
#[cfg(windows)]
pub mod hotkeys;
pub mod infer;
pub mod lifecycle;
pub mod models;
pub mod rpc;
pub mod speaker;
#[cfg(windows)]
pub mod tray;
pub mod voice;
