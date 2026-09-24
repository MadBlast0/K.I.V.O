//! The runtime's parts, as a library so `main.rs` and the end-to-end tests assemble the same
//! pieces (ARCHITECTURE §1): the core state, the turn engine, the voice pipeline, the speech
//! worker's supervisor, the speaker, the records, the IPC handler, the tray and the hotkeys.

pub mod activity;
pub mod agents;
pub mod app;
pub mod args;
pub mod brains;
pub mod brains_rpc;
pub mod browser_bridge;
pub mod catalogs;
pub mod checks;
pub mod computer_tool;
pub mod connectors;
pub mod controls;
pub mod core;
pub mod diagnostics;
pub mod discovery;
pub mod engine;
pub mod extensions_rpc;
#[cfg(windows)]
pub mod gpu;
pub mod hotkeys;
pub mod infer;
pub mod installer;
pub mod lifecycle;
pub mod live_mic;
pub mod managed_browser;
pub mod mcp;
pub mod mcp_bridge;
pub mod memory;
pub mod memory_rpc;
pub mod memory_tools;
pub mod models;
pub mod modes;
pub mod notifier;
pub mod profiles;
pub mod routine_tools;
pub mod routines;
pub mod rpc;
#[cfg(windows)]
pub mod scripted;
pub mod semantic;
pub mod settings_file;
pub mod setup;
pub mod signin;
pub mod skills;
pub mod sounds;
pub mod speaker;
pub mod switch;
pub mod system_rpc;
pub mod task_tools;
pub mod tasks;
pub mod tasks_rpc;
#[cfg(windows)]
pub mod tray;
pub mod triggers;
pub mod updater;
pub mod voice;
pub mod voice_rpc;
pub mod voiceid;
pub mod wake;
pub mod watchers;
pub mod workspaces;
