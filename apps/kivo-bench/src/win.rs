//! Windows measurement helpers for the suites: performance counters (CPU package power from the
//! RAPL energy meter, GPU engine load), process CPU time and memory, screen sampling, synthetic
//! key presses, and a plain backdrop window.

use std::time::Duration;
use windows::Win32::Foundation::{CloseHandle, FILETIME, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleBitmap,
    CreateCompatibleDC, CreateSolidBrush, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Performance::{
    PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PdhAddEnglishCounterW,
    PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
};
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
    VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, FindWindowW, GWL_EXSTYLE,
    GetForegroundWindow, GetWindowLongPtrW, HWND_TOPMOST, IsWindowVisible, MSG, PM_REMOVE,
    PeekMessageW, RegisterClassW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SetWindowPos, ShowWindow,
    WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{HSTRING, PCWSTR, w};

// ───────── Performance counters ─────────

/// A performance-counter query. Wildcard paths sum over their instances.
pub struct Counters {
    query: PDH_HQUERY,
    counters: Vec<PDH_HCOUNTER>,
}

impl Counters {
    /// Opens a query over English counter paths (e.g. `\Energy Meter(*)\Power`).
    pub fn open(paths: &[&str]) -> Result<Self, String> {
        let mut query = PDH_HQUERY::default();
        // SAFETY: a plain PDH call writing the query handle.
        let status = unsafe { PdhOpenQueryW(None, 0, &raw mut query) };
        if status != 0 {
            return Err(format!("PdhOpenQuery failed ({status:#x})"));
        }
        let mut counters = Vec::new();
        for path in paths {
            let mut counter = PDH_HCOUNTER::default();
            // SAFETY: the query is open; the path is a valid wide string.
            let status =
                unsafe { PdhAddEnglishCounterW(query, &HSTRING::from(*path), 0, &raw mut counter) };
            if status != 0 {
                // SAFETY: closing the query this function opened.
                unsafe { PdhCloseQuery(query) };
                return Err(format!("counter {path} is not available ({status:#x})"));
            }
            counters.push(counter);
        }
        // Rates need two collections; the first sets the baseline.
        // SAFETY: the query is open.
        unsafe { PdhCollectQueryData(query) };
        Ok(Self { query, counters })
    }

    /// Collects now and returns each counter's value (instances summed, `_Total` excluded).
    pub fn sample(&self) -> Result<Vec<f64>, String> {
        // SAFETY: the query is open.
        let status = unsafe { PdhCollectQueryData(self.query) };
        if status != 0 {
            return Err(format!("PdhCollectQueryData failed ({status:#x})"));
        }
        self.counters.iter().map(|&c| sum_instances(c)).collect()
    }
}

impl Drop for Counters {
    fn drop(&mut self) {
        // SAFETY: closing the query this struct owns.
        unsafe { PdhCloseQuery(self.query) };
    }
}

fn sum_instances(counter: PDH_HCOUNTER) -> Result<f64, String> {
    let (mut bytes, mut count) = (0u32, 0u32);
    // SAFETY: a size query (no buffer), as documented.
    unsafe {
        PdhGetFormattedCounterArrayW(
            counter,
            PDH_FMT_DOUBLE,
            &raw mut bytes,
            &raw mut count,
            None,
        )
    };
    if bytes == 0 {
        return Ok(0.0);
    }
    let items = (bytes as usize).div_ceil(size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>());
    let mut buf = vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); items];
    // SAFETY: `buf` holds at least `bytes` bytes of properly aligned items.
    let status = unsafe {
        PdhGetFormattedCounterArrayW(
            counter,
            PDH_FMT_DOUBLE,
            &raw mut bytes,
            &raw mut count,
            Some(buf.as_mut_ptr()),
        )
    };
    if status != 0 {
        return Err(format!("reading a counter failed ({status:#x})"));
    }
    let mut total = 0.0;
    for item in &buf[..count as usize] {
        // SAFETY: PDH filled `szName` with a terminated string inside `buf`.
        let name = unsafe { item.szName.to_string() }.unwrap_or_default();
        if name.eq_ignore_ascii_case("_total") {
            continue;
        }
        // SAFETY: PDH_FMT_DOUBLE fills the double member.
        total += unsafe { item.FmtValue.Anonymous.doubleValue };
    }
    Ok(total)
}

