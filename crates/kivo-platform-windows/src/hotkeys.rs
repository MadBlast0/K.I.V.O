//! Global hotkeys (VOICE-41, DECISIONS "Activation"): `RegisterHotKey` on a dedicated thread with
//! its own message loop, so the hotkeys belong to that thread's queue. A press arrives as
//! `WM_HOTKEY`; Windows reports no release, so while a key is held the thread checks it every
//! 15 ms and reports `Released` when it comes up. Nothing runs while no key is held.

use kivo_platform::{Chord, HotkeyEvent, HotkeyId, Hotkeys, PlatformError, PlatformResult};
use std::collections::HashMap;
use std::sync::mpsc;
use std::thread::JoinHandle;
use windows::Win32::Foundation::{ERROR_HOTKEY_ALREADY_REGISTERED, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
    RegisterHotKey, UnregisterHotKey, VIRTUAL_KEY, VK_BACK, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE,
    VK_F1, VK_HOME, VK_INSERT, VK_LEFT, VK_NEXT, VK_PAUSE, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SPACE,
    VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, KillTimer, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, SetTimer, WM_APP,
    WM_HOTKEY, WM_QUIT, WM_TIMER,
};

/// Posted to the hotkey thread when a command is waiting.
const WM_HOTKEY_COMMAND: u32 = WM_APP + 2;
/// How often a held key is checked for release.
const RELEASE_POLL_MS: u32 = 15;

type Reply = mpsc::Sender<PlatformResult<()>>;

enum Command {
    Register(HotkeyId, Chord, Reply),
    Unregister(HotkeyId, Reply),
}

pub struct WindowsHotkeys {
    thread_id: u32,
    commands: mpsc::Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl WindowsHotkeys {
    /// Starts the hotkey thread. `on_event` runs on it for every press and release.
    pub fn start(on_event: impl Fn(HotkeyEvent) + Send + 'static) -> PlatformResult<Self> {
        let (commands, inbox) = mpsc::channel::<Command>();
        let (ready_tx, ready_rx) = mpsc::channel::<u32>();
        let thread = std::thread::Builder::new()
            .name("kivo-hotkeys".into())
            .spawn(move || run(&inbox, &ready_tx, &on_event))
            .map_err(|e| PlatformError::Os {
                code: 0,
                message: e.to_string(),
            })?;
        let thread_id = ready_rx.recv().map_err(|_| PlatformError::Cancelled)?;
        Ok(Self {
            thread_id,
            commands,
            thread: Some(thread),
        })
    }

    fn ask(&self, command: impl FnOnce(Reply) -> Command) -> PlatformResult<()> {
        let (reply, answer) = mpsc::channel();
        self.commands
            .send(command(reply))
            .map_err(|_| PlatformError::Cancelled)?;
        // SAFETY: posting to a thread this struct owns; failure means the loop has ended.
        unsafe { PostThreadMessageW(self.thread_id, WM_HOTKEY_COMMAND, WPARAM(0), LPARAM(0)) }
            .map_err(|_| PlatformError::Cancelled)?;
        answer.recv().map_err(|_| PlatformError::Cancelled)?
    }
}

impl Hotkeys for WindowsHotkeys {
    fn register(&self, id: HotkeyId, chord: &Chord) -> PlatformResult<()> {
        self.ask(|reply| Command::Register(id, chord.clone(), reply))
    }

