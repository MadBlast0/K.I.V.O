//! Connecting a brain without an API key (BRAINS §4, BRAIN-17):
//!
//! - **OpenRouter OAuth PKCE** ("Connect with OpenRouter"): KIVO opens OpenRouter's sign-in in
//!   the browser with a code challenge, listens on a localhost port for the callback, and
//!   exchanges the code (with the verifier) for a key that the user controls on OpenRouter. The
//!   key goes straight into Credential Manager; nothing is copied by hand.
//! - **CLI sign-in**: the CLI's own login flow runs in a terminal window KIVO opens.
//!
//! Keys never travel to the UI (SEC-19).

use crate::brains::Brains;
use base64::Engine as _;
use kivo_brain::NormalizedError;
use kivo_brain::http::Http;
use kivo_platform::SystemControl;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

/// OpenRouter's endpoints (https://openrouter.ai/docs/guides/overview/auth/oauth).
#[derive(Clone, Debug)]
pub struct OpenRouterOAuth {
    pub auth_url: String,
    pub exchange_url: String,
    /// How long the user has to finish signing in.
    pub timeout: Duration,
}

impl Default for OpenRouterOAuth {
    fn default() -> Self {
        Self {
            auth_url: "https://openrouter.ai/auth".into(),
            exchange_url: "https://openrouter.ai/api/v1/auth/keys".into(),
            timeout: Duration::from_secs(300),
        }
    }
}

fn base64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// A PKCE verifier and its S256 challenge (RFC 7636).
pub fn pkce() -> (String, String) {
    let mut random = [0u8; 32];
    getrandom::fill(&mut random).unwrap_or_default();
    let verifier = base64url(&random);
    let challenge = base64url(&Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

/// The page the browser shows after the callback.
const DONE_PAGE: &str = "<!doctype html><meta charset=utf-8><title>KIVO</title>\
<body style=\"font-family:system-ui;padding:3em\"><h2>KIVO is connected to OpenRouter.</h2>\
<p>You can close this tab.</p></body>";

/// Runs the whole flow and stores the key; returns the connection's key handle.
pub async fn connect_openrouter(
    brains: &Brains,
    control: &Arc<dyn SystemControl>,
    oauth: &OpenRouterOAuth,
    cancel: &CancellationToken,
) -> Result<String, String> {
    let (verifier, challenge) = pkce();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let callback = format!("http://localhost:{port}/callback");
    let url = format!(
        "{}?callback_url={}&code_challenge={challenge}&code_challenge_method=S256",
        oauth.auth_url,
        urlencode(&callback)
    );
    control.open_url(&url).map_err(|e| e.to_string())?;
    let code = tokio::select! {
        code = wait_for_code(&listener) => code?,
        () = tokio::time::sleep(oauth.timeout) => return Err("sign-in timed out".into()),
        () = cancel.cancelled() => return Err("cancelled".into()),
    };
    let answer = Http::new()
        .post_json(
            &oauth.exchange_url,
            &[],
            &json!({ "code": code, "code_verifier": verifier, "code_challenge_method": "S256" }),
            cancel,
        )
        .await
        .map_err(|e| match e {
            NormalizedError::Auth => "OpenRouter refused the sign-in".to_owned(),
            other => other.to_string(),
        })?;
    let key = answer["key"]
        .as_str()
        .filter(|k| !k.is_empty())
        .ok_or("OpenRouter sent no key")?;
    brains.set_key("openrouter", key)
}

/// Accepts connections until one carries `?code=`; answers each with a short page.
async fn wait_for_code(listener: &tokio::net::TcpListener) -> Result<String, String> {
    loop {
        let (mut stream, _) = listener.accept().await.map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 8192];
        let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf))
            .await
            .map_err(|_| "no request".to_owned())
            .and_then(|r| r.map_err(|e| e.to_string()))
            .unwrap_or(0);
        let request = String::from_utf8_lossy(&buf[..n]);
        let path = request
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or_default()
            .to_owned();
        let code = path
            .split_once('?')
            .map(|(_, q)| q)
            .into_iter()
            .flat_map(|q| q.split('&'))
            .find_map(|kv| kv.strip_prefix("code="))
            .map(urldecode);
        let (status, body) = if code.is_some() {
            ("200 OK", DONE_PAGE)
        } else {
            ("404 Not Found", "")
        };
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
        if let Some(code) = code.filter(|c| !c.is_empty()) {
            return Ok(code);
        }
    }
}

