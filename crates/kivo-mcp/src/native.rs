//! Native connectors with KIVO's own OAuth apps (INTEGRATIONS §2, INT-05, INT-07): Google
//! Calendar and Drive (`drive.file`: only files KIVO creates), then Microsoft Graph (Outlook
//! calendar). Each tool asks only for the scope it needs, the first time it's used (incremental);
//! restricted scopes (Gmail, full Drive) are not asked for at all (INT-08). What comes back from
//! the service is its content (untrusted); what KIVO sends is the user's own words, confirmed first.

use crate::auth::TokenStore;
use crate::oauth::{self, OAuthError, Provider};
use kivo_core::capability::Capability;
use kivo_core::text;
use kivo_core::tool::{
    CapabilityTier, Platform, Reversibility, Risk, SideEffect, ToolError, ToolErrorCode, ToolSpec,
};
use kivo_tools::{Output, Tool};
use serde_json::{Value, json};
use std::sync::Arc;

pub const GOOGLE_CALENDAR: &str = "https://www.googleapis.com/auth/calendar.events";
pub const GOOGLE_DRIVE_FILE: &str = "https://www.googleapis.com/auth/drive.file";
pub const MS_CALENDARS: &str = "Calendars.ReadWrite";

/// KIVO's registered client ids, built in at compile time by whoever builds KIVO (the project's
/// release build); empty in a build without them, and Connect then says sign-in isn't set up.
pub fn google_client_id() -> &'static str {
    option_env!("KIVO_GOOGLE_CLIENT_ID").unwrap_or("")
}

pub fn microsoft_client_id() -> &'static str {
    option_env!("KIVO_MICROSOFT_CLIENT_ID").unwrap_or("")
}

/// Where each provider's API is.
pub fn api_of(provider: &str) -> &'static str {
    match provider {
        "google" => "https://www.googleapis.com",
        _ => "https://graph.microsoft.com/v1.0",
    }
}

/// A provider by id, with KIVO's client id.
pub fn provider(id: &str) -> Option<Provider> {
    match id {
        "google" => Some(google(google_client_id())),
        "microsoft" => Some(microsoft(microsoft_client_id())),
        _ => None,
    }
}

/// The tools a native connector brings (`prefix` from the directory).
pub fn tools_for(prefix: &str, account: Arc<Account>) -> Vec<Arc<dyn Tool>> {
    let all = match account.provider.id.as_str() {
        "google" => google_tools(account),
        _ => microsoft_tools(account),
    };
    all.into_iter()
        .filter(|t| t.spec().id.starts_with(prefix))
        .collect()
}

/// KIVO's Google app. The client id is the KIVO project's (built in when it registers the app);
/// until then Connect says sign-in isn't set up yet.
pub fn google(client_id: &str) -> Provider {
    Provider {
        id: "google".into(),
        name: "Google".into(),
        auth_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
        token_url: "https://oauth2.googleapis.com/token".into(),
        client_id: client_id.into(),
        base_scopes: Vec::new(),
        extra: vec![
            ("access_type".into(), "offline".into()),
            ("include_granted_scopes".into(), "true".into()),
            ("prompt".into(), "consent".into()),
        ],
    }
}

/// KIVO's Microsoft app (personal and work accounts).
pub fn microsoft(client_id: &str) -> Provider {
    Provider {
        id: "microsoft".into(),
        name: "Microsoft".into(),
        auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".into(),
        token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token".into(),
        client_id: client_id.into(),
        base_scopes: vec!["offline_access".into()],
        extra: Vec::new(),
    }
}

/// Opens a page in the system browser.
pub type Open = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// One provider's connection: where its API is, its tokens, the browser for sign-in.
pub struct Account {
    pub provider: Provider,
    /// `https://www.googleapis.com`, `https://graph.microsoft.com/v1.0` (a mock in tests).
    pub api: String,
    pub store: Arc<dyn TokenStore>,
    pub open: Open,
    pub http: reqwest::Client,
    pub handle: tokio::runtime::Handle,
}

fn failed(e: impl std::fmt::Display) -> ToolError {
    ToolError::new(ToolErrorCode::Failed, text::t("error.failed")).with_detail(e.to_string())
}

