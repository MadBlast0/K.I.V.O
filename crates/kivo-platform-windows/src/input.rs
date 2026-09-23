//! Simulated input (TOOL-33), the last tier of the capability ladder: `SendInput` for clicks,
//! Unicode typing, key combinations and the wheel. The checks around it (foreground and bounds
//! re-validated, never into password fields, at least Medium risk) live in the input tools; this
//! only builds and sends the events. An abort flag stops a long typing run between keys (the
//! emergency stop, SEC-27).
//!
//! Tests only build the event lists: synthetic input never reaches the live desktop from a test.

use kivo_platform::{Input, MouseButton, PlatformError, PlatformResult, Point};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, MOUSE_EVENT_FLAGS, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL,
    MOUSEINPUT, SendInput, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

/// Characters typed per `SendInput` batch; the abort flag is checked between batches.
const TYPE_BATCH: usize = 16;

#[derive(Default)]
pub struct WindowsInput {
    abort: Arc<AtomicBool>,
}

impl WindowsInput {
    /// The flag the emergency stop raises; typing stops at the next batch.
    pub fn abort_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.abort)
    }
}

fn send(inputs: &[INPUT]) -> PlatformResult<()> {
    if inputs.is_empty() {
        return Ok(());
    }
    // SAFETY: a slice of fully initialized INPUT structs with the right size.
    let sent = unsafe { SendInput(inputs, i32::try_from(size_of::<INPUT>()).unwrap_or(0)) };
    if sent as usize == inputs.len() {
        Ok(())
    } else {
        // UIPI blocks input into elevated windows.
        Err(PlatformError::AccessDenied)
    }
}

