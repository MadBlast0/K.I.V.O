//! The M5 requests the Control Center and the Island make: Tasks (UX-24), Routines and their
//! builder (UX-26, ROUTINES §4), the Agents page (UX-25, DISC-06) and "Open in terminal"
//! (CONV-13), workspaces and instructions (CONV-09/10/11), Bypass permissions (SEC-03), the Draft
//! card (CONV-15), "What can I say?" (UX-44), "What did I miss?" (UX-40) and the selection shortcut
//! (UX-42).

use crate::core::Core;
use crate::engine::Engine;
use crate::routines::Routines;
use crate::tasks::Tasks;
use crate::workspaces::Workspaces;
use kivo_core::event::CancelReason;
use kivo_core::routine::Routine;
use kivo_core::text;
use kivo_ipc::RpcError;
use kivo_ipc::protocol::{AgentItem, AgentSessionItem, AgentsOverview, DesktopAiItem, method};
use kivo_tools::agents_tools::{TerminalSession, TerminalSessions};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::path::Path;
use std::sync::Arc;

pub struct TasksRpc {
    pub core: Arc<Core>,
    pub engine: Arc<Engine>,
    pub tasks: Arc<Tasks>,
    pub routines: Arc<Routines>,
    pub workspaces: Arc<Workspaces>,
    pub verifier: Arc<dyn kivo_platform::UserVerifier>,
    pub terminals: Arc<dyn kivo_platform::Terminals>,
    pub terminal_sessions: Arc<TerminalSessions>,
    pub apps: Arc<kivo_tools::appreg::AppRegistry>,
    pub uia: Arc<dyn kivo_platform::UiAutomation>,
    pub clipboard: Arc<dyn kivo_platform::Clipboard>,
    pub db: Arc<std::sync::Mutex<kivo_store::Database>>,
    /// Where exported routines go (Downloads).
    pub exports: Option<std::path::PathBuf>,
    /// Installs CLI agents and tools with the user's consent (DISC-07, DIST-14).
    pub installer: Arc<crate::installer::Installer>,
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RpcError> {
    serde_json::from_value(params).map_err(RpcError::invalid_params)
}

fn ok<T: serde::Serialize>(value: &T) -> Result<Value, RpcError> {
    serde_json::to_value(value).map_err(|e| RpcError::new(RpcError::INTERNAL, e.to_string()))
}

fn refuse(message: String) -> RpcError {
    RpcError::new(RpcError::REFUSED, message)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Id {
    id: String,
}

fn now_ms() -> i64 {
    kivo_store::brains::now_ms()
}

impl TasksRpc {
    /// Handles `name` if it is one of these requests.
    #[allow(clippy::too_many_lines, reason = "one table of requests")]
    pub async fn call(&self, name: &str, params: Value) -> Option<Result<Value, RpcError>> {
        Some(match name {
            // ---- Tasks (UX-24) --------------------------------------------------------------
            method::TASKS_LIST => {
                #[derive(Deserialize, Default)]
                #[serde(default, deny_unknown_fields)]
                struct P {
                    finished: bool,
                    limit: Option<u32>,
                }
                parse::<P>(params)
                    .and_then(|p| ok(&self.tasks.list(p.finished, p.limit.unwrap_or(100).min(500))))
            }
            method::TASKS_GET => parse::<Id>(params).and_then(|Id { id }| {
                self.tasks
                    .view(&id)
                    .map_or_else(|| Err(refuse(text::t("task.notFound"))), |v| ok(&v))
            }),
            method::TASKS_CANCEL => parse::<Id>(params).and_then(|Id { id }| {
                self.tasks
                    .cancel(&id, CancelReason::UserButton)
                    .map(|()| Value::Null)
                    .map_err(refuse)
            }),
            method::TASKS_PAUSE => parse::<Id>(params)
                .and_then(|Id { id }| self.tasks.pause(&id).map(|()| Value::Null).map_err(refuse)),
            method::TASKS_RESUME => parse::<Id>(params)
                .and_then(|Id { id }| self.tasks.resume(&id).map(|()| Value::Null).map_err(refuse)),
            method::TASKS_ANSWER => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    id: String,
                    choice: String,
                }
                parse::<P>(params).and_then(|p| {
                    self.tasks
                        .answer(&p.id, &p.choice)
                        .map(|()| Value::Null)
                        .map_err(refuse)
                })
            }
            method::TASKS_DELETE => parse::<Id>(params)
                .and_then(|Id { id }| self.tasks.delete(&id).map(|()| Value::Null).map_err(refuse)),
            method::TASKS_CLEAR => ok(&self.tasks.clear_finished()),

            // ---- Routines (UX-26, ROUT-01..10) ------------------------------------------------
            method::ROUTINES_LIST => ok(&self.routines.list()),
            method::ROUTINES_TOOLS => ok(&self.routines.tools()),
            method::ROUTINES_DRAFT => ok(&self.routines.take_draft()),
            method::ROUTINES_EXPORT => parse::<Id>(params).and_then(|Id { id }| {
                let dir = self
                    .exports
                    .clone()
                    .ok_or_else(|| refuse(kivo_core::text::t("settings.noExports")))?;
                let doc = self.routines.export(&id).map_err(refuse)?;
                crate::routines::save_file(&dir, &doc)
                    .map(|file| json!({ "file": file.display().to_string() }))
                    .map_err(refuse)
            }),
            method::ROUTINES_TO_SKILL => parse::<Id>(params).and_then(|Id { id }| {
                let routine = self
                    .routines
                    .get(&id)
                    .ok_or_else(|| refuse(kivo_core::text::t("routine.notFound")))?;
                let skills = self
                    .engine
                    .skills()
                    .ok_or_else(|| refuse(kivo_core::text::t("task.notReady")))?;
                skills
                    .from_routine(&routine)
                    .map(|skill| json!({ "skill": skill }))
                    .map_err(refuse)
            }),
            method::INSTALLS_PLAN => parse::<Id>(params).and_then(|Id { id }| {
                let installer = Arc::clone(&self.installer);
                tokio::task::block_in_place(|| installer.plan(&id))
                    .map_err(refuse)
                    .and_then(|p| ok(&p))
            }),
            method::INSTALLS_START => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    id: String,
                    /// The commands the user was shown and agreed to.
                    commands: Vec<String>,
                }
                parse::<P>(params).and_then(|p| {
                    let installer = Arc::clone(&self.installer);
                    tokio::task::block_in_place(|| installer.start(&p.id, &p.commands))
                        .map(|()| Value::Null)
                        .map_err(refuse)
                })
            }
            method::INSTALLS_STATUS => {
                parse::<Id>(params).and_then(|Id { id }| ok(&self.installer.status(&id)))
            }
            method::INSTALLS_CANCEL => {
                parse::<Id>(params).and_then(|Id { id }| ok(&self.installer.cancel(&id)))
            }
            method::INSTALLS_TOOLS => {
                let installer = Arc::clone(&self.installer);
                let tools = tokio::task::block_in_place(|| {
                    crate::installer::DEPENDENCIES
                        .iter()
                        .map(|d| {
                            json!({
                                "id": d.id, "name": d.name, "vendor": d.vendor,
                                "version": installer.version(d.command),
                            })
                        })
                        .collect::<Vec<_>>()
                });
                ok(&tools)
            }
            method::ROUTINES_IMPORT => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    content: String,
                }
                parse::<P>(params).and_then(|p| {
                    self.routines
                        .parse_file(&p.content)
                        .map_err(refuse)
                        .and_then(|r| ok(&r))
                })
            }
            method::ROUTINES_CHECK => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    routine: Routine,
                }
                parse::<P>(params).and_then(|p| ok(&self.routines.check(&p.routine)))
            }
            method::ROUTINES_SAVE => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    routine: Routine,
                    /// The user approved the permission list shown with it (ROUT-03).
                    #[serde(default)]
                    grant: bool,
                }
                parse::<P>(params).and_then(|p| {
                    self.routines
                        .save(p.routine, p.grant)
                        .map_err(refuse)
                        .and_then(|v| ok(&v))
                })
            }
            method::ROUTINES_DELETE => {
                parse::<Id>(params).and_then(|Id { id }| ok(&self.routines.delete(&id)))
            }
            method::ROUTINES_ENABLE => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    id: String,
                    on: bool,
                }
                parse::<P>(params).and_then(|p| {
                    self.routines
                        .enable(&p.id, p.on)
                        .map_err(refuse)
                        .and_then(|v| ok(&v))
                })
            }
            method::ROUTINES_RUN => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    id: String,
                    #[serde(default)]
                    vars: Map<String, Value>,
                }
                parse::<P>(params).and_then(|p| {
                    self.routines
                        .run(&p.id, &p.vars)
                        .map(|task| json!({ "task": task }))
                        .map_err(refuse)
                })
            }

            // ---- Agents (UX-25, DISC-06, CONV-13) ---------------------------------------------
            method::AGENTS_OVERVIEW => ok(&self.overview().await),
            method::AGENTS_OPEN_TERMINAL => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    session: String,
                }
                parse::<P>(params).and_then(|p| {
                    self.open_in_terminal(&p.session)
                        .map(|title| json!({ "title": title }))
                        .map_err(refuse)
                })
            }

            method::AGENTS_START => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    agent: String,
                    folder: String,
                    #[serde(default)]
                    mode: Option<String>,
                }
                match parse::<P>(params) {
                    Ok(p) => {
                        self.start_agent(&p.agent, &p.folder, p.mode.as_deref())
                            .await
                    }
                    Err(e) => Err(e),
                }
            }

            // ---- Workspaces and instructions (CONV-09/10/11) ----------------------------------
            method::WORKSPACES_LIST => ok(&json!({
                "workspaces": self.workspaces.list(),
                "current": self.workspaces.current().map(|w| w.id),
            })),
            method::WORKSPACES_REMEMBER | method::OFFER_ANSWER => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    id: String,
                    accept: bool,
                }
                parse::<P>(params).and_then(|p| {
                    self.engine
                        .answer_offer(&p.id, p.accept)
                        .map_err(refuse)
                        .and_then(|(_, w)| ok(&w))
                })
            }
            method::WORKSPACES_FORGET => parse::<Id>(params).map(|Id { id }| {
                self.workspaces.forget(&id);
                Value::Null
            }),
            method::WORKSPACES_EXPORT_AGENTS => parse::<Id>(params).and_then(|Id { id }| {
                self.workspaces
                    .export_agents_md(&id)
                    .map(|file| json!({ "file": file }))
                    .map_err(refuse)
            }),
            method::INSTRUCTIONS_GET => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    scope: String,
                }
                parse::<P>(params).map(
                    |p| json!({ "scope": p.scope, "text": self.workspaces.instructions(&p.scope) }),
                )
            }
            method::INSTRUCTIONS_SET => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    scope: String,
                    text: String,
                }
                parse::<P>(params).and_then(|p| {
                    self.workspaces
                        .set_instructions(&p.scope, &p.text)
                        .map(|()| Value::Null)
                        .map_err(refuse)
                })
            }

            // ---- Bypass permissions (SEC-03) ---------------------------------------------------
            method::PERMISSIONS_BYPASS => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    on: bool,
                    /// 15, 60, or none for "until turned off".
                    #[serde(default)]
                    minutes: Option<u32>,
                    /// Confirm with Windows Hello first (the dialog's option).
                    #[serde(default)]
                    hello: bool,
                }
                parse::<P>(params).and_then(|p| self.bypass(p.on, p.minutes, p.hello))
            }

            // ---- The Draft card (CONV-15) ------------------------------------------------------
            method::DRAFT_ANSWER => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase", deny_unknown_fields)]
                struct P {
                    call_id: String,
                    text: String,
                }
                parse::<P>(params).and_then(|p| {
                    self.engine
                        .edit_draft(&p.call_id, &p.text)
                        .map(|()| Value::Null)
                        .map_err(refuse)
                })
            }

            // ---- Conveniences (UX-40, UX-42, UX-44) --------------------------------------------
            method::SESSION_HELP => ok(&self.engine.help_examples()),
            method::SESSION_MISSED => ok(&json!({
                "summary": self.tasks.notifier().missed_summary(),
            })),
            method::SELECTION_ACTION => {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct P {
                    action: String,
                    #[serde(default)]
                    language: Option<String>,
                }
                match parse::<P>(params) {
                    Ok(p) => self.selection(&p.action, p.language.as_deref()).await,
                    Err(e) => Err(e),
                }
            }
            _ => return None,
        })
    }

    /// "Start" on the Agents page (CONV-14): a request of its own, so the permission engine
    /// decides and a bypass launch (High) waits for the user's yes in the Island.
    async fn start_agent(
        &self,
        agent: &str,
        folder: &str,
        mode: Option<&str>,
    ) -> Result<Value, RpcError> {
        let entry = self
            .apps
            .entries()
            .iter()
            .find(|e| e.id == agent && e.agent.is_some())
            .cloned()
            .ok_or_else(|| refuse(text::tf("error.agents.unknown", &[("name", &agent)])))?;
        let mode = mode.unwrap_or("default");
        let place = Path::new(folder)
            .file_name()
            .map_or_else(|| folder.to_owned(), |n| n.to_string_lossy().into_owned());
        let request = text::tf(
            "agents.startRequest",
            &[("name", &entry.name), ("place", &place), ("mode", &mode)],
        );
        self.engine
            .act(
                &request,
                "agents.open_terminal",
                json!({ "agent": entry.id, "folder": folder, "mode": mode }),
            )
            .await
            .map(|()| Value::Null)
            .map_err(refuse)
    }

    /// Bypass permissions (SEC-03): only from the dialog, optionally behind Windows Hello, with an
    /// expiry; every change audited.
    fn bypass(&self, on: bool, minutes: Option<u32>, hello: bool) -> Result<Value, RpcError> {
        if !on {
            self.core.disable_bypass();
            self.engine
                .recorder
                .user_action("permissions.bypass", &text::t("bypass.off"));
            return Ok(Value::Null);
        }
        if hello {
            if !self.verifier.available() {
                return Err(refuse(text::t("bypass.noHello")));
            }
            let verified = self
                .verifier
                .verify(&text::t("bypass.helloPrompt"))
                .unwrap_or(false);
            if !verified {
                return Err(refuse(text::t("bypass.notVerified")));
            }
        }
        let until = match minutes {
            Some(m) if (1..=24 * 60).contains(&m) => {
                u64::try_from(now_ms()).unwrap_or(0) + u64::from(m) * 60_000
            }
            Some(_) => return Err(refuse(text::t("bypass.badDuration"))),
            None => crate::core::BYPASS_UNTIL_OFF,
        };
        self.core.enable_bypass(until);
        let summary = match minutes {
            Some(m) => text::tf("bypass.onFor", &[("minutes", &m)]),
            None => text::t("bypass.onUntilOff"),
        };
        self.engine
            .recorder
            .user_action("permissions.bypass", &summary);
        Ok(json!({ "until": until }))
    }

    /// The Agents page (UX-25): CLI agents found or not, desktop AI apps (DISC-06), sessions.
    async fn overview(&self) -> AgentsOverview {
        let cli = kivo_brain::catalog::CATALOG
            .iter()
            .filter(|e| e.agent.is_some())
            .map(|e| {
                let found = self.engine.brains.agent(e.id);
                AgentItem {
                    id: e.id.to_owned(),
                    name: e.name.to_owned(),
                    installed: found.is_some(),
                    signed_in: found.as_ref().and_then(|f| f.signed_in),
                    version: found.and_then(|f| f.version),
                    terminal: self
                        .apps
                        .entries()
                        .iter()
                        .any(|a| a.id == e.id && a.agent.is_some()),
                }
            })
            .collect();
        let installed = self.engine.installed_apps();
        let desktop = self
            .apps
            .entries()
            .iter()
            .filter(|e| e.desktop_ai)
            .filter_map(|e| {
                installed
                    .iter()
                    .find(|a| e.is_named(&a.name) || a.name.eq_ignore_ascii_case(&e.name))
                    .map(|a| DesktopAiItem {
                        id: e.id.clone(),
                        name: e.name.clone(),
                        app_id: a.id.clone(),
                    })
            })
            .collect();
        let running = self.engine.agents.running().await;
        let rows = self
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .agent_sessions(50)
            .unwrap_or_default();
        let mut sessions: Vec<AgentSessionItem> = rows
            .into_iter()
            .map(|r| AgentSessionItem {
                running: running
                    .iter()
                    .any(|(a, w)| *a == r.agent && w.display().to_string() == r.workspace),
                resumable: self.resume_args(&r.agent).is_some(),
                kind: "acp".into(),
                id: r.id,
                agent: r.agent,
                workspace: r.workspace,
                last_used: r.last_used,
            })
            .collect();
        sessions.extend(self.terminal_sessions.list().into_iter().rev().map(|s| {
            AgentSessionItem {
                id: s.id,
                agent: s.agent,
                workspace: s.cwd.display().to_string(),
                kind: "terminal".into(),
                running: true,
                resumable: false,
                last_used: s.started,
            }
        }));
        AgentsOverview {
            cli,
            desktop,
            sessions,
        }
    }

    fn resume_args(&self, agent: &str) -> Option<(String, Vec<String>)> {
        let entry = self.apps.entries().iter().find(|e| e.id == agent)?;
        let launch = entry.agent.as_ref()?;
        (!launch.resume.is_empty()).then(|| (launch.command.clone(), launch.resume.clone()))
    }

    /// CONV-13: hands a KIVO-run agent session to a visible terminal, where the agent can resume
    /// it, so the user can take over.
    fn open_in_terminal(&self, session: &str) -> Result<String, String> {
        let row = self
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .agent_sessions(500)
            .unwrap_or_default()
            .into_iter()
            .find(|r| r.id == session)
            .ok_or_else(|| text::t("agents.noSession"))?;
        let (command, resume) = self
            .resume_args(&row.agent)
            .ok_or_else(|| text::t("agents.cantResume"))?;
        let args: Vec<String> = resume
            .iter()
            .map(|a| a.replace("{session}", &row.id))
            .collect();
        let name =
            kivo_brain::catalog::entry(&row.agent).map_or(row.agent.clone(), |e| e.name.to_owned());
        let cwd = Path::new(&row.workspace);
        let place = cwd.file_name().map_or_else(
            || row.workspace.clone(),
            |n| n.to_string_lossy().into_owned(),
        );
        let title = format!("{name} · {place}");
        self.terminals
            .open(cwd, &title, &command, &args)
            .map_err(|e| e.to_string())?;
        self.terminal_sessions.add(TerminalSession {
            id: format!("term-{}", now_ms()),
            agent: row.agent,
            name,
            title: title.clone(),
            cwd: cwd.to_path_buf(),
            mode: "resume".into(),
            started: now_ms(),
        });
        Ok(title)
    }

    /// UX-42: with text selected in any app, Explain / Rewrite / Translate. The selection comes
    /// from UI Automation (TextPattern) or, with the Clipboard capability on, the clipboard; it is
    /// someone else's text, so it goes to the brain as an untrusted attachment.
    async fn selection(&self, action: &str, language: Option<&str>) -> Result<Value, RpcError> {
        let config = self.core.config();
        let caps = &config.capabilities;
        // What was selected when the text box opened; else what is selected now; else the
        // clipboard.
        let mut selected = self.core.take_selection();
        if selected.is_none() && caps.enabled(kivo_core::Capability::UiAutomation) {
            selected = self.uia.selected_text().ok().flatten();
        }
        if selected.as_deref().is_none_or(|s| s.trim().is_empty())
            && caps.enabled(kivo_core::Capability::Clipboard)
        {
            selected = self.clipboard.read_text().ok().flatten();
        }
        let Some(selected) = selected.filter(|s| !s.trim().is_empty()) else {
            return Err(refuse(text::t("selection.nothing")));
        };
        let request = match action {
            "explain" => text::t("selection.explain"),
            "rewrite" => text::t("selection.rewrite"),
            "translate" => text::tf(
                "selection.translate",
                &[("language", &language.unwrap_or("English"))],
            ),
            _ => return Err(refuse(text::t("selection.unknown"))),
        };
        self.engine
            .say_in(
                &request,
                None,
                None,
                vec![(text::t("selection.source"), selected)],
            )
            .await
            .map(|()| Value::Null)
            .map_err(refuse)
    }
}
