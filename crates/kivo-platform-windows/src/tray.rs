//! The tray icon (ARCHITECTURE §1, UX §1). It lives in the runtime so it survives a UI crash, on a
//! dedicated thread with a Win32 message loop: the icon object must stay on the thread that made
//! it, so other threads send commands through a channel and wake the loop with a posted message.

use kivo_platform::{PlatformError, PlatformResult, Tray, TrayIcon, TrayMenuItem};
use std::sync::mpsc;
use std::thread::JoinHandle;
use tray_icon::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW,
    TranslateMessage, WM_APP, WM_QUIT,
};

/// The KIVO mark, 64 px (Windows scales it for the tray at any DPI).
const MARK_PNG: &[u8] = include_bytes!("../../../apps/kivo-app/src-tauri/icons/64x64.png");
/// Posted to the tray thread when a command is waiting.
const WM_TRAY_COMMAND: u32 = WM_APP + 1;

/// What the user did with the tray icon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrayEvent {
    /// A menu item was chosen, by its id.
    Menu(String),
    /// Left click on the icon (opens the Control Center, UX §1).
    Click,
}

enum Command {
    Icon(TrayIcon),
    Tooltip(String),
    Menu(Vec<TrayMenuItem>),
}

pub struct WindowsTray {
    thread_id: u32,
    commands: mpsc::Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl WindowsTray {
    /// Shows the tray icon. `on_event` runs on the tray thread for every click or menu choice.
    pub fn start(
        tooltip: &str,
        menu: &[TrayMenuItem],
        on_event: impl Fn(TrayEvent) + Send + Sync + 'static,
    ) -> PlatformResult<Self> {
        let on_event = std::sync::Arc::new(on_event);
        let menu_events = std::sync::Arc::clone(&on_event);
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            menu_events(TrayEvent::Menu(e.id.0))
        }));
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = e
            {
                on_event(TrayEvent::Click);
            }
        }));

        let (commands, inbox) = mpsc::channel::<Command>();
        let (ready_tx, ready_rx) = mpsc::channel::<PlatformResult<u32>>();
        let (tooltip, menu) = (tooltip.to_owned(), menu.to_vec());
        let thread = std::thread::Builder::new()
            .name("kivo-tray".into())
            .spawn(move || run(&tooltip, &menu, &inbox, &ready_tx))
            .map_err(|e| os_error(&e))?;
        let thread_id = ready_rx.recv().map_err(|_| PlatformError::Cancelled)??;
        Ok(Self {
            thread_id,
            commands,
            thread: Some(thread),
        })
    }

    fn send(&self, command: Command) -> PlatformResult<()> {
        self.commands
            .send(command)
            .map_err(|_| PlatformError::Cancelled)?;
        // SAFETY: posting a message to a thread we own; failure only means the loop has ended.
        unsafe { PostThreadMessageW(self.thread_id, WM_TRAY_COMMAND, WPARAM(0), LPARAM(0)) }
            .map_err(|e| os_error(&e))
    }
}

impl Tray for WindowsTray {
    fn set_icon(&self, icon: TrayIcon) -> PlatformResult<()> {
        self.send(Command::Icon(icon))
    }
    fn set_tooltip(&self, text: &str) -> PlatformResult<()> {
        self.send(Command::Tooltip(text.to_owned()))
    }
    fn set_menu(&self, items: &[TrayMenuItem]) -> PlatformResult<()> {
        self.send(Command::Menu(items.to_vec()))
    }
}

impl Drop for WindowsTray {
    fn drop(&mut self) {
        // SAFETY: as in `send`; WM_QUIT ends the loop, which drops the icon on its own thread.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        MenuEvent::set_event_handler(None::<fn(MenuEvent)>);
        TrayIconEvent::set_event_handler(None::<fn(TrayIconEvent)>);
    }
}

/// The tray thread: create the icon, then pump messages until WM_QUIT.
fn run(
    tooltip: &str,
    menu: &[TrayMenuItem],
    inbox: &mpsc::Receiver<Command>,
    ready: &mpsc::Sender<PlatformResult<u32>>,
) {
    let mut msg = MSG::default();
    // SAFETY: plain Win32 calls on this thread's own message queue.
    let thread_id = unsafe {
        let _ = PeekMessageW(&raw mut msg, None, 0, 0, PM_NOREMOVE); // make sure the queue exists
        GetCurrentThreadId()
    };
    let tray = match build_menu(menu).and_then(|m| {
        TrayIconBuilder::new()
            .with_icon(icon_for(TrayIcon::Normal)?)
            .with_tooltip(tooltip)
            .with_menu(Box::new(m))
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| os_error(&e))
    }) {
        Ok(tray) => tray,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let _ = ready.send(Ok(thread_id));

    // SAFETY: standard message loop on this thread.
    while unsafe { GetMessageW(&raw mut msg, None, 0, 0) }.as_bool() {
        if msg.message == WM_TRAY_COMMAND {
            for command in inbox.try_iter() {
                let result = match command {
                    Command::Icon(icon) => icon_for(icon)
                        .and_then(|i| tray.set_icon(Some(i)).map_err(|e| os_error(&e))),
                    Command::Tooltip(text) => {
                        tray.set_tooltip(Some(text)).map_err(|e| os_error(&e))
                    }
                    Command::Menu(items) => {
                        build_menu(&items).map(|m| tray.set_menu(Some(Box::new(m))))
                    }
                };
                if let Err(e) = result {
                    tracing::warn!(%e, "tray update failed");
                }
            }
            continue;
        }
        // SAFETY: dispatching a message this loop just received.
        unsafe {
            let _ = TranslateMessage(&raw const msg);
            DispatchMessageW(&raw const msg);
        }
    }
}

