//! Computer use (CAPABILITIES §4, CAP-09–CAP-12): the last rung of the capability ladder, opt-in.
//! A brain asks for `computer.use` with a task and an app; the user confirms the task (with its
//! estimated cost) like any High-risk action. Then, inside the turn:
//!
//! 1. screenshot the app's window → the vision model proposes one action (`ComputerUseProvider`);
//! 2. the action becomes one of KIVO's input tools (`input.click`, `input.type`, …) and goes
//!    through the permission engine: covered by the task's grant for that app's window only, or,
//!    in watch mode, asked on the Island ("Click here?" with Allow / Skip) with the target
//!    highlighted — the password-field limit and the emergency stop apply either way;
//! 3. it runs, and the next screenshot verifies it.
//!
//! Limits (Settings → Capabilities → Computer use): steps, estimated cost, time. The Island shows a
//! controller live activity (step, cost, Pause, Stop) and a banner says KIVO is controlling the
//! screen, with the frame the user chose. Using the mouse pauses it when that option is on.

use super::{Engine, lock};
use kivo_brain::computer::{CuAction, CuSession, Screenshot};
use kivo_core::config::{ApproveSteps, ComputerUse, CuSpeed};
use kivo_core::text;
use kivo_core::tool::{Initiator, Target, ToolCall};
use kivo_ipc::protocol::{ControlView, LiveActivity, PointTarget};
use kivo_platform::{CaptureTarget, WindowInfo};
use kivo_security::{Decision, Grant};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The id of the controller live activity.
pub const ACTIVITY: &str = "computer-use";
/// Database meta key: how many computer-use tasks have run (watch mode for the first ten).
pub const TASKS_KEY: &str = "computer_use.tasks";
/// Watch mode covers this many first tasks when set to "first tasks".
pub const WATCH_FIRST: u64 = 10;
/// The input tools computer use acts through.
pub const INPUT_TOOLS: [&str; 4] = ["input.click", "input.type", "input.press", "input.scroll"];

/// How computer use went, for the brain and the card.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub done: bool,
    pub summary: String,
    pub steps: u32,
    pub cost: f64,
}

/// An action in screenshot pixels → an input tool call in screen pixels on `window`. `None` for
/// "just look again".
pub fn to_call(action: &CuAction, window: &WindowInfo, scale: f64) -> Option<(String, Value)> {
    let at = |x: i32, y: i32| {
        (
            window.bounds.x + (f64::from(x) * scale).round() as i32,
            window.bounds.y + (f64::from(y) * scale).round() as i32,
        )
    };
    let id = window.id.0.to_string();
    Some(match action {
        CuAction::Click {
            x,
            y,
            button,
            count,
        } => {
            let (x, y) = at(*x, *y);
            (
                "input.click".into(),
                json!({ "window": id, "x": x, "y": y, "button": button, "count": count }),
            )
        }
        CuAction::Move { x, y } => {
            // A move is a hover: KIVO's cursor shows it; nothing is sent.
            let _ = at(*x, *y);
            return None;
        }
        CuAction::Type { text } => ("input.type".into(), json!({ "window": id, "text": text })),
        CuAction::TypeAt { x, y, text, .. } => {
            // The click comes first (a separate step); this is the typing.
            let _ = (x, y);
            ("input.type".into(), json!({ "window": id, "text": text }))
        }
        CuAction::Press { keys } => ("input.press".into(), json!({ "window": id, "keys": keys })),
        CuAction::Scroll { x, y, lines } => {
            let (x, y) = at(*x, *y);
            (
                "input.scroll".into(),
                json!({ "window": id, "x": x, "y": y, "lines": lines }),
            )
        }
        CuAction::Look { .. } | CuAction::Done { .. } | CuAction::Stop { .. } => return None,
    })
}

