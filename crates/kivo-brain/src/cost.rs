//! What brains cost (BRAINS §9, BRAIN-35/36). Cost is usage × a price table: a bundled one
//! derived from LiteLLM's price list (MIT), refreshed weekly from the same source unless the user
//! turns that off, with per-model overrides. Local brains cost nothing. Every figure is an
//! *estimate*, and the UI says so.
//!
//! Limits are the user's own (the default is to track, not limit): a scope, a period, an amount,
//! warning thresholds and what happens at the limit, plus per-task caps.

use crate::types::Usage;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Dollars per million tokens.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Price {
    #[serde(rename = "in")]
    pub input: f64,
    #[serde(rename = "out")]
    pub output: f64,
    /// Cache reads, where the provider discounts them.
    #[serde(default)]
    pub cached: Option<f64>,
    #[serde(default)]
    pub window: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PriceTable {
    models: HashMap<String, Price>,
    overrides: HashMap<String, Price>,
}

/// The bundled table.
const BUNDLED: &str = include_str!("../data/prices.json");
/// Where the weekly refresh reads from.
pub const REFRESH_URL: &str =
    "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";

impl PriceTable {
    pub fn bundled() -> Self {
        let doc: Value = serde_json::from_str(BUNDLED).unwrap_or_default();
        let models = doc
            .get("models")
            .and_then(|m| serde_json::from_value(m.clone()).ok())
            .unwrap_or_default();
        Self {
            models,
            overrides: HashMap::new(),
        }
    }

    /// Merges a freshly fetched LiteLLM file over the table (newer prices win).
    pub fn refresh_from_litellm(&mut self, raw: &Value) -> usize {
        let mut n = 0;
        for (key, v) in raw.as_object().into_iter().flatten() {
            let (Some(i), Some(o)) = (
                v.get("input_cost_per_token").and_then(Value::as_f64),
                v.get("output_cost_per_token").and_then(Value::as_f64),
            ) else {
                continue;
            };
            if v.get("mode")
                .and_then(Value::as_str)
                .is_some_and(|m| m != "chat")
            {
                continue;
            }
            let name = key.strip_prefix("gemini/").unwrap_or(key);
            self.models.insert(
                name.to_owned(),
                Price {
                    input: i * 1e6,
                    output: o * 1e6,
                    cached: v
                        .get("cache_read_input_token_cost")
                        .and_then(Value::as_f64)
                        .map(|c| c * 1e6),
                    window: v.get("max_input_tokens").and_then(Value::as_u64),
                },
            );
            n += 1;
        }
        n
    }

    /// The user's own price for a model (negotiated rates, free tiers).
    pub fn set_override(&mut self, model: &str, price: Price) {
        self.overrides.insert(model.to_owned(), price);
    }

    /// The price of `model` at `provider`: an override, the model itself, or the provider's
    /// prefixed entry (`openrouter/…`).
    pub fn price(&self, provider: &str, model: &str) -> Option<Price> {
        let prefixed = format!("{provider}/{model}");
        self.overrides
            .get(model)
            .or_else(|| self.overrides.get(&prefixed))
            .or_else(|| self.models.get(model))
            .or_else(|| self.models.get(&prefixed))
            .copied()
    }

    /// Estimated dollars for `usage`. Local and free brains are $0; unknown models are `None`
    /// (shown as "no estimate", never as free).
    pub fn cost(&self, provider: &str, model: &str, usage: Usage, free: bool) -> Option<f64> {
        if free || model.ends_with(":free") {
            return Some(0.0);
        }
        let p = self.price(provider, model)?;
        #[allow(clippy::cast_precision_loss, reason = "token counts")]
        let (input, cached, output) = (
            usage.input_tokens.saturating_sub(usage.cached_tokens) as f64,
            usage.cached_tokens as f64,
            usage.output_tokens as f64,
        );
        Some((input * p.input + cached * p.cached.unwrap_or(p.input) + output * p.output) / 1e6)
    }

    /// The model prices, to keep a refreshed table between runs.
    pub fn export(&self) -> Value {
        serde_json::to_value(&self.models).unwrap_or_default()
    }

    /// Prices kept by `export`, over the bundled ones.
    pub fn import(&mut self, models: &Value) {
        if let Ok(models) = serde_json::from_value::<HashMap<String, Price>>(models.clone()) {
            self.models.extend(models);
        }
    }

    pub fn len(&self) -> usize {
        self.models.len()
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }
}

/// What a limit covers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "id")]
pub enum Scope {
    Overall,
    Provider(String),
    Profile(String),
    /// `realtime`, `computerUse`, `agents`.
    Feature(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Period {
    Daily,
    Weekly,
    /// Resets on this day of the month (1–28).
    Monthly {
        reset_day: u8,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AtLimit {
    Ask,
    CheaperProfile,
    LocalOnly,
    Block,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Limit {
    pub scope: Scope,
    pub period: Period,
    /// In dollars.
    pub amount: f64,
    /// Percentages that warn once each, e.g. 50 / 80 / 95.
    pub warnings: Vec<u8>,
    pub at_limit: AtLimit,
}

/// One priced request, for adding up.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Spend {
    /// Unix milliseconds.
    pub at: i64,
    pub provider: String,
    pub profile: String,
    pub feature: String,
    pub cost: f64,
}

/// Where one limit stands.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitState {
    pub spent: f64,
    pub amount: f64,
    /// The highest warning threshold crossed, if any.
    pub warning: Option<u8>,
    pub reached: bool,
    pub at_limit: AtLimit,
}

/// Start of the period containing `now` (Unix ms), in local time (`offset_minutes` east of UTC).
pub fn period_start(now: i64, period: Period, offset_minutes: i32) -> i64 {
    const DAY: i64 = 86_400_000;
    let offset = i64::from(offset_minutes) * 60_000;
    let local = now + offset;
    let day = local.div_euclid(DAY);
    let start_day = match period {
        Period::Daily => day,
        // Weeks start on Monday (1970-01-01 was a Thursday).
        Period::Weekly => day - (day + 3).rem_euclid(7),
        Period::Monthly { reset_day } => {
            let (y, m, d) = civil(day);
            let reset = i64::from(reset_day.clamp(1, 28));
            if d >= reset {
                days_from_civil(y, m, reset)
            } else if m == 1 {
                days_from_civil(y - 1, 12, reset)
            } else {
                days_from_civil(y, m - 1, reset)
            }
        }
    };
    start_day * DAY - offset
}

fn civil(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Where `limit` stands given the spending so far.
pub fn check(limit: &Limit, spend: &[Spend], now: i64, offset_minutes: i32) -> LimitState {
    let from = period_start(now, limit.period, offset_minutes);
    let spent: f64 = spend
        .iter()
        .filter(|s| s.at >= from && s.at <= now)
        .filter(|s| match &limit.scope {
            Scope::Overall => true,
            Scope::Provider(p) => &s.provider == p,
            Scope::Profile(p) => &s.profile == p,
            Scope::Feature(f) => &s.feature == f,
        })
        .map(|s| s.cost)
        .sum();
    let fraction = if limit.amount > 0.0 {
        spent / limit.amount
    } else {
        0.0
    };
    LimitState {
        spent,
        amount: limit.amount,
        warning: limit
            .warnings
            .iter()
            .copied()
            .filter(|w| fraction * 100.0 >= f64::from(*w))
            .max(),
        reached: limit.amount > 0.0 && spent >= limit.amount,
        at_limit: limit.at_limit,
    }
}

/// Per-task caps (BRAINS §9): computer use $0.50, agents $2, realtime 30 min; each can be `None`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskCaps {
    pub computer_use: Option<f64>,
    pub agents: Option<f64>,
    pub realtime_minutes: Option<u32>,
}

impl Default for TaskCaps {
    fn default() -> Self {
        Self {
            computer_use: Some(0.50),
            agents: Some(2.0),
            realtime_minutes: Some(30),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(input: u64, cached: u64, output: u64) -> Usage {
        Usage {
            input_tokens: input,
            output_tokens: output,
            cached_tokens: cached,
        }
    }

    #[test]
    fn the_bundled_table_prices_the_catalog_models() {
        let t = PriceTable::bundled();
        assert!(t.len() > 500);
        for model in [
            "claude-sonnet-5",
            "gpt-5-mini",
            "gemini-2.5-flash",
            "deepseek-chat",
        ] {
            assert!(t.price("x", model).is_some(), "{model}");
        }
        // 1M input tokens (half cached) and 100k output on Claude Sonnet 5: $1 + $0.10 + $1.
        let c = t
            .cost(
                "anthropic",
                "claude-sonnet-5",
                usage(1_000_000, 500_000, 100_000),
                false,
            )
            .unwrap();
        assert!((c - 2.1).abs() < 1e-9, "{c}");
        assert!(
            t.price("openrouter", "anthropic/claude-sonnet-4.5")
                .is_some(),
            "prefixed entries"
        );
    }

    #[test]
    fn local_free_and_unknown_models_are_told_apart() {
        let t = PriceTable::bundled();
        assert_eq!(
            t.cost("ollama", "llama3.2", usage(5000, 0, 500), true),
            Some(0.0)
        );
        assert_eq!(
            t.cost(
                "openrouter",
                "meta-llama/llama-3.3-70b-instruct:free",
                usage(1, 0, 1),
                false
            ),
            Some(0.0)
        );
        assert_eq!(
            t.cost("x", "no-such-model", usage(1, 0, 1), false),
            None,
            "unknown is not free"
        );
    }

    #[test]
    fn overrides_and_refreshes_win() {
        let mut t = PriceTable::bundled();
        t.set_override(
            "gpt-5",
            Price {
                input: 0.0,
                output: 0.0,
                cached: None,
                window: None,
            },
        );
        assert_eq!(
            t.cost("openai", "gpt-5", usage(1000, 0, 1000), false),
            Some(0.0)
        );
        let raw = serde_json::json!({
            "brand-new-model": { "input_cost_per_token": 1e-6, "output_cost_per_token": 2e-6, "mode": "chat" },
            "an-embedder": { "input_cost_per_token": 1e-6, "output_cost_per_token": 0.0, "mode": "embedding" },
        });
        assert_eq!(t.refresh_from_litellm(&raw), 1);
        assert!((t.price("x", "brand-new-model").unwrap().output - 2.0).abs() < 1e-9);
    }

    #[test]
    fn periods_start_where_the_user_expects() {
        // 2026-09-23 (a Wednesday) 10:00 UTC.
        let now = 1_790_157_600_000;
        let day = period_start(now, Period::Daily, 0);
        assert_eq!(civil(day / 86_400_000), (2026, 9, 23));
        let week = period_start(now, Period::Weekly, 0);
        assert_eq!(civil(week / 86_400_000), (2026, 9, 21), "Monday");
        let month = period_start(now, Period::Monthly { reset_day: 25 }, 0);
        assert_eq!(civil(month / 86_400_000), (2026, 8, 25));
        // India (+5:30): the local day starts 5.5 h before UTC midnight.
        let ist = period_start(now, Period::Daily, 330);
        assert_eq!(ist, day - 330 * 60_000);
    }

    #[test]
    fn limits_warn_at_thresholds_and_say_when_reached() {
        let limit = Limit {
            scope: Scope::Provider("anthropic".into()),
            period: Period::Monthly { reset_day: 1 },
            amount: 10.0,
            warnings: vec![50, 80, 95],
            at_limit: AtLimit::Ask,
        };
        let now = 1_790_157_600_000;
        let spend = |cost: f64, provider: &str| Spend {
            at: now - 1000,
            provider: provider.into(),
            profile: "default".into(),
            feature: "chat".into(),
            cost,
        };
        let s = check(
            &limit,
            &[spend(8.02, "anthropic"), spend(50.0, "openai")],
            now,
            0,
        );
        assert!((s.spent - 8.02).abs() < 1e-9);
        assert_eq!((s.warning, s.reached), (Some(80), false));
        let s = check(&limit, &[spend(10.5, "anthropic")], now, 0);
        assert!(s.reached);
        let old = Spend {
            at: now - 40 * 86_400_000,
            ..spend(100.0, "anthropic")
        };
        assert_eq!(
            check(&limit, &[old], now, 0).spent,
            0.0,
            "last month doesn't count"
        );
        assert_eq!(TaskCaps::default().agents, Some(2.0));
    }
}
