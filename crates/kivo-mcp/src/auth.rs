//! Signing in to a remote MCP server (INT-04, INTEGRATIONS §0): MCP authorization — OAuth 2.1
//! with PKCE, the server's metadata discovered, KIVO registering itself (dynamic client
//! registration) — in the system browser, with a loopback redirect (RFC 8252). No keys to paste,
//! no embedded webview. The tokens are kept by the caller (Credential Manager) through
//! [`TokenStore`], and refreshed before they expire.

use rmcp::transport::auth::{
    AuthError, AuthorizationManager, AuthorizationRequest, CredentialStore, OAuthState,
    StoredCredentials,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// How long the user has to finish signing in.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

/// Where a connector's tokens are kept: a JSON blob in Credential Manager.
pub trait TokenStore: Send + Sync + 'static {
    fn load(&self) -> Option<String>;
    fn save(&self, json: &str) -> Result<(), String>;
    fn clear(&self);
}

struct Store(Arc<dyn TokenStore>);

#[async_trait::async_trait]
impl CredentialStore for Store {
    async fn load(&self) -> Result<Option<StoredCredentials>, AuthError> {
        Ok(self.0.load().and_then(|j| serde_json::from_str(&j).ok()))
    }

    async fn save(&self, credentials: StoredCredentials) -> Result<(), AuthError> {
        let json = serde_json::to_string(&credentials)
            .map_err(|e| AuthError::CredentialStoreError(e.to_string()))?;
        self.0.save(&json).map_err(AuthError::CredentialStoreError)
    }

    async fn clear(&self) -> Result<(), AuthError> {
        self.0.clear();
        Ok(())
    }
}

/// A manager for `url` whose tokens come from `store`.
pub async fn manager(
    url: &str,
    store: Arc<dyn TokenStore>,
) -> Result<AuthorizationManager, AuthError> {
    let mut manager = AuthorizationManager::new(url).await?;
    manager.set_credential_store(Store(store));
    Ok(manager)
}

/// The query of a loopback request line (`GET /callback?code=…&state=… HTTP/1.1`).
fn query(request: &str) -> Vec<(String, String)> {
    let path = request.split_whitespace().nth(1).unwrap_or_default();
    let Some((_, q)) = path.split_once('?') else {
        return Vec::new();
    };
    url::form_urlencoded::parse(q.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

/// Signs in to the remote MCP server at `url` in the browser: `open` opens the authorization
/// page. The tokens end up in `store`.
pub async fn sign_in(
    url: &str,
    store: Arc<dyn TokenStore>,
    open: impl Fn(&str) -> Result<(), String>,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<(), String> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect = format!("http://127.0.0.1:{port}/callback");
    let manager = manager(url, store).await.map_err(|e| e.to_string())?;
    let mut state = OAuthState::Unauthorized(manager);
    state
        .start_authorization(AuthorizationRequest::new(redirect).with_client_name("KIVO"))
        .await
        .map_err(|e| e.to_string())?;
    let page = state
        .get_authorization_url()
        .await
        .map_err(|e| e.to_string())?;
    open(&page)?;
    let wait = async {
        loop {
            let (mut socket, _) = listener.accept().await.map_err(|e| e.to_string())?;
            let mut buf = vec![0u8; 8192];
            let n = socket.read(&mut buf).await.map_err(|e| e.to_string())?;
            let request = String::from_utf8_lossy(&buf[..n]).into_owned();
            let params = query(&request);
            let get = |k: &str| {
                params
                    .iter()
                    .find(|(key, _)| key == k)
                    .map(|(_, v)| v.clone())
            };
            let (Some(code), Some(csrf)) = (get("code"), get("state")) else {
                // A favicon request or a refusal: an error page, keep waiting only for a callback.
                if let Some(e) = get("error") {
                    let _ = socket
                        .write_all(
                            page_html("KIVO wasn't allowed. You can close this window.").as_bytes(),
                        )
                        .await;
                    return Err(e);
                }
                let _ = socket
                    .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                    .await;
                continue;
            };
            let _ = socket
                .write_all(page_html("KIVO is connected. You can close this window.").as_bytes())
                .await;
            return Ok((code, csrf));
        }
    };
    let (code, csrf) = tokio::select! {
        r = tokio::time::timeout(SIGN_IN_TIMEOUT, wait) => r.map_err(|_| "signing in took too long".to_owned())??,
        () = cancel.cancelled() => return Err("cancelled".into()),
    };
    state
        .handle_callback(&code, &csrf)
        .await
        .map_err(|e| e.to_string())
}

fn page_html(message: &str) -> String {
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>KIVO</title><body style=\"font-family:system-ui;padding:40px\">{message}</body>"
    );
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_callback_is_read_from_the_request_line() {
        let q = query("GET /callback?code=abc%20d&state=xyz HTTP/1.1\r\nHost: 127.0.0.1\r\n");
        assert_eq!(
            q,
            [
                ("code".to_owned(), "abc d".to_owned()),
                ("state".to_owned(), "xyz".to_owned())
            ]
        );
        assert!(query("GET /favicon.ico HTTP/1.1").is_empty());
    }
}