    fn unregister(&self, id: HotkeyId) -> PlatformResult<()> {
        self.ask(|reply| Command::Unregister(id, reply))
    }
}

impl Drop for WindowsHotkeys {
    fn drop(&mut self) {
        // SAFETY: as in `ask`; WM_QUIT ends the loop, which unregisters everything.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Turns "Ctrl+Shift+Space" style key names into Windows modifiers and a virtual key.
pub fn parse(chord: &Chord) -> PlatformResult<(HOT_KEY_MODIFIERS, VIRTUAL_KEY)> {
    let mut mods = HOT_KEY_MODIFIERS(0);
    let mut key = None;
    for name in &chord.0 {
        let modifier = match name.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Some(MOD_CONTROL),
            "alt" => Some(MOD_ALT),
            "shift" => Some(MOD_SHIFT),
            "win" | "windows" => Some(MOD_WIN),
            _ => None,
        };
        if let Some(m) = modifier {
            mods |= m;
            continue;
        }
        if key.is_some() {
            return Err(PlatformError::NotFound(format!(
                "a single key in \"{chord}\" (it names two)"
            )));
        }
        key = Some(
            virtual_key(name)
                .ok_or_else(|| PlatformError::NotFound(format!("the key \"{name}\"")))?,
        );
    }
    let key = key.ok_or_else(|| PlatformError::NotFound(format!("a key in \"{chord}\"")))?;
    Ok((mods, key))
}

fn virtual_key(name: &str) -> Option<VIRTUAL_KEY> {
    let upper = name.to_ascii_uppercase();
    let named = match upper.as_str() {
        "SPACE" => VK_SPACE,
        "ESC" | "ESCAPE" => VK_ESCAPE,
        "ENTER" | "RETURN" => VK_RETURN,
        "TAB" => VK_TAB,
        "BACKSPACE" => VK_BACK,
        "DELETE" | "DEL" => VK_DELETE,
        "INSERT" | "INS" => VK_INSERT,
        "HOME" => VK_HOME,
        "END" => VK_END,
        "PAGEUP" | "PGUP" => VK_PRIOR,
        "PAGEDOWN" | "PGDN" => VK_NEXT,
        "UP" => VK_UP,
        "DOWN" => VK_DOWN,
        "LEFT" => VK_LEFT,
        "RIGHT" => VK_RIGHT,
        "PAUSE" => VK_PAUSE,
        _ => {
            let bytes = upper.as_bytes();
            // Letters and digits: the virtual key is the ASCII code.
            if bytes.len() == 1 && bytes[0].is_ascii_alphanumeric() {
                return Some(VIRTUAL_KEY(u16::from(bytes[0])));
            }
            // F1–F24 are consecutive.
            let n: u16 = upper.strip_prefix('F')?.parse().ok()?;
            return (1..=24).contains(&n).then(|| VIRTUAL_KEY(VK_F1.0 + n - 1));
        }
    };
    Some(named)
}

/// The hotkey thread: register on request, report presses, and poll held keys for release.
fn run(inbox: &mpsc::Receiver<Command>, ready: &mpsc::Sender<u32>, on_event: &dyn Fn(HotkeyEvent)) {
    let mut msg = MSG::default();
    // SAFETY: plain Win32 calls on this thread's own queue.
    let thread_id = unsafe {
        let _ = PeekMessageW(&raw mut msg, None, 0, 0, PM_NOREMOVE); // make sure the queue exists
        GetCurrentThreadId()
    };
    let _ = ready.send(thread_id);

    let mut registered: HashMap<u32, VIRTUAL_KEY> = HashMap::new();
    let mut held: HashMap<u32, VIRTUAL_KEY> = HashMap::new();
    let mut timer = 0usize;

    // SAFETY: standard message loop on this thread.
    while unsafe { GetMessageW(&raw mut msg, None, 0, 0) }.as_bool() {
        match msg.message {
            WM_HOTKEY_COMMAND => {
                for command in inbox.try_iter() {
                    match command {
                        Command::Register(id, chord, reply) => {
                            let _ = reply.send(register(id, &chord, &mut registered));
                        }
                        Command::Unregister(id, reply) => {
                            let result = if registered.remove(&id.0).is_some() {
                                held.remove(&id.0);
                                // SAFETY: unregistering a hotkey this thread registered.
                                let _ = unsafe { UnregisterHotKey(None, hotkey_id(id)) };
                                Ok(())
                            } else {
                                Err(PlatformError::NotFound(format!("hotkey {}", id.0)))
                            };
                            let _ = reply.send(result);
                        }
                    }
                }
            }
            WM_HOTKEY => {
                #[allow(clippy::cast_possible_truncation, reason = "ids are registered as u32")]
                let id = msg.wParam.0 as u32;
                if let Some(&vk) = registered.get(&id)
                    && held.insert(id, vk).is_none()
                {
                    on_event(HotkeyEvent::Pressed(HotkeyId(id)));
                    if timer == 0 {
                        // SAFETY: a thread timer (no window); WM_TIMER arrives in this loop.
                        timer = unsafe { SetTimer(None, 0, RELEASE_POLL_MS, None) };
                    }
                }
            }
            WM_TIMER if msg.wParam.0 == timer => {
                held.retain(|&id, &mut vk| {
                    // SAFETY: a plain key-state query. The high bit is set while the key is down.
                    let down = unsafe { GetAsyncKeyState(i32::from(vk.0)) } < 0;
                    if !down {
                        on_event(HotkeyEvent::Released(HotkeyId(id)));
                    }
                    down
                });
                if held.is_empty() {
                    // SAFETY: killing the timer this loop created.
                    let _ = unsafe { KillTimer(None, timer) };
                    timer = 0;
                }
            }
            _ => {}
        }
    }
    for id in registered.keys() {
        // SAFETY: unregistering hotkeys this thread registered, before it ends.
        let _ = unsafe { UnregisterHotKey(None, hotkey_id(HotkeyId(*id))) };
    }
}

#[allow(clippy::cast_possible_wrap, reason = "hotkey ids are small")]
fn hotkey_id(id: HotkeyId) -> i32 {
    id.0 as i32
}

fn register(
    id: HotkeyId,
    chord: &Chord,
    registered: &mut HashMap<u32, VIRTUAL_KEY>,
) -> PlatformResult<()> {
    if registered.contains_key(&id.0) {
        return Err(PlatformError::Conflict(format!("hotkey id {}", id.0)));
    }
    let (mods, vk) = parse(chord)?;
    // SAFETY: registering for this thread's queue (no window).
    match unsafe { RegisterHotKey(None, hotkey_id(id), mods | MOD_NOREPEAT, u32::from(vk.0)) } {
        Ok(()) => {
            registered.insert(id.0, vk);
            Ok(())
        }
        Err(e) if e.code() == ERROR_HOTKEY_ALREADY_REGISTERED.to_hresult() => {
            Err(PlatformError::Conflict(chord.to_string()))
        }
        Err(e) => Err(PlatformError::Os {
            code: i64::from(e.code().0),
            message: e.message(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(keys: &[&str]) -> Chord {
        Chord(keys.iter().map(ToString::to_string).collect())
    }

    #[test]
    fn chords_parse_into_modifiers_and_a_key() {
        let (mods, vk) = parse(&chord(&["Ctrl", "Space"])).unwrap();
        assert_eq!((mods, vk), (MOD_CONTROL, VK_SPACE));
        let (mods, vk) = parse(&chord(&["Ctrl", "Shift", "k"])).unwrap();
        assert_eq!(
            (mods, vk),
            (MOD_CONTROL | MOD_SHIFT, VIRTUAL_KEY(u16::from(b'K')))
        );
        assert_eq!(
            parse(&chord(&["Alt", "F12"])).unwrap().1,
            VIRTUAL_KEY(VK_F1.0 + 11)
        );
        assert!(parse(&chord(&["Ctrl"])).is_err(), "a key is needed");
        assert!(parse(&chord(&["Ctrl", "A", "B"])).is_err(), "only one key");
        assert!(parse(&chord(&["Ctrl", "F25"])).is_err());
        assert!(parse(&chord(&["Ctrl", "Banana"])).is_err());
    }

    #[test]
    fn registering_a_taken_combination_is_a_conflict() {
        // An unusual combination nobody else uses.
        let taken = chord(&["Ctrl", "Alt", "Shift", "F23"]);
        let first = WindowsHotkeys::start(|_| {}).unwrap();
        first.register(HotkeyId(1), &taken).unwrap();
        let second = WindowsHotkeys::start(|_| {}).unwrap();
        assert_eq!(
            second.register(HotkeyId(1), &taken),
            Err(PlatformError::Conflict("Ctrl+Alt+Shift+F23".into()))
        );
        first.unregister(HotkeyId(1)).unwrap();
        second.register(HotkeyId(1), &taken).unwrap();
        assert!(matches!(
            second.unregister(HotkeyId(2)),
            Err(PlatformError::NotFound(_))
        ));
    }

    /// Presses a registered hotkey with synthetic input and checks press and release arrive.
    /// Ignored by default: it sends keystrokes to the real desktop. Run it on purpose with
    /// `cargo test -p kivo-platform-windows -- --ignored`.
    #[test]
    #[ignore = "sends real key presses"]
    fn a_held_hotkey_reports_press_then_release() {
        use std::time::Duration;
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
            SendInput, VK_CONTROL, VK_F24, VK_MENU, VK_SHIFT,
        };
        let key = |vk: VIRTUAL_KEY, up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    dwFlags: if up {
                        KEYEVENTF_KEYUP
                    } else {
                        KEYBD_EVENT_FLAGS(0)
                    },
                    ..Default::default()
                },
            },
        };
        let (tx, rx) = mpsc::channel();
        let hotkeys = WindowsHotkeys::start(move |e| {
            let _ = tx.send((e, std::time::Instant::now()));
        })
        .unwrap();
        hotkeys
            .register(HotkeyId(7), &chord(&["Ctrl", "Alt", "Shift", "F24"]))
            .unwrap();
        let mods = [VK_CONTROL, VK_MENU, VK_SHIFT];
        let down: Vec<INPUT> = mods
            .iter()
            .chain([&VK_F24])
            .map(|&k| key(k, false))
            .collect();
        let up: Vec<INPUT> = [VK_F24]
            .iter()
            .chain(mods.iter())
            .map(|&k| key(k, true))
            .collect();
        // SAFETY: well-formed keyboard INPUT arrays.
        unsafe { SendInput(&down, i32::try_from(size_of::<INPUT>()).unwrap()) };
        let (pressed, at) = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(pressed, HotkeyEvent::Pressed(HotkeyId(7)));
        std::thread::sleep(Duration::from_millis(200));
        let released_at = std::time::Instant::now();
        // SAFETY: as above.
        unsafe { SendInput(&up, i32::try_from(size_of::<INPUT>()).unwrap()) };
        let (released, when) = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(released, HotkeyEvent::Released(HotkeyId(7)));
        assert!(when > at);
        assert!(
            when.duration_since(released_at) < Duration::from_millis(60),
            "release noticed within a few polls"
        );
    }
}
