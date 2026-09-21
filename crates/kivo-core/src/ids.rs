//! Identifiers. UUID v7 (time-ordered), so they sort by creation time in logs and the database.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// A new, unique, time-ordered id.
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "({})"), self.0)
            }
        }
    };
}

id_type!(
    /// One user utterance or typed request, from start to response (ARCHITECTURE §4.2).
    TurnId
);
id_type!(
    /// Long-running work that can outlive a turn (ARCHITECTURE §4.2).
    TaskId
);
id_type!(
    /// Correlates everything caused by one trigger across processes and logs.
    TraceId
);
id_type!(
    /// A person using KIVO. Everything user-owned is scoped by it from M0 (UX §8.2).
    ProfileId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_time_ordered() {
        let a = TurnId::new();
        let b = TurnId::new();
        assert_ne!(a, b);
        assert!(a < b, "v7 ids sort by creation time");
    }

    #[test]
    fn ids_serialize_as_plain_uuid_strings() {
        let id = TaskId::new();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{id}\""));
        let back: TaskId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }
}
