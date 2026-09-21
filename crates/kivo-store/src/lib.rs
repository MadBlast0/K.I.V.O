//! On-disk state: the settings file, the SQLite database and the log files (ARCHITECTURE §5).
//! Secrets live in the OS store behind `kivo-platform::Secrets`; the model manager joins in M1.

pub mod config;
pub mod db;
pub mod logging;
pub mod paths;

pub use config::{Loaded, Notice};
pub use db::Database;
pub use paths::Paths;
