//! Platform abstraction (ARCHITECTURE §2). Every OS-specific capability KIVO uses is a trait here;
//! core crates depend only on these traits, never on Win32, AppKit or D-Bus. Each OS provides one
//! implementation crate (`kivo-platform-windows` first). Test fakes live in `kivo-testkit`.
//!
//! The traits are object-safe and synchronous: the runtime holds them as `Arc<dyn …>` and moves
//! slow calls onto blocking threads. Streams (audio, hotkey presses, UI events) are delivered
//! through callbacks or channels given to the implementation when it is created.

mod apps;
mod audio;
mod automation;
mod capabilities;
mod control;
mod error;
mod paths;
mod screen;
mod secrets;
mod shell;
mod speech;
mod system;
mod types;

pub use apps::{AppEntry, Apps, WindowInfo, Windows};
pub use audio::{
    AecKind, AudioDevice, AudioIo, AudioStream, EchoCancel, FrameSink, FrameSource, StreamFormat,
};
pub use automation::{
    ElementQuery, ElementRef, Input, MouseButton, UiAction, UiAutomation, UiEvent, UiEventKind,
    UiEventSink, UiNode, UiSubscription,
};
pub use capabilities::{Capabilities, OsFamily};
pub use control::{
    Battery, Clipboard, CommandOutput, CommandRunner, CommandSpec, Displays, FileHit, FileOps,
    Monitor, Power, ShellKind, Snap, UserVerifier,
};
pub use error::{PlatformError, PlatformResult};
pub use kivo_core::Secret;
pub use paths::Paths;
pub use screen::{CaptureTarget, Image, Ocr, Screen, TextLine};
pub use secrets::{SecretHandle, Secrets};
pub use shell::{
    Binding, Chord, HotkeyEvent, HotkeyId, Hotkeys, Notification, NotificationAction,
    Notifications, Tray, TrayIcon, TrayMenuItem,
};
pub use speech::{SpeechSynth, SynthAudio, SystemVoice};
pub use system::{
    Attention, Autostart, GpuInfo, MediaAction, NowPlaying, PowerAction, SystemControl, SystemInfo,
    SystemSnapshot, ThreadQos, VolumeState,
};
pub use types::{DeviceId, Point, Rect, WindowId};
