//! System controls on Windows (TOOLS_AND_CONTROL §3, §5; TOOL-09/11/12/23):
//!
//! - **Volume and mute** through the default endpoints' `IAudioEndpointVolume` (speaker and mic).
//! - **Media** through the system media transport controls (the same session the volume flyout
//!   shows), so it works with Spotify, browsers and any app that publishes its playback.
//! - **Power**: lock, sleep, and restart/shutdown with a short grace period (the permission engine
//!   has always confirmed these first).
//! - **Links** open in the default browser through the shell; only http(s) is accepted.

use crate::com::{Com, os_error};
use kivo_platform::{
    MediaAction, NowPlaying, PlatformError, PlatformResult, PowerAction, SystemControl, VolumeState,
};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionManager as MediaManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};
use windows::Win32::Foundation::{CloseHandle, HANDLE, LUID};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    EDataFlow, IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eConsole, eRender,
};
use windows::Win32::Security::{
    AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED,
    SE_SHUTDOWN_NAME, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::Win32::System::Power::SetSuspendState;
use windows::Win32::System::Shutdown::{
    InitiateShutdownW, LockWorkStation, SHTDN_REASON_FLAG_PLANNED, SHTDN_REASON_MAJOR_OTHER,
    SHUTDOWN_FORCE_OTHERS, SHUTDOWN_POWEROFF, SHUTDOWN_RESTART,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR, w};

/// Seconds Windows waits before a restart or shutdown (apps get to save).
const SHUTDOWN_GRACE: u32 = 5;

fn endpoint(flow: EDataFlow) -> PlatformResult<IAudioEndpointVolume> {
    // SAFETY: COM is initialized by the caller; these are plain COM calls.
    unsafe {
        let devices: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| os_error(&e))?;
        let device = devices
            .GetDefaultAudioEndpoint(flow, eConsole)
            .map_err(|_| {
                PlatformError::NotFound(
                    if flow == eRender {
                        "a speaker"
                    } else {
                        "a microphone"
                    }
                    .into(),
                )
            })?;
        device.Activate(CLSCTX_ALL, None).map_err(|e| os_error(&e))
    }
}

fn read(flow: EDataFlow) -> PlatformResult<VolumeState> {
    let _com = Com::init()?;
    let volume = endpoint(flow)?;
    // SAFETY: plain COM getters.
    unsafe {
        Ok(VolumeState {
            level: volume
                .GetMasterVolumeLevelScalar()
                .map_err(|e| os_error(&e))?,
            muted: volume.GetMute().map_err(|e| os_error(&e))?.as_bool(),
        })
    }
}

fn set_mute(flow: EDataFlow, muted: bool) -> PlatformResult<()> {
    let _com = Com::init()?;
    // SAFETY: a null event context is allowed.
    unsafe { endpoint(flow)?.SetMute(muted, std::ptr::null()) }.map_err(|e| os_error(&e))
}

/// The session the system media flyout shows (the app playing now).
fn media_session()
-> PlatformResult<windows::Media::Control::GlobalSystemMediaTransportControlsSession> {
    let manager = MediaManager::RequestAsync()
        .and_then(|op| op.join())
        .map_err(|e| os_error(&e))?;
    manager
        .GetCurrentSession()
        .map_err(|_| PlatformError::NotFound("anything playing".into()))
}

/// Grants this process the shutdown privilege (required by `InitiateShutdownW`).
fn enable_shutdown_privilege() -> PlatformResult<()> {
    let mut token = HANDLE::default();
    // SAFETY: the token handle is closed below; the privilege struct is fully initialized.
    unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )
        .map_err(|e| os_error(&e))?;
        let mut luid = LUID::default();
        let result =
            LookupPrivilegeValueW(PCWSTR::null(), SE_SHUTDOWN_NAME, &mut luid).and_then(|()| {
                let privileges = TOKEN_PRIVILEGES {
                    PrivilegeCount: 1,
                    Privileges: [LUID_AND_ATTRIBUTES {
                        Luid: luid,
                        Attributes: SE_PRIVILEGE_ENABLED,
                    }],
                };
                AdjustTokenPrivileges(token, false, Some(&raw const privileges), 0, None, None)
            });
        let _ = CloseHandle(token);
        result.map_err(|e| os_error(&e))
    }
}

