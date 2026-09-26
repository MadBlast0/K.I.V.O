//! Adapter contract tests (M3 "How to verify"): each adapter against recorded responses in its
//! provider's streaming format, served by a local mock server. Same contract for all: text
//! arrives as deltas, tool calls arrive whole (their pieces joined), usage is reported, failures
//! and cancellation end the stream with one normalized error.

use crate::anthropic::Anthropic;
use crate::gemini::Gemini;
use crate::http::Http;
use crate::openai::{OpenAi, OpenAiConfig};
use crate::testing::{MockServer, Reply};
use crate::types::{
    ChatRequest, Message, NormalizedError, StopReason, SystemBlock, ToolDef, Usage,
};
use crate::{BrainEvent, BrainProvider, collect};
use kivo_core::Secret;
use serde_json::json;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

fn key() -> Secret<String> {
    Secret::new("sk-test-123".to_owned())
}

fn request() -> ChatRequest {
    let mut r = ChatRequest::new("test-model");
    r.system = vec![
        SystemBlock {
            text: "You are KIVO.".into(),
            cacheable: true,
        },
        SystemBlock {
            text: "About me: call me Sam.".into(),
            cacheable: true,
        },
        SystemBlock {
            text: "Active app: VS Code".into(),
            cacheable: false,
        },
    ];
    r.messages = vec![Message::user("What's the weather?")];
    r.tools = vec![ToolDef {
        name: "apps_launch".into(),
        description: "Open an app".into(),
        params: json!({
            "type": "object",
            "properties": { "app": { "type": "string" } },
            "required": ["app"],
            "additionalProperties": false,
        }),
    }];
    r
}

fn openai_at(url: &str) -> OpenAi {
    let mut config = OpenAiConfig::openai();
    config.base_url = format!("{url}/v1");
    OpenAi::new(config, Some(key()), Http::new())
}

#[tokio::test]
async fn openai_streams_text_tool_calls_and_usage() {
    let server = MockServer::start(|r| {
        if r.path.ends_with("/models") {
            return Reply::json(200, &json!({ "data": [{ "id": "gpt-5" }, { "id": "gpt-5-mini" }] }));
        }
        Reply::sse(&[
            r#"data: {"choices":[{"index":0,"delta":{"role":"assistant","content":"Sunny"}}]}"#,
            r#"data: {"choices":[{"index":0,"delta":{"content":" and warm."}}]}"#,
            r#"data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"apps_launch","arguments":"{\"ap"}}]}}]}"#,
            r#"data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"p\":\"Chrome\"}"}}]}}]}"#,
            r#"data: {"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#,
            r#"data: {"choices":[],"usage":{"prompt_tokens":1200,"completion_tokens":30,"prompt_tokens_details":{"cached_tokens":1024}}}"#,
            "data: [DONE]",
        ])
    })
    .await;
    let brain = openai_at(&server.url);
    let got = collect(brain.chat(request(), CancellationToken::new())).await;
    assert_eq!(got.text, "Sunny and warm.");
    assert_eq!(
        got.tool_calls,
        [(
            "call_1".into(),
            "apps_launch".into(),
            json!({ "app": "Chrome" })
        )]
    );
    assert_eq!(
        got.usage,
        Usage {
            input_tokens: 1200,
            output_tokens: 30,
            cached_tokens: 1024
        }
    );
    assert_eq!(got.stop, Some(StopReason::ToolUse));
    assert_eq!(got.error, None);

    let sent = server.requests().pop().unwrap();
    assert_eq!(sent.path, "/v1/chat/completions");
    assert_eq!(sent.header("authorization"), Some("Bearer sk-test-123"));
    assert_eq!(sent.body["messages"][0]["role"], "system");
    assert!(
        sent.body["messages"][0]["content"]
            .as_str()
            .unwrap()
            .starts_with("You are KIVO."),
        "the stable prefix goes first, for OpenAI's automatic caching"
    );
    assert_eq!(sent.body["tools"][0]["function"]["name"], "apps_launch");
    assert_eq!(sent.body["max_completion_tokens"], 1024);
    assert_eq!(sent.body["stream_options"]["include_usage"], true);

    let models = brain.models().await.unwrap();
    assert_eq!(models.len(), 2);
    assert!(brain.health().await.is_ready());
}

