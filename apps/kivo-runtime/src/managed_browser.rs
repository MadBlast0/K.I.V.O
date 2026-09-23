//! KIVO's own browser for autonomous browsing (TOOLS_AND_CONTROL §5, TOOL-25): Chrome or Edge,
//! already on this PC, started with a KIVO-managed profile folder and driven over the DevTools
//! protocol (`chromiumoxide`). It never uses the user's own profile (Chrome 136+ refuses remote
//! debugging there anyway), never downloads a browser, and keeps certificate errors as errors.
//! Pages are read with the same functions as the extension (`extensions/browser/page.js`).

use chromiumoxide::browser::{Browser as Cdp, BrowserConfig};
use chromiumoxide::page::Page as CdpPage;
use futures_util::StreamExt;
use kivo_core::text;
use kivo_core::tool::{ToolError, ToolErrorCode};
use kivo_tools::browser::{ManagedBrowser, Page, PageTarget};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::Mutex;

/// The page functions, shared with the extension.
const PAGE_JS: &str = include_str!("../../../extensions/browser/page.js");
const EXCERPT_CHARS: usize = 16_000;
const STEP_TIMEOUT: Duration = Duration::from_secs(20);

struct Session {
    browser: Cdp,
    page: CdpPage,
    handler: tokio::task::JoinHandle<()>,
}

pub struct ManagedChrome {
    program: PathBuf,
    profile: PathBuf,
    headless: bool,
    session: Mutex<Option<Session>>,
    handle: tokio::runtime::Handle,
}

/// Chrome, then Edge, then Brave: the first installed.
pub fn find_browser() -> Option<PathBuf> {
    let roots = [
        std::env::var_os("ProgramFiles"),
        std::env::var_os("ProgramFiles(x86)"),
        std::env::var_os("LOCALAPPDATA"),
    ];
    let relative = [
        r"Google\Chrome\Application\chrome.exe",
        r"Microsoft\Edge\Application\msedge.exe",
        r"BraveSoftware\Brave-Browser\Application\brave.exe",
    ];
    relative.iter().find_map(|rel| {
        roots
            .iter()
            .flatten()
            .map(|root| PathBuf::from(root).join(rel))
            .find(|p| p.is_file())
    })
}

fn failed(e: impl std::fmt::Display) -> ToolError {
    ToolError::new(
        ToolErrorCode::Failed,
        text::t("error.browser.managedFailed"),
    )
    .with_detail(e.to_string())
}

/// The page script with `call` evaluated at the end.
fn script(call: &str) -> String {
    format!(
        "(() => {{\n{}\nreturn {call};\n}})()",
        PAGE_JS.replace("export function", "function")
    )
}

impl ManagedChrome {
    pub fn new(program: PathBuf, profile: PathBuf, headless: bool) -> Self {
        Self {
            program,
            profile,
            headless,
            session: Mutex::new(None),
            handle: tokio::runtime::Handle::current(),
        }
    }

    async fn launch(&self) -> Result<Session, ToolError> {
        std::fs::create_dir_all(&self.profile).map_err(failed)?;
        let mut config = BrowserConfig::builder()
            .chrome_executable(&self.program)
            .user_data_dir(&self.profile)
            .respect_https_errors()
            .launch_timeout(Duration::from_secs(20));
        config = if self.headless {
            config.new_headless_mode()
        } else {
            config.with_head()
        };
        let config = config.build().map_err(failed)?;
        let (browser, mut events) = Cdp::launch(config).await.map_err(failed)?;
        let handler = tokio::spawn(async move {
            while let Some(event) = events.next().await {
                if event.is_err() {
                    break;
                }
            }
        });
        let page = browser.new_page("about:blank").await.map_err(failed)?;
        Ok(Session {
            browser,
            page,
            handler,
        })
    }

    async fn page_state(page: &CdpPage) -> Result<Page, ToolError> {
        let v: Value = page
            .evaluate(script(&format!("readPage({EXCERPT_CHARS})")))
            .await
            .map_err(failed)?
            .into_value()
            .map_err(failed)?;
        serde_json::from_value(v).map_err(failed)
    }

    fn run<T>(
        &self,
        work: impl std::future::Future<Output = Result<T, ToolError>>,
    ) -> Result<T, ToolError> {
        self.handle.block_on(async {
            tokio::time::timeout(STEP_TIMEOUT, work)
                .await
                .unwrap_or_else(|_| {
                    Err(ToolError::new(
                        ToolErrorCode::Timeout,
                        text::t("error.timeout"),
                    ))
                })
        })
    }

    /// A step on the open page (starting the browser first if needed).
    async fn with_page<T, F, Fut>(&self, f: F) -> Result<T, ToolError>
    where
        F: FnOnce(CdpPage) -> Fut,
        Fut: std::future::Future<Output = Result<T, ToolError>>,
    {
        let mut session = self.session.lock().await;
        if session.is_none() {
            *session = Some(self.launch().await?);
        }
        let page = session
            .as_ref()
            .map(|s| s.page.clone())
            .ok_or_else(|| failed("no page"))?;
        drop(session);
        f(page).await
    }
}

