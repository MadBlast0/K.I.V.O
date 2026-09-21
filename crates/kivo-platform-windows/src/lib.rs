//! Windows implementations of the `kivo-platform` traits. Empty on other platforms, so the
//! workspace still builds there.

#![cfg(windows)]

mod capabilities;

pub use capabilities::detect as detect_capabilities;
