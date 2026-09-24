//! Hardware and system state (plan §25, §90–91): CPU, RAM, GPUs, power, and whether a fullscreen
//! app or quiet time is active. Used for engine recommendations, the benchmark fingerprint and
//! proactive-speech rules.

use kivo_platform::{GpuInfo, PlatformError, PlatformResult, SystemInfo, SystemSnapshot};
use windows::Win32::Foundation::FILETIME;
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ADAPTER_DESC1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1,
};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::{
    ALL_PROCESSOR_GROUPS, GetActiveProcessorCount, GetSystemTimes,
};
use windows::Win32::UI::Shell::{
    QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_QUIET_TIME, QUNS_RUNNING_D3D_FULL_SCREEN,
    SHQueryUserNotificationState,
};
use windows::core::w;

pub struct WindowsSystemInfo;

impl SystemInfo for WindowsSystemInfo {
    fn gpu_load(&self) -> Option<u8> {
        gpu_load()
    }
    fn gpu_memory(&self, pids: &[u32]) -> Option<u64> {
        gpu_memory(pids)
    }
    fn networks(&self) -> Vec<String> {
        crate::devices::networks()
    }
    fn usb_devices(&self) -> Vec<String> {
        crate::devices::usb_devices()
    }
    fn network(&self) -> Option<kivo_platform::Network> {
        use windows::Win32::NetworkManagement::IpHelper::GetNetworkConnectivityHint;
        use windows::Win32::Networking::WinSock::{
            NL_NETWORK_CONNECTIVITY_HINT, NetworkConnectivityCostHintFixed,
            NetworkConnectivityCostHintVariable,
            NetworkConnectivityLevelHintConstrainedInternetAccess,
            NetworkConnectivityLevelHintInternetAccess, NetworkConnectivityLevelHintUnknown,
        };
        let mut hint = NL_NETWORK_CONNECTIVITY_HINT::default();
        // SAFETY: fills the structure it's given.
        let status = unsafe { GetNetworkConnectivityHint(&raw mut hint) };
        if status.is_err() || hint.ConnectivityLevel == NetworkConnectivityLevelHintUnknown {
            return None;
        }
        Some(kivo_platform::Network {
            online: hint.ConnectivityLevel == NetworkConnectivityLevelHintInternetAccess
                || hint.ConnectivityLevel == NetworkConnectivityLevelHintConstrainedInternetAccess,
            metered: hint.ConnectivityCost == NetworkConnectivityCostHintFixed
                || hint.ConnectivityCost == NetworkConnectivityCostHintVariable
                || hint.OverDataLimit
                || hint.Roaming,
        })
    }
    fn attention(&self) -> PlatformResult<kivo_platform::Attention> {
        let (fullscreen_app, focus_mode) = notification_state();
        Ok(kivo_platform::Attention {
            fullscreen_app,
            focus_mode,
        })
    }

    fn presence(&self) -> kivo_platform::Presence {
        crate::presence::presence()
    }

    fn process_usage(&self, pid: u32) -> Option<kivo_platform::ProcessUsage> {
        process_usage(pid)
    }

    fn snapshot(&self) -> PlatformResult<SystemSnapshot> {
        let (on_battery, battery_percent) = power();
        let (fullscreen_app, focus_mode) = notification_state();
        Ok(SystemSnapshot {
            cpu_name: cpu_name().unwrap_or_else(|| "Unknown CPU".into()),
            // SAFETY: a plain query with no pointers.
            logical_cpus: unsafe { GetActiveProcessorCount(ALL_PROCESSOR_GROUPS) },
            ram_mb: ram_mb()?,
            ram_free_mb: ram_free_mb(),
            cpu_load_percent: cpu_load(),
            gpus: gpus(),
            on_battery,
            battery_percent,
            fullscreen_app,
            focus_mode,
        })
    }
}

