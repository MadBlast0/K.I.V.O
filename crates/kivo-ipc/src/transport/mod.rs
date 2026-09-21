//! The local transport: named pipes on Windows, Unix-domain sockets elsewhere. Both only accept
//! the current user; the session token is checked on top (ARCHITECTURE §3).

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use self::unix::*;

#[cfg(test)]
pub(crate) use self::tests_support::unique_endpoint;

#[cfg(test)]
mod tests_support {
    #[cfg(unix)]
    pub(crate) use super::unix::tests::unique_endpoint;
    #[cfg(windows)]
    pub(crate) use super::windows::tests::unique_endpoint;
}
