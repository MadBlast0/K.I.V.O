//! In-memory fakes of the `kivo-platform` traits. They record what was asked of them and can be
//! told to fail, so runtime logic is testable on any OS without touching the real desktop.

use kivo_core::Secret;
use kivo_platform::{
    AppEntry, Apps, Binding, Chord, HotkeyId, Hotkeys, MediaAction, Notification, Notifications,
    NowPlaying, PlatformError, PlatformResult, PowerAction, SecretHandle, Secrets, SystemControl,
    SystemInfo, SystemSnapshot, Tray, TrayIcon, TrayMenuItem, VolumeState, WindowId, WindowInfo,
    Windows,
};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

/// Locks a fake's state. A poisoned lock means another test thread panicked; carry on with the data.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Hotkeys: combinations in `taken` bind as `Shared`, like one another app registered.
#[derive(Default)]
pub struct FakeHotkeys {
    pub taken: Mutex<HashSet<Chord>>,
    pub registered: Mutex<HashMap<HotkeyId, Chord>>,
}

impl Hotkeys for FakeHotkeys {
    fn register(&self, id: HotkeyId, chord: &Chord) -> PlatformResult<Binding> {
        if lock(&self.registered).contains_key(&id) {
            return Err(PlatformError::Conflict(format!("hotkey id {}", id.0)));
        }
        lock(&self.registered).insert(id, chord.clone());
        Ok(if lock(&self.taken).contains(chord) {
            Binding::Shared
        } else {
            Binding::System
        })
    }

    fn unregister(&self, id: HotkeyId) -> PlatformResult<()> {
        lock(&self.registered)
            .remove(&id)
            .map(|_| ())
            .ok_or_else(|| PlatformError::NotFound(format!("hotkey {}", id.0)))
    }
}

/// Tray: remembers the last icon, tooltip and menu.
#[derive(Default)]
pub struct FakeTray {
    pub icon: Mutex<Option<TrayIcon>>,
    pub tooltip: Mutex<String>,
    pub menu: Mutex<Vec<TrayMenuItem>>,
}

impl Tray for FakeTray {
    fn set_icon(&self, icon: TrayIcon) -> PlatformResult<()> {
        *lock(&self.icon) = Some(icon);
        Ok(())
    }
    fn set_tooltip(&self, text: &str) -> PlatformResult<()> {
        text.clone_into(&mut lock(&self.tooltip));
        Ok(())
    }
    fn set_menu(&self, items: &[TrayMenuItem]) -> PlatformResult<()> {
        *lock(&self.menu) = items.to_vec();
        Ok(())
    }
}

/// Notifications: records everything shown.
#[derive(Default)]
pub struct FakeNotifications {
    pub shown: Mutex<Vec<Notification>>,
}

impl Notifications for FakeNotifications {
    fn show(&self, notification: &Notification) -> PlatformResult<()> {
        lock(&self.shown).push(notification.clone());
        Ok(())
    }
}

/// Apps: a fixed list; launches and closes are recorded, unknown apps fail with `NotFound`.
/// An app counts as running once launched.
#[derive(Default)]
pub struct FakeApps {
    pub installed: Vec<AppEntry>,
    pub launched: Mutex<Vec<String>>,
    pub closed: Mutex<Vec<String>>,
}

impl Apps for FakeApps {
    fn installed(&self) -> PlatformResult<Vec<AppEntry>> {
        Ok(self.installed.clone())
    }
    fn launch(&self, app: &AppEntry, _args: &[String]) -> PlatformResult<()> {
        if !self.installed.iter().any(|a| a.id == app.id) {
            return Err(PlatformError::NotFound(app.name.clone()));
        }
        lock(&self.launched).push(app.id.clone());
        Ok(())
    }
    fn close(&self, app: &AppEntry) -> PlatformResult<usize> {
        if !lock(&self.launched).contains(&app.id) {
            return Err(PlatformError::NotFound(app.name.clone()));
        }
        lock(&self.launched).retain(|id| id != &app.id);
        lock(&self.closed).push(app.id.clone());
        Ok(1)
    }
    fn running(&self, app: &AppEntry) -> PlatformResult<bool> {
        Ok(lock(&self.launched).contains(&app.id))
    }
}

