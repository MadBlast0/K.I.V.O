//! Apps and windows on the Windows desktop (TOOLS_AND_CONTROL §3, TOOL-06/07).
//!
//! - **Apps** come from the shell's `AppsFolder`: every app the Start menu shows, desktop and
//!   packaged alike, each with its AUMID or path as its id. Launching goes through
//!   `shell:AppsFolder\<id>`, which starts either kind correctly.
//! - **Windows** are visible, uncloaked top-level windows with a title (DWM hides cloaked ones on
//!   other virtual desktops). A window's app is its AUMID property or its process's AUMID, else
//!   its program file.
//! - **Focus** follows the foreground-lock rules: the runtime may bring a window forward because
//!   the user's hotkey was the last input it received; if Windows still refuses, the input queues
//!   are attached for the switch.

use crate::com::{Com, os_error};
use kivo_platform::{
    AppEntry, Apps, PlatformError, PlatformResult, Rect, WindowId, WindowInfo, Windows,
};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::Storage::EnhancedStorage::{PKEY_AppUserModel_ID, PKEY_Link_TargetParsingPath};
use windows::Win32::Storage::Packaging::Appx::GetApplicationUserModelId;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentThreadId, OpenProcess, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::Shell::{
    BHID_EnumItems, FOLDERID_AppsFolder, IEnumShellItems, IShellItem, IShellItem2, KF_FLAG_DEFAULT,
    SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, SHGetKnownFolderItem, SIGDN_NORMALDISPLAY,
    SIGDN_PARENTRELATIVEPARSING, ShellExecuteExW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    ASFW_ANY, AllowSetForegroundWindow, EnumWindows, GW_OWNER, GWL_EXSTYLE, GetForegroundWindow,
    GetWindow, GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, PostMessageW, SW_MAXIMIZE, SW_MINIMIZE,
    SW_RESTORE, SW_SHOWNORMAL, SetForegroundWindow, ShowWindow, WM_CLOSE, WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, HSTRING, PCWSTR, PWSTR, w};

/// Names people use for common apps (TOOLS_AND_CONTROL §3: fuzzy match with aliases).
const ALIASES: &[(&str, &[&str])] = &[
    ("Visual Studio Code", &["VS Code", "code"]),
    ("Microsoft Edge", &["Edge"]),
    ("File Explorer", &["Explorer", "Files", "my files"]),
    ("Command Prompt", &["cmd", "command line"]),
    ("Windows PowerShell", &["PowerShell"]),
    ("Windows Terminal", &["Terminal"]),
    ("Terminal", &["Windows Terminal"]),
    ("Settings", &["Windows settings", "PC settings"]),
    ("Calculator", &["calc"]),
    ("Microsoft Teams", &["Teams"]),
    ("Microsoft Store", &["Store", "App Store"]),
    ("Snipping Tool", &["snip"]),
    ("Task Manager", &["tasks"]),
];

/// App index entries that aren't apps: uninstallers, readmes and web links.
fn is_noise(name: &str, parsing: &str) -> bool {
    let lower = parsing.to_lowercase();
    name.to_lowercase().starts_with("uninstall")
        || [
            ".url", ".txt", ".pdf", ".chm", ".html", ".htm", ".rtf", ".ini",
        ]
        .iter()
        .any(|ext| lower.ends_with(ext))
        || lower.starts_with("http:")
        || lower.starts_with("https:")
}

fn pwstr_to_string(p: PWSTR) -> String {
    // SAFETY: `p` is a NUL-terminated string returned by the shell; it is freed right after.
    let s = unsafe { p.to_string() }.unwrap_or_default();
    // SAFETY: the shell allocated it with CoTaskMemAlloc.
    unsafe { CoTaskMemFree(Some(p.0.cast())) };
    s
}

