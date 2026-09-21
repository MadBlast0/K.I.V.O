//! The suites (BENCHMARKS §1). Each lives in its own module.

use crate::harness::Suite;

mod ipc;

/// Every suite, with a one-line description for `kivo-bench list`.
pub const ALL: &[(&str, &str)] = &[(
    "ipc",
    "runtime ↔ UI channel: connect, ping and state round trips",
)];

pub fn create(name: &str) -> Result<Box<dyn Suite>, String> {
    match name {
        "ipc" => Ok(Box::new(ipc::Ipc::start()?)),
        other => Err(format!("unknown suite \"{other}\" (see `kivo-bench list`)")),
    }
}