impl Account {
    /// A token that carries `scope`: asked for in the browser the first time a feature needs it.
    async fn token_for(&self, scope: &str) -> Result<String, ToolError> {
        let current = oauth::access_token(&self.http, &self.provider, self.store.as_ref()).await;
        match current {
            Ok(t) if t.scopes.iter().any(|s| s == scope) => return Ok(t.access_token),
            Ok(_) | Err(OAuthError::NotSignedIn) => {}
            Err(e) => return Err(failed(e)),
        }
        let open = Arc::clone(&self.open);
        let tokens = oauth::sign_in(
            &self.http,
            &self.provider,
            &[scope.to_owned()],
            Arc::clone(&self.store),
            move |url| open(url),
            &tokio_util::sync::CancellationToken::new(),
        )
        .await
        .map_err(|e| match e {
            OAuthError::NotRegistered(name) => ToolError::new(
                ToolErrorCode::Unsupported,
                text::tf("reply.connect.notSetUp", &[("name", &name)]),
            ),
            OAuthError::Refused(_) => ToolError::new(
                ToolErrorCode::AccessDenied,
                text::t("reply.connect.refused"),
            ),
            other => failed(other),
        })?;
        Ok(tokens.access_token)
    }

    async fn call(
        &self,
        scope: &str,
        method: reqwest::Method,
        path: &str,
        body: Option<(String, String)>,
    ) -> Result<Value, ToolError> {
        let token = self.token_for(scope).await?;
        let mut request = self
            .http
            .request(method, format!("{}{path}", self.api))
            .bearer_auth(token);
        if let Some((content_type, body)) = body {
            request = request.header("Content-Type", content_type).body(body);
        }
        let reply = request.send().await.map_err(failed)?;
        let status = reply.status();
        let value: Value = reply.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(failed(format!("{status}: {value}")));
        }
        Ok(value)
    }
}

fn spec(id: &str, title: &str, description: &str, params: Value, write: bool) -> ToolSpec {
    ToolSpec {
        id: id.into(),
        title: title.into(),
        description: description.into(),
        params,
        result: json!({ "type": "object" }),
        risk: if write { Risk::Medium } else { Risk::Safe },
        side_effects: vec![if write {
            SideEffect::ExternalComms
        } else {
            SideEffect::LocalRead
        }],
        data_egress: write,
        timeout_ms: 320_000,
        cancellable: false,
        tier: CapabilityTier::OsApi,
        platforms: vec![Platform::Windows, Platform::MacOs, Platform::Linux],
        reversibility: if write {
            Reversibility::Irreversible
        } else {
            Reversibility::NotApplicable
        },
        capability: Capability::Integrations,
    }
}

type Run = dyn Fn(&Account, &Value) -> Result<Output, ToolError> + Send + Sync;

struct NativeTool {
    spec: ToolSpec,
    account: Arc<Account>,
    run: Box<Run>,
}

impl Tool for NativeTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn run(&self, args: &Value) -> Result<Output, ToolError> {
        (self.run)(&self.account, args)
    }
}

fn window(days: i64) -> (String, String) {
    let now = jiff::Timestamp::now();
    let end = now
        .checked_add(jiff::SignedDuration::from_hours(24 * days.clamp(1, 60)))
        .unwrap_or(now);
    (now.to_string(), end.to_string())
}

fn when(args: &Value, key: &str) -> Result<String, ToolError> {
    args[key]
        .as_str()
        .and_then(|s| s.parse::<jiff::Timestamp>().ok())
        .map(|t| t.to_string())
        .ok_or_else(|| ToolError::new(ToolErrorCode::InvalidArgs, text::t("reply.connect.badTime")))
}

fn source(account: &Account) -> String {
    format!("{} ({})", account.provider.name, account.api)
}

