//! `kivo-test-mcp --http [--no-auth]`: the fixture over Streamable HTTP on 127.0.0.1, behind a
//! small OAuth 2.1 authorization server of its own (protected-resource and authorization-server
//! metadata, dynamic client registration, an authorize endpoint that approves at once, a token
//! endpoint with PKCE), so KIVO's connector sign-in runs for real against localhost. It prints
//! `listening <port>` on stdout. `--no-auth` serves MCP with no sign-in.

use bytes::Bytes;
use http_body_util::{BodyExt, Full, combinators::BoxBody};
use hyper::body::Incoming;
use hyper::{Request, Response, StatusCode};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use serde_json::json;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use tower_service::Service;

type Body = BoxBody<Bytes, Infallible>;

/// The access token this server hands out, and the PKCE challenge it expects for each code.
#[derive(Default)]
struct Auth {
    codes: HashMap<String, String>,
}

const TOKEN: &str = "kivo-test-access-token";

fn full(status: StatusCode, content_type: &str, body: String) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("content-type", content_type)
        .body(Full::new(Bytes::from(body)).boxed())
        .unwrap_or_default()
}

fn json_response(status: StatusCode, v: serde_json::Value) -> Response<Body> {
    full(status, "application/json", v.to_string())
}

fn form(body: &[u8]) -> HashMap<String, String> {
    url::form_urlencoded::parse(body).into_owned().collect()
}

fn s256(verifier: &str) -> String {
    use base64::Engine as _;
    use sha2::Digest as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(sha2::Sha256::digest(verifier.as_bytes()))
}

async fn route(
    req: Request<Incoming>,
    base: String,
    auth: Arc<Mutex<Auth>>,
    mcp: StreamableHttpService<crate::Fixture, LocalSessionManager>,
    open: bool,
) -> Result<Response<Body>, Infallible> {
    let path = req.uri().path().to_owned();
    let query: HashMap<String, String> = req
        .uri()
        .query()
        .map(|q| {
            url::form_urlencoded::parse(q.as_bytes())
                .into_owned()
                .collect()
        })
        .unwrap_or_default();
    Ok(match path.as_str() {
        "/mcp" => {
            let bearer = req
                .headers()
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            if !open && bearer.as_deref() != Some(&format!("Bearer {TOKEN}")) {
                Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .header(
                        "www-authenticate",
                        format!("Bearer resource_metadata=\"{base}/.well-known/oauth-protected-resource\""),
                    )
                    .body(Full::new(Bytes::new()).boxed())
                    .unwrap_or_default()
            } else {
                let mut mcp = mcp;
                mcp.call(req).await.unwrap_or_default()
            }
        }
        p if p.starts_with("/.well-known/oauth-protected-resource") => json_response(
            StatusCode::OK,
            json!({ "resource": format!("{base}/mcp"), "authorization_servers": [base] }),
        ),
        p if p.starts_with("/.well-known/oauth-authorization-server")
            || p.starts_with("/.well-known/openid-configuration") =>
        {
            json_response(
                StatusCode::OK,
                json!({
                    "issuer": base,
                    "authorization_endpoint": format!("{base}/authorize"),
                    "token_endpoint": format!("{base}/token"),
                    "registration_endpoint": format!("{base}/register"),
                    "response_types_supported": ["code"],
                    "grant_types_supported": ["authorization_code", "refresh_token"],
                    "code_challenge_methods_supported": ["S256"],
                    "token_endpoint_auth_methods_supported": ["none"],
                }),
            )
        }
        "/register" => {
            let body = req
                .into_body()
                .collect()
                .await
                .map(|b| b.to_bytes())
                .unwrap_or_default();
            let meta: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
            json_response(
                StatusCode::CREATED,
                json!({
                    "client_id": "kivo-test-client",
                    "redirect_uris": meta["redirect_uris"],
                    "client_name": meta["client_name"],
                    "token_endpoint_auth_method": "none",
                    "grant_types": ["authorization_code", "refresh_token"],
                    "response_types": ["code"],
                }),
            )
        }
        "/authorize" => {
            // Approves at once (the "user" signs in), redirecting to the client's loopback.
            let redirect = query.get("redirect_uri").cloned().unwrap_or_default();
            let state = query.get("state").cloned().unwrap_or_default();
            let challenge = query.get("code_challenge").cloned().unwrap_or_default();
            let code = format!("code-{}", challenge.len());
            if let Ok(mut a) = auth.lock() {
                a.codes.insert(code.clone(), challenge);
            }
            let sep = if redirect.contains('?') { '&' } else { '?' };
            Response::builder()
                .status(StatusCode::FOUND)
                .header(
                    "location",
                    format!("{redirect}{sep}code={code}&state={state}"),
                )
                .body(Full::new(Bytes::new()).boxed())
                .unwrap_or_default()
        }
        "/token" => {
            let body = req
                .into_body()
                .collect()
                .await
                .map(|b| b.to_bytes())
                .unwrap_or_default();
            let f = form(&body);
            let grant = f.get("grant_type").map(String::as_str);
            let ok = match grant {
                Some("authorization_code") => {
                    let code = f.get("code").cloned().unwrap_or_default();
                    let verifier = f.get("code_verifier").cloned().unwrap_or_default();
                    let expected = auth.lock().ok().and_then(|mut a| a.codes.remove(&code));
                    expected.is_some_and(|c| c == s256(&verifier))
                }
                Some("refresh_token") => {
                    f.get("refresh_token").map(String::as_str) == Some("kivo-test-refresh")
                }
                _ => false,
            };
            if ok {
                json_response(
                    StatusCode::OK,
                    json!({
                        "access_token": TOKEN,
                        "token_type": "Bearer",
                        "expires_in": 3600,
                        "refresh_token": "kivo-test-refresh",
                    }),
                )
            } else {
                json_response(StatusCode::BAD_REQUEST, json!({ "error": "invalid_grant" }))
            }
        }
        _ => full(StatusCode::NOT_FOUND, "text/plain", String::new()),
    })
}

/// Serves until the process is stopped.
pub async fn serve(hostile: bool, open: bool) {
    let Ok(listener) = tokio::net::TcpListener::bind("127.0.0.1:0").await else {
        return;
    };
    let Ok(addr) = listener.local_addr() else {
        return;
    };
    let base = format!("http://127.0.0.1:{}", addr.port());
    println!("listening {}", addr.port());
    let auth = Arc::new(Mutex::new(Auth::default()));
    let mcp = StreamableHttpService::new(
        move || {
            Ok(crate::Fixture {
                hostile,
                pulled: Arc::default(),
            })
        },
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        let base = base.clone();
        let auth = Arc::clone(&auth);
        let mcp = mcp.clone();
        tokio::spawn(async move {
            let service = hyper::service::service_fn(move |req| {
                route(req, base.clone(), Arc::clone(&auth), mcp.clone(), open)
            });
            let _ = hyper::server::conn::http1::Builder::new()
                .serve_connection(hyper_util::rt::TokioIo::new(stream), service)
                .await;
        });
    }
}