/// Where an action lands on screen, for the highlight and KIVO's cursor.
pub fn target_of(action: &CuAction, window: &WindowInfo, scale: f64) -> Option<(i32, i32)> {
    let (x, y) = match action {
        CuAction::Click { x, y, .. }
        | CuAction::Move { x, y }
        | CuAction::TypeAt { x, y, .. }
        | CuAction::Scroll { x, y, .. } => (*x, *y),
        _ => return None,
    };
    Some((
        window.bounds.x + (f64::from(x) * scale).round() as i32,
        window.bounds.y + (f64::from(y) * scale).round() as i32,
    ))
}

/// Whether this task is in watch mode (every step asked).
pub fn watching(settings: &ComputerUse, tasks_so_far: u64) -> bool {
    match settings.approve {
        ApproveSteps::Always => true,
        ApproveSteps::FirstTasks => tasks_so_far < WATCH_FIRST,
        ApproveSteps::Never => false,
    }
}

/// The pause between steps for a speed.
pub fn pace(speed: CuSpeed) -> Duration {
    Duration::from_millis(match speed {
        CuSpeed::Careful => 1500,
        CuSpeed::Normal => 600,
        CuSpeed::Fast => 150,
    })
}

/// The most a task can cost, for the confirmation card.
pub fn estimate(settings: &ComputerUse, per_step: f64) -> f64 {
    (f64::from(settings.max_steps) * per_step).min(f64::from(settings.max_cost_cents) / 100.0)
}

impl Engine {
    /// The window of `app` (by name or program), else the window in front.
    fn cu_window(&self, app: Option<&str>) -> Option<WindowInfo> {
        let list = self.windows.list().ok()?;
        let wanted = app.map(str::to_lowercase);
        wanted
            .as_ref()
            .and_then(|w| {
                list.iter().find(|win| {
                    win.app_id.to_lowercase().contains(w) || win.title.to_lowercase().contains(w)
                })
            })
            .cloned()
            .or_else(|| self.windows.foreground().ok().flatten())
    }

    fn cu_view(&self, update: impl FnOnce(&mut Option<ControlView>)) {
        self.core.update_controlling(update);
    }