/// Google Calendar and Drive (`drive.file`).
pub fn google_tools(account: Arc<Account>) -> Vec<Arc<dyn Tool>> {
    let days = json!({ "days": { "type": "integer", "minimum": 1, "maximum": 60 } });
    let event = json!({
        "title": { "type": "string" },
        "start": { "type": "string", "description": "RFC 3339, e.g. 2026-09-25T15:00:00+02:00" },
        "end": { "type": "string" },
        "description": { "type": "string" }
    });
    vec![
        Arc::new(NativeTool {
            spec: spec(
                "google.calendar.upcoming",
                &text::t("tool.google.upcoming"),
                "Upcoming events in the user's Google Calendar.",
                json!({ "type": "object", "properties": days }),
                false,
            ),
            account: Arc::clone(&account),
            run: Box::new(|a, args| {
                let (from, to) = window(args["days"].as_i64().unwrap_or(7));
                let path = format!(
                    "/calendar/v3/calendars/primary/events?singleEvents=true&orderBy=startTime&maxResults=20&timeMin={}&timeMax={}",
                    urlencode(&from),
                    urlencode(&to)
                );
                let v = a.handle.block_on(a.call(
                    GOOGLE_CALENDAR,
                    reqwest::Method::GET,
                    &path,
                    None,
                ))?;
                let events: Vec<Value> = v["items"].as_array().into_iter().flatten().map(|e| json!({
                    "title": e["summary"], "start": e["start"]["dateTime"].as_str().or(e["start"]["date"].as_str()),
                    "end": e["end"]["dateTime"].as_str().or(e["end"]["date"].as_str()), "where": e["location"],
                })).collect();
                let n = events.len() as u64;
                Ok(Output::new(
                    text::plural("reply.connect.events", n, &[]),
                    json!({ "events": events }),
                )
                .untrusted(source(a)))
            }),
        }),
        Arc::new(NativeTool {
            spec: spec(
                "google.calendar.create",
                &text::t("tool.google.create"),
                "Add an event to the user's Google Calendar. The user confirms first.",
                json!({ "type": "object", "properties": event.clone(), "required": ["title", "start", "end"] }),
                true,
            ),
            account: Arc::clone(&account),
            run: Box::new(|a, args| {
                let body = json!({
                    "summary": args["title"].as_str().unwrap_or_default(),
                    "description": args["description"].as_str().unwrap_or_default(),
                    "start": { "dateTime": when(args, "start")? },
                    "end": { "dateTime": when(args, "end")? },
                });
                let v = a.handle.block_on(a.call(
                    GOOGLE_CALENDAR,
                    reqwest::Method::POST,
                    "/calendar/v3/calendars/primary/events",
                    Some(("application/json".into(), body.to_string())),
                ))?;
                Ok(Output::new(
                    text::tf(
                        "reply.connect.added",
                        &[("title", &v["summary"].as_str().unwrap_or_default())],
                    ),
                    json!({ "id": v["id"], "link": v["htmlLink"] }),
                ))
            }),
        }),
        Arc::new(NativeTool {
            spec: spec(
                "google.drive.save",
                &text::t("tool.google.drive"),
                "Save text as a new file in the user's Google Drive (KIVO can only see files it creates there). The user confirms first.",
                json!({ "type": "object", "properties": { "name": { "type": "string" }, "text": { "type": "string" } }, "required": ["name", "text"] }),
                true,
            ),
            account: Arc::clone(&account),
            run: Box::new(|a, args| {
                let name = args["name"].as_str().unwrap_or_default().trim();
                if name.is_empty() {
                    return Err(ToolError::new(
                        ToolErrorCode::InvalidArgs,
                        text::t("reply.connect.noName"),
                    ));
                }
                let boundary = format!("kivo-{}", uuid::Uuid::new_v4().simple());
                let body = format!(
                    "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{}\r\n--{boundary}\r\nContent-Type: text/plain; charset=UTF-8\r\n\r\n{}\r\n--{boundary}--",
                    json!({ "name": name, "mimeType": "text/plain" }),
                    args["text"].as_str().unwrap_or_default()
                );
                let v = a.handle.block_on(a.call(
                    GOOGLE_DRIVE_FILE,
                    reqwest::Method::POST,
                    "/upload/drive/v3/files?uploadType=multipart",
                    Some((format!("multipart/related; boundary={boundary}"), body)),
                ))?;
                Ok(Output::new(
                    text::tf("reply.connect.saved", &[("name", &name)]),
                    json!({ "id": v["id"] }),
                ))
            }),
        }),
    ]
}