/// System controls: in-memory volume, microphone and media state; power actions are recorded,
/// never performed.
pub struct FakeSystemControl {
    pub volume: Mutex<VolumeState>,
    pub mic: Mutex<VolumeState>,
    pub media: Mutex<Vec<MediaAction>>,
    pub playing: Mutex<Option<NowPlaying>>,
    pub power: Mutex<Vec<PowerAction>>,
    pub opened: Mutex<Vec<String>>,
}

impl Default for FakeSystemControl {
    fn default() -> Self {
        let level = |level| {
            Mutex::new(VolumeState {
                level,
                muted: false,
            })
        };
        Self {
            volume: level(0.5),
            mic: level(1.0),
            media: Mutex::default(),
            playing: Mutex::default(),
            power: Mutex::default(),
            opened: Mutex::default(),
        }
    }
}

impl SystemControl for FakeSystemControl {
    fn volume(&self) -> PlatformResult<VolumeState> {
        Ok(*lock(&self.volume))
    }
    fn set_volume(&self, level: f32) -> PlatformResult<()> {
        lock(&self.volume).level = level.clamp(0.0, 1.0);
        Ok(())
    }
    fn set_muted(&self, muted: bool) -> PlatformResult<()> {
        lock(&self.volume).muted = muted;
        Ok(())
    }
    fn microphone(&self) -> PlatformResult<VolumeState> {
        Ok(*lock(&self.mic))
    }
    fn set_mic_muted(&self, muted: bool) -> PlatformResult<()> {
        lock(&self.mic).muted = muted;
        Ok(())
    }
    fn media(&self, action: MediaAction) -> PlatformResult<()> {
        if lock(&self.playing).is_none() {
            return Err(PlatformError::NotFound("anything playing".into()));
        }
        lock(&self.media).push(action);
        Ok(())
    }
    fn now_playing(&self) -> PlatformResult<Option<NowPlaying>> {
        Ok(lock(&self.playing).clone())
    }
    fn power(&self, action: PowerAction) -> PlatformResult<()> {
        lock(&self.power).push(action);
        Ok(())
    }
    fn open_url(&self, url: &str) -> PlatformResult<()> {
        lock(&self.opened).push(url.to_owned());
        Ok(())
    }
    fn open_system_settings(&self, page: &str) -> PlatformResult<()> {
        lock(&self.opened).push(format!("settings:{page}"));
        Ok(())
    }
    fn reveal(&self, folder: &std::path::Path) -> PlatformResult<()> {
        lock(&self.opened).push(format!("folder:{}", folder.display()));
        Ok(())
    }
}

/// Autostart: remembers the registered command.
#[derive(Default)]
pub struct FakeAutostart {
    pub command: Mutex<Option<String>>,
}

impl kivo_platform::Autostart for FakeAutostart {
    fn set(&self, enabled: bool, command: &str) -> PlatformResult<()> {
        *lock(&self.command) = enabled.then(|| command.to_owned());
        Ok(())
    }
    fn current(&self) -> PlatformResult<Option<String>> {
        Ok(lock(&self.command).clone())
    }
}

/// What a fake window was last told to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowAction {
    Focus,
    Minimize,
    Maximize,
    Restore,
    Close,
}

/// Windows: a list whose first entry is the foreground window; actions are recorded.
#[derive(Default)]
pub struct FakeWindows {
    pub windows: Mutex<Vec<WindowInfo>>,
    pub actions: Mutex<Vec<(WindowId, WindowAction)>>,
}

