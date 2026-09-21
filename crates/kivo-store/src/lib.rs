//! On-disk state: the settings file, the SQLite database and the log files (ARCHITECTURE §5).
//! Where those files live is `kivo_platform::Paths`. Secrets live in the OS store behind
//! `kivo-platform::Secrets`; the model manager joins in M1.

pub mod config;
pub mod db;
pub mod logging;

pub use config::{Loaded, Notice};
pub use db::Database;
pub use kivo_platform::Paths;