/// Microsoft Graph: the Outlook calendar.
pub fn microsoft_tools(account: Arc<Account>) -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(NativeTool {
            spec: spec(
                "microsoft.calendar.upcoming",
                &text::t("tool.microsoft.upcoming"),
                "Upcoming events in the user's Outlook calendar (Microsoft 365 or Outlook.com).",
                json!({ "type": "object", "properties": { "days": { "type": "integer", "minimum": 1, "maximum": 60 } } }),
                false,
            ),
            account: Arc::clone(&account),
            run: Box::new(|a, args| {
                let (from, to) = window(args["days"].as_i64().unwrap_or(7));
                let path = format!(
                    "/me/calendarView?startDateTime={}&endDateTime={}&$top=20&$orderby=start/dateTime",
                    urlencode(&from),
                    urlencode(&to)
                );
                let v =
                    a.handle
                        .block_on(a.call(MS_CALENDARS, reqwest::Method::GET, &path, None))?;
                let events: Vec<Value> = v["value"].as_array().into_iter().flatten().map(|e| json!({
                    "title": e["subject"], "start": e["start"]["dateTime"], "end": e["end"]["dateTime"],
                    "where": e["location"]["displayName"],
                })).collect();
                let n = events.len() as u64;
                Ok(Output::new(
                    text::plural("reply.connect.events", n, &[]),
                    json!({ "events": events }),
                )
                .untrusted(source(a)))
            }),
        }),
        Arc::new(NativeTool {
            spec: spec(
                "microsoft.calendar.create",
                &text::t("tool.microsoft.create"),
                "Add an event to the user's Outlook calendar. The user confirms first.",
                json!({ "type": "object", "properties": { "title": { "type": "string" }, "start": { "type": "string" }, "end": { "type": "string" } }, "required": ["title", "start", "end"] }),
                true,
            ),
            account,
            run: Box::new(|a, args| {
                let body = json!({
                    "subject": args["title"].as_str().unwrap_or_default(),
                    "start": { "dateTime": when(args, "start")?, "timeZone": "UTC" },
                    "end": { "dateTime": when(args, "end")?, "timeZone": "UTC" },
                });
                let v = a.handle.block_on(a.call(
                    MS_CALENDARS,
                    reqwest::Method::POST,
                    "/me/events",
                    Some(("application/json".into(), body.to_string())),
                ))?;
                Ok(Output::new(
                    text::tf(
                        "reply.connect.added",
                        &[("title", &v["subject"].as_str().unwrap_or_default())],
                    ),
                    json!({ "id": v["id"], "link": v["webLink"] }),
                ))
            }),
        }),
    ]
}

