//! Logging (ARCHITECTURE §5–6): rolling JSON files in the logs folder, kept for 7 days, with a
//! redaction step in front of the file so keys, tokens and (unless the user turns on "debug
//! transcripts") transcript text never reach disk.

use regex::Regex;
use std::borrow::Cow;
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, LazyLock};
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

const RETENTION_DAYS: usize = 7;

/// Keeps the background log writer alive; logs are flushed when it's dropped.
#[must_use = "logging stops when the guard is dropped"]
pub struct LogGuard(#[allow(dead_code, reason = "held for its Drop")] WorkerGuard);

#[derive(Debug, thiserror::Error)]
pub enum LogError {
    #[error("couldn't create the log folder: {0}")]
    Io(#[from] io::Error),
    #[error("couldn't open the log file: {0}")]
    Appender(#[from] tracing_appender::rolling::InitError),
    #[error("logging is already set up")]
    AlreadySet(#[from] tracing::subscriber::SetGlobalDefaultError),
}

/// Sets up logging for the process. `filter` uses `RUST_LOG` syntax, e.g. "info,kivo_ipc=debug".
pub fn init(dir: &Path, filter: &str, debug_transcripts: bool) -> Result<LogGuard, LogError> {
    let (subscriber, guard) = subscriber(dir, filter, debug_transcripts)?;
    tracing::subscriber::set_global_default(subscriber)?;
    Ok(guard)
}

/// The subscriber `init` installs, for callers (and tests) that scope it themselves.
pub fn subscriber(
    dir: &Path,
    filter: &str,
    debug_transcripts: bool,
) -> Result<(impl tracing::Subscriber + Send + Sync + 'static, LogGuard), LogError> {
    std::fs::create_dir_all(dir)?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("kivo")
        .filename_suffix("log")
        .max_log_files(RETENTION_DAYS)
        .build(dir)?;
    let (file, guard) = tracing_appender::non_blocking(appender);
    let writer = Redacting {
        inner: file,
        redactor: Arc::new(Redactor::new(debug_transcripts)),
    };
    let layer = tracing_subscriber::fmt::layer()
        .json()
        .with_current_span(false)
        .with_writer(writer);
    let subscriber = Registry::default().with(EnvFilter::new(filter)).with(layer);
    Ok((subscriber, LogGuard(guard)))
}

/// Masks secrets in text. Applied to every formatted log line.
pub struct Redactor {
    rules: Vec<(&'static Regex, &'static str)>,
}

static SENSITIVE_FIELD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)("[a-z0-9_]*(?:key|token|secret|password|passwd|authorization|cookie|credential|sensitive)[a-z0-9_]*"\s*:\s*)"(?:[^"\\]|\\.)*""#,
    )
    .expect("valid regex")
});
static TRANSCRIPT_FIELD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)("[a-z0-9_]*transcript[a-z0-9_]*"\s*:\s*)"(?:[^"\\]|\\.)*""#)
        .expect("valid regex")
});
/// Secrets that look the same wherever they appear, including inside messages.
static KNOWN_SECRET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
          \bsk-(?:ant-|or-|proj-)?[A-Za-z0-9_-]{16,}   # OpenAI, Anthropic, OpenRouter keys
        | \bgh[pousr]_[A-Za-z0-9]{20,}                # GitHub tokens
        | \bAIza[0-9A-Za-z_-]{30,}                    # Google API keys
        | \bxox[abprs]-[A-Za-z0-9-]{10,}              # Slack tokens
        | (?i:\bbearer\s+)[A-Za-z0-9._~+/=-]{8,}      # Authorization: Bearer …
        ",
    )
    .expect("valid regex")
});
/// `name=value` / `name: value` pairs in plain text.
static SECRET_ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b((?:api[_-]?key|access[_-]?token|token|secret|password)\s*[=:]\s*)[^\s,;]+")
        .expect("valid regex")
});

