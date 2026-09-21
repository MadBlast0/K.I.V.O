//! Living alongside Windows (UX §1, §7): "Open KIVO when Windows starts" (ARCH-06), what closing
//! the window means (UX-02), and the actionable notifications KIVO shows — the one-time
//! "still running" notice, a blocked microphone, and a crash last time (UX-57, ARCH-10). A
//! notification's buttons come back here and are handled like the matching tray or UI action.

use crate::activity::Recorder;
use crate::core::Core;
use kivo_core::KivoConfig;
use kivo_core::text;
use kivo_platform::{Autostart, Notification, NotificationAction, Notifications, SystemControl};
use std::path::PathBuf;
use std::sync::Arc;

pub const FIRST_CLOSE_SETTINGS: &str = "first-close.settings";
pub const FIRST_CLOSE_QUIT: &str = "first-close.quit";
pub const MIC_SETTINGS: &str = "mic.settings";
pub const MIC_TYPE: &str = "mic.type";
pub const CRASH_FOLDER: &str = "crash.folder";

pub struct Lifecycle {
    core: Arc<Core>,
    notifications: Arc<dyn Notifications>,
    control: Arc<dyn SystemControl>,
    autostart: Arc<dyn Autostart>,
    recorder: Recorder,
    crashes: PathBuf,
    /// What autostart runs: this runtime with `--autostart`.
    command: String,
}

/// A notification button; `key` is its label in the text catalog.
fn action(id: &str, key: &str) -> NotificationAction {
    NotificationAction {
        id: id.into(),
        label: text::t(key),
    }
}

