//! Routines and custom commands (ROUTINES): saved with the permissions they need granted at once,
//! scoped to the routine and its exact arguments (ROUT-03); checked for phrases and hotkeys that
//! clash with KIVO's commands, other routines and wake words (ROUT-07); matched in the grammar
//! stage before anything else, so a routine's phrase costs nothing and needs no AI (ROUT-05,
//! ROUT-06); run as a task (ROUT-02). KIVO ships a few starter routines, all off (ROUT-10).

use crate::core::Core;
use crate::tasks::Tasks;
use kivo_core::routine::{
    ROUTINE_VERSION, Routine, RoutineStep, Trigger, VarDef, VarKind, match_phrase,
};
use kivo_core::task::{OnError, StepAction};
use kivo_core::text;
use kivo_core::tool::Initiator;
use kivo_ipc::protocol::{Collision, GrantLine, RoutineCheck, RoutineView, ToolItem};
use kivo_store::Database;
use kivo_voice::wakeword::{CONFUSABLE, sound_alike};
use serde_json::{Map, Value, json};
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Lower case, words only.
fn norm(s: &str) -> String {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '{' && c != '}')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A chord for comparing: modifiers in a fixed order, then the key, lower case.
fn chord_key(chord: &str) -> String {
    let mut parts: Vec<String> = chord
        .split('+')
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    let order = |p: &str| match p {
        "ctrl" | "control" => 0,
        "alt" => 1,
        "shift" => 2,
        "win" | "windows" | "meta" => 3,
        _ => 4,
    };
    parts.sort_by_key(|p| (order(p), p.clone()));
    parts.join("+")
}

pub struct Routines {
    core: Arc<Core>,
    db: Arc<Mutex<Database>>,
    tasks: Arc<Tasks>,
    registry: Arc<kivo_tools::Registry>,
    hotkeys: watch::Sender<Vec<(String, String)>>,
    /// A routine drafted by voice, chat, "save what you just did" or an import, waiting for the
    /// user to review it in the builder (ROUT-13, ROUT-14). Nothing is saved until they do.
    draft: Mutex<Option<Routine>>,
}

/// What a `.kivo-routine.json` file says it is (ROUT-14).
pub const FILE_KIND: &str = "kivo-routine";
pub const FILE_VERSION: u64 = 1;
/// A routine file bigger than this isn't one.
const MAX_FILE_BYTES: usize = 256 * 1024;
/// Tools a routine can't hold: KIVO's own session commands and drafting itself.
fn routine_tool(tool: &str) -> bool {
    !tool.starts_with("session.") && tool != DRAFT_TOOL
}
/// Writes an exported routine to `dir` as "<name>.kivo-routine.json", never over another file.
pub fn save_file(dir: &std::path::Path, doc: &Value) -> Result<std::path::PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let name: String = doc["routine"]["name"]
        .as_str()
        .unwrap_or("Routine")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let name = if name.trim().is_empty() {
        "Routine".to_owned()
    } else {
        name.trim().to_owned()
    };
    let file = (1..)
        .map(|n: u32| {
            dir.join(if n < 2 {
                format!("{name}.kivo-routine.json")
            } else {
                format!("{name} ({n}).kivo-routine.json")
            })
        })
        .find(|f| !f.exists())
        .unwrap_or_else(|| dir.join("routine.kivo-routine.json"));
    let body = serde_json::to_string_pretty(doc).map_err(|e| e.to_string())?;
    std::fs::write(&file, body).map_err(|e| e.to_string())?;
    Ok(file)
}

/// The tool a brain calls to draft a routine for review (ROUT-13).
pub const DRAFT_TOOL: &str = "routines.draft";

impl Routines {
    pub fn new(
        core: Arc<Core>,
        db: Arc<Mutex<Database>>,
        tasks: Arc<Tasks>,
        registry: Arc<kivo_tools::Registry>,
    ) -> Arc<Self> {
        let routines = Arc::new(Self {
            core,
            db,
            tasks,
            registry,
            hotkeys: watch::Sender::new(Vec::new()),
            draft: Mutex::new(None),
        });
        routines.publish_hotkeys();
        routines
    }

