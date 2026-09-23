//! Monitors and window placement (TOOL-08): move a window to another monitor, snap it to a half or
//! quarter of the work area, and put it back (undo).

use kivo_platform::{Displays, Monitor, PlatformError, PlatformResult, Rect, WindowId};
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowPlacement, IsWindow, MONITORINFOF_PRIMARY, SW_RESTORE, SWP_NOACTIVATE, SWP_NOZORDER,
    SetWindowPos, ShowWindow, WINDOWPLACEMENT,
};
use windows::core::BOOL;

#[derive(Default)]
pub struct WindowsDisplays;

fn rect(r: RECT) -> Rect {
    Rect {
        x: r.left,
        y: r.top,
        width: u32::try_from(r.right - r.left).unwrap_or(0),
        height: u32::try_from(r.bottom - r.top).unwrap_or(0),
    }
}

fn hwnd(id: WindowId) -> PlatformResult<HWND> {
    let h = HWND(isize::try_from(id.0).unwrap_or(0) as *mut _);
    // SAFETY: IsWindow accepts any value.
    if unsafe { IsWindow(Some(h)) }.as_bool() {
        Ok(h)
    } else {
        Err(PlatformError::NotFound("that window".into()))
    }
}

impl Displays for WindowsDisplays {
    fn monitors(&self) -> PlatformResult<Vec<Monitor>> {
        unsafe extern "system" fn each(m: HMONITOR, _: HDC, _: *mut RECT, out: LPARAM) -> BOOL {
            // SAFETY: `out` is the Vec passed below, alive for the enumeration.
            let out = unsafe { &mut *(out.0 as *mut Vec<HMONITOR>) };
            out.push(m);
            BOOL(1)
        }
        let mut handles: Vec<HMONITOR> = Vec::new();
        // SAFETY: the callback only runs during this call.
        let _ = unsafe {
            EnumDisplayMonitors(None, None, Some(each), LPARAM(&raw mut handles as isize))
        };
        let mut monitors: Vec<Monitor> = handles
            .into_iter()
            .filter_map(|m| {
                let mut info = MONITORINFOEXW::default();
                info.monitorInfo.cbSize = u32::try_from(size_of::<MONITORINFOEXW>()).unwrap_or(0);
                // SAFETY: a correctly sized MONITORINFOEXW.
                unsafe { GetMonitorInfoW(m, (&raw mut info).cast()) }
                    .as_bool()
                    .then(|| {
                        let len = info.szDevice.iter().position(|&c| c == 0).unwrap_or(0);
                        Monitor {
                            index: 0,
                            name: String::from_utf16_lossy(&info.szDevice[..len]),
                            bounds: rect(info.monitorInfo.rcMonitor),
                            work_area: rect(info.monitorInfo.rcWork),
                            primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
                        }
                    })
            })
            .collect();
        // "Monitor 1, 2, …" left to right, then top to bottom.
        monitors.sort_by_key(|m| (m.bounds.x, m.bounds.y));
        for (i, m) in monitors.iter_mut().enumerate() {
            m.index = u32::try_from(i + 1).unwrap_or(u32::MAX);
        }
        Ok(monitors)
    }

    fn window_bounds(&self, id: WindowId) -> PlatformResult<Rect> {
        let h = hwnd(id)?;
        let mut placement = WINDOWPLACEMENT {
            length: u32::try_from(size_of::<WINDOWPLACEMENT>()).unwrap_or(0),
            ..Default::default()
        };
        // SAFETY: a correctly sized WINDOWPLACEMENT.
        unsafe { GetWindowPlacement(h, &raw mut placement) }
            .map_err(|e| crate::com::os_error(&e))?;
        Ok(rect(placement.rcNormalPosition))
    }

    fn place_window(&self, id: WindowId, bounds: Rect) -> PlatformResult<()> {
        let h = hwnd(id)?;
        // SAFETY: plain window calls on a live window; never activates it.
        unsafe {
            let _ = ShowWindow(h, SW_RESTORE);
            SetWindowPos(
                h,
                None,
                bounds.x,
                bounds.y,
                i32::try_from(bounds.width).unwrap_or(i32::MAX),
                i32::try_from(bounds.height).unwrap_or(i32::MAX),
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
            .map_err(|e| crate::com::os_error(&e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_is_one_primary_monitor_numbered_from_one() {
        let monitors = WindowsDisplays.monitors().unwrap();
        assert!(!monitors.is_empty());
        assert_eq!(monitors.iter().filter(|m| m.primary).count(), 1);
        assert_eq!(monitors[0].index, 1);
        assert!(monitors.iter().all(|m| m.work_area.width <= m.bounds.width));
    }

    #[test]
    fn missing_windows_are_not_found() {
        assert!(matches!(
            WindowsDisplays.window_bounds(WindowId(0x7fff_fff0)),
            Err(PlatformError::NotFound(_))
        ));
    }
}
