//! Windows toast notifications (TOOL-18, UX-57). KIVO isn't a packaged app, so it registers its
//! own app id under `HKCU\Software\Classes\AppUserModelId` (name and icon), which is what
//! Windows shows as the sender. Buttons and the reply field come back to the runtime through the
//! toast's `Activated` event while KIVO is running (it always is when it shows one).

use crate::com::{Com, os_error};
use kivo_platform::{Notification, Notifications, PlatformResult};
use std::sync::Mutex;
use windows::Data::Xml::Dom::XmlDocument;
use windows::Foundation::TypedEventHandler;
use windows::UI::Notifications::{
    ToastActivatedEventArgs, ToastNotification, ToastNotificationManager,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegSetValueExW,
};
use windows::core::{HSTRING, IInspectable, Interface, PCWSTR};

/// KIVO's app id for notifications.
pub const AUMID: &str = "KIVO.Desktop";
/// How many shown toasts to keep alive for their button events.
const KEEP: usize = 16;

/// A button or reply the user sent from a toast.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToastAnswer {
    /// The action id (`NotificationAction::id`), or empty when the toast body was clicked.
    pub action: String,
    /// Text typed into the reply field, if any.
    pub reply: Option<String>,
}

pub struct WindowsNotifications {
    answers: std::sync::mpsc::Sender<ToastAnswer>,
    shown: Mutex<Vec<ToastNotification>>,
}

impl WindowsNotifications {
    /// Registers KIVO as a notification sender with `icon` (a PNG path) and delivers answers to
    /// `answers`.
    pub fn new(
        icon: Option<&std::path::Path>,
        answers: std::sync::mpsc::Sender<ToastAnswer>,
    ) -> PlatformResult<Self> {
        register(icon)?;
        Ok(Self {
            answers,
            shown: Mutex::new(Vec::new()),
        })
    }
}

fn set_string(key: HKEY, name: &str, value: &str) {
    let name = HSTRING::from(name);
    let wide: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 string valid for the call.
    let bytes = unsafe { std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2) };
    let _ = unsafe { RegSetValueExW(key, &name, None, REG_SZ, Some(bytes)) };
}

fn register(icon: Option<&std::path::Path>) -> PlatformResult<()> {
    let path = HSTRING::from(format!("Software\\Classes\\AppUserModelId\\{AUMID}"));
    let mut key = HKEY::default();
    // SAFETY: creates or opens the per-user key; closed below.
    unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &path,
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut key,
            None,
        )
        .ok()
        .map_err(|e| os_error(&e))?;
    }
    set_string(key, "DisplayName", "KIVO");
    if let Some(icon) = icon {
        set_string(key, "IconUri", &icon.to_string_lossy());
    }
    // SAFETY: the key was opened above.
    let _ = unsafe { RegCloseKey(key) };
    Ok(())
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// The toast XML (ToastGeneric) for a notification.
pub fn toast_xml(n: &Notification) -> String {
    let mut xml = format!(
        "<toast activationType=\"foreground\"><visual><binding template=\"ToastGeneric\"><text>{}</text>",
        escape(&n.title)
    );
    if !n.body.is_empty() {
        xml.push_str(&format!("<text>{}</text>", escape(&n.body)));
    }
    xml.push_str("</binding></visual>");
    if n.reply || !n.actions.is_empty() {
        xml.push_str("<actions>");
        if n.reply {
            xml.push_str(
                "<input id=\"reply\" type=\"text\" placeHolderContent=\"Reply to KIVO\"/>",
            );
        }
        for a in &n.actions {
            let hint = if n.reply && a.id.ends_with(".send") {
                " hint-inputId=\"reply\""
            } else {
                ""
            };
            xml.push_str(&format!(
                "<action content=\"{}\" arguments=\"{}\" activationType=\"foreground\"{hint}/>",
                escape(&a.label),
                escape(&a.id)
            ));
        }
        xml.push_str("</actions>");
    }
    if n.silent {
        xml.push_str("<audio silent=\"true\"/>");
    }
    xml.push_str("</toast>");
    xml
}

impl Notifications for WindowsNotifications {
    fn show(&self, notification: &Notification) -> PlatformResult<()> {
        let _com = Com::init()?;
        let doc = XmlDocument::new().map_err(|e| os_error(&e))?;
        doc.LoadXml(&HSTRING::from(toast_xml(notification)))
            .map_err(|e| os_error(&e))?;
        let toast = ToastNotification::CreateToastNotification(&doc).map_err(|e| os_error(&e))?;
        let answers = self.answers.clone();
        toast
            .Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
                move |_, args| {
                    if let Some(args) = args
                        .as_ref()
                        .and_then(|a| a.cast::<ToastActivatedEventArgs>().ok())
                    {
                        let action = args.Arguments().map(|a| a.to_string()).unwrap_or_default();
                        let reply = args
                            .UserInput()
                            .ok()
                            .and_then(|inputs| inputs.Lookup(&HSTRING::from("reply")).ok())
                            .and_then(|v| v.cast::<windows::Foundation::IPropertyValue>().ok())
                            .and_then(|v| v.GetString().ok())
                            .map(|s| s.to_string())
                            .filter(|s| !s.is_empty());
                        let _ = answers.send(ToastAnswer { action, reply });
                    }
                    Ok(())
                },
            ))
            .map_err(|e| os_error(&e))?;
        let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(AUMID))
            .map_err(|e| os_error(&e))?;
        notifier.Show(&toast).map_err(|e| os_error(&e))?;
        let mut shown = self
            .shown
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        shown.push(toast);
        if shown.len() > KEEP {
            shown.remove(0);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivo_platform::NotificationAction;

    #[test]
    fn toast_xml_escapes_text_and_wires_buttons() {
        let n = Notification {
            title: "KIVO is still running".into(),
            body: "Closing the window keeps KIVO in the tray <here> & listening.".into(),
            actions: vec![
                NotificationAction {
                    id: "first-close.settings".into(),
                    label: "Settings".into(),
                },
                NotificationAction {
                    id: "first-close.quit".into(),
                    label: "Quit KIVO".into(),
                },
            ],
            reply: false,
            silent: true,
        };
        let xml = toast_xml(&n);
        assert!(xml.contains("&lt;here&gt; &amp; listening"));
        assert!(xml.contains("<audio silent=\"true\"/>"));
        assert!(xml.contains("arguments=\"first-close.quit\""));
        assert!(!xml.contains("<input"));
        // The document parses (WinRT's own XML parser).
        let doc = XmlDocument::new().unwrap();
        doc.LoadXml(&HSTRING::from(xml)).unwrap();
    }

    #[test]
    fn a_reply_field_feeds_the_send_button() {
        let n = Notification {
            title: "Task finished".into(),
            body: String::new(),
            actions: vec![NotificationAction {
                id: "task.send".into(),
                label: "Send".into(),
            }],
            reply: true,
            silent: false,
        };
        let xml = toast_xml(&n);
        assert!(xml.contains("<input id=\"reply\""));
        assert!(xml.contains("hint-inputId=\"reply\""));
    }
}
