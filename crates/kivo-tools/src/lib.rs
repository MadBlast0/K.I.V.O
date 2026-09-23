//! Tools (TOOLS_AND_CONTROL §1–8): the registry, which only holds tools whose capability is on;
//! the executor, which runs a call only with the permission engine's `Permit` for it; KIVO's
//! built-in native tools; and the M4 computer-control tools with the capability-ladder router and
//! the App Capability Registry.

pub mod appreg;
pub mod browser;
pub mod builtin;
pub mod controls;
pub mod files;
mod input_tools;
mod plugin_shape;
pub mod registry;
pub mod router;
pub mod screen_tools;
mod security_suite;
pub mod shell;
mod system_tools;
#[cfg(test)]
mod testing;
mod uia_tools;

pub use appreg::AppRegistry;
pub use browser::{Browser, ManagedBrowser, NoExtension};
pub use builtin::{Env, builtin, platform_error, targets};
pub use controls::{Controls, controls};
pub use registry::{Output, Registry, Tool, execute};