/// The Start menu's apps.
fn enumerate_apps() -> PlatformResult<Vec<AppEntry>> {
    let _com = Com::init()?;
    // SAFETY: plain COM calls on an initialized apartment; out-values are owned wrappers.
    let folder: IShellItem =
        unsafe { SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None) }
            .map_err(|e| os_error(&e))?;
    let items: IEnumShellItems =
        unsafe { folder.BindToHandler(None, &BHID_EnumItems) }.map_err(|e| os_error(&e))?;
    let mut apps = Vec::new();
    loop {
        let mut batch = [None];
        let mut fetched = 0;
        // SAFETY: `batch` holds one slot and `fetched` receives the count.
        if unsafe { items.Next(&mut batch, Some(&mut fetched)) }.is_err() || fetched == 0 {
            break;
        }
        let Some(item) = batch[0].take() else { break };
        // SAFETY: display names are CoTaskMemAlloc'd and freed in `pwstr_to_string`.
        let (Ok(name), Ok(parsing)) = (
            unsafe { item.GetDisplayName(SIGDN_NORMALDISPLAY) },
            unsafe { item.GetDisplayName(SIGDN_PARENTRELATIVEPARSING) },
        ) else {
            continue;
        };
        let (name, id) = (pwstr_to_string(name), pwstr_to_string(parsing));
        if name.is_empty() || is_noise(&name, &id) {
            continue;
        }
        let exe = windows::core::Interface::cast::<IShellItem2>(&item)
            .ok()
            // SAFETY: the returned string is CoTaskMemAlloc'd and freed in `pwstr_to_string`.
            .and_then(|i2| unsafe { i2.GetString(&PKEY_Link_TargetParsingPath) }.ok())
            .map(pwstr_to_string)
            .filter(|p| p.to_lowercase().ends_with(".exe"));
        let aliases = ALIASES
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(&name))
            .flat_map(|(_, a)| a.iter().map(|s| (*s).to_owned()))
            .collect();
        apps.push(AppEntry {
            id,
            name,
            aliases,
            exe,
        });
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps.dedup_by(|a, b| a.id == b.id);
    Ok(apps)
}

/// A window's app: its explicit AUMID, the process's AUMID (packaged apps), or its program file.
fn window_app(hwnd: HWND) -> String {
    // SAFETY: reading a property store for a window we don't own is allowed; failures are fine.
    if let Ok(store) = unsafe { SHGetPropertyStoreForWindow::<IPropertyStore>(hwnd) }
        && let Ok(value) = unsafe { store.GetValue(&PKEY_AppUserModel_ID) }
    {
        let id = value.to_string();
        if !id.is_empty() {
            return id;
        }
    }
    let mut pid = 0;
    // SAFETY: `pid` receives the id; a stale window just gives 0.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    process_app(pid).unwrap_or_default()
}

fn process_app(pid: u32) -> Option<String> {
    // SAFETY: a limited query handle, closed below.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buf = [0u16; 1024];
    let mut len = u32::try_from(buf.len()).unwrap_or(0);
    // SAFETY: `buf` is large enough for an AUMID (130 chars max); returns an error when unpackaged.
    let aumid =
        unsafe { GetApplicationUserModelId(process, &mut len, Some(PWSTR(buf.as_mut_ptr()))) };
    let result = if aumid.is_ok() {
        Some(String::from_utf16_lossy(
            &buf[..len.saturating_sub(1) as usize],
        ))
    } else {
        let mut len = u32::try_from(buf.len()).unwrap_or(0);
        // SAFETY: `buf` receives the path; `len` is in/out.
        unsafe {
            QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(buf.as_mut_ptr()),
                &mut len,
            )
        }
        .ok()
        .map(|()| String::from_utf16_lossy(&buf[..len as usize]))
    };
    // SAFETY: the handle came from OpenProcess.
    unsafe {
        let _ = CloseHandle(process);
    }
    result
}

fn title(hwnd: HWND) -> String {
    // SAFETY: plain window queries.
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; usize::try_from(len).unwrap_or(0) + 1];
    let got = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..usize::try_from(got).unwrap_or(0)])
}

fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;
    // SAFETY: DWMWA_CLOAKED writes a DWORD.
    unsafe {
        DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, (&raw mut cloaked).cast(), 4).is_ok()
            && cloaked != 0
    }
}

/// A window a person would call a window: visible, not cloaked, not a tool window, not owned
/// (dialogs belong to their owner), and titled.
fn is_app_window(hwnd: HWND) -> bool {
    // SAFETY: plain window queries.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        IsWindowVisible(hwnd).as_bool()
            && GetWindow(hwnd, GW_OWNER).is_err_or_null()
            && ex & isize::try_from(WS_EX_TOOLWINDOW.0).unwrap_or(0) == 0
            && !is_cloaked(hwnd)
            && GetWindowTextLengthW(hwnd) > 0
    }
}

trait NullCheck {
    fn is_err_or_null(&self) -> bool;
}

impl NullCheck for windows::core::Result<HWND> {
    fn is_err_or_null(&self) -> bool {
        self.as_ref().map_or(true, |h| h.is_invalid())
    }
}

fn info(hwnd: HWND) -> WindowInfo {
    let mut r = RECT::default();
    // SAFETY: `r` receives the rectangle.
    let _ = unsafe { GetWindowRect(hwnd, &mut r) };
    WindowInfo {
        id: WindowId(hwnd.0 as u64),
        title: title(hwnd),
        app_id: window_app(hwnd),
        bounds: Rect {
            x: r.left,
            y: r.top,
            width: u32::try_from(r.right - r.left).unwrap_or(0),
            height: u32::try_from(r.bottom - r.top).unwrap_or(0),
        },
        // SAFETY: plain window query.
        minimized: unsafe { IsIconic(hwnd) }.as_bool(),
    }
}

fn hwnd(id: WindowId) -> HWND {
    HWND(id.0 as *mut _)
}

/// Visible app windows, front to back (EnumWindows walks the Z-order from the top).
fn list_windows() -> Vec<WindowInfo> {
    unsafe extern "system" fn collect(hwnd: HWND, out: LPARAM) -> BOOL {
        // SAFETY: `out` is the `Vec<HWND>` passed to EnumWindows below, alive for the call.
        let out = unsafe { &mut *(out.0 as *mut Vec<HWND>) };
        if is_app_window(hwnd) {
            out.push(hwnd);
        }
        BOOL(1)
    }
    let mut handles: Vec<HWND> = Vec::new();
    // SAFETY: the callback only runs during this call.
    let _ = unsafe { EnumWindows(Some(collect), LPARAM(&raw mut handles as isize)) };
    let _com = Com::init();
    handles.into_iter().map(info).collect()
}

/// Does window `w` belong to `app`?
fn belongs(w: &WindowInfo, app: &AppEntry) -> bool {
    w.app_id.eq_ignore_ascii_case(&app.id)
        || app
            .exe
            .as_ref()
            .is_some_and(|exe| w.app_id.eq_ignore_ascii_case(exe))
}

#[derive(Default)]
pub struct WindowsApps;

impl Apps for WindowsApps {
    fn installed(&self) -> PlatformResult<Vec<AppEntry>> {
        enumerate_apps()
    }

    fn icon(&self, id: &str, size: u32) -> Option<kivo_platform::Image> {
        crate::icons::app_icon(id, size)
    }

