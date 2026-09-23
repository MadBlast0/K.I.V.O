//! KIVO's memory vault (CONVERSATION §6, MEMORY.md).
//!
//! What KIVO remembers lives in plain Markdown files the user can read and edit — an
//! Obsidian-compatible vault in `%APPDATA%\KIVO\memory\`: `about-me.md`, `people/`,
//! `workspaces/<name>/` (overview, decisions, daily logs), `topics/`, `notes/` (single facts the
//! user asked KIVO to remember) and `archive/`. Each note has YAML front-matter (`type`, `tags`,
//! `workspace`, `sensitivity`, `created`, `updated`, `valid_until`, …) and `[[wikilinks]]`.
//!
//! The files are the source of truth: SQLite (in `kivo-store`) is only the index, rebuilt from
//! them, so an edit made in Obsidian always wins. This crate is the pure part — the note format,
//! reading and writing the vault safely, links and tags, the graph a note describes, and the rules
//! that keep memory small (duplicates, superseded facts, condensed logs) — with no database and no
//! clock of its own (callers pass the time).

pub mod date;
pub mod graph;
pub mod note;
pub mod query;
pub mod tidy;
pub mod vault;

pub use note::{FrontMatter, Kind, Note};
pub use vault::{Vault, VaultError};
