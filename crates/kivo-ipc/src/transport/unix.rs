//! Unix-domain sockets for the macOS and Linux ports: the socket lives in the user's private run
//! folder and is chmod 600, the Unix equivalent of the Windows pipe's user-only DACL.

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tokio::net::{UnixListener, UnixStream};

pub type ServerStream = UnixStream;
pub type ClientStream = UnixStream;

/// The runtime's socket, inside the user's private run folder.
pub fn endpoint(run_dir: &Path) -> io::Result<String> {
    Ok(run_dir.join("kivo.sock").display().to_string())
}

/// Binds the socket, replacing a stale file left by a crashed runtime.
pub fn bind(path: &str) -> io::Result<UnixListener> {
    let p = Path::new(path);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    if p.exists() && std::os::unix::net::UnixStream::connect(p).is_err() {
        std::fs::remove_file(p)?;
    }
    let listener = UnixListener::bind(p)?;
    restrict_file_to_user(p)?;
    Ok(listener)
}

pub async fn connect(path: &str) -> io::Result<ClientStream> {
    UnixStream::connect(path).await
}

pub fn restrict_file_to_user(path: &Path) -> io::Result<()> {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn unique_endpoint() -> String {
        std::env::temp_dir()
            .join(format!("kivo-test-{}", kivo_core::TraceId::new()))
            .join("kivo.sock")
            .display()
            .to_string()
    }

    #[tokio::test]
    async fn the_socket_is_private_to_the_user() {
        let path = unique_endpoint();
        let _listener = bind(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
