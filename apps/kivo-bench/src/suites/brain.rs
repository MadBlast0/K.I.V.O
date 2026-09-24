//! `brain`: one brain's speed and reliability on fixed prompts (BENCHMARKS §1, BENCH-09): time to
//! first token, tokens per second, tool-call latency, structured-output validity, cancellation
//! latency and error rate. The brain under test is named by `KIVO_BENCH_BRAIN`:
//!
//! - `agent:<program> [args…]` — a CLI agent over ACP (`agent:gemini --acp`), signed in by
//!   itself. Its permission requests are all refused, and it works in an empty folder.
//! - `openai:<base url>|<model>` — any OpenAI-compatible server (a local one, or a cloud service
//!   with its key in the environment variable `KIVO_BENCH_BRAIN_KEY`; the key never enters a
//!   report).

use crate::harness::{Sample, Suite};
use kivo_brain::acp::{
    AcpClient, AcpSession, AgentCommand, AgentEvent, PermissionAnswer, PermissionAsk,
    PermissionHandler,
};
use kivo_brain::http::Http;
use kivo_brain::openai::{OpenAi, OpenAiConfig};
use kivo_brain::{BrainEvent, BrainProvider, ChatRequest, Message, ToolDef};
use kivo_core::Secret;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

const ANSWER: &str = "In two sentences: why does a laptop fan get louder under load?";
const STRUCTURED: &str = "Reply with only a JSON object, no prose and no code fence, with the keys \
    \"city\" (a string) and \"population\" (a number) for the capital of France.";
const TOOL: &str = "What's the weather in Lisbon right now? Use the tool.";
const LONG: &str = "Write a 400-word story about a lighthouse keeper.";
/// Nothing a brain says should take longer than this.
const TIMEOUT: Duration = Duration::from_secs(120);

enum Target {
    Agent {
        client: Arc<AcpClient>,
        session: AcpSession,
        _dir: tempfile::TempDir,
    },
    Api {
        provider: OpenAi,
        model: String,
    },
}

struct RefuseAll;

#[async_trait::async_trait]
impl PermissionHandler for RefuseAll {
    async fn ask(&self, ask: PermissionAsk) -> PermissionAnswer {
        kivo_brain::acp::pick(&ask, false, false)
    }
}

pub struct Brain {
    rt: tokio::runtime::Runtime,
    target: Target,
    label: String,
}

/// What one prompt did.
#[derive(Default)]
struct Outcome {
    first_token_ms: Option<f64>,
    tool_call_ms: Option<f64>,
    total_ms: f64,
    text: String,
    output_tokens: u64,
    error: bool,
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1e3
}

/// A JSON object with the asked-for keys, tolerating a code fence around it.
fn valid_structured(text: &str) -> bool {
    let body = text.trim();
    let body = body
        .strip_prefix("```json")
        .or_else(|| body.strip_prefix("```"))
        .and_then(|b| b.strip_suffix("```"))
        .unwrap_or(body)
        .trim();
    serde_json::from_str::<Value>(body)
        .is_ok_and(|v| v["city"].is_string() && v["population"].is_number())
}

impl Brain {
    pub fn start() -> Result<Self, String> {
        let spec = std::env::var("KIVO_BENCH_BRAIN").map_err(|_| {
            "set KIVO_BENCH_BRAIN to `agent:<program> [args]` or `openai:<base url>|<model>` \
             (see apps/kivo-bench/src/suites/brain.rs)"
                .to_owned()
        })?;
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let (target, label) = if let Some(command) = spec.strip_prefix("agent:") {
            let mut words = command.split_whitespace();
            let program = words.next().ok_or("agent: needs a program")?;
            let command = AgentCommand {
                program: PathBuf::from(program),
                args: words.map(str::to_owned).collect(),
                env: Vec::new(),
            };
            let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
            let (client, session) = rt
                .block_on(async {
                    let client =
                        AcpClient::spawn(&command, dir.path(), Arc::new(RefuseAll)).await?;
                    let session = client.session(dir.path(), &[], None).await?;
                    Ok::<_, kivo_brain::NormalizedError>((client, session))
                })
                .map_err(|e| format!("couldn't start {program}: {e}"))?;
            (
                Target::Agent {
                    client,
                    session,
                    _dir: dir,
                },
                format!("{program} over ACP"),
            )
        } else if let Some(rest) = spec.strip_prefix("openai:") {
            let (url, model) = rest
                .split_once('|')
                .ok_or("openai: needs <base url>|<model>")?;
            let key = std::env::var("KIVO_BENCH_BRAIN_KEY").ok().map(Secret::new);
            let config = if key.is_some() {
                OpenAiConfig::compatible("bench", "Benchmarked service", url)
            } else {
                OpenAiConfig::local("bench", "Local server", url)
            };
            (
                Target::Api {
                    provider: OpenAi::new(config, key, Http::new()),
                    model: model.to_owned(),
                },
                format!("{model} at {url}"),
            )
        } else {
            return Err("KIVO_BENCH_BRAIN must start with `agent:` or `openai:`".into());
        };
        Ok(Self { rt, target, label })
    }

