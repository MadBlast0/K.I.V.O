//! Named pipes (ARCHITECTURE §3, SECURITY §9): `\\.\pipe\kivo-<user-sid>`, a DACL that grants
//! only the current user, remote clients rejected, and "first instance" creation so another
//! process can't squat the name before the runtime starts.

use std::ffi::c_void;
use std::io;
use std::path::Path;
use std::time::Duration;
use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};
use windows::Win32::Foundation::{CloseHandle, ERROR_PIPE_BUSY, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{
    DACL_SECURITY_INFORMATION, GetTokenInformation, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, SetFileSecurityW, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::{HSTRING, PWSTR};

pub type ServerStream = NamedPipeServer;
pub type ClientStream = NamedPipeClient;

/// The runtime's pipe for the current user (`run_dir` is only used on Unix).
pub fn endpoint(_run_dir: &Path) -> io::Result<String> {
    Ok(format!(r"\\.\pipe\kivo-{}", current_user_sid()?))
}

/// The current user's SID as a string, e.g. `S-1-5-21-…`.
pub fn current_user_sid() -> io::Result<String> {
    // SAFETY: standard token query; the handle is closed below and the SID string is freed.
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token)
            .map_err(io::Error::other)?;
        let mut len = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &raw mut len);
        let mut buf = vec![0u8; len as usize];
        let got = GetTokenInformation(
            token,
            TokenUser,
            Some(buf.as_mut_ptr().cast()),
            len,
            &raw mut len,
        );
        let _ = CloseHandle(token);
        got.map_err(io::Error::other)?;
        let user = &*buf.as_ptr().cast::<TOKEN_USER>();
        let mut text = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &raw mut text).map_err(io::Error::other)?;
        let sid = text.to_string().map_err(io::Error::other);
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
        sid
    }
}

/// A security descriptor granting full access to the current user only (protected: no
/// inherited entries, so Administrators and SYSTEM are not added implicitly).
struct UserOnly {
    descriptor: PSECURITY_DESCRIPTOR,
}

impl UserOnly {
    fn new() -> io::Result<Self> {
        let sddl = HSTRING::from(format!("D:P(A;;GA;;;{})", current_user_sid()?));
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: the SDDL string is valid for the call; the result is freed in Drop.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                &sddl,
                SDDL_REVISION_1,
                &raw mut descriptor,
                None,
            )
            .map_err(io::Error::other)?;
        }
        Ok(Self { descriptor })
    }
}

impl Drop for UserOnly {
    fn drop(&mut self) {
        // SAFETY: allocated by ConvertStringSecurityDescriptorToSecurityDescriptorW (LocalAlloc).
        unsafe {
            let _ = LocalFree(Some(HLOCAL(self.descriptor.0)));
        }
    }
}

/// Creates a pipe instance. `first` must be true for the first one: creation then fails if the
/// name already exists, instead of silently joining someone else's pipe.
pub fn create_server(name: &str, first: bool) -> io::Result<ServerStream> {
    let sd = UserOnly::new()?;
    let mut attrs = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).unwrap_or(0),
        lpSecurityDescriptor: sd.descriptor.0,
        bInheritHandle: false.into(),
    };
    let mut options = ServerOptions::new();
    options
        .first_pipe_instance(first)
        .reject_remote_clients(true);
    // SAFETY: `attrs` and the descriptor it points to outlive the call.
    unsafe { options.create_with_security_attributes_raw(name, (&raw mut attrs).cast::<c_void>()) }
}