// ───────── Processes ─────────

/// Process ids with this executable name (e.g. "kivo-runtime.exe").
pub fn processes(name: &str) -> Vec<u32> {
    all_processes()
        .into_iter()
        .filter(|(_, _, exe)| exe.eq_ignore_ascii_case(name))
        .map(|(pid, _, _)| pid)
        .collect()
}

/// A process and everything it started, recursively (the app's WebView2 processes).
pub fn with_descendants(root: u32) -> Vec<u32> {
    let all = all_processes();
    let mut tree = vec![root];
    let mut i = 0;
    while i < tree.len() {
        let parent = tree[i];
        tree.extend(
            all.iter()
                .filter(|(pid, ppid, _)| *ppid == parent && *pid != parent && !tree.contains(pid))
                .map(|(pid, _, _)| *pid)
                .collect::<Vec<_>>(),
        );
        i += 1;
    }
    tree
}

/// (pid, parent pid, executable name) for every process.
fn all_processes() -> Vec<(u32, u32, String)> {
    let mut list = Vec::new();
    // SAFETY: a snapshot handle closed below; entries are sized as documented.
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return list;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: u32::try_from(size_of::<PROCESSENTRY32W>()).unwrap_or(0),
            ..Default::default()
        };
        let mut ok = Process32FirstW(snap, &raw mut entry).is_ok();
        while ok {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
            list.push((
                entry.th32ProcessID,
                entry.th32ParentProcessID,
                String::from_utf16_lossy(&entry.szExeFile[..len]),
            ));
            ok = Process32NextW(snap, &raw mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    list
}

/// Total CPU time (user + kernel) and private memory of a process.
pub fn process_usage(pid: u32) -> Result<(Duration, u64), String> {
    // SAFETY: the handle is closed below; the out-structures are sized as documented.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
            .map_err(|e| format!("process {pid}: {e}"))?;
        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        let times = GetProcessTimes(
            handle,
            &raw mut created,
            &raw mut exited,
            &raw mut kernel,
            &raw mut user,
        );
        let mut mem = PROCESS_MEMORY_COUNTERS_EX {
            cb: u32::try_from(size_of::<PROCESS_MEMORY_COUNTERS_EX>()).unwrap_or(0),
            ..Default::default()
        };
        let memory = GetProcessMemoryInfo(handle, (&raw mut mem).cast(), mem.cb);
        let _ = CloseHandle(handle);
        times.map_err(|e| e.to_string())?;
        memory.map_err(|e| e.to_string())?;
        let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
        // FILETIME counts 100 ns ticks.
        let cpu = Duration::from_nanos((ticks(kernel) + ticks(user)) * 100);
        Ok((cpu, mem.PrivateUsage as u64))
    }
}

// ───────── Screen ─────────

/// Grabs a screen rectangle as RGB triples, row by row (desktop composition included).
pub fn grab(x: i32, y: i32, width: i32, height: i32) -> Result<Vec<[u8; 3]>, String> {
    // SAFETY: GDI objects are created, used and released here; the buffer is sized for the DIB.
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        let old = SelectObject(mem, bitmap.into());
        let copied = BitBlt(
            mem,
            0,
            0,
            width,
            height,
            Some(screen),
            x,
            y,
            SRCCOPY | CAPTUREBLT,
        );
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: u32::try_from(size_of::<BITMAPINFOHEADER>()).unwrap_or(0),
                biWidth: width,
                biHeight: -height, // top-down rows
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let pixels = usize::try_from(width * height).unwrap_or(0);
        let mut bgra = vec![0u8; pixels * 4];
        let rows = GetDIBits(
            mem,
            bitmap,
            0,
            u32::try_from(height).unwrap_or(0),
            Some(bgra.as_mut_ptr().cast()),
            &raw mut info,
            DIB_RGB_COLORS,
        );
        SelectObject(mem, old);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        copied.map_err(|e| e.to_string())?;
        if rows == 0 {
            return Err("GetDIBits returned no rows".into());
        }
        Ok(bgra.chunks_exact(4).map(|p| [p[2], p[1], p[0]]).collect())
    }
}