/// The busiest adapter's 3D-engine use, from the "GPU Engine" performance counters over 200 ms:
/// each process's share summed per adapter (`luid_0x…_phys_N`), the highest adapter wins.
fn gpu_load() -> Option<u8> {
    use windows::Win32::System::Performance::{
        PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA,
        PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
        PdhOpenQueryW,
    };
    // SAFETY: the query is closed on every path; buffers are sized as PDH reports.
    unsafe {
        let mut query = PDH_HQUERY::default();
        if PdhOpenQueryW(None, 0, &raw mut query) != 0 {
            return None;
        }
        let close = |q: PDH_HQUERY| {
            let _ = PdhCloseQuery(q);
        };
        let mut counter = PDH_HCOUNTER::default();
        if PdhAddEnglishCounterW(
            query,
            w!("\\GPU Engine(*engtype_3D)\\Utilization Percentage"),
            0,
            &raw mut counter,
        ) != 0
        {
            close(query);
            return None;
        }
        // Rates need two samples.
        let _ = PdhCollectQueryData(query);
        std::thread::sleep(std::time::Duration::from_millis(200));
        if PdhCollectQueryData(query) != 0 {
            close(query);
            return None;
        }
        let (mut size, mut count) = (0u32, 0u32);
        if PdhGetFormattedCounterArrayW(
            counter,
            PDH_FMT_DOUBLE,
            &raw mut size,
            &raw mut count,
            None,
        ) != PDH_MORE_DATA
        {
            close(query);
            return Some(0);
        }
        let items = (size as usize).div_ceil(std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>());
        let mut buffer = vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); items.max(1)];
        let status = PdhGetFormattedCounterArrayW(
            counter,
            PDH_FMT_DOUBLE,
            &raw mut size,
            &raw mut count,
            Some(buffer.as_mut_ptr()),
        );
        if status != 0 {
            close(query);
            return None;
        }
        let mut per_adapter: std::collections::HashMap<String, f64> =
            std::collections::HashMap::new();
        for item in &buffer[..count as usize] {
            let name = item.szName.to_string().unwrap_or_default();
            // "pid_1234_luid_0x0000_0x0000A1B2_phys_0_eng_0_engtype_3D": the adapter part.
            let adapter = name
                .split("_eng_")
                .next()
                .and_then(|n| n.split_once("luid_"))
                .map_or(name.clone(), |(_, rest)| rest.to_owned());
            *per_adapter.entry(adapter).or_default() += item.FmtValue.Anonymous.doubleValue;
        }
        close(query);
        let busiest = per_adapter.values().copied().fold(0.0_f64, f64::max);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "0–100"
        )]
        Some(busiest.clamp(0.0, 100.0).round() as u8)
    }
}

/// The dedicated GPU memory of `pids`, from the "GPU Process Memory" counters (one instance per
/// process and adapter, named `pid_1234_luid_…_phys_0`).
fn gpu_memory(pids: &[u32]) -> Option<u64> {
    use windows::Win32::System::Performance::{
        PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_LARGE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA,
        PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
        PdhOpenQueryW,
    };
    // SAFETY: the query is closed on every path; buffers are sized as PDH reports.
    unsafe {
        let mut query = PDH_HQUERY::default();
        if PdhOpenQueryW(None, 0, &raw mut query) != 0 {
            return None;
        }
        let mut counter = PDH_HCOUNTER::default();
        let ok = PdhAddEnglishCounterW(
            query,
            w!("\\GPU Process Memory(*)\\Dedicated Usage"),
            0,
            &raw mut counter,
        ) == 0
            && PdhCollectQueryData(query) == 0;
        let mut total = None;
        if ok {
            let (mut size, mut count) = (0u32, 0u32);
            if PdhGetFormattedCounterArrayW(
                counter,
                PDH_FMT_LARGE,
                &raw mut size,
                &raw mut count,
                None,
            ) == PDH_MORE_DATA
            {
                let items =
                    (size as usize).div_ceil(std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>());
                let mut buffer = vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); items.max(1)];
                if PdhGetFormattedCounterArrayW(
                    counter,
                    PDH_FMT_LARGE,
                    &raw mut size,
                    &raw mut count,
                    Some(buffer.as_mut_ptr()),
                ) == 0
                {
                    let mut sum = 0u64;
                    for item in &buffer[..count as usize] {
                        let name = item.szName.to_string().unwrap_or_default();
                        let pid = name
                            .strip_prefix("pid_")
                            .and_then(|r| r.split('_').next())
                            .and_then(|p| p.parse::<u32>().ok());
                        if pid.is_some_and(|p| pids.contains(&p)) {
                            sum += u64::try_from(item.FmtValue.Anonymous.largeValue).unwrap_or(0);
                        }
                    }
                    total = Some(sum);
                }
            } else {
                total = Some(0);
            }
        }
        let _ = PdhCloseQuery(query);
        total
    }
}

