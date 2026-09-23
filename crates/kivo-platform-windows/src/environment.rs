//! The user's environment as Windows has it now (DISCOVERY §3, DISC-15): the current `PATH` from
//! the registry (a program installed after KIVO started changes it there, not in KIVO's own
//! environment), and `WM_SETTINGCHANGE` "Environment" broadcasts, which Windows sends when it
//! changes.

use std::path::PathBuf;
use std::sync::mpsc;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
    RegGetValueW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, MSG,
    PostMessageW, PostQuitMessage, RegisterClassW, TranslateMessage, WM_CLOSE, WM_DESTROY,
    WM_SETTINGCHANGE, WNDCLASSW, WS_EX_TOOLWINDOW, WS_OVERLAPPED,
};
use windows::core::{PCWSTR, w};

fn reg_string(root: HKEY, key: PCWSTR, value: PCWSTR) -> Option<String> {
    let mut bytes = 0u32;
    // SAFETY: a size query (no buffer); RRF_NOEXPAND keeps %VARS% for us to expand.
    let status = unsafe {
        RegGetValueW(
            root,
            key,
            value,
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND,
            None,
            None,
            Some(&raw mut bytes),
        )
    };
    if status.is_err() || bytes == 0 {
        return None;
    }
    let mut buf = vec![0u16; (bytes as usize).div_ceil(2) + 1];
    let mut size = u32::try_from(buf.len() * 2).ok()?;
    // SAFETY: `buf` is writable for `size` bytes.
    let status = unsafe {
        RegGetValueW(
            root,
            key,
            value,
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&raw mut size),
        )
    };
    if status.is_err() {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

fn expand(text: &str) -> String {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: a size query, then a call with a buffer of that size.
    let needed = unsafe { ExpandEnvironmentStringsW(PCWSTR(wide.as_ptr()), None) };
    if needed == 0 {
        return text.to_owned();
    }
    let mut out = vec![0u16; needed as usize];
    // SAFETY: `out` holds `needed` characters.
    let written = unsafe { ExpandEnvironmentStringsW(PCWSTR(wide.as_ptr()), Some(&mut out)) };
    if written == 0 {
        return text.to_owned();
    }
    let len = out.iter().position(|&c| c == 0).unwrap_or(out.len());
    String::from_utf16_lossy(&out[..len])
}

/// The folders on `PATH` as Windows has them now: the machine's, then the user's.
pub fn current_path() -> Vec<PathBuf> {
    let machine = reg_string(
        HKEY_LOCAL_MACHINE,
        w!(r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment"),
        w!("Path"),
    );
    let user = reg_string(HKEY_CURRENT_USER, w!("Environment"), w!("Path"));
    let joined = [machine, user]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(";");
    if joined.is_empty() {
        return std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
    }
    expand(&joined)
        .split(';')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Watches for environment changes. Dropping it stops the watch.
pub struct EnvironmentWatch {
    hwnd: isize,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for EnvironmentWatch {
    fn drop(&mut self) {
        // SAFETY: posting to a window this watch created; a gone window just fails.
        let _ = unsafe { PostMessageW(Some(HWND(self.hwnd as _)), WM_CLOSE, WPARAM(0), LPARAM(0)) };
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

thread_local! {
    static ON_CHANGE: std::cell::RefCell<Option<Box<dyn Fn()>>> = const { std::cell::RefCell::new(None) };
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_SETTINGCHANGE => {
            // lParam names the changed area: "Environment" for PATH and other variables.
            let area = if lparam.0 == 0 {
                String::new()
            } else {
                // SAFETY: for WM_SETTINGCHANGE, a non-null lParam is a terminated string.
                unsafe { PCWSTR(lparam.0 as *const u16).to_string() }.unwrap_or_default()
            };
            if area.eq_ignore_ascii_case("Environment") {
                ON_CHANGE.with(|f| {
                    if let Some(f) = f.borrow().as_ref() {
                        f();
                    }
                });
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            // SAFETY: our own window.
            let _ = unsafe { DestroyWindow(hwnd) };
            LRESULT(0)
        }
        WM_DESTROY => {
            // SAFETY: ends this thread's message loop.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        // SAFETY: default handling for everything else.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Calls `on_change` (on the watch's own thread) whenever Windows broadcasts that environment
/// variables changed. Broadcasts reach top-level windows only, so the watch owns a hidden one.
pub fn watch(on_change: impl Fn() + Send + 'static) -> Option<EnvironmentWatch> {
    let (tx, rx) = mpsc::channel::<isize>();
    let thread = std::thread::Builder::new()
        .name("kivo-environment".into())
        .spawn(move || {
            ON_CHANGE.with(|f| *f.borrow_mut() = Some(Box::new(on_change)));
            // SAFETY: registering a class and creating a hidden tool window on this thread; the
            // class name and procedure outlive the window.
            let hwnd = unsafe {
                let instance = GetModuleHandleW(None).unwrap_or_default();
                let class = WNDCLASSW {
                    lpfnWndProc: Some(proc),
                    hInstance: instance.into(),
                    lpszClassName: w!("KivoEnvironmentWatch"),
                    ..Default::default()
                };
                RegisterClassW(&raw const class);
                CreateWindowExW(
                    WS_EX_TOOLWINDOW,
                    w!("KivoEnvironmentWatch"),
                    w!(""),
                    WS_OVERLAPPED,
                    0,
                    0,
                    0,
                    0,
                    None,
                    None,
                    Some(instance.into()),
                    None,
                )
            };
            let Ok(hwnd) = hwnd else {
                let _ = tx.send(0);
                return;
            };
            let _ = tx.send(hwnd.0 as isize);
            let mut msg = MSG::default();
            // SAFETY: a standard message loop on the thread that owns the window.
            while unsafe { GetMessageW(&raw mut msg, None, 0, 0) }.as_bool() {
                // SAFETY: as above.
                unsafe {
                    let _ = TranslateMessage(&raw const msg);
                    DispatchMessageW(&raw const msg);
                }
            }
        })
        .ok()?;
    let hwnd = rx.recv().ok().filter(|h| *h != 0)?;
    Some(EnvironmentWatch {
        hwnd,
        thread: Some(thread),
    })
}

/// Opens `program args` in a new console window the user can see and type in (a CLI's own
/// sign-in, BRAIN-17). The window stays open when the program ends.
pub fn open_in_terminal(program: &std::path::Path, args: &[String]) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let quote = |s: &str| {
        if s.contains([' ', '&', '(', ')', '^', '|', '<', '>']) {
            format!("\"{s}\"")
        } else {
            s.to_owned()
        }
    };
    let mut line = quote(&program.to_string_lossy());
    for a in args {
        line.push(' ');
        line.push_str(&quote(a));
    }
    // `start` opens a new console; `cmd /k` keeps it open to read the result.
    std::process::Command::new("cmd")
        .args(["/d", "/c", "start", "\"KIVO sign-in\"", "cmd", "/k"])
        .raw_arg(line)
        .creation_flags(0x0800_0000) // the outer cmd has no window of its own
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_current_path_comes_from_the_registry() {
        let path = current_path();
        assert!(!path.is_empty());
        assert!(
            path.iter()
                .any(|p| p.to_string_lossy().to_lowercase().contains("windows")),
            "{path:?}"
        );
        assert!(
            path.iter().all(|p| !p.to_string_lossy().contains('%')),
            "expanded"
        );
    }

    #[test]
    fn a_watch_starts_and_stops() {
        let watch = watch(|| {}).expect("the watch window is created");
        drop(watch);
    }
}