impl Lifecycle {
    pub fn new(
        core: Arc<Core>,
        notifications: Arc<dyn Notifications>,
        control: Arc<dyn SystemControl>,
        autostart: Arc<dyn Autostart>,
        recorder: Recorder,
        crashes: PathBuf,
    ) -> Self {
        let exe = std::env::current_exe()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "kivo-runtime.exe".into());
        Self {
            core,
            notifications,
            control,
            autostart,
            recorder,
            crashes,
            command: format!("\"{exe}\" --autostart"),
        }
    }

    fn notify(&self, notification: &Notification) {
        if !self
            .core
            .config()
            .capabilities
            .enabled(kivo_core::Capability::Notifications)
        {
            return;
        }
        if let Err(e) = self.notifications.show(notification) {
            tracing::warn!(%e, title = notification.title, "couldn't show a notification");
        }
    }

    /// Keeps the sign-in entry in step with "Open KIVO when Windows starts" (ARCH-06).
    pub fn apply_autostart(&self, config: &KivoConfig) {
        let wanted = config.general.start_with_windows;
        let current = self.autostart.current().ok().flatten();
        let registered = current.as_deref() == Some(self.command.as_str());
        if wanted != registered
            && let Err(e) = self.autostart.set(wanted, &self.command)
        {
            tracing::warn!(%e, wanted, "couldn't update the startup entry");
        }
    }

    /// The Control Center window was closed. Returns whether KIVO keeps running (UX-02).
    pub fn window_closed(&self) -> bool {
        let config = self.core.config();
        if !config.general.keep_running_on_close {
            self.core.quit();
            return false;
        }
        if !config.general.first_close_seen {
            self.core
                .update_config(|c| c.general.first_close_seen = true);
            self.notify(&Notification {
                title: text::t("notice.firstClose.title"),
                body: text::t("notice.firstClose.body"),
                actions: vec![
                    action(FIRST_CLOSE_SETTINGS, "notice.firstClose.settings"),
                    action(FIRST_CLOSE_QUIT, "notice.firstClose.quit"),
                ],
                reply: false,
            });
        }
        true
    }

    /// Windows is blocking the microphone for KIVO (UX-57).
    pub fn microphone_blocked(&self) {
        self.notify(&Notification {
            title: text::t("notice.mic.title"),
            body: text::t("notice.mic.body"),
            actions: vec![
                action(MIC_SETTINGS, "notice.mic.settings"),
                action(MIC_TYPE, "notice.mic.type"),
            ],
            reply: false,
        });
    }

    /// Reports crashes since the last start (ARCH-10): in Activity, the log and a notification.
    /// The reports stay on this PC.
    pub fn report_crashes(&self) {
        let new = kivo_store::crashes::take_new(&self.crashes);
        if new.is_empty() {
            return;
        }
        for report in &new {
            tracing::warn!(
                process = report.process,
                native = report.native,
                file = %report.path.display(),
                "KIVO crashed last time"
            );
            self.recorder.crash_reported(report);
        }
        self.notify(&Notification {
            title: text::t("notice.crash.title"),
            body: text::t("notice.crash.body"),
            actions: vec![action(CRASH_FOLDER, "notice.crash.folder")],
            reply: false,
        });
    }

    /// A notification button was pressed.
    pub fn answered(&self, action: &str) {
        let result = match action {
            FIRST_CLOSE_SETTINGS => {
                self.core.open_control_center(Some("settings"));
                Ok(())
            }
            FIRST_CLOSE_QUIT => {
                self.core.quit();
                Ok(())
            }
            MIC_SETTINGS => self.control.open_system_settings("privacy-microphone"),
            MIC_TYPE => {
                self.core.open_control_center(Some("type"));
                Ok(())
            }
            CRASH_FOLDER => self.control.reveal(&self.crashes),
            // The body of the notification was clicked: open KIVO.
            "" => {
                self.core.open_control_center(None);
                Ok(())
            }
            other => {
                tracing::warn!(action = other, "unknown notification button");
                Ok(())
            }
        };
        if let Err(e) = result {
            tracing::warn!(%e, action, "notification action failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivo_store::Database;
    use kivo_testkit::{FakeAutostart, FakeNotifications, FakeSystemControl};
    use std::sync::Mutex;

    struct Rig {
        life: Lifecycle,
        core: Arc<Core>,
        shown: Arc<FakeNotifications>,
        control: Arc<FakeSystemControl>,
        autostart: Arc<FakeAutostart>,
        _dir: tempfile::TempDir,
    }

    fn rig() -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let core = Arc::new(Core::default());
        let shown = Arc::new(FakeNotifications::default());
        let control = Arc::new(FakeSystemControl::default());
        let autostart = Arc::new(FakeAutostart::default());
        let recorder = Recorder::new(Arc::new(Mutex::new(Database::in_memory().unwrap())), true);
        let life = Lifecycle::new(
            Arc::clone(&core),
            shown.clone(),
            control.clone(),
            autostart.clone(),
            recorder,
            dir.path().to_path_buf(),
        );
        Rig {
            life,
            core,
            shown,
            control,
            autostart,
            _dir: dir,
        }
    }

    #[test]
    fn the_first_close_notice_shows_once_and_its_buttons_work() {
        let r = rig();
        assert!(r.life.window_closed(), "KIVO keeps running");
        assert!(r.life.window_closed());
        let shown = r.shown.shown.lock().unwrap();
        assert_eq!(shown.len(), 1, "only the first close");
        assert_eq!(shown[0].actions[1].id, FIRST_CLOSE_QUIT);
        drop(shown);
        r.life.answered(FIRST_CLOSE_QUIT);
        assert!(r.core.shutdown().is_cancelled());
    }

    #[test]
    fn closing_quits_when_keep_running_is_off() {
        let r = rig();
        r.core
            .update_config(|c| c.general.keep_running_on_close = false);
        assert!(!r.life.window_closed());
        assert!(r.core.shutdown().is_cancelled());
    }

    #[test]
    fn a_blocked_microphone_offers_settings_or_typing() {
        let r = rig();
        r.life.microphone_blocked();
        r.life.answered(MIC_SETTINGS);
        assert_eq!(
            *r.control.opened.lock().unwrap(),
            ["settings:privacy-microphone"]
        );
    }

    #[test]
    fn autostart_follows_the_setting() {
        let r = rig();
        let mut config = KivoConfig::default();
        r.life.apply_autostart(&config);
        assert_eq!(*r.autostart.command.lock().unwrap(), None, "off by default");
        config.general.start_with_windows = true;
        r.life.apply_autostart(&config);
        assert!(
            r.autostart
                .command
                .lock()
                .unwrap()
                .as_deref()
                .is_some_and(|c| c.ends_with("--autostart"))
        );
    }

    #[test]
    fn crashes_are_reported_once_on_the_next_start() {
        let r = rig();
        std::fs::write(r.life.crashes.join("kivo-infer-100.dmp"), b"MDMP").unwrap();
        r.life.report_crashes();
        r.life.report_crashes();
        assert_eq!(r.shown.shown.lock().unwrap().len(), 1);
    }

    #[test]
    fn notifications_respect_their_capability() {
        let r = rig();
        r.core.update_config(|c| {
            c.capabilities
                .set(kivo_core::Capability::Notifications, false);
        });
        r.life.microphone_blocked();
        assert!(r.shown.shown.lock().unwrap().is_empty());
    }
}
