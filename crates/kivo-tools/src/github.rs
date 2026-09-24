//! GitHub through the user's own `gh` sign-in (INTEGRATIONS §2, INT-06): pull requests, issues,
//! notifications and who's signed in, over the REST and GraphQL APIs via `gh api`. KIVO never
//! sees or stores the token; `gh` uses its own. Everything read is GitHub's content, so it's
//! untrusted. Creating an issue sends the user's words to GitHub: it asks first, and its text
//! goes in a file `gh` reads, never into the command line.

use crate::builtin::{Def, object, platform_error};
use crate::controls::{Builder, Controls};
use crate::registry::{Output, Tool};
use kivo_core::capability::Capability;
use kivo_core::text;
use kivo_core::tool::{CapabilityTier, Reversibility, Risk, SideEffect, ToolError, ToolErrorCode};
use kivo_platform::{CommandSpec, ShellKind};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const TIMEOUT: Duration = Duration::from_secs(30);
/// Items a list returns.
const LIMIT: usize = 20;

fn def(
    id: &'static str,
    description: &'static str,
    params: Value,
    risk: Risk,
    effects: &'static [SideEffect],
    egress: bool,
) -> Def {
    Def {
        id,
        description,
        params,
        risk,
        effects,
        capability: Capability::Integrations,
        reversibility: if egress {
            Reversibility::Irreversible
        } else {
            Reversibility::NotApplicable
        },
        egress,
        timeout_ms: 40_000,
        tier: CapabilityTier::AppCli,
    }
}

/// `owner/name`, as GitHub allows it: nothing that could reach a shell.
pub fn repo_arg(args: &Value) -> Result<String, ToolError> {
    let repo = args["repo"].as_str().unwrap_or_default().trim();
    let ok = repo.split_once('/').is_some_and(|(o, n)| {
        let part = |s: &str| {
            !s.is_empty()
                && s.len() <= 100
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        };
        part(o) && part(n) && !n.contains('/')
    });
    ok.then(|| repo.to_owned())
        .ok_or_else(|| ToolError::new(ToolErrorCode::InvalidArgs, text::t("reply.github.badRepo")))
}

/// Runs `gh …` (arguments KIVO built; never user text) and parses its JSON.
fn gh(c: &Controls, args: &str, cancel: &CancellationToken) -> Result<Value, ToolError> {
    let spec = CommandSpec {
        command: format!("gh {args}"),
        shell: ShellKind::Cmd,
        cwd: c.shell_home.clone(),
        timeout: TIMEOUT,
        env: vec![("GH_PROMPT_DISABLED".into(), "1".into())],
        max_memory_mb: 512,
        max_cpu_percent: 50,
    };
    let out = c
        .commands
        .run(&spec, &|| cancel.is_cancelled())
        .map_err(platform_error)?;
    if out.exit_code != Some(0) {
        let why = format!("{}\n{}", out.stdout, out.stderr);
        let lower = why.to_lowercase();
        let message = if lower.contains("not recognized")
            || lower.contains("not found") && lower.contains("gh")
        {
            text::t("reply.github.noGh")
        } else if lower.contains("auth login") || lower.contains("not logged") {
            text::t("reply.github.signIn")
        } else {
            text::t("reply.github.failed")
        };
        return Err(ToolError::new(ToolErrorCode::Failed, message).with_detail(
            why.lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or_default()
                .trim()
                .to_owned(),
        ));
    }
    serde_json::from_str(out.stdout.trim())
        .map_err(|_| ToolError::new(ToolErrorCode::Failed, text::t("reply.github.failed")))
}

const SOURCE: &str = "github.com";

