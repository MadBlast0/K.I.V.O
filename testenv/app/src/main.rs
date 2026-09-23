//! KIVO's dummy app (TOOLS_AND_CONTROL §10, TOOL-38): the only desktop app computer-control
//! tests ever act on. Standard Win32 controls, so UI Automation sees them with their control ids
//! as AutomationIds (listed in `testenv/README.md`):
//!
//! - 101 name field, 102 password field, 103 "Greet", 104 "Remember me" check box,
//!   105 list (Alpha, Beta, Gamma), 106 colour combo box (Red, Green, Blue),
//!   107 "Export…" (opens the Export window), 108 status text, 109 notes, 110 address bar;
//! - the Export window: 201 file name, 202 "Save", 203 "Cancel". Save writes the name field into
//!   that file inside the `--out` folder (never anywhere else).
//!
//! `--title <text>` changes the window title (a browser-like window for the UIA fallback tests).

#[cfg(windows)]
mod app {
    use std::path::PathBuf;
    use std::sync::OnceLock;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::{COLOR_WINDOW, HBRUSH};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        BS_AUTOCHECKBOX, BS_PUSHBUTTON, CB_ADDSTRING, CB_SETCURSEL, CBS_DROPDOWNLIST,
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, ES_AUTOHSCROLL,
        ES_MULTILINE, ES_PASSWORD, GetDlgItem, GetMessageW, GetWindowTextLengthW, GetWindowTextW,
        HMENU, IsDialogMessageW, LB_ADDSTRING, LBS_NOTIFY, MSG, PostQuitMessage, RegisterClassW,
        SW_SHOWNOACTIVATE, SetWindowTextW, ShowWindow, TranslateMessage, WINDOW_EX_STYLE,
        WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_DESTROY, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD,
        WS_OVERLAPPEDWINDOW, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
    };
    use windows::core::{HSTRING, PCWSTR, w};

    const MAIN_CLASS: PCWSTR = w!("KivoTestApp");
    const EXPORT_CLASS: PCWSTR = w!("KivoTestExport");

    static OUT: OnceLock<PathBuf> = OnceLock::new();
    static mut MAIN: isize = 0;
    static mut EXPORT: isize = 0;

    fn control(
        parent: HWND,
        class: PCWSTR,
        text: &str,
        style: u32,
        id: i32,
        (x, y, w, h): (i32, i32, i32, i32),
    ) -> HWND {
        // SAFETY: a child control of a window we own, created on this thread.
        unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class,
                &HSTRING::from(text),
                WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | style),
                x,
                y,
                w,
                h,
                Some(parent),
                Some(HMENU(id as isize as *mut _)),
                GetModuleHandleW(None).ok().map(Into::into),
                None,
            )
            .unwrap_or_default()
        }
    }

    fn text_of(parent: HWND, id: i32) -> String {
        // SAFETY: reading a child control's text into a buffer of its length.
        unsafe {
            let Ok(item) = GetDlgItem(Some(parent), id) else {
                return String::new();
            };
            let len = GetWindowTextLengthW(item);
            let mut buf = vec![0u16; usize::try_from(len).unwrap_or(0) + 1];
            let n = GetWindowTextW(item, &mut buf);
            String::from_utf16_lossy(&buf[..usize::try_from(n).unwrap_or(0)])
        }
    }

    fn set_status(text: &str) {
        // SAFETY: the main window exists while its controls are used.
        unsafe {
            if let Ok(item) = GetDlgItem(Some(HWND(MAIN as *mut _)), 108) {
                let _ = SetWindowTextW(item, &HSTRING::from(text));
            }
        }
    }

    fn open_export() {
        // SAFETY: creating our own top-level window on the UI thread.
        unsafe {
            if EXPORT != 0 {
                return;
            }
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                EXPORT_CLASS,
                w!("Export"),
                WINDOW_STYLE(WS_CAPTION.0 | WS_SYSMENU.0),
                200,
                200,
                340,
                160,
                None,
                None,
                GetModuleHandleW(None).ok().map(Into::into),
                None,
            )
            .unwrap_or_default();
            EXPORT = hwnd.0 as isize;
            control(hwnd, w!("STATIC"), "File name:", 0, 200, (10, 12, 80, 20));
            control(
                hwnd,
                w!("EDIT"),
                "export.txt",
                WS_BORDER.0 | WS_TABSTOP.0 | ES_AUTOHSCROLL as u32,
                201,
                (95, 10, 220, 24),
            );
            control(
                hwnd,
                w!("BUTTON"),
                "Save",
                WS_TABSTOP.0 | BS_PUSHBUTTON as u32,
                202,
                (95, 50, 100, 30),
            );
            control(
                hwnd,
                w!("BUTTON"),
                "Cancel",
                WS_TABSTOP.0 | BS_PUSHBUTTON as u32,
                203,
                (205, 50, 100, 30),
            );
            // Never take focus from the person at the desk (tests run beside their work).
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
    }

    fn close_export() {
        // SAFETY: destroying our own window.
        unsafe {
            if EXPORT != 0 {
                let _ = DestroyWindow(HWND(EXPORT as *mut _));
                EXPORT = 0;
            }
        }
    }

    unsafe extern "system" fn export_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_COMMAND => {
                match (wparam.0 & 0xffff) as i32 {
                    202 => {
                        let file = text_of(hwnd, 201);
                        // Only a plain file name, only inside the output folder.
                        let safe = !file.is_empty()
                            && !file.contains(['/', '\\', ':'])
                            && file != ".."
                            && file != ".";
                        let name = text_of(HWND(unsafe { MAIN } as *mut _), 101);
                        match (safe, OUT.get()) {
                            (true, Some(out)) => match std::fs::write(out.join(&file), name) {
                                Ok(()) => set_status(&format!("Exported {file}")),
                                Err(e) => set_status(&format!("Export failed: {e}")),
                            },
                            _ => set_status("Export refused"),
                        }
                        close_export();
                    }
                    203 => {
                        set_status("Export cancelled");
                        close_export();
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                close_export();
                LRESULT(0)
            }
            // SAFETY: default handling.
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    unsafe extern "system" fn main_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_COMMAND => {
                match (wparam.0 & 0xffff) as i32 {
                    103 => {
                        let name = text_of(hwnd, 101);
                        set_status(&format!("Hello, {name}"));
                    }
                    107 => open_export(),
                    _ => {}
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                // SAFETY: ends this thread's message loop.
                unsafe { PostQuitMessage(0) };
                LRESULT(0)
            }
            // SAFETY: default handling.
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    pub fn run() {
        let mut args = std::env::args().skip(1);
        let mut title = "KIVO Test App".to_owned();
        let mut out = std::env::temp_dir().join("kivo-testenv-out");
        while let Some(a) = args.next() {
            match a.as_str() {
                "--out" => out = args.next().map(PathBuf::from).unwrap_or(out),
                "--title" => title = args.next().unwrap_or(title),
                _ => {}
            }
        }
        let _ = std::fs::create_dir_all(&out);
        let _ = OUT.set(out);
        // SAFETY: registering our window classes and creating the window on this thread.
        unsafe {
            let instance = GetModuleHandleW(None).unwrap_or_default();
            for (class, proc_) in [
                (
                    MAIN_CLASS,
                    main_proc as unsafe extern "system" fn(_, _, _, _) -> _,
                ),
                (EXPORT_CLASS, export_proc),
            ] {
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(proc_),
                    hInstance: instance.into(),
                    lpszClassName: class,
                    hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as isize as *mut _),
                    ..Default::default()
                };
                RegisterClassW(&raw const wc);
            }
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                MAIN_CLASS,
                &HSTRING::from(title.as_str()),
                WS_OVERLAPPEDWINDOW,
                120,
                120,
                520,
                420,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .unwrap_or_default();
            MAIN = hwnd.0 as isize;
            let edit = WS_BORDER.0 | WS_TABSTOP.0 | ES_AUTOHSCROLL as u32;
            control(hwnd, w!("STATIC"), "Name:", 0, 100, (10, 12, 90, 20));
            control(hwnd, w!("EDIT"), "", edit, 101, (105, 10, 250, 24));
            control(hwnd, w!("STATIC"), "Password:", 0, 111, (10, 44, 90, 20));
            control(
                hwnd,
                w!("EDIT"),
                "",
                edit | ES_PASSWORD as u32,
                102,
                (105, 42, 250, 24),
            );
            control(
                hwnd,
                w!("BUTTON"),
                "Greet",
                WS_TABSTOP.0 | BS_PUSHBUTTON as u32,
                103,
                (365, 10, 120, 28),
            );
            control(
                hwnd,
                w!("BUTTON"),
                "Remember me",
                WS_TABSTOP.0 | BS_AUTOCHECKBOX as u32,
                104,
                (105, 74, 200, 22),
            );
            let list = control(
                hwnd,
                w!("LISTBOX"),
                "",
                WS_BORDER.0 | WS_TABSTOP.0 | WS_VSCROLL.0 | LBS_NOTIFY as u32,
                105,
                (105, 102, 120, 70),
            );
            for item in ["Alpha", "Beta", "Gamma"] {
                windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    list,
                    LB_ADDSTRING,
                    None,
                    Some(LPARAM(HSTRING::from(item).as_ptr() as isize)),
                );
            }
            control(hwnd, w!("STATIC"), "Colour:", 0, 112, (240, 104, 60, 20));
            let combo = control(
                hwnd,
                w!("COMBOBOX"),
                "",
                WS_TABSTOP.0 | WS_VSCROLL.0 | CBS_DROPDOWNLIST as u32,
                106,
                (300, 102, 120, 120),
            );
            for item in ["Red", "Green", "Blue"] {
                windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                    combo,
                    CB_ADDSTRING,
                    None,
                    Some(LPARAM(HSTRING::from(item).as_ptr() as isize)),
                );
            }
            windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                combo,
                CB_SETCURSEL,
                Some(WPARAM(0)),
                None,
            );
            control(
                hwnd,
                w!("BUTTON"),
                "Export…",
                WS_TABSTOP.0 | BS_PUSHBUTTON as u32,
                107,
                (365, 44, 120, 28),
            );
            control(hwnd, w!("STATIC"), "Ready", 0, 108, (10, 182, 480, 20));
            control(hwnd, w!("STATIC"), "Notes:", 0, 113, (10, 210, 90, 20));
            control(
                hwnd,
                w!("EDIT"),
                "",
                edit | ES_MULTILINE as u32 | WS_VSCROLL.0,
                109,
                (105, 208, 380, 90),
            );
            control(
                hwnd,
                w!("STATIC"),
                "Address and search bar",
                0,
                114,
                (10, 312, 90, 20),
            );
            control(
                hwnd,
                w!("EDIT"),
                "https://example.test/page",
                edit,
                110,
                (105, 310, 380, 24),
            );
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let mut msg = MSG::default();
            while GetMessageW(&raw mut msg, None, 0, 0).as_bool() {
                let export = HWND(EXPORT as *mut _);
                if (EXPORT != 0 && IsDialogMessageW(export, &raw const msg).as_bool())
                    || IsDialogMessageW(hwnd, &raw const msg).as_bool()
                {
                    continue;
                }
                let _ = TranslateMessage(&raw const msg);
                DispatchMessageW(&raw const msg);
            }
        }
    }
}

fn main() {
    #[cfg(windows)]
    app::run();
}