    fn launch(&self, app: &AppEntry, args: &[String]) -> PlatformResult<()> {
        let _com = Com::init()?;
        // With arguments, a desktop app is started directly; otherwise through the shell, which
        // also handles packaged apps.
        let (file, params) = match (&app.exe, args.is_empty()) {
            (Some(exe), false) => (HSTRING::from(exe.as_str()), HSTRING::from(args.join(" "))),
            _ => (
                HSTRING::from(format!("shell:AppsFolder\\{}", app.id)),
                HSTRING::new(),
            ),
        };
        // Let the new app take the foreground (the runtime has the right, from the user's hotkey).
        // SAFETY: plain call; failure only means the app may open behind.
        let _ = unsafe { AllowSetForegroundWindow(ASFW_ANY) };
        let mut info = SHELLEXECUTEINFOW {
            cbSize: u32::try_from(std::mem::size_of::<SHELLEXECUTEINFOW>()).unwrap_or(0),
            fMask: SEE_MASK_NOASYNC,
            lpVerb: w!("open"),
            lpFile: PCWSTR(file.as_ptr()),
            lpParameters: if params.is_empty() {
                PCWSTR::null()
            } else {
                PCWSTR(params.as_ptr())
            },
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        // SAFETY: `info` and the strings it points to outlive the call.
        unsafe { ShellExecuteExW(&mut info) }.map_err(|e| {
            if e.code().0 as u32 == 0x8007_0002 {
                PlatformError::NotFound(app.name.clone())
            } else {
                os_error(&e)
            }
        })
    }

    fn close(&self, app: &AppEntry) -> PlatformResult<usize> {
        let windows: Vec<_> = list_windows()
            .into_iter()
            .filter(|w| belongs(w, app))
            .collect();
        if windows.is_empty() {
            return Err(PlatformError::NotFound(format!("{} running", app.name)));
        }
        for w in &windows {
            // SAFETY: WM_CLOSE asks politely; the app decides (and may ask to save).
            let _ = unsafe { PostMessageW(Some(hwnd(w.id)), WM_CLOSE, WPARAM(0), LPARAM(0)) };
        }
        Ok(windows.len())
    }

    fn running(&self, app: &AppEntry) -> PlatformResult<bool> {
        Ok(list_windows().iter().any(|w| belongs(w, app)))
    }
}

#[derive(Default)]
pub struct WindowsWindows;

fn check(id: WindowId) -> PlatformResult<HWND> {
    let h = hwnd(id);
    if is_app_window(h) || unsafe { IsIconic(h) }.as_bool() {
        Ok(h)
    } else {
        Err(PlatformError::NotFound("that window".into()))
    }
}

impl Windows for WindowsWindows {
    fn list(&self) -> PlatformResult<Vec<WindowInfo>> {
        Ok(list_windows())
    }

    fn title_bar(&self, id: WindowId) -> PlatformResult<Option<kivo_platform::Rect>> {
        let h = check(id)?;
        let mut frame = windows::Win32::Foundation::RECT::default();
        let mut buttons = windows::Win32::Foundation::RECT::default();
        // SAFETY: plain queries into RECTs owned here.
        unsafe {
            use windows::Win32::Graphics::Dwm::{
                DWMWA_CAPTION_BUTTON_BOUNDS, DWMWA_EXTENDED_FRAME_BOUNDS,
            };
            let size = u32::try_from(size_of::<windows::Win32::Foundation::RECT>()).unwrap_or(0);
            if DwmGetWindowAttribute(
                h,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                (&raw mut frame).cast(),
                size,
            )
            .is_err()
            {
                return Ok(None);
            }
            // The caption buttons' height is the title bar's (or a browser's tab strip's).
            let height = if DwmGetWindowAttribute(
                h,
                DWMWA_CAPTION_BUTTON_BOUNDS,
                (&raw mut buttons).cast(),
                size,
            )
            .is_ok()
                && buttons.bottom > buttons.top
            {
                buttons.bottom
            } else {
                use windows::Win32::UI::HiDpi::GetDpiForWindow;
                use windows::Win32::UI::WindowsAndMessaging::{
                    SM_CXPADDEDBORDER, SM_CYCAPTION, SM_CYFRAME,
                };
                let dpi = GetDpiForWindow(h);
                let m = |i| windows::Win32::UI::HiDpi::GetSystemMetricsForDpi(i, dpi);
                m(SM_CYCAPTION) + m(SM_CYFRAME) + m(SM_CXPADDEDBORDER)
            };
            Ok(Some(kivo_platform::Rect {
                x: frame.left,
                y: frame.top,
                width: u32::try_from(frame.right - frame.left).unwrap_or(0),
                height: u32::try_from(height.max(0)).unwrap_or(0),
            }))
        }
    }