fn mouse(dx: i32, dy: i32, data: i32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                #[allow(clippy::cast_sign_loss, reason = "wheel deltas are signed in a u32")]
                mouseData: data as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn key(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// A point in the virtual desktop as `SendInput`'s 0–65535 absolute coordinates.
pub(crate) fn absolute(at: Point, desk: (i32, i32, i32, i32)) -> (i32, i32) {
    let (x0, y0, w, h) = desk;
    let scale = |v: i32, origin: i32, size: i32| {
        let size = i64::from(size.max(2) - 1);
        i32::try_from((i64::from(v - origin) * 65_535) / size).unwrap_or(0)
    };
    (scale(at.x, x0, w), scale(at.y, y0, h))
}

fn virtual_desk() -> (i32, i32, i32, i32) {
    // SAFETY: plain metric queries.
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

pub(crate) fn click_events(
    at: Point,
    button: MouseButton,
    count: u8,
    desk: (i32, i32, i32, i32),
) -> Vec<INPUT> {
    let (x, y) = absolute(at, desk);
    let (down, up) = match button {
        MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
    };
    let mut out = vec![mouse(
        x,
        y,
        0,
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
    )];
    for _ in 0..count.clamp(1, 3) {
        out.push(mouse(0, 0, 0, down));
        out.push(mouse(0, 0, 0, up));
    }
    out
}

/// Unicode typing: each UTF-16 unit down and up (surrogate pairs work this way too).
pub(crate) fn type_events(text: &str) -> Vec<INPUT> {
    text.encode_utf16()
        .flat_map(|unit| {
            [
                key(0, unit, KEYEVENTF_UNICODE),
                key(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP),
            ]
        })
        .collect()
}

/// A key's virtual-key code from its display name ("Ctrl", "Enter", "F5", "S").
pub(crate) fn vk(name: &str) -> Option<u16> {
    let n = name.trim().to_ascii_lowercase();
    let code = match n.as_str() {
        "ctrl" | "control" => 0x11,
        "shift" => 0x10,
        "alt" => 0x12,
        "win" | "windows" | "super" => 0x5B,
        "enter" | "return" => 0x0D,
        "esc" | "escape" => 0x1B,
        "tab" => 0x09,
        "space" => 0x20,
        "backspace" => 0x08,
        "delete" | "del" => 0x2E,
        "insert" => 0x2D,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" | "page up" => 0x21,
        "pagedown" | "page down" => 0x22,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        _ => {
            let b = n.as_bytes();
            if b.len() == 1 && (b[0].is_ascii_alphanumeric()) {
                u16::from(b[0].to_ascii_uppercase())
            } else if let Some(f) = n.strip_prefix('f').and_then(|d| d.parse::<u16>().ok())
                && (1..=24).contains(&f)
            {
                0x6F + f
            } else {
                return None;
            }
        }
    };
    Some(code)
}

/// Every key down in order, then up in reverse.
pub(crate) fn chord_events(keys: &[String]) -> PlatformResult<Vec<INPUT>> {
    let codes = keys
        .iter()
        .map(|k| vk(k).ok_or_else(|| PlatformError::NotFound(format!("a key called {k}"))))
        .collect::<PlatformResult<Vec<u16>>>()?;
    let mut out: Vec<INPUT> = codes
        .iter()
        .map(|&c| key(c, 0, KEYBD_EVENT_FLAGS(0)))
        .collect();
    out.extend(codes.iter().rev().map(|&c| key(c, 0, KEYEVENTF_KEYUP)));
    Ok(out)
}

impl Input for WindowsInput {
    fn click(&self, at: Point, button: MouseButton, count: u8) -> PlatformResult<()> {
        self.abort.store(false, Ordering::SeqCst);
        send(&click_events(at, button, count, virtual_desk()))
    }

    fn type_text(&self, text: &str) -> PlatformResult<()> {
        self.abort.store(false, Ordering::SeqCst);
        let events = type_events(text);
        for batch in events.chunks(TYPE_BATCH * 2) {
            if self.abort.load(Ordering::SeqCst) {
                return Err(PlatformError::Cancelled);
            }
            send(batch)?;
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Ok(())
    }

    fn press(&self, keys: &[String]) -> PlatformResult<()> {
        self.abort.store(false, Ordering::SeqCst);
        send(&chord_events(keys)?)
    }

    fn scroll(&self, at: Point, delta_y: i32) -> PlatformResult<()> {
        let (x, y) = absolute(at, virtual_desk());
        send(&[
            mouse(
                x,
                y,
                0,
                MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
            ),
            mouse(0, 0, delta_y.saturating_mul(120), MOUSEEVENTF_WHEEL),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_coordinates_span_the_virtual_desktop() {
        let desk = (-1920, 0, 3840, 1080);
        assert_eq!(absolute(Point { x: -1920, y: 0 }, desk), (0, 0));
        assert_eq!(absolute(Point { x: 1919, y: 1079 }, desk), (65_535, 65_535));
    }

    #[test]
    fn a_double_click_is_a_move_then_two_presses() {
        let e = click_events(
            Point { x: 10, y: 10 },
            MouseButton::Left,
            2,
            (0, 0, 100, 100),
        );
        assert_eq!(e.len(), 5);
        // SAFETY: reading the union member these were built with.
        let flags: Vec<MOUSE_EVENT_FLAGS> = e
            .iter()
            .map(|i| unsafe { i.Anonymous.mi.dwFlags })
            .collect();
        assert!(flags[0].contains(MOUSEEVENTF_ABSOLUTE));
        assert_eq!(flags[1], MOUSEEVENTF_LEFTDOWN);
        assert_eq!(flags[4], MOUSEEVENTF_LEFTUP);
    }

    #[test]
    fn typing_is_unicode_down_and_up_per_unit() {
        let e = type_events("hé😀");
        // h, é, and a surrogate pair: 4 units, each down and up.
        assert_eq!(e.len(), 8);
        // SAFETY: reading the union member these were built with.
        let first = unsafe { e[0].Anonymous.ki };
        assert_eq!(first.wScan, u16::from(b'h'));
        assert!(first.dwFlags.contains(KEYEVENTF_UNICODE));
    }

    #[test]
    fn chords_press_in_order_and_release_in_reverse() {
        let e = chord_events(&["Ctrl".into(), "Shift".into(), "S".into()]).unwrap();
        // SAFETY: reading the union member these were built with.
        let keys: Vec<(u16, bool)> = e
            .iter()
            .map(|i| unsafe {
                (
                    i.Anonymous.ki.wVk.0,
                    i.Anonymous.ki.dwFlags.contains(KEYEVENTF_KEYUP),
                )
            })
            .collect();
        assert_eq!(
            keys,
            vec![
                (0x11, false),
                (0x10, false),
                (0x53, false),
                (0x53, true),
                (0x10, true),
                (0x11, true)
            ]
        );
        assert_eq!(vk("F5"), Some(0x74));
        assert!(chord_events(&["Hyper".into()]).is_err());
    }
}
