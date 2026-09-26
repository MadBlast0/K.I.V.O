//! How hard a brain thinks before it answers (owner, 2026-09-26): the user picks a level per
//! brain choice (Off, Low, Medium, High) where the model can take one, and each API gets it in
//! its own words: OpenAI's `reasoning_effort`, OpenRouter's `reasoning`, Anthropic's extended
//! thinking budget, Gemini's thinking budget. A model that can't take a level gets nothing, so
//! a choice never breaks a request.

use serde::{Deserialize, Serialize};

/// The level. `None` in a choice means the model's own default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Effort {
    Off,
    Low,
    Medium,
    High,
}

const ALL: [Effort; 4] = [Effort::Off, Effort::Low, Effort::Medium, Effort::High];
const ON: [Effort; 3] = [Effort::Low, Effort::Medium, Effort::High];

/// The model's own name without a router's `vendor/` prefix: `openai/o3` → `o3`.
fn bare(model: &str) -> &str {
    model.rsplit('/').next().unwrap_or(model)
}

/// OpenAI's reasoning models: the o-series and GPT-5. GPT-5 can also barely reason
/// (`minimal`), which is what Off means for it.
fn openai_levels(model: &str) -> &'static [Effort] {
    let m = bare(model);
    if m.starts_with("gpt-5") {
        &ALL
    } else if ["o1", "o3", "o4"].iter().any(|p| m.starts_with(p)) {
        &ON
    } else {
        &[]
    }
}

/// Claude models with extended thinking: 3.7 and the 4 family.
fn anthropic_thinks(model: &str) -> bool {
    let m = bare(model);
    m.starts_with("claude") && (m.contains("3-7") || m.contains("3.7") || m.contains("-4"))
}

/// Gemini's thinking models: 2.5 and later. Pro can't turn thinking off.
fn gemini_levels(model: &str) -> &'static [Effort] {
    let m = bare(model);
    let thinks = m.starts_with("gemini-2.5") || m.starts_with("gemini-3");
    if !thinks {
        &[]
    } else if m.contains("pro") {
        &ON
    } else {
        &ALL
    }
}

/// The levels `model` of `provider` can take; empty when it can't be set.
pub fn levels(provider: &str, model: &str) -> Vec<Effort> {
    match provider {
        "openai" => openai_levels(model).to_vec(),
        "anthropic" if anthropic_thinks(model) => ALL.to_vec(),
        "gemini" => gemini_levels(model).to_vec(),
        // OpenRouter takes one `reasoning` field for every family and passes it on.
        "openrouter" => {
            let m = bare(model);
            if !openai_levels(m).is_empty()
                || anthropic_thinks(m)
                || !gemini_levels(m).is_empty()
                || [
                    "deepseek-r1",
                    "qwq",
                    "qwen3",
                    "grok-3-mini",
                    "gpt-oss",
                    "magistral",
                ]
                .iter()
                .any(|f| m.starts_with(f) || m.contains(":thinking"))
            {
                ALL.to_vec()
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

/// `effort` when `model` of `provider` can take it.
pub fn applies(provider: &str, model: &str, effort: Option<Effort>) -> Option<Effort> {
    effort.filter(|e| levels(provider, model).contains(e))
}

/// OpenAI's `reasoning_effort`.
pub fn openai_effort(effort: Effort) -> &'static str {
    match effort {
        Effort::Off => "minimal",
        Effort::Low => "low",
        Effort::Medium => "medium",
        Effort::High => "high",
    }
}

/// Anthropic's thinking budget in tokens; `None` is thinking off.
pub fn anthropic_budget(effort: Effort) -> Option<u32> {
    match effort {
        Effort::Off => None,
        Effort::Low => Some(2_048),
        Effort::Medium => Some(8_192),
        Effort::High => Some(24_576),
    }
}

/// Gemini's `thinkingBudget` in tokens (0 turns it off).
pub fn gemini_budget(effort: Effort) -> u32 {
    match effort {
        Effort::Off => 0,
        Effort::Low => 1_024,
        Effort::Medium => 8_192,
        Effort::High => 24_576,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_family_takes_the_levels_its_api_has() {
        assert_eq!(levels("openai", "gpt-5-mini"), ALL);
        assert_eq!(levels("openai", "o3"), ON);
        assert!(levels("openai", "gpt-4o").is_empty());
        assert_eq!(levels("anthropic", "claude-sonnet-4-5"), ALL);
        assert_eq!(levels("anthropic", "claude-3-7-sonnet-latest"), ALL);
        assert!(levels("anthropic", "claude-3-5-haiku-latest").is_empty());
        assert_eq!(levels("gemini", "gemini-2.5-flash"), ALL);
        assert_eq!(levels("gemini", "gemini-2.5-pro"), ON, "Pro always thinks");
        assert!(levels("gemini", "gemini-2.0-flash").is_empty());
        assert_eq!(levels("openrouter", "deepseek/deepseek-r1"), ALL);
        assert_eq!(levels("openrouter", "anthropic/claude-opus-4.1"), ALL);
        assert!(levels("openrouter", "meta-llama/llama-3.3-70b-instruct").is_empty());
        assert!(levels("groq", "llama-3.3-70b").is_empty());
        // A level the model can't take is dropped, never sent.
        assert_eq!(applies("openai", "o3", Some(Effort::Off)), None);
        assert_eq!(
            applies("openai", "o3", Some(Effort::High)),
            Some(Effort::High)
        );
    }
}
