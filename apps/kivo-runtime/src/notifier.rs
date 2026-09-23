//! What KIVO says without being asked (UX §7, UX-40): a task finished, a watcher fired, a
//! reminder. The situation decides how: spoken when the user is at the PC and free, never during a
//! call, queued in fullscreen, Focus or while away (and read out on "what did I miss?" or on
//! return), toasts only in quiet hours; at most one spoken interruption per 10 minutes unless it
//! is urgent; each source can be set to speak, toast or stay silent.

use crate::core::Core;
use kivo_core::config::{AnnounceMode, Automation};
use kivo_core::text;
use kivo_platform::{Notification, NotificationAction, Notifications, SystemInfo};
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::{Duration, Instant};

/// At most one spoken interruption this often, unless urgent.
pub const SPOKEN_GAP: Duration = Duration::from_secs(10 * 60);
/// Idle this long counts as away.
pub const AWAY_AFTER_SECONDS: u32 = 5 * 60;
/// How long a notice stays in the Island.
const NOTICE_FOR: Duration = Duration::from_secs(6);
/// How often KIVO looks for the user's return while something is waiting to be told.
const RETURN_CHECK: Duration = Duration::from_secs(30);

/// Something to tell the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Announcement {
    /// `tasks`, `reminders`, `watchers`, `routines`, `agents`.
    pub source: String,
    pub title: String,
    pub text: String,
    /// A reminder the user marked, or a task that can't continue without them.
    pub urgent: bool,
    /// "Tell me when …": the user asked to hear it.
    pub tell_me: bool,
    /// The task it's about, for the toast's buttons and reply (UX-58).
    pub task: Option<String>,
}

/// How an announcement reached the user.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// Earcon, the Island's notice and speech.
    Spoken,
    /// Earcon and the Island's notice, no speech (grouped, or not "tell me").
    Shown,
    /// A Windows toast (with the earcon when urgent).
    Toast,
    /// Kept for "what did I miss?", with a toast as a quiet badge.
    Queued,
    /// Only in Activity (the source is set to silent).
    Silent,
}

/// What the user is doing now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Situation {
    pub in_call: bool,
    pub fullscreen: bool,
    pub focus: bool,
    pub away: bool,
    pub quiet_hours: bool,
    /// KIVO is in the middle of a request: it doesn't talk over itself.
    pub busy: bool,
}

/// The rules of UX §7, as a pure function.
/// The toast button that sends what was typed into its reply field (UX-58).
pub const REPLY_SEND: &str = "kivo.reply.send";

/// What a toast's reply asks KIVO, if anything: the text typed into its reply field ("Want me to
/// commit?" → "yes, commit it") is a request like a typed one.
pub fn reply_request(reply: Option<&str>) -> Option<String> {
    reply
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(str::to_owned)
}

pub fn decide(
    mode: Option<AnnounceMode>,
    a: &Announcement,
    s: &Situation,
    recently_spoke: bool,
) -> Delivery {
    let mode = mode.unwrap_or(AnnounceMode::Speak);
    if mode == AnnounceMode::Silent {
        return Delivery::Silent;
    }
    // Calls: never speech. Urgent gets a toast; the rest waits.
    if s.in_call || s.fullscreen || s.focus || s.away {
        return if a.urgent {
            Delivery::Toast
        } else {
            Delivery::Queued
        };
    }
    if s.quiet_hours || mode == AnnounceMode::Toast {
        return Delivery::Toast;
    }
    if s.busy {
        return Delivery::Toast;
    }
    if a.tell_me && (a.urgent || !recently_spoke) {
        Delivery::Spoken
    } else {
        Delivery::Shown
    }
}

/// The user's own choices on top of the rules (Settings → Notifications): "Say it out loud"
/// never speaks at all, or speaks whenever it isn't rude (not in a call, a quiet time, or over
/// KIVO itself); with Windows notifications off, a toast waits in the Control Center instead.
pub fn with_prefs(
    decided: Delivery,
    speak: kivo_core::config::SpeakMode,
    toasts: bool,
    a: &Announcement,
    s: &Situation,
) -> Delivery {
    use kivo_core::config::SpeakMode;
    let d = match (speak, decided) {
        (SpeakMode::Never, Delivery::Spoken) => Delivery::Shown,
        (SpeakMode::Always, Delivery::Queued | Delivery::Toast)
            if a.tell_me && !s.in_call && !s.quiet_hours && !s.busy =>
        {
            Delivery::Spoken
        }
        (_, d) => d,
    };
    if !toasts && d == Delivery::Toast {
        Delivery::Queued
    } else {
        d
    }
}