impl FakeWindows {
    fn act(&self, id: WindowId, action: WindowAction) -> PlatformResult<()> {
        let mut windows = lock(&self.windows);
        let Some(pos) = windows.iter().position(|w| w.id == id) else {
            return Err(PlatformError::NotFound(format!("window {}", id.0)));
        };
        match action {
            WindowAction::Focus => {
                let w = windows.remove(pos);
                windows.insert(0, w);
            }
            WindowAction::Minimize => windows[pos].minimized = true,
            WindowAction::Maximize | WindowAction::Restore => windows[pos].minimized = false,
            WindowAction::Close => {
                windows.remove(pos);
            }
        }
        lock(&self.actions).push((id, action));
        Ok(())
    }
}

impl Windows for FakeWindows {
    fn list(&self) -> PlatformResult<Vec<WindowInfo>> {
        Ok(lock(&self.windows).clone())
    }
    fn foreground(&self) -> PlatformResult<Option<WindowInfo>> {
        Ok(lock(&self.windows).first().cloned())
    }
    fn focus(&self, id: WindowId) -> PlatformResult<()> {
        self.act(id, WindowAction::Focus)
    }
    fn minimize(&self, id: WindowId) -> PlatformResult<()> {
        self.act(id, WindowAction::Minimize)
    }
    fn maximize(&self, id: WindowId) -> PlatformResult<()> {
        self.act(id, WindowAction::Maximize)
    }
    fn restore(&self, id: WindowId) -> PlatformResult<()> {
        self.act(id, WindowAction::Restore)
    }
    fn close(&self, id: WindowId) -> PlatformResult<()> {
        self.act(id, WindowAction::Close)
    }
}

/// Secrets: an in-memory store. `protect` is reversible but not real encryption.
#[derive(Default)]
pub struct FakeSecrets {
    store: Mutex<HashMap<SecretHandle, String>>,
}

impl Secrets for FakeSecrets {
    fn get(&self, handle: &SecretHandle) -> PlatformResult<Option<Secret<String>>> {
        Ok(lock(&self.store).get(handle).cloned().map(Secret::new))
    }
    fn set(&self, handle: &SecretHandle, value: Secret<String>) -> PlatformResult<()> {
        lock(&self.store).insert(handle.clone(), value.expose().clone());
        Ok(())
    }
    fn delete(&self, handle: &SecretHandle) -> PlatformResult<()> {
        lock(&self.store).remove(handle);
        Ok(())
    }
    fn protect(&self, data: &[u8]) -> PlatformResult<Vec<u8>> {
        Ok(data.iter().rev().map(|b| b ^ 0x5a).collect())
    }
    fn unprotect(&self, data: &[u8]) -> PlatformResult<Vec<u8>> {
        Ok(data.iter().rev().map(|b| b ^ 0x5a).collect())
    }
}

/// System info: returns whatever snapshot the test sets.
pub struct FakeSystemInfo {
    pub snapshot: Mutex<SystemSnapshot>,
}

impl Default for FakeSystemInfo {
    /// A mid-tier laptop on mains power (BENCHMARKS §2).
    fn default() -> Self {
        Self {
            snapshot: Mutex::new(SystemSnapshot {
                cpu_name: "Fake 8-core".into(),
                logical_cpus: 16,
                ram_mb: 16 * 1024,
                ram_free_mb: 8 * 1024,
                cpu_load_percent: 10,
                gpus: Vec::new(),
                on_battery: false,
                battery_percent: None,
                fullscreen_app: false,
                focus_mode: false,
            }),
        }
    }
}