fn build_menu(items: &[TrayMenuItem]) -> PlatformResult<Menu> {
    let menu = Menu::new();
    for item in items {
        menu.append(build_item(item).as_ref())
            .map_err(|e| os_error(&e))?;
    }
    Ok(menu)
}

fn build_item(item: &TrayMenuItem) -> Box<dyn IsMenuItem> {
    match item {
        TrayMenuItem::Item { id, label, enabled } => {
            Box::new(MenuItem::with_id(id.as_str(), label, *enabled, None))
        }
        TrayMenuItem::Check { id, label, checked } => Box::new(CheckMenuItem::with_id(
            id.as_str(),
            label,
            true,
            *checked,
            None,
        )),
        TrayMenuItem::Submenu { label, items } => {
            let sub = Submenu::new(label, true);
            for child in items {
                let _ = sub.append(build_item(child).as_ref());
            }
            Box::new(sub)
        }
        TrayMenuItem::Separator => Box::new(PredefinedMenuItem::separator()),
    }
}

/// The icon for a state: the KIVO mark, greyed when paused, with a status dot for errors and updates.
fn icon_for(state: TrayIcon) -> PlatformResult<Icon> {
    let (rgba, w, h) = pixels_for(state)?;
    Icon::from_rgba(rgba, w, h).map_err(|e| os_error(&e))
}

fn pixels_for(state: TrayIcon) -> PlatformResult<(Vec<u8>, u32, u32)> {
    let (mut rgba, w, h) = decode_mark()?;
    match state {
        TrayIcon::Normal | TrayIcon::Listening => {}
        TrayIcon::Paused => {
            for px in rgba.chunks_exact_mut(4) {
                let grey = ((u32::from(px[0]) * 30 + u32::from(px[1]) * 59 + u32::from(px[2]) * 11)
                    / 100) as u8;
                px[..3].fill(grey);
                px[3] = px[3] / 2 + px[3] / 4; // 75 % opacity
            }
        }
        TrayIcon::Error => dot(&mut rgba, w, h, [0xFF, 0x51, 0x47]),
        TrayIcon::Updating => dot(&mut rgba, w, h, [0x3B, 0x8B, 0xFF]),
    }
    Ok((rgba, w, h))
}

fn decode_mark() -> PlatformResult<(Vec<u8>, u32, u32)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(MARK_PNG));
    decoder.set_transformations(
        png::Transformations::normalize_to_color8() | png::Transformations::ALPHA,
    );
    let mut reader = decoder.read_info().map_err(|e| os_error(&e))?;
    let mut buf = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader.next_frame(&mut buf).map_err(|e| os_error(&e))?;
    buf.truncate(info.buffer_size());
    Ok((buf, info.width, info.height))
}

/// A filled circle in the bottom-right quarter.
fn dot(rgba: &mut [u8], w: u32, h: u32, color: [u8; 3]) {
    let r = f64::from(w.min(h)) * 0.2;
    let (cx, cy) = (f64::from(w) - r - 1.0, f64::from(h) - r - 1.0);
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (f64::from(x) + 0.5 - cx, f64::from(y) + 0.5 - cy);
            if dx * dx + dy * dy <= r * r {
                let i = ((y * w + x) * 4) as usize;
                rgba[i..i + 3].copy_from_slice(&color);
                rgba[i + 3] = 255;
            }
        }
    }
}

fn os_error(e: &dyn std::fmt::Display) -> PlatformError {
    PlatformError::Os {
        code: 0,
        message: e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mark_decodes_to_rgba() {
        let (rgba, w, h) = decode_mark().unwrap();
        assert_eq!((w, h), (64, 64));
        assert_eq!(rgba.len(), 64 * 64 * 4);
        let center = ((32 * 64 + 32) * 4) as usize;
        assert_eq!(rgba[center + 3], 255, "the mark's centre is opaque");
    }

    #[test]
    fn each_state_looks_different_except_listening() {
        let px = |s| pixels_for(s).unwrap().0;
        assert_eq!(
            px(TrayIcon::Listening),
            px(TrayIcon::Normal),
            "the Island shows listening, not the tray"
        );
        let distinct = [
            TrayIcon::Normal,
            TrayIcon::Paused,
            TrayIcon::Error,
            TrayIcon::Updating,
        ]
        .map(px);
        for i in 0..distinct.len() {
            for j in i + 1..distinct.len() {
                assert_ne!(distinct[i], distinct[j]);
            }
        }
        for state in [
            TrayIcon::Normal,
            TrayIcon::Listening,
            TrayIcon::Paused,
            TrayIcon::Error,
            TrayIcon::Updating,
        ] {
            icon_for(state).unwrap();
        }
    }

    /// Needs an interactive desktop session (not CI): shows a real tray icon, updates it, removes it.
    #[test]
    #[ignore = "shows a real tray icon"]
    fn a_real_tray_icon_starts_updates_and_stops() {
        let (tx, _rx) = mpsc::channel();
        let tray = WindowsTray::start(
            "KIVO test",
            &[TrayMenuItem::Item {
                id: "quit".into(),
                label: "Quit".into(),
                enabled: true,
            }],
            move |e| {
                let _ = tx.send(e);
            },
        )
        .unwrap();
        tray.set_icon(TrayIcon::Paused).unwrap();
        tray.set_tooltip("KIVO · Paused").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        drop(tray);
    }
}
