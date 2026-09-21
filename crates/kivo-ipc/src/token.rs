//! The session token (ARCHITECTURE §3): 256 random bits the runtime writes to a file only the
//! current user can read. Clients must present it in `hello`, so another program that can reach
//! the pipe still can't talk to KIVO without being able to read the user's files.

use std::fmt;
use std::path::Path;

pub struct SessionToken(String);

impl SessionToken {
    /// A fresh random token.
    pub fn generate() -> std::io::Result<Self> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(Self(bytes.iter().map(|b| format!("{b:02x}")).collect()))
    }

    /// Writes the token to `path`, readable only by the current user.
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Create it empty, lock it down, then write, so the secret never sits in a readable file.
        std::fs::write(path, "")?;
        crate::transport::restrict_file_to_user(path)?;
        std::fs::write(path, &self.0)
    }

    pub fn read(path: &Path) -> std::io::Result<Self> {
        Ok(Self(std::fs::read_to_string(path)?.trim().to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Compares in constant time, so response timing reveals nothing about the token.
    pub fn matches(&self, presented: &str) -> bool {
        let (a, b) = (self.0.as_bytes(), presented.as_bytes());
        a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
    }
}

impl fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionToken(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_256_bit_hex_and_unique() {
        let a = SessionToken::generate().unwrap();
        let b = SessionToken::generate().unwrap();
        assert_eq!(a.as_str().len(), 64);
        assert!(a.as_str().bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a.as_str(), b.as_str());
        assert_eq!(format!("{a:?}"), "SessionToken(***)");
    }

    #[test]
    fn matches_only_the_exact_token() {
        let t = SessionToken::generate().unwrap();
        assert!(t.matches(t.as_str()));
        assert!(!t.matches(&t.as_str()[..63]));
        assert!(!t.matches(""));
        let mut wrong = t.as_str().to_owned();
        wrong.replace_range(0..1, if wrong.starts_with('a') { "b" } else { "a" });
        assert!(!t.matches(&wrong));
    }

    #[test]
    fn round_trips_through_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run").join("session.token");
        let t = SessionToken::generate().unwrap();
        t.write(&path).unwrap();
        assert!(SessionToken::read(&path).unwrap().matches(t.as_str()));
    }
}