fn urlencode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Tokens in memory, as Credential Manager would keep them.
    #[derive(Default)]
    struct Memory(Mutex<Option<String>>);
    impl TokenStore for Memory {
        fn load(&self) -> Option<String> {
            self.0.lock().unwrap().clone()
        }
        fn save(&self, json: &str) -> Result<(), String> {
            *self.0.lock().unwrap() = Some(json.to_owned());
            Ok(())
        }
        fn clear(&self) {
            *self.0.lock().unwrap() = None;
        }
    }

    /// A mock identity provider and API: checks PKCE, hands out tokens with the scopes asked,
    /// and answers the calendar and drive calls when the token carries their scope.
    async fn server() -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let log = Arc::new(Mutex::new(Vec::new()));
        let challenges: Arc<Mutex<HashMap<String, (String, String)>>> = Arc::default();
        let seen = Arc::clone(&log);
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else {
                    return;
                };
                let (seen, challenges) = (Arc::clone(&seen), Arc::clone(&challenges));
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 65536];
                    let n = s.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let line = req.lines().next().unwrap_or_default().to_owned();
                    seen.lock().unwrap().push(line.clone());
                    let body = req.split("\r\n\r\n").nth(1).unwrap_or_default().to_owned();
                    let form: HashMap<String, String> =
                        url::form_urlencoded::parse(body.as_bytes())
                            .into_owned()
                            .collect();
                    let auth = req
                        .lines()
                        .find(|l| l.to_lowercase().starts_with("authorization:"))
                        .unwrap_or_default()
                        .to_owned();
                    let (status, reply) = if line.starts_with("GET /auth") {
                        // The "browser": approve and go back to the loopback with a code.
                        let q: HashMap<String, String> = url::Url::parse(&format!(
                            "http://x{}",
                            line.split_whitespace().nth(1).unwrap()
                        ))
                        .unwrap()
                        .query_pairs()
                        .into_owned()
                        .collect();
                        let code = format!("code-{}", challenges.lock().unwrap().len() + 1);
                        challenges.lock().unwrap().insert(
                            code.clone(),
                            (q["code_challenge"].clone(), q["scope"].clone()),
                        );
                        let back =
                            format!("{}?code={}&state={}", q["redirect_uri"], code, q["state"]);
                        let _ = reqwest::get(back).await;
                        ("200 OK", "{}".to_owned())
                    } else if line.starts_with("POST /token") {
                        if form["grant_type"] == "authorization_code" {
                            let (challenge, scope) =
                                challenges.lock().unwrap()[&form["code"]].clone();
                            if crate::oauth::challenge(&form["code_verifier"]) == challenge {
                                ("200 OK", json!({ "access_token": format!("at:{scope}"), "refresh_token": "rt", "expires_in": 3600, "scope": scope }).to_string())
                            } else {
                                (
                                    "400 Bad Request",
                                    "{\"error\":\"invalid_grant\"}".to_owned(),
                                )
                            }
                        } else {
                            (
                                "400 Bad Request",
                                "{\"error\":\"invalid_grant\"}".to_owned(),
                            )
                        }
                    } else if line.contains("/calendar/v3/calendars/primary/events")
                        && auth.contains(GOOGLE_CALENDAR)
                    {
                        if line.starts_with("POST") {
                            ("200 OK", json!({ "id": "e1", "summary": serde_json::from_str::<Value>(&body).unwrap()["summary"], "htmlLink": "https://calendar/e1" }).to_string())
                        } else {
                            ("200 OK", json!({ "items": [{ "summary": "Standup", "start": { "dateTime": "2026-09-25T09:00:00Z" }, "end": { "dateTime": "2026-09-25T09:15:00Z" } }] }).to_string())
                        }
                    } else if line.contains("/upload/drive/v3/files")
                        && auth.contains(GOOGLE_DRIVE_FILE)
                    {
                        ("200 OK", json!({ "id": "f1" }).to_string())
                    } else {
                        ("401 Unauthorized", "{}".to_owned())
                    };
                    let _ = s.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len()).as_bytes()).await;
                });
            }
        });
        (base, log)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn calendar_and_drive_ask_for_their_own_scope_when_first_used() {
        let (base, log) = server().await;
        let mut provider = google("kivo-test-client");
        provider.auth_url = format!("{base}/auth");
        provider.token_url = format!("{base}/token");
        let store: Arc<dyn TokenStore> = Arc::new(Memory::default());
        let opened = Arc::new(Mutex::new(Vec::<String>::new()));
        let account = Arc::new(Account {
            provider,
            api: base.clone(),
            store: Arc::clone(&store),
            open: {
                let opened = Arc::clone(&opened);
                Arc::new(move |url: &str| {
                    opened.lock().unwrap().push(url.to_owned());
                    let url = url.to_owned();
                    // The system browser: it visits the page, which redirects back.
                    tokio::spawn(async move {
                        let _ = reqwest::get(url).await;
                    });
                    Ok(())
                })
            },
            http: reqwest::Client::new(),
            handle: tokio::runtime::Handle::current(),
        });
        let tools = google_tools(Arc::clone(&account));
        let run = |id: &str, args: Value| {
            let tool = tools.iter().find(|t| t.spec().id == id).cloned().unwrap();
            tokio::task::spawn_blocking(move || tool.run(&args))
        };
        let upcoming = run("google.calendar.upcoming", json!({ "days": 3 }))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(upcoming.data["events"][0]["title"], "Standup");
        assert!(
            upcoming.source.is_some(),
            "the calendar's content is untrusted"
        );
        assert_eq!(opened.lock().unwrap().len(), 1);
        assert!(opened.lock().unwrap()[0].contains("calendar.events"));
        assert!(
            !opened.lock().unwrap()[0].contains("drive"),
            "only the scope it needs"
        );
        // Signed in: the next calendar call doesn't open the browser.
        run("google.calendar.create", json!({ "title": "Dentist", "start": "2026-09-30T10:00:00Z", "end": "2026-09-30T11:00:00Z" })).await.unwrap().unwrap();
        assert_eq!(opened.lock().unwrap().len(), 1);
        // Drive asks for its scope on top (incremental), keeping the calendar's.
        let saved = run(
            "google.drive.save",
            json!({ "name": "notes.txt", "text": "hello" }),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(saved.data["id"], "f1");
        assert_eq!(opened.lock().unwrap().len(), 2);
        let second = opened.lock().unwrap()[1].clone();
        assert!(
            second.contains("drive.file") && second.contains("calendar.events"),
            "{second}"
        );
        let kept: Value = serde_json::from_str(&store.load().unwrap()).unwrap();
        assert_eq!(kept["refresh_token"], "rt");
        assert!(
            log.lock()
                .unwrap()
                .iter()
                .any(|l| l.starts_with("POST /token"))
        );
        // A bad time is refused before anything is sent.
        assert!(
            run(
                "google.calendar.create",
                json!({ "title": "x", "start": "tomorrow", "end": "later" })
            )
            .await
            .unwrap()
            .is_err()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn without_kivos_app_registered_sign_in_says_so() {
        let account = Arc::new(Account {
            provider: microsoft(""),
            api: "http://127.0.0.1:9".into(),
            store: Arc::new(Memory::default()),
            open: Arc::new(|_: &str| Ok(())),
            http: reqwest::Client::new(),
            handle: tokio::runtime::Handle::current(),
        });
        let tool = microsoft_tools(account).remove(0);
        let e = tokio::task::spawn_blocking(move || tool.run(&json!({})))
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(e.code, ToolErrorCode::Unsupported);
    }
}
