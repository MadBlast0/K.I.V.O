# Reflex — a tiny, local, calibrated intent-hint engine

Status: Draft v1, 2026-09-23. Standalone project spec: written so it can seed its own repository.
KIVO is the first consumer ([§16](#16-integration-guide-kivo)); nothing else in this document
depends on KIVO.

> **One line:** Reflex turns a short request (plus a few cheap context signals) into typed,
> calibrated *hints* ("this is a coding request, 0.91") in well under a millisecond, on the CPU,
> offline, from a few hundred KB of weights on top of a sentence embedding the host already has.

---

## 1. What Reflex is, and what it is not

Reflex is a **System-1 hint layer** for assistants and agents. It sits between cheap
deterministic matching (grammars, exact commands) and expensive reasoning (LLMs, agents), and
answers a small, fixed set of typed questions about a request so the host can pick the right
expensive path without asking an LLM to classify first.

**Goals**

1. **Fast:** head inference ≤ 1 ms p95 on a mid-range laptop CPU, excluding the encoder.
2. **Small:** ≤ 1 MB of weights per language bundle; no model resident beyond what the host
   already loads for embeddings.
3. **Local and offline:** no network, no telemetry, no Python at runtime.
4. **Calibrated:** a stated confidence means that fraction correct on held-out data (ECE ≤ 0.05).
5. **Abstains:** below its threshold it says "no hint", and the host falls back to its default.
6. **Explainable:** every hint carries a short machine- and human-readable reason.
7. **Learns from corrections** locally, boundedly and resettably.
8. **Pure Rust runtime** with no heavy dependencies; the encoder is pluggable.

**Non-goals (hard rules)**

- Reflex is **never an authority.** It does not decide risk, permissions, confirmation, privacy
  class, data classification or whether a screen may be captured. Those stay deterministic in the
  host. A Reflex hint may only *narrow* or *prefer*; it can never *grant*.
- Reflex does **not** generate text, extract slots or call tools.
- Reflex does **not** replace the host's grammar or exact matching; it runs after them.
- Reflex does **not** handle open-ended classification with hundreds of labels. Heads are small
  and fixed (≤ 16 labels each) and ship with the bundle.

## 2. Background and prior art

Reflex exists because two "System-1 decision model" options were evaluated for KIVO and rejected
(2026-09-23). Facts below are from their published pages on that date; see [Sources](#22-sources).

| | Laya (convaiinnovations) | Jev (TypeSafe AI) | Reflex |
|---|---|---|---|
| What | Open-weights typed-decision model: `choice`, `score`, `noul` | Hosted typed-decision API: Choice (≤ 255 options), Score, Noul | Small typed heads on an existing embedding |
| Size | 421M params (ModernBERT-large, EN) / 322M (mmBERT-base, multilingual) | Undisclosed | ≤ 1 MB of heads + the host's encoder |
| Latency | ~33–40 ms on a T4 GPU; **193–464 ms on CPU**; 7–10 s cold load | 70–500 ms end to end, network included | ≤ 1 ms heads (+ encoder, which the host already runs) |
| Runtime | Python, PyTorch, Transformers | HTTP API, early access, waitlist | Pure Rust |
| Offline | Yes | **No** | Yes |
| Out of the box | Near chance zero-shot on its own typed-decisions benchmark (0.36 vs 0.318 random); 0.766 after fine-tuning | Zero-shot claimed | Needs its own training data (by design) |
| License | Apache 2.0 | Commercial API | Owner's choice (see §21) |

**Why not them:** both need data or a network to be useful, and both cost 100–1000× the latency
budget of a voice assistant's routing step. Reflex keeps their good idea (typed, calibrated
answers to a fixed question set) and drops the heavy model: the heavy lifting is done by a small
sentence encoder the host already needs for semantic matching and memory search.

## 3. Concepts

| Term | Meaning |
|---|---|
| **Request** | The user's text: a final transcript, a streaming partial, or typed text |
| **Context** | A few cheap, non-sensitive signals from the host (§7) |
| **State** | Request + context, the only input to Reflex |
| **Encoder** | A sentence-embedding model that maps text to a fixed vector (host-provided, pluggable) |
| **Head** | One typed question with a fixed label set, e.g. `route ∈ {event_task, brain}` |
| **Head kind** | `choice` (one of N), `multi` (any of N, independent), `yes_no` (a calibrated probability) |
| **Hint** | A head's answer: label(s), calibrated probabilities, margin, reason; or `Abstain` |
| **Bundle** | The versioned artifact: manifest + weights for one encoder and a set of heads |
| **Policy** | Per-head thresholds and a cost matrix that turn probabilities into a hint or an abstention |

## 4. Architecture

```text
                       ┌──────────────────────── host ─────────────────────────┐
 request text ───────► │ Encoder (e.g. MiniLM-class ONNX)  →  text vector e ∈ ℝᵈ │
 context signals ────► │ (the host already computes e for its semantic stage)    │
                       └───────────────┬────────────────────────────────────────┘
                                       │ e, context
                       ┌───────────────▼──────────── reflex-core ───────────────┐
                       │ FeatureBuilder   x = [ L2norm(e) ‖ ctx one-hots ‖ bias ]│
                       │        │                                               │
                       │        ├──► Head "route"    (choice)  ─┐               │
                       │        ├──► Head "profile"  (choice)   ├─► logits      │
                       │        └──► Head "tools"    (multi)   ─┘               │
                       │                         │                              │
                       │ Calibrator   p = softmax(logits / T_head)  or σ(z/T)   │
                       │ Adapter      + bounded local correction (§13)          │
                       │ Policy       threshold, margin, cost matrix → hint     │
                       │ Explainer    "reflex: profile=coding 0.91 (ctx: vscode)"│
                       └───────────────┬────────────────────────────────────────┘
                                       │ Hints { per head: Hint | Abstain }
                                       ▼
                        host rules decide (Reflex only suggests)
```

**Components**

| Component | Crate | Responsibility |
|---|---|---|
| `Encoder` trait | `reflex-core` | `fn encode(&self, text: &str) -> Result<Vec<f32>>`; reports `id`, `dim`, `hash` |
| `OrtEncoder` | `reflex-encode` | ONNX Runtime implementation (tokenizer + mean pooling + L2 norm) |
| `FeatureBuilder` | `reflex-core` | Concatenates the normalized embedding and encoded context features |
| `Head` | `reflex-core` | Linear or small MLP, `f32`, evaluated with plain loops (no BLAS) |
| `Calibrator` | `reflex-core` | Per-head temperature (choice/yes_no) or per-label temperature (multi) |
| `Adapter` | `reflex-core` | Optional bounded logit correction learned from local corrections |
| `Policy` | `reflex-core` | Thresholds, margins, cost matrix, abstention |
| `Explainer` | `reflex-core` | Produces the reason string and structured trace |
| `Trainer` | `reflex-train` | Offline CLI: embed data, fit heads, calibrate, evaluate, write bundle |

## 5. Model

### 5.1 Encoder

- **Default:** a MiniLM-class sentence encoder, 384 dimensions, run through ONNX Runtime, shared
  with the host's semantic matching and memory search.
- **Multilingual from day one:** a multilingual encoder is chosen so one set of heads serves every
  language the host supports. Candidates to benchmark (licenses and numbers to be verified before
  adoption): `paraphrase-multilingual-MiniLM-L12-v2`, `multilingual-e5-small`.
- **Estimated cost** (to be measured, §14): 5–20 ms per short request on a mid-range CPU;
  ~100–130 MB on disk for a 118M-parameter model at `f32`, less when quantized.
- **Bundles are bound to one encoder:** the manifest stores the encoder's `id`, `dim` and file
  hash. Loading a bundle against a different encoder is an error, never a silent degradation.

### 5.2 Features

```text
x = [ e / ‖e‖            384 floats
    , ctx one-hots        ~32 floats   (§7; unknown values map to an "other" slot)
    , 1.0 ]               bias
```

Context features are always optional: a bundle trained with context must still work when the host
passes `Context::default()` (training randomly drops context, §11.3).

### 5.3 Heads

| Variant | Shape | Params (d = 417, 4 labels) | Size (`f32`) | Use |
|---|---|---|---|---|
| **Linear** (default) | `W ∈ ℝ^{k×d}` | ~1.7 k | ~7 KB | Always tried first |
| **MLP-1** | `d → 64 (GELU) → k` | ~27 k | ~110 KB | Only if it beats linear by ≥ 2 points macro-F1 on held-out data |

Evaluation is a handful of dot products: tens of thousands of multiply-adds, i.e. microseconds.

### 5.4 Calibration

- **Temperature scaling** per head (per label for `multi`), fitted on a held-out calibration split
  by minimizing NLL.
- Target: **ECE ≤ 0.05** per head and per language on the test split; the bundle fails to build
  otherwise (§14.3).

### 5.5 Decision policy

For each head, after calibration:

1. **Abstain** unless `p_top ≥ τ_head` **and** `p_top − p_second ≥ margin_head`.
2. **Asymmetric costs:** each head has a cost matrix `C[true][predicted]`. The chosen label
   minimizes expected cost, not just maximizes probability. Example for `profile`:

   | true ↓ / predicted → | fast | smart | coding |
   |---|---|---|---|
   | fast | 0 | 1 | 2 |
   | smart | 4 | 0 | 2 |
   | coding | 6 | 3 | 0 |

   Under-powering (coding sent to fast) wastes a whole turn; over-powering only costs a little
   money and latency, so cheaper labels need more confidence.
3. **`multi` heads** emit every label whose calibrated probability ≥ its own threshold; an empty
   set means abstain.
4. Thresholds are chosen on the calibration split to hit a **target selective accuracy** (for
   example 95% correct among non-abstained answers) and recorded in the manifest.

## 6. Streaming mode (speculative warm-up)

Voice hosts get partial transcripts while the user is still speaking. Reflex can run on each
partial so the host can **prepare** the likely path early.

```text
partial "fix the"            → profile abstain
partial "fix the failing"    → profile coding 0.71   (below τ_warm)
partial "fix the failing te" → profile coding 0.88   stable 1/2
partial "fix the failing tes"→ profile coding 0.90   stable 2/2  → Prewarm(coding)
final   "fix the failing tests in kivo" → profile coding 0.93 → Hint(coding)
```

- **API:** `Reflex::observe_partial(&mut StreamSession, text, ctx) -> Option<Prewarm>`.
- **Stability rule:** emit `Prewarm(label)` once the same top label has been seen for
  `stable_n` consecutive partials (default 2) with `p ≥ τ_warm` (default `τ_head − 0.05`).
- **Change rule:** if the top label changes after a `Prewarm`, emit `CancelPrewarm(label)`.
- **Hard rule:** a `Prewarm` may only cause **side-effect-free preparation** by the host (load a
  model, open a connection, spawn an idle agent process). Nothing is executed, sent or shown on
  the basis of a partial. The final hint is computed from the final transcript only.
- **Budget:** at most one encoder call per partial and at most `max_partials_per_sec` (default 5);
  extra partials are dropped, not queued.

## 7. Context signals

Context is what separates "fix this" in a code editor from "fix this" in a word processor. Only
**cheap, low-sensitivity** signals are allowed.

| Signal | Encoding | Notes |
|---|---|---|
| `app_category` | one-hot: `code_editor`, `terminal`, `browser`, `office`, `file_manager`, `media`, `chat`, `game`, `other`, `none` | The host maps the foreground process to a category. **Never** window titles, URLs or document names |
| `input_mode` | one-hot: `voice`, `typed` | |
| `previous_route` | one-hot over the route head's labels + `none` | From the host's last turn |
| `previous_profile` | one-hot over the profile head's labels + `none` | |
| `since_last_turn` | buckets: `<10 s`, `<60 s`, `<10 min`, `longer`, `none` | Follow-up detection |
| `task_running` | yes/no | A background task or agent is active |
| `language` | one-hot over the bundle's languages | From the host's STT/language ID |

**Never features:** screen contents, clipboard, file contents, window titles, URLs, contacts,
user identity, location. The host is responsible for mapping raw state into these categories;
Reflex rejects unknown keys.

## 8. Follow-ups

Short follow-ups ("and the tests too", "do the same for the other one") are the most common
misroute for any text classifier. Two layers handle them:

1. **Host rule (recommended, deterministic):** if `since_last_turn < 10 s` and the request is
   short or starts with a continuation cue, keep the previous route and profile.
2. **Reflex features:** `previous_route`, `previous_profile` and `since_last_turn` let the heads
   learn the pattern for cases the rule misses.

## 9. Heads

A bundle declares its heads in the manifest. The heads below are the **KIVO v1 set**; other hosts
define their own.

| Head | Kind | Labels | Phase | Host uses it for |
|---|---|---|---|---|
| `route` | choice | `event_task`, `brain` | R1 | Send "tell me when…"-like requests the grammar missed to the task engine |
| `profile` | choice | `fast`, `smart`, `coding` | R1 | The "task class" rule of the brain router |
| `tool_families` | multi | `files`, `browser`, `web`, `media`, `windows`, `apps`, `system`, `messaging`, `calendar`, `code` | R4 | Expose a smaller tool set to the brain (only narrows; permissions still check every call) |
| `needs_web` | yes_no | — | R4 | Warm up web search early |

**Forbidden heads** (enforced by a lint in `reflex-train` against a denylist of head names and by
review): anything named or used for `risk`, `confirm`, `permission`, `privacy`, `sensitive`,
`screen`, `allow`, `deny`.

## 10. Training data

### 10.1 Format

JSON Lines, one example per line:

```json
{"id":"en-0412","text":"fix the failing tests","lang":"en",
 "ctx":{"app_category":"code_editor","input_mode":"voice"},
 "labels":{"route":"brain","profile":"coding","tool_families":["code","files"]},
 "source":"seed","split":"train"}
```

- Any head label may be missing (partial labels are allowed; the loss skips missing heads).
- `source ∈ {seed, paraphrase, correction, hard_negative}`.

### 10.2 Sources

| Source | How | Review |
|---|---|---|
| **Seed** | Hand-written examples per label and language (≥ 50 per label per language to start) | Owner |
| **Paraphrase** | Generated offline by an LLM from seeds (5–20 per seed), in each language | Human review of a sample; dedupe; drop any that change the label |
| **Hard negatives** | Near-misses: "tell me a joke about downloads" (brain, not event_task), "open the code for…" (coding) | Owner |
| **Corrections** | Host-exported misroute corrections, **only with the user's explicit opt-in**, anonymized to text + labels | Owner |

### 10.3 Hygiene

- Splits are made **by seed group** (a seed and all its paraphrases share a split) so paraphrases
  cannot leak between train and test.
- Near-duplicate removal by embedding cosine ≥ 0.97 within a split.
- A frozen **golden test set** per language, never trained on, versioned separately.

## 11. Training pipeline

### 11.1 CLI

```text
reflex embed    --encoder model.onnx --data data/*.jsonl --out cache/        # cache vectors
reflex train    --config reflex.toml --cache cache/ --out build/bundle/      # fit heads
reflex calibrate --bundle build/bundle/ --split calib                        # temperatures, thresholds
reflex eval     --bundle build/bundle/ --split test --report build/report.md # gates (§14.3)
reflex pack     --bundle build/bundle/ --out dist/kivo-en-hi-v1.reflex       # single file
```

### 11.2 Fitting

- Pure Rust: softmax regression fitted with L-BFGS (full batch; datasets are small) and L2
  regularization chosen by cross-validation; the optional MLP with Adam.
- Deterministic: fixed seeds, sorted inputs, recorded in the manifest with the data hash.
- **Class balance:** inverse-frequency weights per head.

### 11.3 Context dropout

During training, each example's context is dropped entirely with probability 0.3 and each
signal independently with probability 0.2, so the heads never become dependent on context the
host may not supply.

## 12. Bundle format

A `.reflex` file is a zip (store, no compression) containing:

```text
manifest.json
weights.safetensors        # one tensor per head (+ MLP layers), f32
report.md                  # the eval report the bundle was accepted with
```

`manifest.json`:

```json
{
  "format": 1,
  "name": "kivo-en-hi",
  "version": "1.0.0",
  "encoder": { "id": "paraphrase-multilingual-MiniLM-L12-v2", "dim": 384, "sha256": "…" },
  "languages": ["en", "hi"],
  "context_schema": 1,
  "heads": [
    { "name": "route", "kind": "choice", "labels": ["event_task", "brain"],
      "arch": "linear", "temperature": 1.37,
      "policy": { "threshold": 0.85, "margin": 0.2,
                  "cost": [[0, 1], [3, 0]] } },
    { "name": "profile", "kind": "choice", "labels": ["fast", "smart", "coding"],
      "arch": "linear", "temperature": 1.52,
      "policy": { "threshold": 0.8, "margin": 0.15,
                  "cost": [[0, 1, 2], [4, 0, 2], [6, 3, 0]] } }
  ],
  "training": { "data_sha256": "…", "seed": 7, "created": "2026-10-01" },
  "metrics": { "profile": { "macro_f1": 0.0, "ece": 0.0, "coverage_at_95": 0.0 } }
}
```

Loading validates: format version, encoder id/dim/hash, tensor shapes, label counts, policy
ranges. Any failure → `Error::InvalidBundle`, and the host runs without Reflex.

## 13. Local adaptation

Users correct misroutes ("that's not what I meant" → pick the right path). Reflex can learn from
that **on the device**:

- **Model:** a per-head additive correction `Δ = Uᵀ x` with `U = A·B` a low-rank `d × k` matrix
  (rank ≤ 4, `B` initialised to zero so `Δ` starts at zero), updated by a few SGD steps per correction with strong L2 toward zero.
- **Bounds:** `‖Δ‖∞ ≤ 2.0` logits; at most 500 stored corrections, oldest evicted; exponential
  decay (half-life 90 days).
- **Only explicit corrections** train it. Implicit signals (the user didn't complain) never do,
  which limits drift and poisoning through crafted content.
- **Scope:** stored per host profile, in the host's data directory, never uploaded; one call
  resets it.
- **Guard:** if adaptation lowers accuracy on the bundled golden sample (re-checked after every
  50 corrections), the adapter is rolled back and the host is told.

## 14. Evaluation

### 14.1 Metrics (per head, per language, per context-present/absent)

- Macro-F1 and accuracy; confusion matrix.
- **Selective accuracy vs coverage** curve; coverage at the target selective accuracy.
- **Expected cost** under the head's cost matrix.
- **ECE** (15 bins) before and after calibration.
- Latency: head p50/p95; encoder p50/p95 (reported separately); memory delta.
- Streaming: time-to-stable-prewarm and prewarm precision (prewarms later confirmed by the final
  hint).

### 14.2 Baselines (every report includes them)

1. **Always default** (for `profile`: always `smart`).
2. **kNN on exemplars** (the host's semantic stage, k = 5, cosine).
3. **Reflex linear**, 4. **Reflex MLP-1**.

### 14.3 Ship gates (a bundle that fails is not packed)

| Gate | Value |
|---|---|
| Expected cost vs "always default" | ≥ 25% lower |
| Beats kNN | macro-F1 ≥ kNN + 3 points |
| Selective accuracy at the chosen threshold | ≥ 95% |
| Coverage at that threshold | ≥ 60% |
| ECE after calibration | ≤ 0.05 |
| Head latency p95 | ≤ 1 ms |
| Every language in the bundle | passes the gates on its own golden set |

## 15. Public API (Rust)

```rust
pub trait Encoder: Send + Sync {
    fn id(&self) -> &str;
    fn dim(&self) -> usize;
    fn encode(&self, text: &str) -> Result<Vec<f32>, EncodeError>;
}

pub struct Reflex { /* bundle, heads, policy, adapter */ }

impl Reflex {
    pub fn load(bundle: &Path, encoder_id: &str, encoder_dim: usize) -> Result<Self, Error>;

    /// Main entry when the host already has the embedding (the usual case).
    pub fn predict_embedded(&self, e: &[f32], ctx: &Context) -> Hints;

    /// Convenience: encode, then predict.
    pub fn predict(&self, enc: &dyn Encoder, text: &str, ctx: &Context) -> Result<Hints, Error>;

    pub fn stream(&self) -> StreamSession;
    pub fn observe_partial(&self, s: &mut StreamSession, e: &[f32], ctx: &Context)
        -> Option<StreamEvent>;                       // Prewarm(label) | CancelPrewarm(label)

    pub fn correct(&mut self, e: &[f32], ctx: &Context, head: &str, truth: Label)
        -> Result<(), Error>;                         // §13
    pub fn reset_adaptation(&mut self);
}

pub struct Context {
    pub app_category: Option<AppCategory>,
    pub input_mode: Option<InputMode>,
    pub previous_route: Option<Label>,
    pub previous_profile: Option<Label>,
    pub since_last_turn: Option<Duration>,
    pub task_running: Option<bool>,
    pub language: Option<LangTag>,
}

pub struct Hints { pub heads: BTreeMap<String, HeadResult>, pub elapsed: Duration }

pub enum HeadResult {
    Choice { label: Label, p: f32, margin: f32, reason: Reason },
    Multi  { labels: Vec<(Label, f32)>, reason: Reason },
    YesNo  { p: f32, answer: bool, reason: Reason },
    Abstain { best: Option<(Label, f32)>, why: AbstainReason },   // BelowThreshold | SmallMargin | NoBundle
}

pub struct Reason { pub short: String /* "reflex: profile=coding 0.91" */, pub top_ctx: Vec<&'static str> }
```

- `reflex-core` has **no** dependency on ONNX Runtime, tokenizers, async runtimes or BLAS. Its
  dependencies are limited to `serde`, `serde_json`, `safetensors` and `zip` (read-only).
- All functions are synchronous and allocation-light; callers run them wherever they like.
- `Reflex` is `Send + Sync` for prediction; `correct` needs `&mut`.

## 16. Integration guide (KIVO)

Where Reflex plugs into KIVO's intent router
([BRAINS.md §2](../architecture/BRAINS.md), [§5](../architecture/BRAINS.md)):

```text
transcript
  → 1. grammar match            (deterministic, µs)          ─ hit → fast-path tool
  → 2. semantic match (kNN)     (embedding e computed here)  ─ hit → fast-path tool
  → 3. Reflex(e, ctx)           (reuses e, ≤ 1 ms)           ─ route=event_task → task engine
  → 4. brain router rules       rule 2 "task class" reads the profile hint; abstain → default
  → 5. permission engine        unchanged, authoritative
  → execution
```

| Concern | KIVO rule |
|---|---|
| Authority | Hints are inputs to rules, never decisions. An explicit user choice ("use Claude") always wins. Abstain → the profile's default |
| Invariants | No effect on permissions, taint, privacy class or capture (invariants 4, 10, 11). Runs offline (12). Failure isolated: any Reflex error → skip stage 3 (9) |
| Where it runs | In the runtime process (`kivo-intent`), pure Rust. The encoder stays in `kivo-infer`; the runtime receives `e` with the semantic-stage result |
| Streaming | STT partials → `observe_partial` → `Prewarm` warms the chosen brain profile or agent (no execution); `CancelPrewarm` stops it. Cancellation follows ARCH-26 |
| Explanation | The hint's `reason.short` is recorded next to the routing rule that fired and can be shown on the Island card |
| Corrections | The "that's not what I meant" button feeds `correct()`; stored in the KIVO profile; reset from Settings |
| Measurement | `kivo-bench routing` suite runs §14 against KIVO's golden sets; stage-3 latency is part of the T-spans |
| Bundles | Shipped per language pack; downloaded and verified by the model manager like other models |
| Milestone | M3 (brain profiles). Before M3 there is no profile choice to improve |

## 17. Performance budgets

| Item | Budget | How measured |
|---|---|---|
| Head inference, all heads | ≤ 1 ms p95 | `reflex eval` + `kivo-bench routing` |
| Load a bundle | ≤ 20 ms | bench |
| Extra resident memory | ≤ 2 MB (+ adapter ≤ 1 MB) | bench |
| Bundle size | ≤ 1 MB per bundle | `reflex pack` fails above |
| Encoder (host's, reported only) | target ≤ 20 ms p95 for ≤ 32 tokens | bench |
| Streaming | ≤ 5 encodes/s | policy |

## 18. Security and privacy

- **Untrusted text:** requests may contain pasted web or document text. Reflex's output is a
  bounded label set, so injected text can at worst cause a wrong *hint*, which the host's rules
  and permission engine contain. Hints may only narrow tool exposure, never widen it.
- **No data leaves the device** at runtime. Training-data export from a host is opt-in and
  anonymized to text + labels.
- **Adaptation poisoning** is limited by explicit-only corrections, bounds, decay and the golden
  rollback guard (§13).
- **Supply chain:** bundles are hashed in the manifest and, in hosts that sign models, verified
  like any other model file.

## 19. Repository layout (standalone)

```text
reflex/
├─ Cargo.toml                    # workspace
├─ crates/
│  ├─ reflex-core/               # runtime: features, heads, calibration, policy, adapter, API
│  ├─ reflex-encode/             # Encoder impls (ONNX Runtime), feature-gated
│  ├─ reflex-train/              # `reflex` CLI: embed, train, calibrate, eval, pack
│  └─ reflex-eval/               # metrics, reports, baselines (used by the CLI and hosts' benches)
├─ data/
│  ├─ schema/                    # JSON Schemas for examples and manifests
│  ├─ seeds/<lang>/*.jsonl
│  ├─ golden/<lang>/*.jsonl      # frozen test sets
│  └─ configs/kivo.toml          # heads, labels, policies for the KIVO bundle
├─ examples/
│  ├─ route_cli.rs               # type a request, see hints
│  └─ stream_demo.rs             # feed partials, see prewarm events
├─ docs/REFLEX.md                # this document
├─ benches/                      # criterion benches for head latency
└─ .github/workflows/            # per the owner's CI rules
```

## 20. Roadmap and build checklist

Phases are local to this project (R0–R4). KIVO consumes a bundle from R2 on, at its M3.

- [ ] **RFX-01** · R0 · Workspace, `reflex-core` types (`Context`, `Hints`, `HeadResult`, `Reason`) and the `Encoder` trait (§15)
- [ ] **RFX-02** · R0 · Bundle format: manifest schema, safetensors weights, loader with full validation (§12)
- [ ] **RFX-03** · R0 · `OrtEncoder` for one MiniLM-class model with tokenizer, mean pooling and L2 norm (§5.1)
- [ ] **RFX-04** · R1 · Data schema, seed sets for `route` and `profile` in English and Hindi, golden sets, group-aware splits (§10)
- [ ] **RFX-05** · R1 · `reflex embed` and `reflex train` with linear heads (L-BFGS, class weights, deterministic) (§11)
- [ ] **RFX-06** · R1 · `reflex eval` with all §14.1 metrics and the §14.2 baselines, writing `report.md` (§14)
- [ ] **RFX-07** · R2 · Temperature calibration, thresholds, margins and cost-matrix policy; abstention (§5.4, §5.5)
- [ ] **RFX-08** · R2 · Ship gates enforced by `reflex pack` (§14.3)
- [ ] **RFX-09** · R2 · Streaming session with stability and change rules, `Prewarm` / `CancelPrewarm` (§6)
- [ ] **RFX-10** · R2 · Head latency and load-time benches meeting §17
- [ ] **RFX-11** · R3 · Context features with dropout training; context-present/absent metrics (§7, §11.3)
- [ ] **RFX-12** · R3 · Local adaptation with bounds, decay, reset and golden rollback guard (§13)
- [ ] **RFX-13** · R3 · Optional MLP-1 heads, adopted only when they beat linear by the §5.3 margin
- [ ] **RFX-14** · R3 · Forbidden-head lint in `reflex-train` (§9)
- [ ] **RFX-15** · R4 · `tool_families` (multi) and `needs_web` (yes/no) heads (§9)
- [ ] **RFX-16** · R4 · Paraphrase generation tooling with review sampling (§10.2)
- [ ] **RFX-17** · R4 · Multilingual encoder benchmark and final encoder choice (§5.1)

## 21. Open questions (owner)

1. **Name.** "Reflex" is already a well-known Python web framework (reflex.dev), and the `reflex`
   crate name may be taken on crates.io. Options: keep "Reflex" as the product name inside KIVO and
   publish as `kivo-reflex`; or rename (e.g. "Twitch", "Snap", "Spark").
2. **License** for the standalone repo (KIVO's licensing is covered by
   [DISTRIBUTION.md](../architecture/DISTRIBUTION.md)).
3. **Repo home:** personal account `MadBlast0`, public or private.
4. **Paraphrase generator:** which LLM, and whether generated data may be published with the repo.
5. **Encoder:** confirm the multilingual choice after RFX-17 (it is shared with KIVO's memory
   search, MEM-09).

## 22. Sources

Checked 2026-09-23.

- [convaiinnovations/laya on Hugging Face](https://huggingface.co/convaiinnovations/laya)
- [NandhaKishorM/laya on GitHub](https://github.com/NandhaKishorM/laya)
- [MarkTechPost: TypeSafe AI releases Jev (2026-09-19)](https://www.marktechpost.com/2026/09/19/typesafe-ai-releases-jev/)
- [DataCamp: Jev, TypeSafe's System One model](https://www.datacamp.com/blog/system-one-models-jev)
- Calibration by temperature scaling: Guo et al., *On Calibration of Modern Neural Networks*, ICML 2017.