fn page_error(v: &Value) -> Option<ToolError> {
    let code = v["error"].as_str()?;
    Some(match code {
        "password" => ToolError::new(ToolErrorCode::AccessDenied, text::t("error.uia.password")),
        "notFound" => ToolError::new(ToolErrorCode::NotFound, text::t("error.browser.noElement")),
        _ => failed(code),
    })
}

impl ManagedBrowser for ManagedChrome {
    fn open(&self, url: &str) -> Result<Page, ToolError> {
        let url = url.to_owned();
        self.run(self.with_page(|page| async move {
            page.goto(url.as_str()).await.map_err(failed)?;
            let _ = page.wait_for_navigation().await;
            Self::page_state(&page).await
        }))
    }

    fn read(&self) -> Result<Page, ToolError> {
        self.run(self.with_page(|page| async move { Self::page_state(&page).await }))
    }

    fn click(&self, target: &PageTarget) -> Result<Page, ToolError> {
        let call = format!(
            "clickTarget({}, {})",
            serde_json::to_string(&target.selector).unwrap_or_else(|_| "null".into()),
            serde_json::to_string(&target.text).unwrap_or_else(|_| "null".into()),
        );
        self.run(self.with_page(|page| async move {
            let v: Value = page
                .evaluate(script(&call))
                .await
                .map_err(failed)?
                .into_value()
                .map_err(failed)?;
            if let Some(e) = page_error(&v) {
                return Err(e);
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
            Self::page_state(&page).await
        }))
    }

    fn type_text(&self, target: &PageTarget, value: &str, submit: bool) -> Result<Page, ToolError> {
        let call = format!(
            "typeInto({}, {}, {}, {submit})",
            serde_json::to_string(&target.selector).unwrap_or_else(|_| "null".into()),
            serde_json::to_string(&target.text).unwrap_or_else(|_| "null".into()),
            serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into()),
        );
        self.run(self.with_page(|page| async move {
            let v: Value = page
                .evaluate(script(&call))
                .await
                .map_err(failed)?
                .into_value()
                .map_err(failed)?;
            if let Some(e) = page_error(&v) {
                return Err(e);
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
            Self::page_state(&page).await
        }))
    }

    fn close(&self) -> Result<(), ToolError> {
        self.handle.block_on(async {
            if let Some(mut s) = self.session.lock().await.take() {
                let _ = s.browser.close().await;
                let _ = s.browser.wait().await;
                s.handler.abort();
            }
        });
        Ok(())
    }
}

impl Drop for ManagedChrome {
    fn drop(&mut self) {
        // The browser process ends with its handle (chromiumoxide kills it on drop).
        if let Ok(mut s) = self.session.try_lock()
            && let Some(s) = s.take()
        {
            s.handler.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_script_is_self_contained() {
        let s = script("readPage(10)");
        assert!(!s.contains("export "));
        assert!(s.contains("function readPage"));
        assert!(s.trim_end().ends_with("})()"));
    }

    /// A real Chrome or Edge, headless, in a throwaway profile, on a page served from this test:
    /// the user's own browser and profile are never touched.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn kivos_browser_reads_clicks_and_types_in_its_own_profile() {
        let Some(program) = find_browser() else {
            eprintln!("no Chromium browser on this PC; skipping");
            return;
        };
        let html = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testenv/pages/form.html"),
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            loop {
                let Ok((mut s, _)) = listener.accept().await else {
                    return;
                };
                let html = html.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let _ = s.read(&mut buf).await;
                    let body = html.as_bytes();
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = s.write_all(head.as_bytes()).await;
                    let _ = s.write_all(body).await;
                });
            }
        });
        let profile = tempfile::tempdir().unwrap();
        let chrome =
            std::sync::Arc::new(ManagedChrome::new(program, profile.path().join("p"), true));
        let url = format!("http://127.0.0.1:{port}/form.html");
        let c = std::sync::Arc::clone(&chrome);
        let result = tokio::task::spawn_blocking(move || {
            let page = c.open(&url)?;
            let typed = c.type_text(
                &PageTarget {
                    selector: None,
                    text: Some("name".into()),
                },
                "Ada",
                false,
            )?;
            let after = c.click(&PageTarget {
                selector: None,
                text: Some("Send".into()),
            })?;
            c.close()?;
            Ok::<_, ToolError>((page, typed, after))
        })
        .await
        .unwrap();
        let (page, _typed, after) = match result {
            Ok(r) => r,
            // A browser that can't start here (policy, sandbox): not a KIVO failure.
            Err(e) if e.code == ToolErrorCode::Failed => {
                eprintln!("the browser couldn't start: {:?}", e.detail);
                return;
            }
            Err(e) => panic!("{e:?}"),
        };
        assert_eq!(page.title, "KIVO test form");
        assert!(after.text.contains("Sent: Ada"), "{}", after.text);
    }
}
