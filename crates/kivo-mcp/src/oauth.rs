//! Signing in with KIVO's own OAuth apps (INTEGRATIONS §0, §2; INT-05, INT-07): Google and
//! Microsoft, as public clients with PKCE (S256), in the system browser with a loopback redirect
//! (RFC 8252) — never an embedded webview. Scopes are asked for per feature, when first needed,
//! and added to what the account already granted (incremental). Tokens are kept by the caller in
//! Credential Manager ([`TokenStore`]) and refreshed shortly before they expire. Nothing else of
//! the user's session (cookies, other tokens) is ever read.

use crate::auth::TokenStore;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// How long the user has to finish signing in.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);
/// A token this close to expiring is refreshed first.
const REFRESH_EARLY_MS: i64 = 60_000;

/// An OAuth provider KIVO has its own app with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub auth_url: String,
    pub token_url: String,
    /// KIVO's public client id; empty until the KIVO project has registered its app there.
    pub client_id: String,
    /// Asked for with every sign-in (Microsoft needs `offline_access` for a refresh token).
    pub base_scopes: Vec<String>,
    /// Extra authorization parameters (Google: offline access, incremental grants).
    pub extra: Vec<(String, String)>,
}

/// The tokens for one account, as kept in Credential Manager.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Epoch ms.
    pub expires_at: i64,
    /// What the account has granted so far.
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OAuthError {
    #[error("KIVO's sign-in with {0} isn't set up yet")]
    NotRegistered(String),
    #[error("not signed in")]
    NotSignedIn,
    #[error("the sign-in was refused: {0}")]
    Refused(String),
    #[error("the sign-in didn't finish in time")]
    TimedOut,
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Failed(String),
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// A PKCE verifier and its S256 challenge (RFC 7636).
pub fn pkce() -> (String, String) {
    let verifier = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    (verifier.clone(), challenge(&verifier))
}

pub fn challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// The authorization page for `scopes` (plus what's already granted).
pub fn authorize_url(
    p: &Provider,
    redirect: &str,
    scopes: &[String],
    state: &str,
    challenge: &str,
) -> String {
    let mut url = url::Url::parse(&p.auth_url).expect("a provider's authorization URL");
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("client_id", &p.client_id)
            .append_pair("redirect_uri", redirect)
            .append_pair("response_type", "code")
            .append_pair("scope", &scopes.join(" "))
            .append_pair("state", state)
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256");
        for (k, v) in &p.extra {
            q.append_pair(k, v);
        }
    }
    url.to_string()
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    scope: Option<String>,
}