/// CPU time and private memory of one process (`PROCESS_QUERY_LIMITED_INFORMATION` only).
fn process_usage(pid: u32) -> Option<kivo_platform::ProcessUsage> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX2,
    };
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    // SAFETY: the handle is closed below; the out-structures are sized as documented.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
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
        let mut mem = PROCESS_MEMORY_COUNTERS_EX2 {
            cb: u32::try_from(size_of::<PROCESS_MEMORY_COUNTERS_EX2>()).unwrap_or(0),
            ..Default::default()
        };
        let memory = GetProcessMemoryInfo(handle, (&raw mut mem).cast(), mem.cb);
        let _ = CloseHandle(handle);
        times.ok()?;
        memory.ok()?;
        let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
        // FILETIME counts 100 ns ticks.
        Some(kivo_platform::ProcessUsage {
            cpu_ms: (ticks(kernel) + ticks(user)) / 10_000,
            memory_bytes: mem.PrivateWorkingSetSize as u64,
        })
    }
}

fn os_error(e: &windows::core::Error) -> PlatformError {
    PlatformError::Os {
        code: i64::from(e.code().0),
        message: e.message(),
    }
}

/// The marketing name, e.g. "AMD Ryzen 7 7840HS w/ Radeon 780M Graphics".
fn cpu_name() -> Option<String> {
    let mut buf = [0u16; 128];
    let mut bytes = u32::try_from(size_of_val(&buf)).ok()?;
    // SAFETY: `buf` is writable for `bytes` bytes; RRF_RT_REG_SZ guarantees a terminated string.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0"),
            w!("ProcessorNameString"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&raw mut bytes),
        )
    };
    if status.is_err() {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]).trim().to_owned()).filter(|s| !s.is_empty())
}