    fn foreground(&self) -> PlatformResult<Option<WindowInfo>> {
        // SAFETY: plain query.
        let h = unsafe { GetForegroundWindow() };
        let _com = Com::init();
        Ok((!h.is_invalid()).then(|| info(h)))
    }

    fn focus(&self, id: WindowId) -> PlatformResult<()> {
        let h = check(id)?;
        // SAFETY: plain window calls; the thread-input attach is undone right after.
        unsafe {
            if IsIconic(h).as_bool() {
                let _ = ShowWindow(h, SW_RESTORE);
            }
            if SetForegroundWindow(h).as_bool() {
                return Ok(());
            }
            // Foreground lock: join the foreground window's input queue for the switch.
            let front = GetForegroundWindow();
            let front_thread = GetWindowThreadProcessId(front, None);
            let me = GetCurrentThreadId();
            let attached = front_thread != 0 && AttachThreadInput(me, front_thread, true).as_bool();
            let ok = SetForegroundWindow(h).as_bool();
            if attached {
                let _ = AttachThreadInput(me, front_thread, false);
            }
            if ok {
                Ok(())
            } else {
                Err(PlatformError::AccessDenied)
            }
        }
    }

    fn minimize(&self, id: WindowId) -> PlatformResult<()> {
        let h = check(id)?;
        // SAFETY: plain window call.
        let _ = unsafe { ShowWindow(h, SW_MINIMIZE) };
        Ok(())
    }

    fn maximize(&self, id: WindowId) -> PlatformResult<()> {
        let h = check(id)?;
        // SAFETY: plain window call.
        let _ = unsafe { ShowWindow(h, SW_MAXIMIZE) };
        Ok(())
    }

    fn restore(&self, id: WindowId) -> PlatformResult<()> {
        let h = check(id)?;
        // SAFETY: plain window call.
        let _ = unsafe { ShowWindow(h, SW_RESTORE) };
        Ok(())
    }

    fn close(&self, id: WindowId) -> PlatformResult<()> {
        let h = check(id)?;
        // SAFETY: WM_CLOSE asks the window to close; it may ask to save first.
        unsafe { PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0)) }.map_err(|e| os_error(&e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_start_menu_lists_real_apps_without_uninstallers() {
        let apps = WindowsApps.installed().unwrap();
        assert!(apps.len() > 10, "{} apps", apps.len());
        assert!(
            apps.iter()
                .all(|a| !a.name.to_lowercase().starts_with("uninstall"))
        );
        // Every Windows install has these.
        for expected in ["Settings", "File Explorer"] {
            assert!(
                apps.iter().any(|a| a.name == expected),
                "missing {expected}"
            );
        }
        let explorer = apps.iter().find(|a| a.name == "File Explorer").unwrap();
        assert!(explorer.aliases.contains(&"Explorer".to_owned()));
    }

    #[test]
    fn windows_are_listed_front_to_back_with_their_app() {
        let windows = WindowsWindows.list().unwrap();
        // A test machine has at least a shell window; each has a title and an app.
        for w in &windows {
            assert!(!w.title.is_empty());
        }
        if let Some(first) = windows.first() {
            assert!(!first.app_id.is_empty(), "{first:?}");
        }
    }

    #[test]
    fn noise_entries_are_recognized() {
        assert!(is_noise("Uninstall Foo", "C:\\foo\\uninst.exe"));
        assert!(is_noise("Foo Website", "https://foo.example"));
        assert!(is_noise("Readme", "C:\\foo\\readme.txt"));
        assert!(!is_noise("Google Chrome", "Chrome"));
    }
}