pub(crate) fn tools(c: &Arc<Controls>) -> Vec<Arc<dyn Tool>> {
    let b = Builder::new(c);
    vec![
        b.tool(
            &def(
                "github.pull_requests",
                "Open pull requests: in a repository (owner/name), or the user's own anywhere on GitHub.",
                object(json!({ "repo": { "type": "string", "description": "owner/name; the user's own when left out" } }), &[]),
                Risk::Safe,
                &[SideEffect::LocalRead],
                false,
            ),
            Box::new(|args, c, cancel| {
                let fields = "number,title,author,url,updatedAt,repository";
                let list = if args["repo"].is_string() {
                    let repo = repo_arg(args)?;
                    gh(c, &format!("pr list --repo {repo} --state open --limit {LIMIT} --json number,title,author,url,updatedAt"), cancel)?
                } else {
                    gh(c, &format!("search prs --author @me --state open --limit {LIMIT} --json {fields}"), cancel)?
                };
                let n = list.as_array().map_or(0, Vec::len) as u64;
                Ok(Output::new(text::plural("reply.github.prs", n, &[]), json!({ "pullRequests": list }))
                    .untrusted(SOURCE))
            }),
        )
        .build(),
        b.tool(
            &def(
                "github.issues",
                "Open issues in a repository (owner/name), optionally only those assigned to the user.",
                object(
                    json!({
                        "repo": { "type": "string" },
                        "mine": { "type": "boolean", "description": "Only issues assigned to the user" }
                    }),
                    &["repo"],
                ),
                Risk::Safe,
                &[SideEffect::LocalRead],
                false,
            ),
            Box::new(|args, c, cancel| {
                let repo = repo_arg(args)?;
                let mine = if args["mine"].as_bool() == Some(true) { " --assignee @me" } else { "" };
                let list = gh(c, &format!("issue list --repo {repo} --state open --limit {LIMIT}{mine} --json number,title,labels,url,updatedAt"), cancel)?;
                let n = list.as_array().map_or(0, Vec::len) as u64;
                Ok(Output::new(text::plural("reply.github.issues", n, &[]), json!({ "issues": list }))
                    .untrusted(SOURCE))
            }),
        )
        .build(),
        b.tool(
            &def(
                "github.notifications",
                "The user's unread GitHub notifications (REST).",
                object(json!({}), &[]),
                Risk::Safe,
                &[SideEffect::LocalRead],
                false,
            ),
            Box::new(|_, c, cancel| {
                let list = gh(c, "api notifications", cancel)?;
                let items: Vec<Value> = list
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(LIMIT)
                    .map(|n| json!({
                        "title": n["subject"]["title"], "type": n["subject"]["type"],
                        "repo": n["repository"]["full_name"], "reason": n["reason"],
                    }))
                    .collect();
                let n = items.len() as u64;
                Ok(Output::new(text::plural("reply.github.notifications", n, &[]), json!({ "notifications": items }))
                    .untrusted(SOURCE))
            }),
        )
        .build(),
        b.tool(
            &def(
                "github.whoami",
                "Who is signed in to GitHub through gh, and how many pull requests they have open (GraphQL).",
                object(json!({}), &[]),
                Risk::Safe,
                &[SideEffect::LocalRead],
                false,
            ),
            Box::new(|_, c, cancel| {
                let data = gh(c, "api graphql -f query=\"query{viewer{login name pullRequests(states:OPEN){totalCount}}}\"", cancel)?;
                let viewer = &data["data"]["viewer"];
                let login = viewer["login"].as_str().unwrap_or_default().to_owned();
                let open = viewer["pullRequests"]["totalCount"].as_u64().unwrap_or(0);
                Ok(Output::new(
                    text::tf("reply.github.whoami", &[("login", &login), ("open", &open)]),
                    json!({ "login": login, "name": viewer["name"], "openPullRequests": open }),
                )
                .untrusted(SOURCE))
            }),
        )
        .build(),
        b.tool(
            &def(
                "github.create_issue",
                "Open an issue in a repository (owner/name) with a title and body. Sends the text to GitHub; the user confirms first.",
                object(
                    json!({
                        "repo": { "type": "string" },
                        "title": { "type": "string" },
                        "body": { "type": "string" }
                    }),
                    &["repo", "title"],
                ),
                Risk::Medium,
                &[SideEffect::ExternalComms],
                true,
            ),
            Box::new(|args, c, cancel| {
                let repo = repo_arg(args)?;
                let title = args["title"].as_str().unwrap_or_default().trim();
                if title.is_empty() {
                    return Err(ToolError::new(ToolErrorCode::InvalidArgs, text::t("reply.github.noTitle")));
                }
                // The text travels in a file `gh` reads, so nothing the user wrote reaches a shell.
                let body = json!({ "title": title, "body": args["body"].as_str().unwrap_or_default() });
                let file = std::env::temp_dir().join(format!(
                    "kivo-gh-issue-{}-{}.json",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_nanos())
                ));
                std::fs::write(&file, body.to_string()).map_err(|e| {
                    ToolError::new(ToolErrorCode::Failed, text::t("reply.github.failed")).with_detail(e.to_string())
                })?;
                let created = gh(
                    c,
                    &format!("api repos/{repo}/issues --method POST --input \"{}\"", file.display()),
                    cancel,
                );
                let _ = std::fs::remove_file(&file);
                let created = created?;
                let number = created["number"].as_u64().unwrap_or(0);
                Ok(Output::new(
                    text::tf("reply.github.created", &[("number", &number), ("repo", &repo)]),
                    json!({ "number": number, "url": created["html_url"] }),
                ))
            }),
        )
        .build(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::rig;
    use kivo_platform::CommandOutput;

    fn replying(r: &crate::testing::Rig, stdout: &str) {
        r.commands.replies.lock().unwrap().push_back(CommandOutput {
            exit_code: Some(0),
            stdout: stdout.into(),
            ..CommandOutput::default()
        });
    }

    #[test]
    fn reads_go_through_gh_and_are_untrusted() {
        let r = rig();
        replying(
            &r,
            r#"[{"number":7,"title":"Fix the build","url":"https://github.com/o/r/pull/7"}]"#,
        );
        let out = r
            .tool("github.pull_requests")
            .run(&json!({ "repo": "MadBlast0/K.I.V.O" }))
            .unwrap();
        assert_eq!(out.data["pullRequests"][0]["number"], 7);
        assert!(out.source.is_some());
        let ran = r
            .commands
            .ran
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .command
            .clone();
        assert!(
            ran.starts_with("gh pr list --repo MadBlast0/K.I.V.O --state open"),
            "{ran}"
        );

        replying(
            &r,
            r#"{"data":{"viewer":{"login":"MadBlast0","name":null,"pullRequests":{"totalCount":2}}}}"#,
        );
        let who = r.tool("github.whoami").run(&json!({})).unwrap();
        assert_eq!(who.data["login"], "MadBlast0");
        assert!(
            r.commands
                .ran
                .lock()
                .unwrap()
                .last()
                .unwrap()
                .command
                .contains("api graphql")
        );

        replying(
            &r,
            r#"[{"subject":{"title":"Review requested","type":"PullRequest"},"repository":{"full_name":"o/r"},"reason":"review_requested"}]"#,
        );
        let n = r.tool("github.notifications").run(&json!({})).unwrap();
        assert_eq!(n.data["notifications"][0]["repo"], "o/r");
    }

    #[test]
    fn user_text_never_reaches_the_command_line() {
        let r = rig();
        assert!(
            r.tool("github.issues")
                .run(&json!({ "repo": "o/r & del C:\\x" }))
                .is_err()
        );
        assert!(
            r.tool("github.issues")
                .run(&json!({ "repo": "../../etc" }))
                .is_err()
        );
        replying(
            &r,
            r#"{"number":12,"html_url":"https://github.com/o/r/issues/12"}"#,
        );
        let out = r
            .tool("github.create_issue")
            .run(&json!({ "repo": "o/r", "title": "It breaks \" & echo pwned", "body": "%PATH% ^ | >" }))
            .unwrap();
        assert_eq!(out.data["number"], 12);
        let ran = r
            .commands
            .ran
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .command
            .clone();
        assert!(
            ran.starts_with("gh api repos/o/r/issues --method POST --input"),
            "{ran}"
        );
        assert!(!ran.contains("pwned") && !ran.contains("%PATH%"), "{ran}");
        let spec = r.tool("github.create_issue").spec().clone();
        assert!(spec.data_egress && spec.risk == Risk::Medium);
    }

    #[test]
    fn a_missing_gh_or_sign_in_says_what_to_do() {
        let r = rig();
        r.commands.replies.lock().unwrap().push_back(CommandOutput {
            exit_code: Some(1),
            stderr: "To get started with GitHub CLI, please run:  gh auth login".into(),
            ..CommandOutput::default()
        });
        let e = r.tool("github.notifications").run(&json!({})).unwrap_err();
        assert!(e.message.contains("sign"), "{}", e.message);
    }
}