    /// Runs a confirmed `computer.use` call to the end: done, stopped, out of steps, money or
    /// time. Never panics the turn: every failure is an outcome the brain is told.
    pub(super) async fn computer_use(self: &Arc<Self>, call: &ToolCall) -> Outcome {
        let config = self.core.config();
        let settings = config.tools.computer_use.clone();
        let task = call.args["task"].as_str().unwrap_or_default().to_owned();
        let app = call.args["app"].as_str().map(str::to_owned);
        let fail = |summary: String| Outcome {
            done: false,
            summary,
            steps: 0,
            cost: 0.0,
        };
        let Some(provider) = self.brains.computer_provider(&config) else {
            return fail(text::t("computer.noProvider"));
        };
        let Some(screen) = self.screen() else {
            return fail(text::t("computer.noScreen"));
        };
        let Some(window) = self.cu_window(app.as_deref()) else {
            return fail(text::t("computer.noWindow"));
        };
        let cancel = lock(&self.turn)
            .as_ref()
            .map_or_else(tokio_util::sync::CancellationToken::new, |t| {
                t.cancel.clone()
            });
        let db = self.recorder.database();
        let so_far = db
            .lock()
            .ok()
            .and_then(|d| d.meta(TASKS_KEY).ok().flatten())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        if let Ok(d) = db.lock() {
            let _ = d.set_meta(TASKS_KEY, &(so_far + 1).to_string());
        }
        let watch = watching(&settings, so_far);
        // What the user approved: KIVO's input tools, on this app's window only.
        let grants: Vec<Grant> = INPUT_TOOLS
            .iter()
            .map(|t| Grant {
                scope: Some(window.app_id.clone()),
                ..Grant::tool(*t)
            })
            .collect();
        let name = if window.title.is_empty() {
            window.app_id.clone()
        } else {
            window.title.clone()
        };
        let started = Instant::now();
        let mut session = CuSession::new(&task);
        let mut cost = 0.0;
        let mut step = 0_u32;
        let mut last_ours = Instant::now();
        let limit = Duration::from_secs(u64::from(settings.time_limit_seconds));
        let show = |engine: &Engine, step: u32, cost: f64, paused: bool, at: Option<(i32, i32)>| {
            engine.core.set_activity(LiveActivity {
                id: ACTIVITY.into(),
                kind: "computer".into(),
                title: text::tf("computer.controlling", &[("app", &name)]),
                detail: Some(if paused {
                    text::t("computer.paused")
                } else {
                    text::tf(
                        "computer.progress",
                        &[
                            ("step", &step),
                            ("max", &settings.max_steps),
                            ("cost", &format!("{cost:.2}")),
                        ],
                    )
                }),
                progress: Some(step as f32 / settings.max_steps.max(1) as f32),
                until: None,
                task_id: None,
            });
            engine.cu_view(|v| {
                *v = Some(ControlView {
                    app: name.clone(),
                    step,
                    max_steps: settings.max_steps,
                    cost_cents: (cost * 100.0).round() as u32,
                    paused,
                    frame: settings.frame,
                    cursor: at
                        .filter(|_| settings.show_cursor)
                        .map(|(x, y)| kivo_ipc::protocol::ScreenPoint { x, y }),
                });
            });
        };
        let outcome = loop {
            if cancel.is_cancelled() {
                break Outcome {
                    done: false,
                    summary: text::t("computer.stopped"),
                    steps: step,
                    cost,
                };
            }
            if started.elapsed() >= limit {
                break Outcome {
                    done: false,
                    summary: text::t("computer.outOfTime"),
                    steps: step,
                    cost,
                };
            }
            if step >= settings.max_steps {
                break Outcome {
                    done: false,
                    summary: text::t("computer.outOfSteps"),
                    steps: step,
                    cost,
                };
            }
            if cost + provider.cost_per_step() > f64::from(settings.max_cost_cents) / 100.0 {
                break Outcome {
                    done: false,
                    summary: text::t("computer.outOfMoney"),
                    steps: step,
                    cost,
                };
            }
            // The user moved the mouse or typed since KIVO's last action: wait until they stop.
            // Idle time comes in whole seconds and KIVO's own input resets it too, so only input
            // at least a second after KIVO's last action counts as the user's.
            if settings.pause_on_mouse || self.cu_paused() {
                let mut paused_shown = false;
                loop {
                    let idle = Duration::from_secs(u64::from(self.system.presence().idle_seconds));
                    let user_active = settings.pause_on_mouse
                        && idle + Duration::from_secs(1) < last_ours.elapsed();
                    if !(user_active || self.cu_paused()) || cancel.is_cancelled() {
                        break;
                    }
                    if !paused_shown {
                        show(self, step, cost, true, None);
                        paused_shown = true;
                    }
                    tokio::time::sleep(Duration::from_millis(300)).await;
                }
            }
            show(self, step, cost, false, None);
            // 1. Look.
            let shot = {
                let screen = Arc::clone(&screen);
                let id = window.id;
                tokio::task::spawn_blocking(move || screen.capture(CaptureTarget::Window { id }))
                    .await
                    .ok()
                    .and_then(Result::ok)
            };
            let Some(image) = shot else {
                break Outcome {
                    done: false,
                    summary: text::t("computer.noScreen"),
                    steps: step,
                    cost,
                };
            };
            let scale = f64::from(window.bounds.width.max(1)) / f64::from(image.width.max(1));
            let Ok(png) = kivo_tools::screen_tools::png(&image) else {
                break Outcome {
                    done: false,
                    summary: text::t("computer.noScreen"),
                    steps: step,
                    cost,
                };
            };
            let screenshot = Screenshot {
                png,
                width: image.width,
                height: image.height,
            };
            // 2. Ask the model.
            let action = tokio::select! {
                a = provider.step(&mut session, &screenshot, &cancel) => a,
                () = cancel.cancelled() => continue,
            };
            cost += provider.cost_per_step();
            step += 1;
            let action = match action {
                Ok(a) => a,
                Err(e) => {
                    break Outcome {
                        done: false,
                        summary: super::brain::failure_message(provider.name(), &e),
                        steps: step,
                        cost,
                    };
                }
            };
            match &action {
                CuAction::Done { summary } => {
                    break Outcome {
                        done: true,
                        summary: summary.clone(),
                        steps: step,
                        cost,
                    };
                }
                CuAction::Stop { reason } => {
                    break Outcome {
                        done: false,
                        summary: reason.clone(),
                        steps: step,
                        cost,
                    };
                }
                CuAction::Look { ms } => {
                    tokio::time::sleep(Duration::from_millis(*ms)).await;
                    continue;
                }
                _ => {}
            }
            let at = target_of(&action, &window, scale);
            show(self, step, cost, false, at);
            // The target, highlighted where it will act (CAP-12).
            if let Some((x, y)) = at.filter(|_| settings.highlight) {
                self.core.update_turn(|view| {
                    view.point = Some(PointTarget {
                        x: x - 14,
                        y: y - 14,
                        width: 28,
                        height: 28,
                        label: String::new(),
                    });
                });
            }
            // Gemini's "type at": click there first.
            let mut calls = Vec::new();
            if let CuAction::TypeAt { x, y, enter, .. } = &action {
                if let Some(c) = to_call(
                    &CuAction::Click {
                        x: *x,
                        y: *y,
                        button: "left".into(),
                        count: 1,
                    },
                    &window,
                    scale,
                ) {
                    calls.push(c);
                }
                if let Some(c) = to_call(&action, &window, scale) {
                    calls.push(c);
                }
                if *enter {
                    calls.push((
                        "input.press".into(),
                        json!({ "window": window.id.0.to_string(), "keys": ["enter"] }),
                    ));
                }
            } else if let Some(c) = to_call(&action, &window, scale) {
                calls.push(c);
            }
            let mut stopped = None;
            for (n, (tool_id, args)) in calls.into_iter().enumerate() {
                match self
                    .cu_act(&tool_id, args, &grants, watch, step, n, &window)
                    .await
                {
                    Ok(()) => last_ours = Instant::now(),
                    Err(Some(reason)) => {
                        stopped = Some(reason);
                        break;
                    }
                    // Skipped in watch mode: the model sees the unchanged screen next.
                    Err(None) => {}
                }
            }
            if let Some(reason) = stopped {
                break Outcome {
                    done: false,
                    summary: reason,
                    steps: step,
                    cost,
                };
            }
            tokio::time::sleep(pace(settings.speed)).await;
        };
        self.core.remove_activity(ACTIVITY);
        self.cu_view(|v| *v = None);
        self.core.update_turn(|view| view.point = None);
        self.recorder.computer_use_done(
            &self.turn_key(),
            &task,
            outcome.steps,
            outcome.cost,
            outcome.done,
        );
        outcome
    }