impl Redactor {
    pub fn new(debug_transcripts: bool) -> Self {
        let mut rules: Vec<(&'static Regex, &'static str)> = vec![
            (&SENSITIVE_FIELD, r#"$1"***""#),
            (&KNOWN_SECRET, "***"),
            (&SECRET_ASSIGNMENT, "${1}***"),
        ];
        if !debug_transcripts {
            rules.push((&TRANSCRIPT_FIELD, r#"$1"[transcript hidden]""#));
        }
        Self { rules }
    }

    pub fn redact<'a>(&self, line: &'a str) -> Cow<'a, str> {
        let mut out = Cow::Borrowed(line);
        for (re, replacement) in &self.rules {
            if re.is_match(&out) {
                out = Cow::Owned(re.replace_all(&out, *replacement).into_owned());
            }
        }
        out
    }
}

/// A `MakeWriter` that redacts each formatted event before handing it to the file writer.
#[derive(Clone)]
struct Redacting {
    inner: NonBlocking,
    redactor: Arc<Redactor>,
}

impl<'a> MakeWriter<'a> for Redacting {
    type Writer = RedactingLine;
    fn make_writer(&'a self) -> Self::Writer {
        RedactingLine {
            buf: Vec::with_capacity(256),
            inner: self.inner.clone(),
            redactor: Arc::clone(&self.redactor),
        }
    }
}

/// Buffers one event (the formatter writes it in pieces), then redacts and writes it on drop.
struct RedactingLine {
    buf: Vec<u8>,
    inner: NonBlocking,
    redactor: Arc<Redactor>,
}

impl Write for RedactingLine {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for RedactingLine {
    fn drop(&mut self) {
        if self.buf.is_empty() {
            return;
        }
        let text = String::from_utf8_lossy(&self.buf);
        let clean = self.redactor.redact(&text);
        // Logging must never take the app down; a failed write is dropped.
        let _ = self.inner.write_all(clean.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_sensitive_fields_by_name() {
        let r = Redactor::new(false);
        let line = r#"{"fields":{"message":"saved","api_key":"abc","refresh_token":"t\"x","sensitive_path":"C:/secret"}}"#;
        assert_eq!(
            r.redact(line),
            r#"{"fields":{"message":"saved","api_key":"***","refresh_token":"***","sensitive_path":"***"}}"#
        );
    }

    #[test]
    fn masks_known_secret_formats_anywhere() {
        let r = Redactor::new(false);
        let msg = "calling with sk-ant-api03-abcdefghijklmnop and ghp_0123456789abcdefghijABCD, Bearer eyJhbGciOi.x.y";
        let out = r.redact(msg);
        assert!(
            !out.contains("abcdefghijklmnop")
                && !out.contains("0123456789abcdefghij")
                && !out.contains("eyJhbGciOi")
        );
        assert_eq!(r.redact("password=hunter2, next"), "password=***, next");
    }

    #[test]
    fn hides_transcripts_unless_debug_transcripts_is_on() {
        let line = r#"{"fields":{"final_transcript":"call my mom"}}"#;
        assert_eq!(
            Redactor::new(false).redact(line),
            r#"{"fields":{"final_transcript":"[transcript hidden]"}}"#
        );
        assert_eq!(Redactor::new(true).redact(line), line);
    }

    #[test]
    fn leaves_ordinary_lines_alone() {
        let line = r#"{"level":"INFO","fields":{"message":"session transition","to":"Listening"}}"#;
        assert!(matches!(
            Redactor::new(false).redact(line),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn writes_redacted_json_lines_to_the_log_folder() {
        let dir = tempfile::tempdir().unwrap();
        let (subscriber, guard) = subscriber(dir.path(), "info", false).unwrap();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(
                api_key = "sk-live-abcdefghijklmnopqrstu",
                transcript = "open chrome",
                "provider configured"
            );
            tracing::debug!("filtered out below info");
        });
        drop(guard); // flush the background writer
        let files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(files.len(), 1);
        let name = files[0].file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with("kivo.") && name.ends_with(".log"),
            "{name}"
        );
        let text = std::fs::read_to_string(&files[0]).unwrap();
        assert_eq!(text.lines().count(), 1, "one JSON line; debug was filtered");
        assert!(text.contains("provider configured"));
        assert!(
            !text.contains("sk-live") && !text.contains("open chrome"),
            "{text}"
        );
        let _: serde_json::Value =
            serde_json::from_str(text.trim()).expect("still valid JSON after redaction");
    }
}
