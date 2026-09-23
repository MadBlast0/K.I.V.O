//! Fast-path command grammar and intent routing (BRAINS §2): the grammar stage matches slot-based
//! commands from per-language data files against live app and window indexes, so common commands
//! run without any AI.

pub mod answers;
pub mod grammar;
pub mod index;
pub mod normalize;
pub mod router;
pub mod semantic;
pub mod stop;
pub mod wake;

pub use grammar::{Context, Grammar, GrammarError, Match};
pub use index::{Index, IndexEntry, Resolved};
pub use router::{Decision, IntentRouter, Route, RouterMetrics, Stage};
pub use semantic::{Embedder, Semantic};
pub use stop::is_stop_request;
pub use wake::strip_wake_phrase;