    /// One input action, through the permission engine. `Ok` when it ran; `Err(None)` when the
    /// user skipped it (watch mode); `Err(Some(why))` when the task can't go on.
    #[allow(clippy::too_many_arguments, reason = "one step's context")]
    async fn cu_act(
        self: &Arc<Self>,
        tool_id: &str,
        args: Value,
        grants: &[Grant],
        watch: bool,
        step: u32,
        n: usize,
        window: &WindowInfo,
    ) -> Result<(), Option<String>> {
        let capabilities = self.core.config().capabilities;
        let Some(tool) = self.registry.get(tool_id, &capabilities) else {
            return Err(Some(text::t("computer.inputOff")));
        };
        let mut call = ToolCall {
            id: format!("{}-cu{step}-{n}", self.turn_key()),
            tool: tool_id.to_owned(),
            args,
            initiated_by: Initiator::Task,
            targets: vec![Target::App {
                id: window.app_id.clone(),
                name: window.title.clone(),
            }],
        };
        let decision = self.authorize_with(tool.as_ref(), &mut call, Some(grants));
        self.recorder
            .tool_decision(&self.turn_key(), &call, tool.spec(), &decision);
        let permit = match decision {
            Decision::Deny(d) => return Err(Some(d.message)),
            Decision::Allow(permit) if !watch => permit,
            // Watch mode, or something the grant doesn't cover: the Island asks, with the target
            // highlighted (Allow / Skip).
            Decision::Allow(_) | Decision::Confirm(_) => {
                let spec = self.cu_confirm(tool.as_ref(), &call);
                match self.decide_for_agent(spec.clone(), call.clone()).await {
                    Some((true, _, by)) => {
                        kivo_security::confirmed(&spec, &call, kivo_security::Answer::Allow { by })
                            .map_err(|d| Some(d.message))?
                    }
                    Some((false, ..)) => return Err(None),
                    None => return Err(Some(text::t("computer.stopped"))),
                }
            }
        };
        let title = tool.spec().title.clone();
        let (outcome, _) = self.run_step(tool, &call, permit, &title).await;
        outcome.status.map(|_| ()).map_err(|e| Some(e.message))
    }