impl SystemInfo for FakeSystemInfo {
    fn snapshot(&self) -> PlatformResult<SystemSnapshot> {
        Ok(lock(&self.snapshot).clone())
    }
    fn attention(&self) -> PlatformResult<kivo_platform::Attention> {
        let s = lock(&self.snapshot);
        Ok(kivo_platform::Attention {
            fullscreen_app: s.fullscreen_app,
            focus_mode: s.focus_mode,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivo_platform::Rect;

    fn chord(keys: &[&str]) -> Chord {
        Chord(keys.iter().map(|k| (*k).to_owned()).collect())
    }

    #[test]
    fn hotkeys_report_shared_combinations_and_track_registrations() {
        let hk = FakeHotkeys::default();
        lock(&hk.taken).insert(chord(&["Ctrl", "Alt", "W"]));
        assert_eq!(
            hk.register(HotkeyId(1), &chord(&["Ctrl", "Alt", "W"])),
            Ok(Binding::Shared)
        );
        assert_eq!(
            hk.register(HotkeyId(2), &chord(&["Ctrl", "Space"])),
            Ok(Binding::System)
        );
        assert_eq!(lock(&hk.registered).len(), 2);
        hk.unregister(HotkeyId(2)).unwrap();
        assert!(hk.unregister(HotkeyId(2)).is_err());
    }

    #[test]
    fn windows_focus_moves_to_the_front_and_close_removes() {
        let win = |id, title: &str| WindowInfo {
            id: WindowId(id),
            title: title.into(),
            app_id: title.into(),
            bounds: Rect {
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            },
            minimized: false,
        };
        let fw = FakeWindows {
            windows: Mutex::new(vec![win(1, "Notes"), win(2, "Chrome")]),
            ..Default::default()
        };
        fw.focus(WindowId(2)).unwrap();
        assert_eq!(fw.foreground().unwrap().unwrap().title, "Chrome");
        fw.close(WindowId(1)).unwrap();
        assert_eq!(fw.list().unwrap().len(), 1);
        assert!(matches!(
            fw.minimize(WindowId(9)),
            Err(PlatformError::NotFound(_))
        ));
        assert_eq!(
            *lock(&fw.actions),
            vec![
                (WindowId(2), WindowAction::Focus),
                (WindowId(1), WindowAction::Close)
            ]
        );
    }

    #[test]
    fn apps_launch_only_what_is_installed() {
        let chrome = AppEntry {
            id: "chrome".into(),
            name: "Google Chrome".into(),
            aliases: vec!["Chrome".into()],
            exe: None,
        };
        let apps = FakeApps {
            installed: vec![chrome.clone()],
            ..Default::default()
        };
        apps.launch(&chrome, &[]).unwrap();
        let missing = AppEntry {
            id: "x".into(),
            name: "Nope".into(),
            aliases: vec![],
            exe: None,
        };
        assert!(apps.launch(&missing, &[]).is_err());
        assert_eq!(*lock(&apps.launched), vec!["chrome".to_owned()]);
    }

    #[test]
    fn secrets_round_trip_and_protect_is_reversible() {
        let s = FakeSecrets::default();
        let h = SecretHandle {
            provider: "openrouter".into(),
            name: "key".into(),
        };
        s.set(&h, Secret::new("sk-or-1".into())).unwrap();
        assert_eq!(s.get(&h).unwrap().unwrap().expose(), "sk-or-1");
        s.delete(&h).unwrap();
        assert!(s.get(&h).unwrap().is_none());
        let sealed = s.protect(b"voiceprint").unwrap();
        assert_ne!(sealed, b"voiceprint");
        assert_eq!(s.unprotect(&sealed).unwrap(), b"voiceprint");
    }

    #[test]
    fn tray_and_notifications_record_what_was_shown() {
        let tray = FakeTray::default();
        tray.set_icon(TrayIcon::Listening).unwrap();
        tray.set_tooltip("KIVO · Listening").unwrap();
        assert_eq!(*lock(&tray.icon), Some(TrayIcon::Listening));
        let n = FakeNotifications::default();
        let note = Notification {
            title: "KIVO is still running".into(),
            body: String::new(),
            actions: vec![],
            reply: false,
        };
        n.show(&note).unwrap();
        assert_eq!(*lock(&n.shown), vec![note]);
    }
}