async fn token_request(
    http: &reqwest::Client,
    p: &Provider,
    form: &[(&str, &str)],
    previous: Option<&Tokens>,
    asked: &[String],
) -> Result<Tokens, OAuthError> {
    let reply = http
        .post(&p.token_url)
        .form(form)
        .send()
        .await
        .map_err(|e| OAuthError::Failed(e.to_string()))?;
    if !reply.status().is_success() {
        let status = reply.status();
        let body = reply.text().await.unwrap_or_default();
        return Err(
            if status.as_u16() == 400 && body.contains("invalid_grant") {
                OAuthError::NotSignedIn
            } else {
                OAuthError::Failed(format!("{status}: {body}"))
            },
        );
    }
    let t: TokenReply = reply
        .json()
        .await
        .map_err(|e| OAuthError::Failed(e.to_string()))?;
    let mut scopes: Vec<String> = t
        .scope
        .map(|s| s.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_else(|| asked.to_vec());
    // Grants only grow: keep what the account had.
    for s in previous.map(|p| p.scopes.clone()).unwrap_or_default() {
        if !scopes.contains(&s) {
            scopes.push(s);
        }
    }
    Ok(Tokens {
        access_token: t.access_token,
        refresh_token: t
            .refresh_token
            .or_else(|| previous.and_then(|p| p.refresh_token.clone())),
        expires_at: now_ms() + t.expires_in.unwrap_or(3600) * 1000,
        scopes,
    })
}

fn load(store: &dyn TokenStore) -> Option<Tokens> {
    store.load().and_then(|j| serde_json::from_str(&j).ok())
}

fn save(store: &dyn TokenStore, tokens: &Tokens) -> Result<(), OAuthError> {
    store
        .save(&serde_json::to_string(tokens).map_err(|e| OAuthError::Failed(e.to_string()))?)
        .map_err(OAuthError::Failed)
}

fn page(message: &str) -> String {
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>KIVO</title><body style=\"font:16px system-ui;padding:40px\">{message}</body>"
    );
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn query(request: &str) -> Vec<(String, String)> {
    let path = request.split_whitespace().nth(1).unwrap_or_default();
    let Some((_, q)) = path.split_once('?') else {
        return Vec::new();
    };
    url::form_urlencoded::parse(q.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

/// Signs in (or adds `scopes` to an existing sign-in) in the browser; `open` opens the page.
pub async fn sign_in(
    http: &reqwest::Client,
    p: &Provider,
    scopes: &[String],
    store: Arc<dyn TokenStore>,
    open: impl Fn(&str) -> Result<(), String>,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Tokens, OAuthError> {
    if p.client_id.is_empty() {
        return Err(OAuthError::NotRegistered(p.name.clone()));
    }
    let previous = load(store.as_ref());
    let mut asked: Vec<String> = p.base_scopes.clone();
    for s in previous.iter().flat_map(|t| t.scopes.iter()).chain(scopes) {
        if !asked.contains(s) {
            asked.push(s.clone());
        }
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| OAuthError::Failed(e.to_string()))?;
    let port = listener
        .local_addr()
        .map_err(|e| OAuthError::Failed(e.to_string()))?
        .port();
    let redirect = format!("http://127.0.0.1:{port}/callback");
    let (verifier, challenge) = pkce();
    let state = uuid::Uuid::new_v4().simple().to_string();
    open(&authorize_url(p, &redirect, &asked, &state, &challenge)).map_err(OAuthError::Failed)?;
    let wait = async {
        loop {
            let (mut socket, _) = listener
                .accept()
                .await
                .map_err(|e| OAuthError::Failed(e.to_string()))?;
            let mut buf = vec![0u8; 8192];
            let n = socket
                .read(&mut buf)
                .await
                .map_err(|e| OAuthError::Failed(e.to_string()))?;
            let params = query(&String::from_utf8_lossy(&buf[..n]));
            let get = |k: &str| {
                params
                    .iter()
                    .find(|(key, _)| key == k)
                    .map(|(_, v)| v.clone())
            };
            if let Some(e) = get("error") {
                let _ = socket
                    .write_all(page("KIVO wasn't allowed. You can close this window.").as_bytes())
                    .await;
                return Err(OAuthError::Refused(e));
            }
            let (Some(code), Some(back)) = (get("code"), get("state")) else {
                let _ = socket
                    .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                    .await;
                continue;
            };
            // A callback for another sign-in (or a forged one) is refused (CSRF).
            if back != state {
                let _ = socket
                    .write_all(
                        page("That sign-in wasn't KIVO's. You can close this window.").as_bytes(),
                    )
                    .await;
                continue;
            }
            let _ = socket
                .write_all(page("KIVO is connected. You can close this window.").as_bytes())
                .await;
            return Ok(code);
        }
    };
    let code = tokio::select! {
        r = tokio::time::timeout(SIGN_IN_TIMEOUT, wait) => r.map_err(|_| OAuthError::TimedOut)??,
        () = cancel.cancelled() => return Err(OAuthError::Cancelled),
    };
    let tokens = token_request(
        http,
        p,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", &redirect),
            ("client_id", &p.client_id),
            ("code_verifier", &verifier),
        ],
        previous.as_ref(),
        &asked,
    )
    .await?;
    save(store.as_ref(), &tokens)?;
    Ok(tokens)
}

/// A current access token, refreshed first when it's about to expire. `NotSignedIn` when there
/// is none or the refresh was refused (the user signed out elsewhere): connect again.
pub async fn access_token(
    http: &reqwest::Client,
    p: &Provider,
    store: &dyn TokenStore,
) -> Result<Tokens, OAuthError> {
    let tokens = load(store).ok_or(OAuthError::NotSignedIn)?;
    if tokens.expires_at - now_ms() > REFRESH_EARLY_MS {
        return Ok(tokens);
    }
    let refresh = tokens
        .refresh_token
        .clone()
        .ok_or(OAuthError::NotSignedIn)?;
    let fresh = token_request(
        http,
        p,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh),
            ("client_id", &p.client_id),
        ],
        Some(&tokens),
        &tokens.scopes,
    )
    .await;
    match fresh {
        Ok(t) => {
            save(store, &t)?;
            Ok(t)
        }
        Err(OAuthError::NotSignedIn) => {
            store.clear();
            Err(OAuthError::NotSignedIn)
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_is_s256_of_the_verifier() {
        let (v, c) = pkce();
        assert!(v.len() >= 43 && v.len() <= 128);
        assert_eq!(c, challenge(&v));
        // RFC 7636 appendix B.
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn the_authorization_page_asks_for_exactly_the_scopes() {
        let p = Provider {
            id: "google".into(),
            name: "Google".into(),
            auth_url: "https://accounts.example/auth".into(),
            token_url: "https://accounts.example/token".into(),
            client_id: "kivo".into(),
            base_scopes: Vec::new(),
            extra: vec![("access_type".into(), "offline".into())],
        };
        let url = authorize_url(
            &p,
            "http://127.0.0.1:5000/callback",
            &["calendar.events".into()],
            "s1",
            "c1",
        );
        let q: Vec<(String, String)> = url::Url::parse(&url)
            .unwrap()
            .query_pairs()
            .into_owned()
            .collect();
        let get = |k: &str| q.iter().find(|(key, _)| key == k).map(|(_, v)| v.as_str());
        assert_eq!(get("scope"), Some("calendar.events"));
        assert_eq!(get("code_challenge_method"), Some("S256"));
        assert_eq!(get("redirect_uri"), Some("http://127.0.0.1:5000/callback"));
        assert_eq!(get("access_type"), Some("offline"));
        assert_eq!(get("response_type"), Some("code"));
    }
}