    /// Every routine, as saved.
    pub fn all(&self) -> Vec<(Routine, kivo_store::tasks::StoredRoutine)> {
        lock(&self.db)
            .routines()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|row| {
                serde_json::from_str::<Routine>(&row.body)
                    .ok()
                    .map(|r| (r, row))
            })
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<Routine> {
        let row = lock(&self.db).routine(id).ok().flatten()?;
        serde_json::from_str(&row.body).ok()
    }

    fn view(routine: Routine, row: &kivo_store::tasks::StoredRoutine) -> RoutineView {
        RoutineView {
            contains_ai: routine.contains_ai(),
            custom_command: routine.is_custom_command(),
            granted: routine.is_granted(),
            last_run: row.last_run,
            updated_at: row.updated_at,
            routine,
        }
    }

    pub fn list(&self) -> Vec<RoutineView> {
        self.all()
            .into_iter()
            .map(|(r, row)| Self::view(r, &row))
            .collect()
    }

    /// The enabled routines' hotkeys: (chord, routine id).
    pub fn hotkeys(&self) -> watch::Receiver<Vec<(String, String)>> {
        self.hotkeys.subscribe()
    }

    fn publish_hotkeys(&self) {
        let keys: Vec<(String, String)> = self
            .all()
            .into_iter()
            .filter(|(r, _)| r.enabled)
            .flat_map(|(r, _)| {
                r.hotkeys()
                    .map(|c| (c.to_owned(), r.id.clone()))
                    .collect::<Vec<_>>()
            })
            .collect();
        self.hotkeys.send_replace(keys);
    }

