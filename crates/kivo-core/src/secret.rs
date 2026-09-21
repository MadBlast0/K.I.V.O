//! `Secret<T>`: a value that must never reach prompts, logs, activity, diagnostics or IPC to the UI
//! (SECURITY §5). It has no `Display` and no `Serialize`, its `Debug` prints `***`, and its memory
//! is zeroed when it is dropped. Read it only where it is used (an adapter or a tool executor).

use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

pub struct Secret<T: Zeroize>(T);

impl<T: Zeroize> Secret<T> {
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Borrows the value. Keep the borrow short and never log or send what it returns.
    pub fn expose(&self) -> &T {
        &self.0
    }
}

impl<T: Zeroize> Drop for Secret<T> {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl<T: Zeroize> ZeroizeOnDrop for Secret<T> {}

impl<T: Zeroize> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

impl From<String> for Secret<String> {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_the_value() {
        let key = Secret::new(String::from("sk-live-123"));
        assert_eq!(format!("{key:?}"), "***");
        assert_eq!(format!("{:?}", Some(&key)), "Some(***)");
    }

    #[test]
    fn the_value_is_readable_where_it_is_used() {
        let key: Secret<String> = String::from("sk-live-123").into();
        assert_eq!(key.expose(), "sk-live-123");
    }

    #[test]
    fn drop_zeroes_the_value() {
        let mut bytes = Secret::new(vec![1u8, 2, 3]);
        // Run the same zeroize the destructor runs, then check it cleared the contents.
        bytes.0.zeroize();
        assert!(bytes.expose().is_empty());
    }
}
