//! Test fakes (ARCHITECTURE §7): an in-memory platform and a scripted audio device, so runtime
//! logic is tested on any OS without touching the real desktop. A fake brain joins when the
//! `BrainProvider` trait exists (M3).

pub mod audio;
pub mod platform;

pub use audio::FakeAudio;
pub use platform::{
    FakeApps, FakeAutostart, FakeHotkeys, FakeNotifications, FakeSecrets, FakeSystemControl,
    FakeSystemInfo, FakeTray, FakeWindows, WindowAction,
};