/// Whether `minute` (after local midnight) is inside quiet hours `(from, to)`, which may wrap
/// midnight.
pub fn in_quiet_hours(quiet: (u32, u32), minute: u32) -> bool {
    let (from, to) = quiet;
    if from <= to {
        (from..to).contains(&minute)
    } else {
        minute >= from || minute < to
    }
}

/// A queued announcement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Missed {
    pub source: String,
    pub text: String,
    pub at: i64,
}

/// Speaks outside a turn (the engine, once it exists).
#[async_trait::async_trait]
pub trait Voice: Send + Sync {
    async fn say(&self, text: &str);
    fn earcon(&self);
}

pub struct Notifier {
    core: Arc<Core>,
    system: Arc<dyn SystemInfo>,
    notifications: Arc<dyn Notifications>,
    voice: RwLock<Option<Weak<dyn Voice>>>,
    missed: Mutex<Vec<Missed>>,
    last_spoken: Mutex<Option<Instant>>,
    utc_offset_minutes: i32,
    /// The return watch is running.
    watching: std::sync::atomic::AtomicBool,
}

impl Notifier {
    pub fn new(
        core: Arc<Core>,
        system: Arc<dyn SystemInfo>,
        notifications: Arc<dyn Notifications>,
        utc_offset_minutes: i32,
    ) -> Arc<Self> {
        Arc::new(Self {
            core,
            system,
            notifications,
            voice: RwLock::new(None),
            missed: Mutex::default(),
            last_spoken: Mutex::new(None),
            utc_offset_minutes,
            watching: std::sync::atomic::AtomicBool::new(false),
        })
    }

    pub fn set_voice(&self, voice: Weak<dyn Voice>) {
        *self.voice.write().unwrap_or_else(|e| e.into_inner()) = Some(voice);
    }

    fn voice(&self) -> Option<Arc<dyn Voice>> {
        self.voice
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .and_then(Weak::upgrade)
    }

    /// The user's situation now.
    pub fn situation(&self) -> Situation {
        let attention = self.system.attention().unwrap_or_default();
        let presence = self.system.presence();
        let config = self.core.config();
        Situation {
            in_call: presence.mic_in_use_elsewhere,
            fullscreen: attention.fullscreen_app,
            // Windows Focus counts only when the user chose that (Settings → Notifications).
            focus: attention.focus_mode && config.automation.follow_focus,
            away: presence.locked || presence.idle_seconds >= AWAY_AFTER_SECONDS,
            quiet_hours: quiet_now(&config.automation, self.utc_offset_minutes),
            busy: self.core.state().borrow().session != kivo_core::SessionState::Idle,
        }
    }

    /// Tells the user, as the situation allows; returns how.
    pub async fn announce(self: &Arc<Self>, a: Announcement) -> Delivery {
        let config = self.core.config();
        let situation = self.situation();
        let recently = lock(&self.last_spoken).is_some_and(|t| t.elapsed() < SPOKEN_GAP);
        let delivery = with_prefs(
            decide(
                config.automation.sources.get(&a.source).copied(),
                &a,
                &situation,
                recently,
            ),
            config.automation.speak,
            config.automation.toasts,
            &a,
            &situation,
        );
        tracing::info!(source = %a.source, ?delivery, "announcement");
        match delivery {
            Delivery::Spoken => {
                *lock(&self.last_spoken) = Some(Instant::now());
                self.core.flash_notice(&a.text, NOTICE_FOR);
                match self.voice() {
                    Some(voice) => {
                        voice.earcon();
                        voice.say(&a.text).await;
                    }
                    None => self.toast(&a),
                }
            }
            Delivery::Shown => {
                if let Some(voice) = self.voice() {
                    voice.earcon();
                }
                self.core.flash_notice(&a.text, NOTICE_FOR);
            }
            Delivery::Toast => {
                if a.urgent
                    && !situation.in_call
                    && let Some(voice) = self.voice()
                {
                    voice.earcon();
                }
                self.toast(&a);
            }
            Delivery::Queued => {
                lock(&self.missed).push(Missed {
                    source: a.source.clone(),
                    text: a.text.clone(),
                    at: kivo_store::brains::now_ms(),
                });
                // A quiet badge, unless Windows notifications are off.
                if config.automation.toasts {
                    self.toast(&a);
                }
                if config.automation.catch_up_on_return && situation.away {
                    self.watch_for_return();
                }
            }
            Delivery::Silent => {}
        }
        delivery
    }

