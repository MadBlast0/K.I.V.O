//! On-disk state: the settings file, the SQLite database and the log files (ARCHITECTURE §5).
//! Where those files live is `kivo_platform::Paths`. Secrets live in the OS store behind
//! `kivo-platform::Secrets`. Speech models are downloaded and verified by `models`.

pub mod config;
pub mod crashes;
pub mod db;
pub mod logging;
pub mod models;
pub mod records;
pub mod voice;
pub mod wake;

pub use config::{Loaded, Notice};
pub use db::Database;
pub use kivo_platform::Paths;
