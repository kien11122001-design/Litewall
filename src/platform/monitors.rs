//! Monitor queries. M2 needs only the primary monitor and a DPI-awareness check;
//! M4 extends this file to full enumeration (`Vec<MonitorInfo>`, stable ids).
use super::sys::*;
use crate::core::geometry::RectI32;
use crate::core::monitor::{choose_id, dedupe_ids, MonitorInfo};
use std::io;
use std::ptr::null;

const MONITORINFOF_PRIMARY: u32 = 1;

unsafe extern "system" fn primary_cb(hmon: isize, _dc: isize, _r: *mut RECT, data: isize) -> i32 {
    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        rcMonitor: RECT::default(),
        rcWork: RECT::default(),
        dwFlags: 0,
    };
    // SAFETY: `mi` is valid with `cbSize` set; `data` is the address of the caller's
    // live `Option<RectI32>` (the callback only runs during `EnumDisplayMonitors`).
    unsafe {
        if GetMonitorInfoW(hmon, &mut mi) != 0 && mi.dwFlags & MONITORINFOF_PRIMARY != 0 {
            let r = mi.rcMonitor;
            *(data as *mut Option<RectI32>) = Some(RectI32::new(r.left, r.top, r.right, r.bottom));
        }
    }
    1
}

/// Primary monitor rectangle in virtual-desktop physical pixels.
/// Physical (not scaled) values require PerMonitorV2 awareness from the manifest.
pub fn primary_rect() -> io::Result<RectI32> {
    let mut found: Option<RectI32> = None;
    // SAFETY: the callback runs only during this call and `found` outlives it.
    unsafe {
        EnumDisplayMonitors(
            0,
            null(),
            primary_cb,
            &mut found as *mut Option<RectI32> as isize,
        );
    }
    found.ok_or_else(|| io::Error::other("no primary monitor reported by EnumDisplayMonitors"))
}

/// Whether the manifest took effect (`PerMonitorV2`). Logged at startup.
pub fn is_per_monitor_v2() -> bool {
    // SAFETY: plain handle values in and out; no preconditions.
    unsafe {
        AreDpiAwarenessContextsEqual(
            GetThreadDpiAwarenessContext(),
            DPI_CONTEXT_PER_MONITOR_AWARE_V2,
        ) != 0
    }
}

// ------------------------------------------------------------------ M4: full enumeration

fn utf16_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

unsafe extern "system" fn collect_cb(hmon: isize, _dc: isize, _r: *mut RECT, data: isize) -> i32 {
    // SAFETY: `data` is the address of the caller's live `Vec<isize>`; the callback only
    // runs during `EnumDisplayMonitors`.
    unsafe { (*(data as *mut Vec<isize>)).push(hmon) };
    1
}

/// Monitor rect, primary flag and GDI device name for one `HMONITOR`.
fn monitor_info(hmon: isize) -> Option<(RECT, bool, [u16; 32])> {
    // SAFETY: MONITORINFOEXW is plain old data; all-zero is a valid value.
    let mut mi: MONITORINFOEXW = unsafe { std::mem::zeroed() };
    mi.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: the struct starts with a MONITORINFO and `cbSize` announces the larger
    // MONITORINFOEXW, which is exactly how the API is meant to be called.
    let ok = unsafe { GetMonitorInfoW(hmon, &mut mi as *mut MONITORINFOEXW as *mut MONITORINFO) };
    (ok != 0).then_some((
        mi.rcMonitor,
        mi.dwFlags & MONITORINFOF_PRIMARY != 0,
        mi.szDevice,
    ))
}

/// Device-interface path of the monitor attached to GDI output `device`
/// (`\\?\DISPLAY#<EDID id>#<instance>&UID<n>#{guid}`); stable across reboots.
fn interface_path(device: &[u16; 32]) -> Option<String> {
    let mut first = None;
    for i in 0..8 {
        // SAFETY: DISPLAY_DEVICEW is plain old data.
        let mut dd: DISPLAY_DEVICEW = unsafe { std::mem::zeroed() };
        dd.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
        // SAFETY: `device` is NUL-terminated (filled by GetMonitorInfoW) and `dd` is valid.
        let ok = unsafe {
            EnumDisplayDevicesW(device.as_ptr(), i, &mut dd, EDD_GET_DEVICE_INTERFACE_NAME)
        };
        if ok == 0 {
            break;
        }
        let id = utf16_to_string(&dd.DeviceID);
        if dd.StateFlags & DISPLAY_DEVICE_ACTIVE != 0 {
            return Some(id);
        }
        first.get_or_insert(id);
    }
    first
}

/// Current refresh rate in Hz, `0.0` when the driver does not report one.
fn refresh_hz(device: &[u16; 32]) -> f32 {
    // SAFETY: DEVMODEW is plain old data.
    let mut dm: DEVMODEW = unsafe { std::mem::zeroed() };
    dm.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
    // SAFETY: `device` is NUL-terminated and `dm` is valid with `dmSize` set.
    let ok = unsafe { EnumDisplaySettingsW(device.as_ptr(), ENUM_CURRENT_SETTINGS, &mut dm) };
    // 0 and 1 mean "hardware default".
    if ok != 0 && dm.dmDisplayFrequency > 1 {
        dm.dmDisplayFrequency as f32
    } else {
        0.0
    }
}

/// Effective DPI (96 = 100 %); 96 when unavailable.
fn dpi(hmon: isize) -> u32 {
    let (mut x, mut y) = (0u32, 0u32);
    // SAFETY: valid out-pointers; a stale handle just returns a failure HRESULT.
    let hr = unsafe { GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut x, &mut y) };
    if hr >= 0 && x > 0 {
        x
    } else {
        96
    }
}

/// All attached monitors, sorted left-to-right then top-to-bottom, with unique stable ids.
/// Rects are physical pixels in virtual-desktop coordinates (needs PerMonitorV2).
pub fn enumerate() -> io::Result<Vec<MonitorInfo>> {
    let mut handles: Vec<isize> = Vec::new();
    // SAFETY: the callback runs only during this call and `handles` outlives it.
    let ok = unsafe {
        EnumDisplayMonitors(
            0,
            null(),
            collect_cb,
            &mut handles as *mut Vec<isize> as isize,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut out = Vec::new();
    for h in handles {
        let Some((r, primary, device)) = monitor_info(h) else {
            log::warn!("GetMonitorInfoW failed for 0x{h:X}; skipping");
            continue;
        };
        let device_name = utf16_to_string(&device);
        let iface = interface_path(&device).unwrap_or_default();
        log::debug!("monitor 0x{h:X}: device={device_name} interface={iface:?}");
        out.push(MonitorInfo {
            id: choose_id(&iface, &device_name),
            rect_px: RectI32::new(r.left, r.top, r.right, r.bottom),
            dpi: dpi(h),
            refresh_hz: refresh_hz(&device),
            primary,
        });
    }
    out.sort_by_key(|m| (m.rect_px.left, m.rect_px.top));
    dedupe_ids(&mut out);
    if out.is_empty() {
        return Err(io::Error::other(
            "no monitors reported by EnumDisplayMonitors",
        ));
    }
    Ok(out)
}