/// Makes this process see physical pixels on every monitor (call once at start).
pub fn dpi_aware() {
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
    };
    // SAFETY: a process-wide setting made before any window exists.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
}

/// The primary monitor's width and height in physical pixels, and its scale (1.0 = 96 DPI).
pub fn primary_screen() -> (i32, i32, f64) {
    use windows::Win32::UI::HiDpi::GetDpiForSystem;
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    // SAFETY: plain queries.
    unsafe {
        (
            GetSystemMetrics(SM_CXSCREEN),
            GetSystemMetrics(SM_CYSCREEN),
            f64::from(GetDpiForSystem()) / 96.0,
        )
    }
}

// ───────── Input ─────────

/// Presses (or releases) keys, in order, with synthetic input.
pub fn keys(vks: &[u16], up: bool) {
    let inputs: Vec<INPUT> = vks
        .iter()
        .map(|&vk| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    dwFlags: if up {
                        KEYEVENTF_KEYUP
                    } else {
                        KEYBD_EVENT_FLAGS(0)
                    },
                    ..Default::default()
                },
            },
        })
        .collect();
    // SAFETY: a well-formed array of keyboard INPUTs.
    unsafe { SendInput(&inputs, i32::try_from(size_of::<INPUT>()).unwrap_or(0)) };
}

// ───────── Windows ─────────

/// The extended style and visibility of the top-level window with this title.
pub fn window_style(title: &str) -> Option<(u32, bool)> {
    // SAFETY: plain window queries.
    unsafe {
        let hwnd = FindWindowW(PCWSTR::null(), &HSTRING::from(title)).ok()?;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "style bits"
        )]
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        Some((ex, IsWindowVisible(hwnd).as_bool()))
    }
}

pub fn foreground() -> isize {
    // SAFETY: a plain query.
    unsafe { GetForegroundWindow() }.0 as isize
}

pub const EX_NOACTIVATE: u32 = WS_EX_NOACTIVATE.0;
pub const EX_TOPMOST: u32 = WS_EX_TOPMOST.0;
pub const EX_TOOLWINDOW: u32 = WS_EX_TOOLWINDOW.0;
/// Click-through (with layered).
pub const EX_TRANSPARENT: u32 = windows::Win32::UI::WindowsAndMessaging::WS_EX_TRANSPARENT.0;

/// A plain, non-activating grey window: a known background to measure the Island against.
pub struct Backdrop(HWND);

extern "system" fn backdrop_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // SAFETY: default handling for a window this module created.
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}

impl Backdrop {
    pub fn show(x: i32, y: i32, width: i32, height: i32, grey: u8) -> Result<Self, String> {
        // SAFETY: registering a class (ignored if it exists) and creating a popup window.
        unsafe {
            let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
            let class = WNDCLASSW {
                lpfnWndProc: Some(backdrop_proc),
                hInstance: instance.into(),
                lpszClassName: w!("KivoBenchBackdrop"),
                hbrBackground: CreateSolidBrush(windows::Win32::Foundation::COLORREF(
                    u32::from(grey) * 0x0001_0101,
                )),
                ..Default::default()
            };
            RegisterClassW(&raw const class);
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                w!("KivoBenchBackdrop"),
                w!("kivo-bench backdrop"),
                WS_POPUP,
                x,
                y,
                width,
                height,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .map_err(|e| e.to_string())?;
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE,
            );
            let backdrop = Self(hwnd);
            backdrop.pump();
            Ok(backdrop)
        }
    }

    /// Lets the window paint.
    pub fn pump(&self) {
        let mut msg = MSG::default();
        // SAFETY: draining this thread's queue.
        while unsafe { PeekMessageW(&raw mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            // SAFETY: dispatching a message just received.
            unsafe { DispatchMessageW(&raw const msg) };
        }
    }
}

impl Drop for Backdrop {
    fn drop(&mut self) {
        // SAFETY: destroying the window this struct created.
        let _ = unsafe { DestroyWindow(self.0) };
    }
}
