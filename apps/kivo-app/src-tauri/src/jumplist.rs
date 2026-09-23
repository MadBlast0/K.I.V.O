//! The taskbar jump list (UX-58): Routines, New conversation, Pause listening and Stop everything.
//! Each item starts KIVO with `--action <name>`; the running app gets it (single instance) and
//! does it, so a jump list item works whether or not a window is open.

/// The jump list's items: (title, action).
pub const TASKS: &[(&str, &str)] = &[
    ("Routines", "routines"),
    ("New conversation", "new-conversation"),
    ("Pause listening", "pause-listening"),
    ("Stop everything", "stop-everything"),
];

/// Replaces KIVO's jump list with `TASKS`.
#[cfg(windows)]
pub fn install() -> windows_core::Result<()> {
    use windows::Win32::Storage::EnhancedStorage::PKEY_Title;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    };
    use windows::Win32::UI::Shell::Common::{IObjectArray, IObjectCollection};
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{
        DestinationList, EnumerableObjectCollection, ICustomDestinationList, IShellLinkW, ShellLink,
    };
    use windows_core::{HSTRING, Interface};

    let exe = std::env::current_exe()
        .map_err(|e| windows_core::Error::new(windows_core::HRESULT(-1), e.to_string()))?;
    // SAFETY: plain COM calls on this thread; every interface is released when dropped.
    unsafe {
        // Already initialized on this thread (by WebView2) is fine.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let list: ICustomDestinationList =
            CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)?;
        let mut max = 0u32;
        let _removed: IObjectArray = list.BeginList(&raw mut max)?;
        let tasks: IObjectCollection =
            CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)?;
        for (title, action) in TASKS {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            link.SetPath(&HSTRING::from(exe.as_os_str()))?;
            link.SetArguments(&HSTRING::from(format!("--action {action}")))?;
            link.SetIconLocation(&HSTRING::from(exe.as_os_str()), 0)?;
            let store: IPropertyStore = link.cast()?;
            // Freed when dropped (`PROPVARIANT` clears itself).
            let value = wide_value(title);
            store.SetValue(&PKEY_Title, &value)?;
            store.Commit()?;
            tasks.AddObject(&link)?;
        }
        let array: IObjectArray = tasks.cast()?;
        list.AddUserTasks(&array)?;
        list.CommitList()?;
    }
    Ok(())
}

/// A `VT_LPWSTR` value (the jump list's titles), its text in task memory, freed by
/// `PropVariantClear` when the value is dropped.
#[cfg(windows)]
fn wide_value(text: &str) -> windows::Win32::System::Com::StructuredStorage::PROPVARIANT {
    use windows::Win32::System::Com::CoTaskMemAlloc;
    use windows::Win32::System::Com::StructuredStorage::{
        PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
    };
    use windows::Win32::System::Variant::VT_LPWSTR;
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    // SAFETY: the buffer holds the whole string with its terminator.
    let ptr = unsafe {
        let ptr = CoTaskMemAlloc(wide.len() * 2).cast::<u16>();
        if !ptr.is_null() {
            std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
        }
        ptr
    };
    PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_LPWSTR,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 {
                    pwszVal: windows_core::PWSTR(ptr),
                },
            }),
        },
    }
}

#[cfg(not(windows))]
pub fn install() -> Result<(), String> {
    Ok(())
}