    /// Sends one prompt; `cancel_after_first` cancels as soon as the first token arrives and
    /// reports how long the stream took to end after that.
    fn ask(&self, prompt: &str, tool: bool, cancel_after_first: bool) -> Outcome {
        let cancel = CancellationToken::new();
        self.rt.block_on(async {
            let started = Instant::now();
            let mut out = Outcome::default();
            let mut cancelled_at: Option<Instant> = None;
            match &self.target {
                Target::Agent { session, .. } => {
                    let mut events = session.prompt(prompt, cancel.clone());
                    loop {
                        let Ok(Some(event)) = tokio::time::timeout(TIMEOUT, events.recv()).await
                        else {
                            out.error = true;
                            break;
                        };
                        match event {
                            AgentEvent::Message(t) => {
                                if out.first_token_ms.is_none() {
                                    out.first_token_ms = Some(ms(started));
                                    if cancel_after_first {
                                        cancelled_at = Some(Instant::now());
                                        cancel.cancel();
                                    }
                                }
                                out.text.push_str(&t);
                            }
                            AgentEvent::Done(_) => break,
                            AgentEvent::Error(_) => {
                                out.error = cancelled_at.is_none();
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                Target::Api { provider, model } => {
                    let mut request = ChatRequest::new(model.clone());
                    request.messages.push(Message::user(prompt));
                    request.max_tokens = 800;
                    if tool {
                        request.tools.push(ToolDef {
                            name: "weather".into(),
                            description: "The current weather in a city.".into(),
                            params: json!({
                                "type": "object",
                                "properties": { "city": { "type": "string" } },
                                "required": ["city"],
                            }),
                        });
                    }
                    let mut stream = provider.chat(request, cancel.clone());
                    loop {
                        let Ok(Some(event)) = tokio::time::timeout(TIMEOUT, stream.recv()).await
                        else {
                            out.error = true;
                            break;
                        };
                        match event {
                            BrainEvent::TextDelta(t) => {
                                if out.first_token_ms.is_none() {
                                    out.first_token_ms = Some(ms(started));
                                    if cancel_after_first {
                                        cancelled_at = Some(Instant::now());
                                        cancel.cancel();
                                    }
                                }
                                out.text.push_str(&t);
                            }
                            BrainEvent::ToolCall { .. } => {
                                out.tool_call_ms.get_or_insert(ms(started));
                            }
                            BrainEvent::Usage(u) => out.output_tokens += u.output_tokens,
                            BrainEvent::Done(_) => break,
                            BrainEvent::Error(_) => {
                                out.error = cancelled_at.is_none();
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
            out.total_ms = cancelled_at.map_or_else(|| ms(started), ms);
            out
        })
    }
}

impl Drop for Brain {
    fn drop(&mut self) {
        if let Target::Agent { client, .. } = &self.target {
            client.stop();
        }
    }
}

impl Suite for Brain {
    fn name(&self) -> &'static str {
        "brain"
    }

    fn run(&mut self) -> Result<Vec<Sample>, String> {
        let answer = self.ask(ANSWER, false, false);
        let structured = self.ask(STRUCTURED, false, false);
        let long = self.ask(LONG, false, false);
        let cancelled = self.ask(LONG, false, true);
        let api = matches!(self.target, Target::Api { .. });
        let tool = api.then(|| self.ask(TOOL, true, false));

        let asked = [&answer, &structured, &long, &cancelled]
            .into_iter()
            .chain(tool.as_ref())
            .collect::<Vec<_>>();
        let errors = asked.iter().filter(|o| o.error).count();
        if errors == asked.len() {
            return Err(format!("every prompt failed on {}", self.label));
        }
        let first = answer
            .first_token_ms
            .ok_or("the short answer had no text")?;
        let mut samples = vec![
            Sample::cost("time to first token (short answer)", "ms", first),
            Sample::cost("short answer complete", "ms", answer.total_ms),
            Sample::cost(
                "cancel → stream ended",
                "ms",
                if cancelled.first_token_ms.is_some() {
                    cancelled.total_ms
                } else {
                    return Err("the cancelled prompt never started answering".into());
                },
            ),
            #[allow(clippy::cast_precision_loss, reason = "a handful of prompts")]
            Sample::cost(
                "error rate",
                "%",
                errors as f64 * 100.0 / asked.len() as f64,
            ),
            Sample {
                metric: "structured output valid".into(),
                unit: "%",
                lower_is_better: false,
                value: if valid_structured(&structured.text) {
                    100.0
                } else {
                    0.0
                },
            },
        ];
        // Tokens per second from the provider's own count, or estimated from the text (about
        // four characters a token) when it reports none, as ACP agents don't.
        let generating = (long.total_ms - long.first_token_ms.unwrap_or(0.0)).max(1.0) / 1e3;
        #[allow(clippy::cast_precision_loss, reason = "token counts")]
        let tokens = if long.output_tokens > 0 {
            long.output_tokens as f64
        } else {
            long.text.chars().count() as f64 / 4.0
        };
        samples.push(Sample {
            metric: "output speed (400-word story)".into(),
            unit: "tokens/s",
            lower_is_better: false,
            value: tokens / generating,
        });
        if let Some(tool) = tool {
            samples.push(Sample::cost(
                "prompt → tool call",
                "ms",
                tool.tool_call_ms.ok_or("the model didn't call the tool")?,
            ));
        }
        Ok(samples)
    }

    fn notes(&self) -> Vec<String> {
        vec![format!(
            "{}. Fixed prompts: a two-sentence answer, a JSON-only answer, a 400-word story (also \
             cancelled at its first token), and for API brains a weather question with one tool. \
             ACP agents report no token counts, so their speed is estimated at four characters a \
             token.",
            self.label
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::valid_structured;

    #[test]
    fn structured_answers_are_checked() {
        assert!(valid_structured(r#"{"city":"Paris","population":2100000}"#));
        assert!(valid_structured(
            "```json\n{\"city\":\"Paris\",\"population\":2.1e6}\n```"
        ));
        assert!(!valid_structured("Paris has about 2.1 million people."));
        assert!(!valid_structured(r#"{"city":"Paris"}"#));
    }
}