/// BRAIN-11: Groq, Mistral, DeepSeek and xAI through the OpenAI-compatible adapter, each with its
/// own quirks: Mistral refuses `stream_options` and reports usage in its last chunk; DeepSeek and
/// xAI stream reasoning as `reasoning_content`; all take `max_tokens`.
#[tokio::test]
async fn groq_mistral_deepseek_and_xai_speak_the_compatible_dialect() {
    let server = MockServer::start(|r| {
        if r.body.get("stream_options").is_some() && r.path.starts_with("/mistral") {
            return Reply::json(
                422,
                &json!({ "detail": [{ "type": "extra_forbidden", "loc": ["body", "stream_options"] }] }),
            );
        }
        Reply::sse(&[
            r#"data: {"choices":[{"index":0,"delta":{"reasoning_content":"Think."}}]}"#,
            r#"data: {"choices":[{"index":0,"delta":{"content":"Hi."}}]}"#,
            r#"data: {"choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":2}}"#,
            "data: [DONE]",
        ])
    })
    .await;
    for id in ["groq", "mistral", "deepseek", "xai"] {
        let entry = crate::catalog::entry(id).expect("in the catalog");
        let config =
            OpenAiConfig::compatible(entry.id, entry.name, format!("{}/{id}/v1", server.url));
        let brain = OpenAi::new(config, Some(key()), Http::new());
        let got = collect(brain.chat(request(), CancellationToken::new())).await;
        assert_eq!(got.error, None, "{id}");
        assert_eq!(got.text, "Hi.", "{id}: reasoning is never text");
        assert_eq!(got.usage.output_tokens, 2, "{id}");
        let sent = server.requests().pop().unwrap();
        assert_eq!(sent.path, format!("/{id}/v1/chat/completions"));
        assert_eq!(sent.body["max_tokens"], 1024, "{id}");
        assert_eq!(
            sent.body.get("stream_options").is_some(),
            id != "mistral",
            "{id}"
        );
    }
}

#[tokio::test]
async fn reasoning_is_reported_apart_and_never_as_text() {
    let server = MockServer::start(|_| {
        Reply::sse(&[
            r#"data: {"choices":[{"index":0,"delta":{"reasoning_content":"The user wants..."}}]}"#,
            r#"data: {"choices":[{"index":0,"delta":{"content":"Done."},"finish_reason":"stop"}]}"#,
            "data: [DONE]",
        ])
    })
    .await;
    let got = collect(openai_at(&server.url).chat(request(), CancellationToken::new())).await;
    assert_eq!(got.text, "Done.");
    assert_eq!(got.reasoning, "The user wants...");
}

#[tokio::test]
async fn http_failures_and_in_stream_errors_are_normalized() {
    let server = MockServer::start(|r| match r.body["model"].as_str() {
        Some("denied") => Reply::json(401, &json!({ "error": { "message": "Incorrect API key" } })),
        Some("busy") => {
            let mut reply = Reply::json(
                429,
                &json!({ "error": { "message": "Rate limit reached" } }),
            );
            reply.headers.push(("retry-after".into(), "7".into()));
            reply
        }
        _ => Reply::sse(&[
            r#"data: {"choices":[{"index":0,"delta":{"content":"Par"}}]}"#,
            r#"data: {"error":{"message":"upstream overloaded","code":503}}"#,
        ]),
    })
    .await;
    let brain = openai_at(&server.url);
    let chat = |model: &str| {
        let mut r = request();
        r.model = model.into();
        collect(brain.chat(r, CancellationToken::new()))
    };
    assert_eq!(chat("denied").await.error, Some(NormalizedError::Auth));
    assert_eq!(
        chat("busy").await.error,
        Some(NormalizedError::RateLimited {
            retry_after: Some(Duration::from_secs(7))
        })
    );
    let broken = chat("other").await;
    assert_eq!(broken.text, "Par");
    assert_eq!(
        broken.error,
        Some(NormalizedError::ProviderDown("upstream overloaded".into()))
    );
}

