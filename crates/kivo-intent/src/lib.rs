//! Fast-path command grammar and intent routing (BRAINS §2): the grammar stage matches slot-based
//! commands from per-language data files against live app and window indexes, so common commands
//! run without any AI.

pub mod grammar;
pub mod index;
pub mod normalize;
pub mod router;

pub use grammar::{Context, Grammar, GrammarError, Match};
pub use index::{Index, IndexEntry, Resolved};
pub use router::{Decision, IntentRouter, Route, RouterMetrics, Stage};