/// Opens a client connection, waiting briefly while all pipe instances are busy.
pub async fn connect(name: &str) -> io::Result<ClientStream> {
    loop {
        match ClientOptions::new().open(name) {
            Ok(client) => return Ok(client),
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY.0.cast_signed()) => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Makes a file readable and writable by the current user only (for the session token).
pub fn restrict_file_to_user(path: &Path) -> io::Result<()> {
    let sd = UserOnly::new()?;
    // SAFETY: the path string and descriptor are valid for the call.
    unsafe {
        SetFileSecurityW(
            &HSTRING::from(path),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            sd.descriptor,
        )
        .ok()
        .map_err(io::Error::other)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetSecurityInfo, SE_FILE_OBJECT,
        SE_KERNEL_OBJECT, SE_OBJECT_TYPE,
    };
    use windows::Win32::Security::{ACL, PSID};

    /// The DACL of a handle's object, as SDDL.
    fn dacl_of(handle: HANDLE, kind: SE_OBJECT_TYPE) -> String {
        // SAFETY: querying the DACL of a live handle; buffers are freed with LocalFree.
        unsafe {
            let mut sd = PSECURITY_DESCRIPTOR::default();
            let mut dacl: *mut ACL = std::ptr::null_mut();
            let status = GetSecurityInfo(
                handle,
                kind,
                DACL_SECURITY_INFORMATION,
                Some(std::ptr::null_mut::<PSID>()),
                Some(std::ptr::null_mut::<PSID>()),
                Some(&raw mut dacl),
                None,
                Some(&raw mut sd),
            );
            assert!(status.is_ok(), "GetSecurityInfo: {status:?}");
            let mut text = PWSTR::null();
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                sd,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &raw mut text,
                None,
            )
            .unwrap();
            let s = text.to_string().unwrap();
            let _ = LocalFree(Some(HLOCAL(text.0.cast())));
            let _ = LocalFree(Some(HLOCAL(sd.0)));
            s
        }
    }

    /// `D:P(A;;FA;;;<sid>)` as Windows prints it: well-known SIDs become aliases (the built-in
    /// Administrator's SID prints as `LA`, as on CI runners).
    fn expected_user_only_dacl() -> String {
        let sd = UserOnly::new().unwrap();
        // SAFETY: converting a valid descriptor to text; the buffer is freed with LocalFree.
        unsafe {
            let mut text = PWSTR::null();
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                sd.descriptor,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &raw mut text,
                None,
            )
            .unwrap();
            let s = text.to_string().unwrap();
            let _ = LocalFree(Some(HLOCAL(text.0.cast())));
            s.replace(";GA;", ";FA;") // generic all is stored as file-all on pipes and files
        }
    }

    pub(crate) fn unique_endpoint() -> String {
        format!(r"\\.\pipe\kivo-test-{}", kivo_core::TraceId::new())
    }

    #[test]
    fn sid_looks_like_a_user_sid() {
        let sid = current_user_sid().unwrap();
        assert!(sid.starts_with("S-1-5-"), "{sid}");
        assert!(endpoint(Path::new("unused")).unwrap().ends_with(&sid));
    }

    #[tokio::test]
    async fn the_pipe_grants_only_the_current_user() {
        let pipe = create_server(&unique_endpoint(), true).unwrap();
        let sddl = dacl_of(HANDLE(pipe.as_raw_handle()), SE_KERNEL_OBJECT);
        // Protected DACL with exactly one allow entry (full access) for this user.
        assert_eq!(sddl, expected_user_only_dacl());
    }

    #[tokio::test]
    async fn a_second_first_instance_is_refused() {
        let name = unique_endpoint();
        let _first = create_server(&name, true).unwrap();
        assert!(
            create_server(&name, true).is_err(),
            "squatting on an existing name must fail"
        );
        assert!(
            create_server(&name, false).is_ok(),
            "further instances of our own pipe are fine"
        );
    }

    #[test]
    fn restricted_files_grant_only_the_current_user() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret.txt");
        std::fs::write(&path, "x").unwrap();
        restrict_file_to_user(&path).unwrap();
        let file = std::fs::File::open(&path).unwrap();
        let sddl = dacl_of(HANDLE(file.as_raw_handle()), SE_FILE_OBJECT);
        assert_eq!(sddl, expected_user_only_dacl());
    }
}