/// Opens an app URI (`spotify:search:jazz`, `ms-settings:bluetooth`) with its registered app.
/// Only a plain `scheme:` URI: never a path, a file or script URL, or a web address (those have
/// their own, checked ways in).
pub fn open_uri(uri: &str) -> PlatformResult<()> {
    let Some((scheme, rest)) = uri.split_once(':') else {
        return Err(PlatformError::NotFound("that link".into()));
    };
    let scheme = scheme.to_ascii_lowercase();
    let plain = scheme.len() > 1
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic());
    let refused = matches!(
        scheme.as_str(),
        "file"
            | "javascript"
            | "vbscript"
            | "data"
            | "http"
            | "https"
            | "ms-msdt"
            | "search-ms"
            | "search"
            | "ms-officecmd"
    );
    if !plain || refused || rest.chars().any(|c| c.is_control() || c == '"') {
        return Err(PlatformError::AccessDenied);
    }
    shell_open(uri)
}

/// Opens a document, folder or URI with its default handler.
/// Opens a document KIVO ships (the third-party notices) in its usual app. Only an existing
/// `.txt`, `.md` or `.pdf` file: never a program, a folder or a link.
pub fn open_document(path: &std::path::Path) -> PlatformResult<()> {
    let readable = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ["txt", "md", "pdf"].contains(&e.to_ascii_lowercase().as_str()));
    if !readable {
        return Err(PlatformError::Unsupported);
    }
    if !path.is_file() {
        return Err(PlatformError::NotFound("that document".into()));
    }
    shell_open(&path.to_string_lossy())
}

fn shell_open(target: &str) -> PlatformResult<()> {
    let _com = Com::init()?;
    let target = HSTRING::from(target);
    // SAFETY: plain shell call; results above 32 mean success.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &target,
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err(PlatformError::Os {
            code: result.0 as i64,
            message: "ShellExecuteW".into(),
        })
    }
}

/// Only web addresses open in the browser; anything else could launch a program.
fn is_web_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://"))
        && url.len() > "https://".len()
        && !url.chars().any(|c| c.is_whitespace() || c == '"')
}

#[derive(Default)]
pub struct WindowsControl;

impl SystemControl for WindowsControl {
    fn volume(&self) -> PlatformResult<VolumeState> {
        read(eRender)
    }

    fn set_volume(&self, level: f32) -> PlatformResult<()> {
        let _com = Com::init()?;
        let volume = endpoint(eRender)?;
        // SAFETY: plain COM setters; unmute so the new level is heard.
        unsafe {
            volume
                .SetMasterVolumeLevelScalar(level.clamp(0.0, 1.0), std::ptr::null())
                .map_err(|e| os_error(&e))?;
            volume
                .SetMute(false, std::ptr::null())
                .map_err(|e| os_error(&e))
        }
    }

    fn set_muted(&self, muted: bool) -> PlatformResult<()> {
        set_mute(eRender, muted)
    }

    fn microphone(&self) -> PlatformResult<VolumeState> {
        read(eCapture)
    }

    fn set_mic_muted(&self, muted: bool) -> PlatformResult<()> {
        set_mute(eCapture, muted)
    }

    fn media(&self, action: MediaAction) -> PlatformResult<()> {
        let session = media_session()?;
        let op = match action {
            MediaAction::PlayPause => session.TryTogglePlayPauseAsync(),
            MediaAction::Next => session.TrySkipNextAsync(),
            MediaAction::Previous => session.TrySkipPreviousAsync(),
        };
        let done = op.and_then(|op| op.join()).map_err(|e| os_error(&e))?;
        if done {
            Ok(())
        } else {
            Err(PlatformError::Unsupported)
        }
    }

    fn now_playing(&self) -> PlatformResult<Option<NowPlaying>> {
        let Ok(session) = media_session() else {
            return Ok(None);
        };
        let props = session
            .TryGetMediaPropertiesAsync()
            .and_then(|op| op.join())
            .map_err(|e| os_error(&e))?;
        let playing = session
            .GetPlaybackInfo()
            .and_then(|i| i.PlaybackStatus())
            .is_ok_and(|s| s == PlaybackStatus::Playing);
        Ok(Some(NowPlaying {
            title: props.Title().map(|t| t.to_string()).unwrap_or_default(),
            artist: props.Artist().map(|a| a.to_string()).unwrap_or_default(),
            app: session
                .SourceAppUserModelId()
                .map(|a| a.to_string())
                .unwrap_or_default(),
            playing,
        }))
    }