    /// The watch-mode card for one step: "Click here?" with Allow / Skip.
    fn cu_confirm(
        &self,
        tool: &dyn kivo_tools::Tool,
        call: &ToolCall,
    ) -> kivo_core::tool::ConfirmSpec {
        kivo_security::watch_card(tool.spec(), call)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivo_platform::{Rect, WindowId};

    fn window() -> WindowInfo {
        WindowInfo {
            id: WindowId(7),
            title: "Notes".into(),
            app_id: "notes.exe".into(),
            bounds: Rect {
                x: 100,
                y: 50,
                width: 1600,
                height: 1000,
            },
            minimized: false,
        }
    }

    #[test]
    fn actions_become_input_calls_in_screen_pixels() {
        // The screenshot was taken at half size: coordinates double and move to the window.
        let (tool, args) = to_call(
            &CuAction::Click {
                x: 10,
                y: 20,
                button: "left".into(),
                count: 2,
            },
            &window(),
            2.0,
        )
        .unwrap();
        assert_eq!(tool, "input.click");
        assert_eq!(
            (
                args["x"].as_i64(),
                args["y"].as_i64(),
                args["count"].as_i64()
            ),
            (Some(120), Some(90), Some(2))
        );
        assert_eq!(args["window"], "7");
        let (tool, args) = to_call(
            &CuAction::Press {
                keys: vec!["ctrl".into(), "s".into()],
            },
            &window(),
            1.0,
        )
        .unwrap();
        assert_eq!(
            (tool.as_str(), args["keys"][1].as_str()),
            ("input.press", Some("s"))
        );
        assert!(to_call(&CuAction::Look { ms: 0 }, &window(), 1.0).is_none());
        assert!(to_call(&CuAction::Move { x: 1, y: 1 }, &window(), 1.0).is_none());
        assert_eq!(
            target_of(&CuAction::Move { x: 5, y: 5 }, &window(), 1.0),
            Some((105, 55))
        );
    }

    #[test]
    fn watch_mode_limits_and_the_estimate() {
        let mut s = ComputerUse::default();
        assert!(
            watching(&s, 0) && watching(&s, 9) && !watching(&s, 10),
            "the first ten tasks"
        );
        s.approve = ApproveSteps::Always;
        assert!(watching(&s, 500));
        s.approve = ApproveSteps::Never;
        assert!(!watching(&s, 0));
        let s = ComputerUse::default();
        assert_eq!(
            (s.max_steps, s.max_cost_cents, s.time_limit_seconds),
            (25, 50, 120)
        );
        assert!((estimate(&s, 0.02) - 0.5).abs() < 1e-9);
        assert!((estimate(&s, 0.01) - 0.25).abs() < 1e-9);
        assert!(pace(CuSpeed::Careful) > pace(CuSpeed::Fast));
    }
}
