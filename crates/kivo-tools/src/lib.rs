//! Tools (TOOLS_AND_CONTROL §1–3, §5): the registry, which only holds tools whose capability is on;
//! the executor, which runs a call only with the permission engine's `Permit` for it; and KIVO's
//! built-in native tools.

pub mod builtin;
pub mod registry;

pub use builtin::{Env, builtin, platform_error, targets};
pub use registry::{Output, Registry, Tool, execute};