    fn power(&self, action: PowerAction) -> PlatformResult<()> {
        // SAFETY: documented power calls; restart/shutdown give apps a grace period to save.
        unsafe {
            match action {
                PowerAction::Lock => LockWorkStation().map_err(|e| os_error(&e)),
                PowerAction::Sleep => {
                    if SetSuspendState(false, false, false) {
                        Ok(())
                    } else {
                        Err(PlatformError::AccessDenied)
                    }
                }
                PowerAction::Restart | PowerAction::Shutdown => {
                    enable_shutdown_privilege()?;
                    let flags = if action == PowerAction::Restart {
                        SHUTDOWN_RESTART
                    } else {
                        SHUTDOWN_POWEROFF
                    };
                    let code = InitiateShutdownW(
                        PCWSTR::null(),
                        w!("KIVO: shutting down as you asked."),
                        SHUTDOWN_GRACE,
                        flags | SHUTDOWN_FORCE_OTHERS,
                        SHTDN_REASON_MAJOR_OTHER | SHTDN_REASON_FLAG_PLANNED,
                    );
                    if code == 0 {
                        Ok(())
                    } else {
                        Err(PlatformError::Os {
                            code: i64::from(code),
                            message: "InitiateShutdownW failed".into(),
                        })
                    }
                }
            }
        }
    }

    fn open_system_settings(&self, page: &str) -> PlatformResult<()> {
        // Only settings pages: a page name is letters, digits and dashes.
        if page.is_empty() || !page.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(PlatformError::Unsupported);
        }
        shell_open(&format!("ms-settings:{page}"))
    }

    fn reveal(&self, folder: &std::path::Path) -> PlatformResult<()> {
        if !folder.is_dir() {
            return Err(PlatformError::NotFound("that folder".into()));
        }
        shell_open(&folder.to_string_lossy())
    }

    fn open_url(&self, url: &str) -> PlatformResult<()> {
        if !is_web_url(url) {
            return Err(PlatformError::Unsupported);
        }
        shell_open(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_shipped_documents_open() {
        // Refusals only: opening one would start an app on the desktop.
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("setup.exe");
        std::fs::write(&program, b"MZ").unwrap();
        assert!(matches!(
            open_document(&program),
            Err(PlatformError::Unsupported)
        ));
        assert!(matches!(
            open_document(dir.path()),
            Err(PlatformError::Unsupported)
        ));
        assert!(matches!(
            open_document(&dir.path().join("THIRD_PARTY_NOTICES.txt")),
            Err(PlatformError::NotFound(_))
        ));
    }

    #[test]
    fn settings_pages_and_folders_are_checked_before_opening() {
        let control = WindowsControl;
        assert_eq!(
            control.open_system_settings("privacy mic&calc"),
            Err(PlatformError::Unsupported)
        );
        assert_eq!(
            control.open_system_settings(""),
            Err(PlatformError::Unsupported)
        );
        assert!(matches!(
            control.reveal(std::path::Path::new(r"Z:\no\such\folder")),
            Err(PlatformError::NotFound(_))
        ));
    }

    #[test]
    fn only_web_addresses_are_opened() {
        assert!(is_web_url("https://youtube.com"));
        assert!(is_web_url("http://localhost:1420/x"));
        assert!(!is_web_url("file:///C:/Windows/System32/calc.exe"));
        assert!(!is_web_url("C:\\Windows\\notepad.exe"));
        assert!(!is_web_url("https://a.com\" --evil"));
        assert!(!is_web_url("https://"));
    }

    #[test]
    fn the_speaker_and_microphone_levels_are_readable() {
        // Read only: tests never change the user's volume.
        let control = WindowsControl;
        let speaker = control.volume().unwrap();
        assert!((0.0..=1.0).contains(&speaker.level));
        if let Ok(mic) = control.microphone() {
            assert!((0.0..=1.0).contains(&mic.level));
        }
        // With nothing playing this is None; either way it must not fail.
        control.now_playing().unwrap();
    }
}