#[tokio::test]
async fn cancelling_mid_stream_ends_it_at_once() {
    let server = MockServer::start(|_| {
        let mut reply =
            Reply::sse(&[r#"data: {"choices":[{"index":0,"delta":{"content":"Once upon"}}]}"#]);
        reply.hang = true;
        reply
    })
    .await;
    let cancel = CancellationToken::new();
    let mut stream = openai_at(&server.url).chat(request(), cancel.clone());
    assert_eq!(
        stream.recv().await,
        Some(BrainEvent::TextDelta("Once upon".into()))
    );
    let started = Instant::now();
    cancel.cancel();
    assert_eq!(
        stream.recv().await,
        Some(BrainEvent::Error(NormalizedError::Cancelled))
    );
    assert_eq!(stream.recv().await, None);
    assert!(
        started.elapsed() < Duration::from_millis(100),
        "{:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn an_unreachable_provider_is_a_network_error() {
    let mut config = OpenAiConfig::local("ollama", "Ollama", "http://127.0.0.1:9/v1");
    config.base_url = "http://127.0.0.1:9/v1".into();
    let brain = OpenAi::new(config, None, Http::new());
    let got = collect(brain.chat(request(), CancellationToken::new())).await;
    assert!(
        matches!(got.error, Some(NormalizedError::Network(_))),
        "{got:?}"
    );
    assert!(!brain.health().await.is_ready());
}

#[tokio::test]
async fn a_local_server_needs_no_key_and_lists_its_models() {
    let server = MockServer::start(|r| {
        if r.path.ends_with("/models") {
            return Reply::json(
                200,
                &json!({ "object": "list", "data": [{ "id": "llama3.2:3b" }] }),
            );
        }
        Reply::sse(&[
            r#"data: {"choices":[{"index":0,"delta":{"content":"Hi"},"finish_reason":"stop"}]}"#,
            "data: [DONE]",
        ])
    })
    .await;
    let brain = OpenAi::new(
        OpenAiConfig::local("ollama", "Ollama", format!("{}/v1", server.url)),
        None,
        Http::new(),
    );
    assert_eq!(brain.models().await.unwrap()[0].id, "llama3.2:3b");
    let got = collect(brain.chat(request(), CancellationToken::new())).await;
    assert_eq!(got.text, "Hi");
    let sent = server.requests().pop().unwrap();
    assert_eq!(sent.header("authorization"), None);
    assert_eq!(sent.body["max_tokens"], 1024);
}

#[tokio::test]
async fn openrouter_reports_context_and_prices_and_names_the_app() {
    let server = MockServer::start(|_| {
        Reply::json(
            200,
            &json!({ "data": [{
                "id": "meta-llama/llama-3.3-70b-instruct:free",
                "context_length": 131072,
                "pricing": { "prompt": "0", "completion": "0" },
            }, {
                "id": "anthropic/claude-sonnet-4.5",
                "context_length": 200000,
                "pricing": { "prompt": "0.000003", "completion": "0.000015" },
                "architecture": { "input_modalities": ["text", "image"] },
            }] }),
        )
    })
    .await;
    let mut config = OpenAiConfig::openrouter();
    config.base_url = format!("{}/api/v1", server.url);
    let brain = OpenAi::new(config, Some(key()), Http::new());
    let models = brain.models().await.unwrap();
    assert_eq!(models[0].input_price, Some(0.0));
    assert_eq!(models[1].context_window, Some(200_000));
    assert!((models[1].output_price.unwrap() - 15.0).abs() < 1e-9);
    assert!(models[1].vision);
    assert_eq!(server.requests()[0].header("x-title"), Some("KIVO"));
}

#[tokio::test]
async fn anthropic_streams_and_caches_the_stable_prefix() {
    let server = MockServer::start(|_| {
        Reply::sse(&[
            r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_1","role":"assistant","usage":{"input_tokens":50,"cache_read_input_tokens":1500,"cache_creation_input_tokens":0,"output_tokens":1}}}"#,
            r#"event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#,
            r#"event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Let me think."}}"#,
            r#"event: content_block_start
data: {"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}"#,
            r#"event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Opening Chrome."}}"#,
            r#"event: content_block_stop
data: {"type":"content_block_stop","index":1}"#,
            r#"event: content_block_start
data: {"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"toolu_1","name":"apps_launch","input":{}}}"#,
            r#"event: content_block_delta
data: {"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"app\": \"Chr"}}"#,
            r#"event: content_block_delta
data: {"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"ome\"}"}}"#,
            r#"event: content_block_stop
data: {"type":"content_block_stop","index":2}"#,
            r#"event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":42}}"#,
            r#"event: message_stop
data: {"type":"message_stop"}"#,
        ])
    })
    .await;
    let brain = Anthropic::new(server.url.clone(), key(), Http::new());
    let got = collect(brain.chat(request(), CancellationToken::new())).await;
    assert_eq!(got.text, "Opening Chrome.");
    assert_eq!(got.reasoning, "Let me think.");
    assert_eq!(
        got.tool_calls,
        [(
            "toolu_1".into(),
            "apps_launch".into(),
            json!({ "app": "Chrome" })
        )]
    );
    assert_eq!(
        got.usage,
        Usage {
            input_tokens: 1550,
            output_tokens: 42,
            cached_tokens: 1500
        }
    );
    assert_eq!(got.stop, Some(StopReason::ToolUse));

    let sent = server.requests().pop().unwrap();
    assert_eq!(sent.path, "/v1/messages");
    assert_eq!(sent.header("x-api-key"), Some("sk-test-123"));
    assert_eq!(sent.header("anthropic-version"), Some("2023-06-01"));
    let system = sent.body["system"].as_array().unwrap();
    assert_eq!(system.len(), 3);
    assert!(system[0].get("cache_control").is_none());
    assert_eq!(
        system[1]["cache_control"]["type"], "ephemeral",
        "the last stable block closes the cached prefix"
    );
    assert!(
        system[2].get("cache_control").is_none(),
        "live context isn't cached"
    );
    assert_eq!(sent.body["tools"][0]["input_schema"]["required"][0], "app");
}

#[tokio::test]
async fn anthropic_overload_mid_stream_is_provider_down() {
    let server = MockServer::start(|_| {
        Reply::sse(&[r#"event: error
data: {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#])
    })
    .await;
    let brain = Anthropic::new(server.url.clone(), key(), Http::new());
    let got = collect(brain.chat(request(), CancellationToken::new())).await;
    assert_eq!(
        got.error,
        Some(NormalizedError::ProviderDown("Overloaded".into()))
    );
}

#[tokio::test]
async fn gemini_streams_function_calls_and_thoughts() {
    let server = MockServer::start(|r| {
        if r.method == "GET" {
            return Reply::json(200, &json!({ "models": [
                { "name": "models/gemini-2.5-flash", "inputTokenLimit": 1048576, "supportedGenerationMethods": ["generateContent", "countTokens"] },
                { "name": "models/text-embedding-004", "supportedGenerationMethods": ["embedContent"] },
            ] }));
        }
        Reply::sse(&[
            r#"data: {"candidates":[{"content":{"role":"model","parts":[{"text":"Planning","thought":true}]}}],"usageMetadata":{"promptTokenCount":900,"candidatesTokenCount":1}}"#,
            r#"data: {"candidates":[{"content":{"role":"model","parts":[{"text":"Sure, "}]}}]}"#,
            r#"data: {"candidates":[{"content":{"role":"model","parts":[{"functionCall":{"name":"apps_launch","args":{"app":"Chrome"}}}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":900,"candidatesTokenCount":12,"cachedContentTokenCount":800}}"#,
        ])
    })
    .await;
    let brain = Gemini::new(server.url.clone(), key(), Http::new());
    let got = collect(brain.chat(request(), CancellationToken::new())).await;
    assert_eq!(got.text, "Sure, ");
    assert_eq!(got.reasoning, "Planning");
    assert_eq!(got.tool_calls.len(), 1);
    assert_eq!(got.tool_calls[0].2, json!({ "app": "Chrome" }));
    assert_eq!(
        got.usage,
        Usage {
            input_tokens: 900,
            output_tokens: 12,
            cached_tokens: 800
        },
        "the last running total, once"
    );
    assert_eq!(got.stop, Some(StopReason::ToolUse));

    let sent = server.requests().pop().unwrap();
    assert!(
        sent.path
            .starts_with("/v1beta/models/test-model:streamGenerateContent")
    );
    assert_eq!(sent.header("x-goog-api-key"), Some("sk-test-123"));
    let params = &sent.body["tools"][0]["functionDeclarations"][0]["parameters"];
    assert!(
        params.get("additionalProperties").is_none(),
        "Gemini rejects additionalProperties"
    );
    assert_eq!(params["required"][0], "app");

    let models = brain.models().await.unwrap();
    assert_eq!(models.len(), 1, "only models that generate content");
    assert_eq!(models[0].id, "gemini-2.5-flash");
}

#[tokio::test]
async fn tool_results_go_back_in_each_providers_shape() {
    use crate::types::{Part, Role};
    let mut r = request();
    r.messages.push(crate::types::Message {
        role: Role::Assistant,
        parts: vec![Part::ToolCall {
            id: "call_1".into(),
            name: "apps_launch".into(),
            args: json!({ "app": "Chrome" }),
        }],
    });
    r.messages.push(crate::types::Message {
        role: Role::Tool,
        parts: vec![Part::ToolResult {
            id: "call_1".into(),
            name: "apps_launch".into(),
            content: "Opened Google Chrome".into(),
            is_error: false,
        }],
    });
    let o = crate::openai::body(&OpenAiConfig::openai(), &r);
    assert_eq!(
        o["messages"][2]["tool_calls"][0]["function"]["arguments"],
        r#"{"app":"Chrome"}"#
    );
    assert_eq!(o["messages"][3]["role"], "tool");
    assert_eq!(o["messages"][3]["tool_call_id"], "call_1");
    let a = crate::anthropic::body(&r);
    assert_eq!(a["messages"][1]["content"][0]["type"], "tool_use");
    assert_eq!(a["messages"][2]["role"], "user");
    assert_eq!(a["messages"][2]["content"][0]["tool_use_id"], "call_1");
    let g = crate::gemini::body(&r);
    assert_eq!(g["contents"][1]["role"], "model");
    assert_eq!(
        g["contents"][2]["parts"][0]["functionResponse"]["name"],
        "apps_launch"
    );
}

#[test]
fn screenshots_reach_vision_models_in_each_providers_shape() {
    use crate::types::{Part, Role};
    let mut r = request();
    r.messages.push(crate::types::Message {
        role: Role::Assistant,
        parts: vec![Part::ToolCall {
            id: "call_1".into(),
            name: "screen_look".into(),
            args: json!({}),
        }],
    });
    r.messages.push(crate::types::Message {
        role: Role::Tool,
        parts: vec![
            Part::ToolResult {
                id: "call_1".into(),
                name: "screen_look".into(),
                content: "{}".into(),
                is_error: false,
            },
            Part::Image {
                media_type: "image/png".into(),
                data: "iVBORw0KGgo=".into(),
            },
        ],
    });
    let a = crate::anthropic::body(&r);
    let content = &a["messages"][2]["content"];
    assert_eq!(content[0]["type"], "tool_result");
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["source"]["media_type"], "image/png");
    let o = crate::openai::body(&OpenAiConfig::openai(), &r);
    assert_eq!(o["messages"][3]["role"], "tool");
    assert_eq!(
        o["messages"][4]["role"], "user",
        "images follow the tool message"
    );
    assert_eq!(
        o["messages"][4]["content"][1]["image_url"]["url"],
        "data:image/png;base64,iVBORw0KGgo="
    );
    let g = crate::gemini::body(&r);
    assert_eq!(
        g["contents"][2]["parts"][1]["inlineData"]["mimeType"],
        "image/png"
    );
}

/// The reasoning level reaches each API in its own words, only for models that take it; and
/// Anthropic's signed thinking goes back with the tool loop it belongs to (owner, 2026-09-26).
#[test]
fn the_reasoning_level_reaches_each_api_in_its_words() {
    use crate::reasoning::Effort;
    use crate::types::{Part, Role};
    let mut r = request();
    r.temperature = Some(0.3);
    r.reasoning = Some(Effort::High);

    r.model = "o3".into();
    let o = crate::openai::body(&OpenAiConfig::openai(), &r);
    assert_eq!(o["reasoning_effort"], "high");
    assert!(
        o.get("temperature").is_none(),
        "reasoning models take no temperature"
    );
    r.model = "gpt-4o".into();
    let o = crate::openai::body(&OpenAiConfig::openai(), &r);
    assert!(
        o.get("reasoning_effort").is_none(),
        "a model that can't reason gets nothing"
    );

    r.model = "deepseek/deepseek-r1".into();
    let or = crate::openai::body(&OpenAiConfig::openrouter(), &r);
    assert_eq!(or["reasoning"]["effort"], "high");
    r.reasoning = Some(Effort::Off);
    let or = crate::openai::body(&OpenAiConfig::openrouter(), &r);
    assert_eq!(or["reasoning"]["enabled"], false);

    r.model = "gemini-2.5-flash".into();
    let g = crate::gemini::body(&r);
    assert_eq!(g["generationConfig"]["thinkingConfig"]["thinkingBudget"], 0);

    r.model = "claude-sonnet-4-5".into();
    r.reasoning = Some(Effort::Medium);
    r.max_tokens = 1024;
    // A tool loop in progress: the signed thinking comes back first in the assistant turn.
    r.messages.push(crate::types::Message {
        role: Role::Assistant,
        parts: vec![
            Part::Thinking {
                text: "The user wants Chrome.".into(),
                signature: "sig-123".into(),
                redacted: false,
            },
            Part::ToolCall {
                id: "call_1".into(),
                name: "apps_launch".into(),
                args: json!({ "app": "Chrome" }),
            },
        ],
    });
    let a = crate::anthropic::body(&r);
    assert_eq!(a["thinking"]["budget_tokens"], 8192);
    assert_eq!(a["max_tokens"], 1024 + 8192, "the answer keeps its room");
    assert!(a.get("temperature").is_none());
    let turn = &a["messages"][1]["content"];
    assert_eq!(turn[0]["type"], "thinking");
    assert_eq!(turn[0]["signature"], "sig-123");
    assert_eq!(turn[1]["type"], "tool_use");
    // Thinking off: no thinking field, and the old block isn't sent.
    r.reasoning = Some(Effort::Off);
    let a = crate::anthropic::body(&r);
    assert!(a.get("thinking").is_none());
    assert_eq!(a["messages"][1]["content"][0]["type"], "tool_use");
}

/// Anthropic's streamed thinking arrives whole, with its signature, before the tool call.
#[test]
fn anthropics_thinking_is_collected_with_its_signature() {
    use crate::http::Decode;
    use crate::sse::SseEvent;
    let mut decoder = crate::anthropic::Decoder::default();
    let mut out = Vec::new();
    for data in [
        json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "thinking", "thinking": "" } }),
        json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "thinking_delta", "thinking": "Open " } }),
        json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "thinking_delta", "thinking": "Chrome." } }),
        json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "signature_delta", "signature": "abc" } }),
        json!({ "type": "content_block_stop", "index": 0 }),
    ] {
        decoder.decode(
            &SseEvent {
                event: None,
                data: data.to_string(),
            },
            &mut out,
        );
    }
    assert!(out.contains(&BrainEvent::Thinking {
        text: "Open Chrome.".into(),
        signature: "abc".into(),
        redacted: false,
    }));
}