fn ram_mb() -> PlatformResult<u64> {
    let mut status = MEMORYSTATUSEX {
        dwLength: u32::try_from(size_of::<MEMORYSTATUSEX>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: `status` is writable with its length field set, as documented.
    unsafe { GlobalMemoryStatusEx(&raw mut status) }.map_err(|e| os_error(&e))?;
    Ok(status.ullTotalPhys / (1024 * 1024))
}

fn ram_free_mb() -> u64 {
    let mut status = MEMORYSTATUSEX {
        dwLength: u32::try_from(size_of::<MEMORYSTATUSEX>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: `status` is writable with its length field set, as documented.
    unsafe { GlobalMemoryStatusEx(&raw mut status) }
        .map_or(0, |()| status.ullAvailPhys / (1024 * 1024))
}

/// CPU busy share over 100 ms, from the system's idle/kernel/user times.
fn cpu_load() -> u8 {
    fn times() -> Option<(u64, u64)> {
        let (mut idle, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        // SAFETY: three writable FILETIMEs.
        unsafe {
            GetSystemTimes(
                Some(&raw mut idle),
                Some(&raw mut kernel),
                Some(&raw mut user),
            )
        }
        .ok()?;
        let n = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
        // Kernel time includes idle time.
        Some((n(idle), n(kernel) + n(user)))
    }
    let Some((idle1, total1)) = times() else {
        return 0;
    };
    std::thread::sleep(std::time::Duration::from_millis(100));
    let Some((idle2, total2)) = times() else {
        return 0;
    };
    let total = total2.saturating_sub(total1);
    let busy = total.saturating_sub(idle2.saturating_sub(idle1));
    #[allow(clippy::cast_possible_truncation, reason = "0–100")]
    let percent = (busy * 100).checked_div(total).unwrap_or(0).min(100) as u8;
    percent
}

/// Hardware adapters with their dedicated memory; the software (WARP) adapter is skipped.
fn gpus() -> Vec<GpuInfo> {
    let list = || -> windows::core::Result<Vec<GpuInfo>> {
        let mut out = Vec::new();
        // SAFETY: plain COM calls on the factory DXGI returned; enumeration stops at NOT_FOUND.
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
            let mut i = 0;
            while let Ok(adapter) = factory.EnumAdapters1(i) {
                i += 1;
                let desc: DXGI_ADAPTER_DESC1 = adapter.GetDesc1()?;
                #[allow(clippy::cast_sign_loss, reason = "DXGI flags are a bit set")]
                if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                    continue;
                }
                let len = desc
                    .Description
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(desc.Description.len());
                out.push(GpuInfo {
                    name: String::from_utf16_lossy(&desc.Description[..len]),
                    vram_mb: (desc.DedicatedVideoMemory / (1024 * 1024)) as u64,
                });
            }
        }
        Ok(out)
    };
    list().unwrap_or_default()
}

/// On battery, and the charge if a battery is present.
fn power() -> (bool, Option<u8>) {
    let mut status = SYSTEM_POWER_STATUS::default();
    // SAFETY: `status` is a writable SYSTEM_POWER_STATUS.
    if unsafe { GetSystemPowerStatus(&raw mut status) }.is_err() {
        return (false, None);
    }
    // ACLineStatus: 0 offline, 1 online, 255 unknown. BatteryLifePercent: 255 means unknown.
    let on_battery = status.ACLineStatus == 0;
    let percent = (status.BatteryLifePercent <= 100 && status.BatteryFlag != 128)
        .then_some(status.BatteryLifePercent);
    (on_battery, percent)
}

/// (fullscreen app or presentation, quiet time). Windows reports these through the shell's
/// notification state, which is what it uses to hold back its own notifications.
fn notification_state() -> (bool, bool) {
    // SAFETY: a plain query with no pointers.
    match unsafe { SHQueryUserNotificationState() } {
        Ok(state) => (
            state == QUNS_BUSY
                || state == QUNS_RUNNING_D3D_FULL_SCREEN
                || state == QUNS_PRESENTATION_MODE,
            state == QUNS_QUIET_TIME,
        ),
        Err(_) => (false, false),
    }
}

/// Minutes to add to UTC for local time now, daylight saving included (limit periods reset at
/// local midnight, BRAINS §9).
pub fn utc_offset_minutes() -> i32 {
    use windows::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
    let mut info = TIME_ZONE_INFORMATION::default();
    // SAFETY: `info` is a valid, writable TIME_ZONE_INFORMATION. 2 is TIME_ZONE_ID_DAYLIGHT.
    let id = unsafe { GetTimeZoneInformation(&raw mut info) };
    let bias = info.Bias
        + if id == 2 {
            info.DaylightBias
        } else {
            info.StandardBias
        };
    -bias
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_processs_gpu_memory() {
        // This test process holds no GPU memory; the call still answers.
        let used = WindowsSystemInfo.gpu_memory(&[std::process::id()]);
        println!("{used:?}");
        assert!(used.is_some());
    }

    #[test]
    fn reads_how_busy_the_gpu_is() {
        // Any GPU has performance counters on Windows 10 1709+; the value depends on what runs.
        let load = WindowsSystemInfo.gpu_load();
        println!("{load:?}");
        assert!(load.is_some_and(|l| l <= 100));
    }

    #[test]
    fn tells_whether_it_is_online() {
        // Windows 10 2004 and later always answer; this dev PC is online.
        let network = WindowsSystemInfo.network().expect("a connectivity hint");
        println!("{network:?}");
        assert!(network.online);
    }

    #[test]
    fn describes_this_machine() {
        let snap = WindowsSystemInfo.snapshot().unwrap();
        println!("{snap:#?}");
        assert!(!snap.cpu_name.is_empty() && snap.cpu_name != "Unknown CPU");
        assert!(snap.logical_cpus >= 1);
        assert!(snap.ram_mb >= 1024, "any supported PC has at least 1 GB");
        assert!(snap.gpus.iter().all(|g| !g.name.is_empty()));
        if let Some(p) = snap.battery_percent {
            assert!(p <= 100);
        }
    }
}