    /// The tools a step can use, with their schemas (ROUT-09).
    pub fn tools(&self) -> Vec<ToolItem> {
        let mut out: Vec<ToolItem> = self
            .registry
            .all_specs()
            .into_iter()
            .filter(|s| !s.id.starts_with("agent."))
            .map(|s| ToolItem {
                id: s.id.clone(),
                title: s.title.clone(),
                description: s.description.clone(),
                params: s.params.clone(),
                risk: s.risk,
                capability: s.capability,
            })
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    /// What a draft needs and where it clashes (ROUT-03, ROUT-07).
    pub fn check(&self, draft: &Routine) -> RoutineCheck {
        let config = self.core.config();
        let mut problems = Vec::new();
        if draft.name.trim().is_empty() {
            problems.push(text::t("routine.noName"));
        }
        if draft.steps.is_empty() {
            problems.push(text::t("routine.noSteps"));
        }
        for s in &draft.steps {
            if let StepAction::Tool { tool, .. } = &s.action
                && self.registry.known(tool).is_none()
            {
                problems.push(text::tf("task.unknownTool", &[("tool", &tool)]));
            }
        }
        let mut seen_ids = std::collections::HashSet::new();
        for s in &draft.steps {
            if !seen_ids.insert(s.id.as_str()) {
                problems.push(text::tf("routine.duplicateStep", &[("id", &s.id)]));
            }
        }
        let grants = draft
            .needed_grants()
            .into_iter()
            .map(|g| {
                let spec = self.registry.known(&g.tool);
                let tool = self.registry.get(&g.tool, &config.capabilities);
                GrantLine {
                    title: spec.as_ref().map_or_else(
                        || g.tool.clone(),
                        |s| kivo_security::render_title(&s.title, &g.args),
                    ),
                    risk: tool.as_ref().map_or_else(
                        || {
                            spec.as_ref()
                                .map_or(kivo_core::tool::Risk::High, |s| s.risk)
                        },
                        |t| t.assess(&g.args, Initiator::Task),
                    ),
                    capability: spec
                        .as_ref()
                        .map_or(kivo_core::Capability::Routines, |s| s.capability),
                    capability_off: tool.is_none(),
                    tool: g.tool,
                }
            })
            .collect();
        RoutineCheck {
            collisions: self.collisions(draft),
            grants,
            problems,
        }
    }

    fn collisions(&self, draft: &Routine) -> Vec<Collision> {
        let config = self.core.config();
        let mut out = Vec::new();
        let grammar = kivo_intent::Grammar::bundled(&config.general.language)
            .or_else(|_| kivo_intent::Grammar::bundled("en"))
            .ok();
        let fixed = grammar.as_ref().map(|g| g.phrases()).unwrap_or_default();
        let others: Vec<Routine> = self
            .all()
            .into_iter()
            .map(|(r, _)| r)
            .filter(|r| r.id != draft.id)
            .collect();
        let mut wake: Vec<String> = lock(&self.db)
            .wake_words()
            .unwrap_or_default()
            .into_iter()
            .filter(|w| w.enabled)
            .map(|w| w.spoken().to_owned())
            .collect();
        if wake.is_empty() {
            wake.push("Hey Kivo".into());
        }
        let empty = kivo_intent::Index::default();
        for phrase in draft.phrases() {
            let sample = sample_text(phrase, &draft.variables);
            // A built-in command would be heard the same way.
            if let Some(g) = &grammar
                && let Some(m) = g.match_text(
                    &sample,
                    &kivo_intent::Context {
                        apps: &empty,
                        windows: &empty,
                    },
                )
            {
                out.push(Collision {
                    phrase: phrase.to_owned(),
                    kind: "command".into(),
                    with: m.pattern,
                    blocking: true,
                });
                continue;
            }
            let spoken = strip_vars(phrase);
            for command in &fixed {
                if sound_alike(&spoken, command) >= CONFUSABLE {
                    out.push(Collision {
                        phrase: phrase.to_owned(),
                        kind: "command".into(),
                        with: command.clone(),
                        blocking: false,
                    });
                }
            }
            for other in &others {
                for theirs in other.phrases() {
                    let exact = norm(theirs) == norm(phrase);
                    if exact || sound_alike(&strip_vars(theirs), &spoken) >= CONFUSABLE {
                        out.push(Collision {
                            phrase: phrase.to_owned(),
                            kind: "routine".into(),
                            with: other.name.clone(),
                            blocking: exact,
                        });
                    }
                }
            }
            for w in &wake {
                let exact = norm(w) == norm(phrase);
                if exact || sound_alike(w, &spoken) >= CONFUSABLE {
                    out.push(Collision {
                        phrase: phrase.to_owned(),
                        kind: "wakeWord".into(),
                        with: w.clone(),
                        blocking: exact,
                    });
                }
            }
        }
        // Hotkeys: KIVO's own, and other routines'.
        let own = [
            (
                config.voice.push_to_talk.join("+"),
                text::t("routine.keys.talk"),
            ),
            (
                config.voice.type_to_kivo.join("+"),
                text::t("routine.keys.type"),
            ),
            ("Ctrl+Shift+M".to_owned(), text::t("routine.keys.mode")),
            (
                config.permissions.emergency_stop.join("+"),
                text::t("routine.keys.stop"),
            ),
        ];
        for chord in draft.hotkeys() {
            let key = chord_key(chord);
            for (theirs, what) in &own {
                if chord_key(theirs) == key {
                    out.push(Collision {
                        phrase: chord.to_owned(),
                        kind: "hotkey".into(),
                        with: what.clone(),
                        blocking: true,
                    });
                }
            }
            for other in &others {
                if other.hotkeys().any(|c| chord_key(c) == key) {
                    out.push(Collision {
                        phrase: chord.to_owned(),
                        kind: "hotkey".into(),
                        with: other.name.clone(),
                        blocking: true,
                    });
                }
            }
        }
        out
    }

    /// Saves a routine. `grant`: the user approved the permission list shown with it, so it is
    /// granted exactly those steps (ROUT-03); without it, an edited routine keeps only the grants
    /// that still match its steps.
    pub fn save(&self, mut draft: Routine, grant: bool) -> Result<RoutineView, String> {
        draft.check_triggers()?;
        if draft.id.trim().is_empty() {
            draft.id = kivo_core::TaskId::new().to_string();
        }
        // Phrase variables not declared yet are text.
        let mut declared: Vec<String> = draft.variables.iter().map(|v| v.name.clone()).collect();
        let phrases: Vec<String> = draft.phrases().map(str::to_owned).collect();
        for p in &phrases {
            for var in phrase_vars(p) {
                if !declared.contains(&var) {
                    declared.push(var.clone());
                    draft.variables.push(VarDef {
                        name: var,
                        kind: VarKind::Text,
                    });
                }
            }
        }
        let check = self.check(&draft);
        if let Some(p) = check.problems.first() {
            return Err(p.clone());
        }
        if let Some(c) = check.collisions.iter().find(|c| c.blocking) {
            return Err(text::tf(
                "routine.clash",
                &[("phrase", &c.phrase), ("with", &c.with)],
            ));
        }
        let needed = draft.needed_grants();
        draft.grants = if grant {
            needed
        } else {
            let previous = self.get(&draft.id).map(|r| r.grants).unwrap_or_default();
            previous
                .into_iter()
                .filter(|g| needed.contains(g))
                .collect()
        };
        let body = serde_json::to_string(&draft).map_err(|e| e.to_string())?;
        lock(&self.db)
            .save_routine(
                &draft.id,
                &draft.name,
                draft.enabled,
                ROUTINE_VERSION,
                &body,
                draft.starter.as_deref(),
            )
            .map_err(|e| e.to_string())?;
        self.publish_hotkeys();
        let row = lock(&self.db)
            .routine(&draft.id)
            .ok()
            .flatten()
            .ok_or_else(|| text::t("routine.notFound"))?;
        Ok(Self::view(draft, &row))
    }

    pub fn delete(&self, id: &str) -> bool {
        let deleted = lock(&self.db).delete_routine(id).unwrap_or(false);
        self.publish_hotkeys();
        deleted
    }

    pub fn enable(&self, id: &str, on: bool) -> Result<RoutineView, String> {
        let mut routine = self.get(id).ok_or_else(|| text::t("routine.notFound"))?;
        if on && !routine.is_granted() {
            return Err(text::t("routine.needsGrant"));
        }
        routine.enabled = on;
        self.save(routine, false)
    }

    /// Runs a routine as a task (ROUT-02); `vars` from the phrase.
    pub fn run(&self, id: &str, vars: &Map<String, Value>) -> Result<String, String> {
        let routine = self.get(id).ok_or_else(|| text::t("routine.notFound"))?;
        if !routine.is_granted() {
            return Err(text::tf("routine.notGranted", &[("name", &routine.name)]));
        }
        self.tasks.create(routine.compile(vars))
    }

    /// Runs a routine that a schedule or an event started (ROUT-11), with no one at the keyboard
    /// having asked: its High-risk steps ask on screen first even though the routine is granted
    /// (ROUT-12). `cause` says what started it, for Activity.
    pub fn run_unattended(&self, id: &str, cause: &str) -> Result<String, String> {
        let routine = self.get(id).ok_or_else(|| text::t("routine.notFound"))?;
        if !routine.enabled {
            return Err(text::tf("routine.off", &[("name", &routine.name)]));
        }
        if !routine.is_granted() {
            return Err(text::tf("routine.notGranted", &[("name", &routine.name)]));
        }
        let config = self.core.config();
        let mut spec = routine.compile(&Map::new());
        for step in &mut spec.steps {
            if let kivo_core::task::StepAction::Tool { tool, args } = &step.action {
                let risk = self.registry.get(tool, &config.capabilities).map_or_else(
                    || {
                        self.registry
                            .known(tool)
                            .map_or(kivo_core::tool::Risk::High, |s| s.risk)
                    },
                    |t| t.assess(args, Initiator::Task),
                );
                if risk >= kivo_core::tool::Risk::High {
                    step.confirm = true;
                }
            }
        }
        tracing::info!(routine = %routine.name, cause, "a routine started on its own");
        self.tasks.create(spec)
    }

    /// Offers a draft for review: it's kept until the Routines page takes it, and the Control
    /// Center opens there (ROUT-13). Whatever it came with, it starts off, granted nothing, new.
    pub fn offer_draft(&self, mut draft: Routine) {
        draft.id = String::new();
        draft.enabled = false;
        draft.grants = Vec::new();
        draft.starter = None;
        draft.steps.retain(|s| match &s.action {
            StepAction::Tool { tool, .. } => routine_tool(tool),
            _ => true,
        });
        *lock(&self.draft) = Some(draft);
        self.core.open_control_center(Some("routines"));
        // An open Routines page takes it at once.
        self.core
            .bus
            .publish(kivo_core::Event::new(kivo_core::EventKind::System(
                kivo_core::event::SystemEvent::DiscoveryChanged {
                    section: "routines".into(),
                },
            )));
    }

    /// Whether a routine step can use `tool` (it exists in KIVO).
    pub fn knows_tool(&self, tool: &str) -> bool {
        self.registry.known(tool).is_some()
    }

    /// The draft waiting for review, if any; taking it clears it.
    pub fn take_draft(&self) -> Option<Routine> {
        lock(&self.draft).take()
    }

    /// A draft from what the last request did (ROUT-13 "Save what you just did as a routine"):
    /// its tool calls in order, named after what was said. `None` when it did nothing to repeat.
    pub fn draft_from_calls(said: &str, calls: &[(String, Value)]) -> Option<Routine> {
        let steps: Vec<RoutineStep> = calls
            .iter()
            .filter(|(tool, _)| routine_tool(tool))
            .enumerate()
            .map(|(i, (tool, args))| RoutineStep {
                id: format!("s{}", i + 1),
                action: StepAction::Tool {
                    tool: tool.clone(),
                    args: args.clone(),
                },
                on_error: OnError::Stop,
                delay_ms: None,
                parallel_group: None,
                confirm: false,
            })
            .collect();
        if steps.is_empty() {
            return None;
        }
        let mut name: String = said.trim().chars().take(40).collect();
        if let Some(first) = name.chars().next() {
            name = first.to_uppercase().chain(name.chars().skip(1)).collect();
        }
        Some(Routine {
            id: String::new(),
            name,
            description: String::new(),
            enabled: false,
            triggers: vec![Trigger::Manual],
            steps,
            variables: Vec::new(),
            grants: Vec::new(),
            starter: None,
        })
    }

    /// A routine as a `.kivo-routine.json` file (ROUT-14): what it does and what starts it,
    /// never its grants, id or whether it's on.
    pub fn export(&self, id: &str) -> Result<Value, String> {
        let r = self.get(id).ok_or_else(|| text::t("routine.notFound"))?;
        Ok(json!({
            "kind": FILE_KIND,
            "version": FILE_VERSION,
            "routine": {
                "name": r.name,
                "description": r.description,
                "triggers": r.triggers,
                "steps": r.steps,
                "variables": r.variables,
            },
        }))
    }

    /// Reads a `.kivo-routine.json` someone else may have written (ROUT-14): untrusted, so it
    /// must be the right kind and version, its steps must use tools KIVO has, and it arrives as a
    /// draft (off, granted nothing) whose permissions the builder shows before it's saved.
    pub fn parse_file(&self, content: &str) -> Result<Routine, String> {
        if content.len() > MAX_FILE_BYTES {
            return Err(text::t("routine.importBad"));
        }
        let doc: Value = serde_json::from_str(content).map_err(|_| text::t("routine.importBad"))?;
        if doc["kind"] != FILE_KIND || doc["version"].as_u64().is_none_or(|v| v > FILE_VERSION) {
            return Err(text::t("routine.importBad"));
        }
        let mut body = doc["routine"].clone();
        let Some(map) = body.as_object_mut() else {
            return Err(text::t("routine.importBad"));
        };
        map.insert("id".into(), json!(""));
        map.insert("enabled".into(), json!(false));
        map.insert("grants".into(), json!([]));
        map.remove("starter");
        let routine: Routine =
            serde_json::from_value(body).map_err(|_| text::t("routine.importBad"))?;
        for s in &routine.steps {
            if let StepAction::Tool { tool, .. } = &s.action
                && (!routine_tool(tool) || self.registry.known(tool).is_none())
            {
                return Err(text::tf("routine.importUnknownTool", &[("tool", tool)]));
            }
        }
        routine.check_triggers()?;
        Ok(routine)
    }

    /// The enabled routines, for the trigger runner.
    pub fn enabled(&self) -> Vec<Routine> {
        self.all()
            .into_iter()
            .map(|(r, _)| r)
            .filter(|r| r.enabled)
            .collect()
    }

    /// The enabled routine a request names, with its variables (the grammar stage, ROUT-05).
    pub fn match_text(&self, said: &str) -> Option<(Routine, Map<String, Value>)> {
        self.all()
            .into_iter()
            .map(|(r, _)| r)
            .filter(|r| r.enabled)
            .find_map(|r| {
                let vars = r
                    .phrases()
                    .find_map(|p| match_phrase(p, said, &r.variables))?;
                Some((r, vars))
            })
    }

    /// Adds the starter routines once, all off (ROUT-10). Deleting one doesn't bring it back.
    pub fn seed_starters(&self) {
        for starter in starters() {
            let key = starter.starter.clone().unwrap_or_default();
            let seen = lock(&self.db).starter_seen(&key).unwrap_or(true);
            if seen {
                continue;
            }
            if let Err(e) = self.save(starter, false) {
                tracing::warn!(%e, key, "couldn't add a starter routine");
            }
        }
    }
}

/// The phrase's `{variables}`.
fn phrase_vars(phrase: &str) -> Vec<String> {
    phrase
        .split('{')
        .skip(1)
        .filter_map(|p| p.split_once('}').map(|(name, _)| name.trim().to_owned()))
        .filter(|n| !n.is_empty())
        .collect()
}

/// The phrase as it might be said: variables filled with a sample value.
fn sample_text(phrase: &str, vars: &[VarDef]) -> String {
    let mut out = phrase.to_owned();
    for name in phrase_vars(phrase) {
        let number = vars
            .iter()
            .any(|v| v.name == name && v.kind == VarKind::Number);
        out = out.replace(
            &format!("{{{name}}}"),
            if number { "5" } else { "something" },
        );
    }
    out
}

/// The phrase without its variables, for comparing how it sounds.
fn strip_vars(phrase: &str) -> String {
    let mut out = phrase.to_owned();
    for name in phrase_vars(phrase) {
        out = out.replace(&format!("{{{name}}}"), "");
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn tool_step(id: &str, tool: &str, args: Value) -> RoutineStep {
    RoutineStep {
        id: id.into(),
        action: StepAction::Tool {
            tool: tool.into(),
            args,
        },
        on_error: OnError::Continue,
        delay_ms: None,
        parallel_group: None,
        confirm: false,
    }
}

fn say_step(id: &str, key: &str) -> RoutineStep {
    RoutineStep {
        id: id.into(),
        action: StepAction::Say { text: text::t(key) },
        on_error: OnError::Continue,
        delay_ms: None,
        parallel_group: None,
        confirm: false,
    }
}

fn phrase(key: &str) -> Trigger {
    Trigger::Phrase {
        phrases: text::t(key)
            .split('|')
            .map(|p| p.trim().to_owned())
            .collect(),
        lang: "en".into(),
    }
}

/// The starter routines (ROUTINES §6), built from tools KIVO has. Windows offers no documented
/// way to turn Focus on, mute other apps' notifications or change the default microphone, so
/// those parts are left for the user (DECISIONS "Starter routines").
pub fn starters() -> Vec<Routine> {
    let routine = |key: &str, steps: Vec<RoutineStep>, trigger: Trigger| Routine {
        id: String::new(),
        name: text::t(&format!("routine.starter.{key}.name")),
        description: text::t(&format!("routine.starter.{key}.about")),
        enabled: false,
        triggers: vec![trigger],
        steps,
        variables: Vec::new(),
        grants: Vec::new(),
        starter: Some(key.to_owned()),
    };
    let app = |id: &str, name: &str| json!({ "app": { "id": id, "name": name } });
    let mut work_code = tool_step(
        "code",
        "apps.launch",
        app("Visual Studio Code", "Visual Studio Code"),
    );
    work_code.parallel_group = Some("open".into());
    let mut work_browser = tool_step("browser", "apps.launch", app("Chrome", "Google Chrome"));
    work_browser.parallel_group = Some("open".into());
    let mut lock_step = tool_step("lock", "system.lock", json!({}));
    lock_step.confirm = true;
    let mut sleep_step = tool_step("sleep", "system.sleep", json!({}));
    sleep_step.confirm = true;
    let mut focus = routine(
        "focus",
        vec![
            tool_step("pause", "media.play_pause", json!({})),
            tool_step(
                "timer",
                "tasks.remind",
                json!({ "number": "{minutes}", "unit": "minutes", "text": text::t("routine.starter.focus.over") }),
            ),
            say_step("say", "routine.starter.focus.say"),
        ],
        phrase("routine.starter.focus.phrases"),
    );
    focus.variables = vec![VarDef {
        name: "minutes".into(),
        kind: VarKind::Number,
    }];
    vec![
        routine(
            "work-mode",
            vec![
                work_code,
                work_browser,
                say_step("say", "routine.starter.work-mode.say"),
            ],
            phrase("routine.starter.work-mode.phrases"),
        ),
        routine(
            "break",
            vec![tool_step("pause", "media.play_pause", json!({})), lock_step],
            phrase("routine.starter.break.phrases"),
        ),
        routine(
            "meeting",
            vec![
                tool_step("mic", "audio.mic_unmute", json!({})),
                tool_step(
                    "calendar",
                    "browser.open_url",
                    json!({ "url": "https://calendar.google.com" }),
                ),
                say_step("say", "routine.starter.meeting.say"),
            ],
            phrase("routine.starter.meeting.phrases"),
        ),
        routine(
            "goodnight",
            vec![say_step("say", "routine.starter.goodnight.say"), sleep_step],
            phrase("routine.starter.goodnight.phrases"),
        ),
        focus,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_compare_in_any_order_and_case() {
        assert_eq!(chord_key("Shift+ctrl+W"), chord_key("Ctrl+Shift+w"));
        assert_ne!(chord_key("Ctrl+W"), chord_key("Alt+W"));
    }

    #[test]
    fn phrase_samples_and_variables() {
        let vars = [VarDef {
            name: "minutes".into(),
            kind: VarKind::Number,
        }];
        assert_eq!(
            phrase_vars("focus for {minutes} min on {thing}"),
            ["minutes", "thing"]
        );
        assert_eq!(sample_text("focus for {minutes}", &vars), "focus for 5");
        assert_eq!(
            strip_vars("focus for {minutes} minutes"),
            "focus for minutes"
        );
    }

    #[test]
    fn starters_are_off_and_granted_nothing_yet() {
        let all = starters();
        assert_eq!(all.len(), 5);
        for r in &all {
            assert!(
                !r.enabled && r.grants.is_empty() && r.starter.is_some(),
                "{}",
                r.name
            );
            assert!(!r.phrases().collect::<Vec<_>>().is_empty());
        }
        let focus = all
            .iter()
            .find(|r| r.starter.as_deref() == Some("focus"))
            .unwrap();
        let task = focus.compile(json!({ "minutes": 25 }).as_object().unwrap());
        assert_eq!(task.validate(), Ok(()));
        assert!(matches!(
            &task.steps[1].action,
            StepAction::Tool { args, .. } if args["number"] == 25
        ));
    }
}