    /// A Windows toast with "Open", and "Reply to KIVO" with Send (UX-58).
    fn toast(&self, a: &Announcement) {
        let mut actions = Vec::new();
        if let Some(task) = &a.task {
            actions.push(NotificationAction {
                id: format!("task:{task}:open"),
                label: text::t("notify.openTask"),
            });
        }
        // The reply field is sent with this button (its id ends in `.send`).
        actions.push(NotificationAction {
            id: REPLY_SEND.to_owned(),
            label: text::t("notify.send"),
        });
        let shown = self.notifications.show(&Notification {
            title: a.title.clone(),
            body: a.text.clone(),
            actions,
            reply: true,
            silent: !self.core.config().automation.notification_sound,
        });
        if let Err(e) = shown {
            tracing::warn!(%e, "couldn't show a notification");
        }
    }

    /// What was queued, in order, and the queue emptied ("what did I miss?").
    pub fn take_missed(&self) -> Vec<Missed> {
        std::mem::take(&mut *lock(&self.missed))
    }

    pub fn missed_count(&self) -> usize {
        lock(&self.missed).len()
    }

    /// "While you were away: …" or "Nothing new." — the queue is emptied.
    pub fn missed_summary(&self) -> String {
        let missed = self.take_missed();
        if missed.is_empty() {
            return text::t("notify.nothingMissed");
        }
        let items: Vec<String> = missed.into_iter().map(|m| m.text).collect();
        text::tf("notify.missed", &[("items", &items.join(" "))])
    }

    /// While things wait and the user is away, looks every 30 s for their return, then reads
    /// them out once (UX §7 "summarize on return").
    fn watch_for_return(self: &Arc<Self>) {
        if self
            .watching
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }
        let me = Arc::clone(self);
        let shutdown = self.core.shutdown();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = tokio::time::sleep(RETURN_CHECK) => {}
                    () = shutdown.cancelled() => break,
                }
                if me.missed_count() == 0 {
                    break;
                }
                let s = me.situation();
                if !s.away {
                    if !s.in_call && !s.fullscreen && !s.focus && !s.busy && !s.quiet_hours {
                        let summary = me.missed_summary();
                        me.core.flash_notice(&summary, NOTICE_FOR);
                        if let Some(voice) = me.voice() {
                            voice.earcon();
                            voice.say(&summary).await;
                        }
                    }
                    break;
                }
            }
            me.watching
                .store(false, std::sync::atomic::Ordering::SeqCst);
        });
    }
}

