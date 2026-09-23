//! Test fakes (ARCHITECTURE §7): an in-memory platform, a scripted audio device and a scripted
//! brain (with a mock HTTP server for adapter contract tests), so runtime logic is tested on any
//! OS without touching the real desktop or calling a real AI.

pub mod audio;
pub mod control;
pub mod platform;
pub mod testenv;

pub use audio::FakeAudio;
pub use control::{
    FakeClipboard, FakeCommands, FakeDisplays, FakeFiles, FakeInput, FakeOcr, FakePower, FakeUia,
    FakeVerifier, InputAction,
};
pub use kivo_brain::testing::{MockServer, Reply, Script, ScriptedBrain};
pub use platform::{
    FakeApps, FakeAutostart, FakeHotkeys, FakeNotifications, FakeSecrets, FakeSystemControl,
    FakeSystemInfo, FakeThreadQos, FakeTray, FakeWindows, WindowAction,
};