fn urlencode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn urldecode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                if let Ok(v) = u8::from_str_radix(hex, 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
                out.push(b'%');
            }
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Runs a command in a visible terminal (the platform's; tests pass a recorder).
pub type OpenTerminal<'a> = dyn Fn(&std::path::Path, &[String]) -> Result<(), String> + 'a;

/// Opens the CLI's own sign-in in a terminal window (BRAIN-17).
pub fn cli_login(
    id: &str,
    program: Option<&std::path::Path>,
    open: &OpenTerminal<'_>,
) -> Result<(), String> {
    let tool = kivo_brain::catalog::cli_tool(id).ok_or("not a CLI agent")?;
    let (command, args) = tool.login.split_first().ok_or("no sign-in command")?;
    let program = program.map_or_else(
        || std::path::PathBuf::from(command),
        std::path::Path::to_path_buf,
    );
    let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    open(&program, &args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivo_brain::testing::{MockServer, Reply};
    use kivo_platform::Secrets;
    use kivo_store::Database;
    use std::sync::Mutex;

    #[test]
    fn pkce_challenge_is_the_s256_of_the_verifier() {
        let (verifier, challenge) = pkce();
        assert!(verifier.len() >= 43);
        assert_eq!(challenge, base64url(&Sha256::digest(verifier.as_bytes())));
        assert!(!challenge.contains('=') && !challenge.contains('+'));
        assert_ne!(pkce().0, verifier, "random each time");
        // RFC 7636 appendix B.
        assert_eq!(
            base64url(&Sha256::digest(
                b"dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
            )),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[tokio::test]
    async fn openrouter_oauth_ends_with_a_stored_key_and_no_copying() {
        let exchange = MockServer::start(|r| {
            assert_eq!(r.body["code"], "the-code");
            assert_eq!(r.body["code_challenge_method"], "S256");
            assert!(
                r.body["code_verifier"]
                    .as_str()
                    .is_some_and(|v| v.len() >= 43)
            );
            Reply::json(
                200,
                &serde_json::json!({"key": "sk-or-v1-abc", "user_id": "u"}),
            )
        })
        .await;
        let control = Arc::new(kivo_testkit::FakeSystemControl::default());
        let secrets = Arc::new(kivo_testkit::FakeSecrets::default());
        let brains = Brains::new(
            Arc::new(Mutex::new(Database::in_memory().unwrap())),
            secrets.clone(),
            0,
        );
        let oauth = OpenRouterOAuth {
            auth_url: "https://openrouter.test/auth".into(),
            exchange_url: format!("{}/api/v1/auth/keys", exchange.url),
            timeout: Duration::from_secs(10),
        };
        let control_dyn: Arc<dyn SystemControl> = control.clone();
        let flow = {
            let control = Arc::clone(&control_dyn);
            tokio::spawn(async move {
                connect_openrouter(&brains, &control, &oauth, &CancellationToken::new()).await
            })
        };
        // The "browser": read the URL KIVO opened and follow the callback with a code.
        let opened = loop {
            if let Some(url) = control.opened.lock().unwrap().first().cloned() {
                break url;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        assert!(
            opened
                .starts_with("https://openrouter.test/auth?callback_url=http%3A%2F%2Flocalhost%3A")
        );
        assert!(opened.contains("code_challenge_method=S256"));
        let callback = urldecode(
            opened
                .split("callback_url=")
                .nth(1)
                .unwrap()
                .split('&')
                .next()
                .unwrap(),
        );
        let mut stream = tokio::net::TcpStream::connect(
            callback
                .trim_start_matches("http://")
                .split('/')
                .next()
                .unwrap()
                .replace("localhost", "127.0.0.1"),
        )
        .await
        .unwrap();
        stream
            .write_all(b"GET /callback?code=the-code HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        let mut page = String::new();
        stream.read_to_string(&mut page).await.unwrap();
        assert!(page.contains("connected to OpenRouter"));
        let handle = flow.await.unwrap().expect("connected");
        assert_eq!(handle, "secret://kivo/openrouter/api-key");
        let stored = secrets
            .get(&crate::brains::key_handle("openrouter"))
            .unwrap()
            .unwrap();
        assert_eq!(stored.expose(), "sk-or-v1-abc");
    }

    #[test]
    fn a_cli_sign_in_opens_the_clis_own_login() {
        let opened = Mutex::new(Vec::new());
        cli_login(
            "codex",
            Some(std::path::Path::new(r"C:\npm\codex.cmd")),
            &|p, a| {
                opened.lock().unwrap().push((p.to_path_buf(), a.to_vec()));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            *opened.lock().unwrap(),
            [(
                std::path::PathBuf::from(r"C:\npm\codex.cmd"),
                vec!["login".to_owned()]
            )]
        );
        assert!(cli_login("openai", None, &|_, _| Ok(())).is_err());
    }

    #[test]
    fn url_coding_round_trips() {
        let s = "http://localhost:4321/callback?a=b c";
        assert_eq!(urldecode(&urlencode(s)), s);
    }
}