fn quiet_now(automation: &Automation, utc_offset_minutes: i32) -> bool {
    let Some(quiet) = automation.quiet() else {
        return false;
    };
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
        + i64::from(utc_offset_minutes) * 60;
    let minute = u32::try_from(secs.rem_euclid(86_400) / 60).unwrap_or(0);
    in_quiet_hours(quiet, minute)
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_users_choices_come_on_top_of_the_rules() {
        use kivo_core::config::SpeakMode;
        let a = Announcement {
            source: "tasks".into(),
            title: "Task finished".into(),
            text: "The build passed.".into(),
            urgent: false,
            tell_me: true,
            task: None,
        };
        let free = Situation::default();
        let fullscreen = Situation {
            fullscreen: true,
            ..Situation::default()
        };
        let call = Situation {
            in_call: true,
            ..Situation::default()
        };
        // Never: shown, not spoken.
        assert_eq!(
            with_prefs(Delivery::Spoken, SpeakMode::Never, true, &a, &free),
            Delivery::Shown
        );
        // Always: spoken in a fullscreen app, but never over a call.
        assert_eq!(
            with_prefs(Delivery::Queued, SpeakMode::Always, true, &a, &fullscreen),
            Delivery::Spoken
        );
        assert_eq!(
            with_prefs(Delivery::Queued, SpeakMode::Always, true, &a, &call),
            Delivery::Queued
        );
        // No Windows notifications: it waits in the Control Center instead.
        assert_eq!(
            with_prefs(Delivery::Toast, SpeakMode::WhenFree, false, &a, &free),
            Delivery::Queued
        );
        assert_eq!(
            with_prefs(Delivery::Shown, SpeakMode::WhenFree, true, &a, &free),
            Delivery::Shown
        );
    }

    fn tell(urgent: bool) -> Announcement {
        Announcement {
            source: "watchers".into(),
            title: "Build".into(),
            text: "The build finished.".into(),
            urgent,
            tell_me: true,
            task: Some("t1".into()),
        }
    }

    #[test]
    fn a_toast_reply_is_a_request() {
        assert_eq!(
            reply_request(Some("  yes, commit it ")).as_deref(),
            Some("yes, commit it")
        );
        assert_eq!(reply_request(Some("   ")), None);
        assert_eq!(reply_request(None), None);
    }

    #[tokio::test]
    async fn task_toasts_offer_a_reply() {
        let system = Arc::new(kivo_testkit::FakeSystemInfo::default());
        system.presence.lock().unwrap().mic_in_use_elsewhere = true;
        let toasts = Arc::new(kivo_testkit::FakeNotifications::default());
        let notifier = Notifier::new(Arc::new(Core::default()), system, toasts.clone(), 0);
        // Urgent news during a call is a toast.
        assert_eq!(notifier.announce(tell(true)).await, Delivery::Toast);
        let shown = toasts.shown.lock().unwrap().clone();
        assert_eq!(shown.len(), 1);
        assert!(shown[0].reply, "a reply field");
        let ids: Vec<&str> = shown[0].actions.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["task:t1:open", REPLY_SEND]);
        assert!(
            REPLY_SEND.ends_with(".send"),
            "Windows sends the reply with it"
        );
    }

    #[test]
    fn the_situation_decides_how_to_tell() {
        let free = Situation::default();
        assert_eq!(decide(None, &tell(false), &free, false), Delivery::Spoken);
        // Grouped: one spoken interruption per 10 minutes, unless urgent.
        assert_eq!(decide(None, &tell(false), &free, true), Delivery::Shown);
        assert_eq!(decide(None, &tell(true), &free, true), Delivery::Spoken);
        // Not asked to be told: the Island only.
        let mut quiet_item = tell(false);
        quiet_item.tell_me = false;
        assert_eq!(decide(None, &quiet_item, &free, false), Delivery::Shown);
        // A call: never speech, even urgent.
        let call = Situation {
            in_call: true,
            ..free
        };
        assert_eq!(decide(None, &tell(false), &call, false), Delivery::Queued);
        assert_eq!(decide(None, &tell(true), &call, false), Delivery::Toast);
        for busy in [
            Situation {
                fullscreen: true,
                ..free
            },
            Situation {
                focus: true,
                ..free
            },
            Situation { away: true, ..free },
        ] {
            assert_eq!(decide(None, &tell(false), &busy, false), Delivery::Queued);
            assert_eq!(decide(None, &tell(true), &busy, false), Delivery::Toast);
        }
        let night = Situation {
            quiet_hours: true,
            ..free
        };
        assert_eq!(decide(None, &tell(true), &night, false), Delivery::Toast);
        // Per-source settings.
        assert_eq!(
            decide(Some(AnnounceMode::Silent), &tell(true), &free, false),
            Delivery::Silent
        );
        assert_eq!(
            decide(Some(AnnounceMode::Toast), &tell(false), &free, false),
            Delivery::Toast
        );
        // KIVO is busy with a request: it doesn't talk over itself.
        let working = Situation { busy: true, ..free };
        assert_eq!(decide(None, &tell(false), &working, false), Delivery::Toast);
    }

    #[test]
    fn quiet_hours_can_wrap_midnight() {
        let night = (22 * 60, 7 * 60);
        assert!(in_quiet_hours(night, 23 * 60));
        assert!(in_quiet_hours(night, 3 * 60));
        assert!(!in_quiet_hours(night, 12 * 60));
        let lunch = (12 * 60, 13 * 60);
        assert!(in_quiet_hours(lunch, 12 * 60 + 30));
        assert!(!in_quiet_hours(lunch, 13 * 60));
    }
}
