//! Per-monitor wallpaper window.
//!
//! M2 paints a GDI test pattern so placement can be judged by eye; M3 replaces the
//! painting with libmpv (`wid` = this HWND). Style flow as proven in M0: create as
//! `WS_POPUP`, convert to `WS_CHILD`, `SetParent` into the wallpaper `WorkerW`, then
//! position in parent-relative coordinates (never `WS_POPUP | WS_CHILD` together).
use super::desktop::Host;
use super::sys::*;
use crate::core::geometry::{to_parent_relative, RectI32};
use std::io;
use std::ptr::{null, null_mut};

pub const CLASS_NAME: &str = "LiteWall.Wallpaper";

// COLORREF is 0x00BBGGRR.
const COLOR_BACKGROUND: u32 = 0x0064_5A10; // teal
const COLOR_BORDER: u32 = 0x0000_8CFF; // orange
const COLOR_CROSS: u32 = 0x00FF_FFFF; // white
const COLOR_ORIGIN: u32 = 0x0028_28DC; // red square in the top-left corner

const BORDER: i32 = 8;
const MARKER: i32 = 120;

/// Test pattern: teal fill, white centre cross, orange border, red top-left square.
/// A wrong size shows as a missing border edge, a wrong origin as a misplaced square.
unsafe fn paint(h: HWND) {
    // SAFETY: standard BeginPaint/EndPaint pair on the window's own thread.
    unsafe {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let hdc = BeginPaint(h, &mut ps);
        if hdc == 0 {
            return;
        }
        let mut rc = RECT::default();
        GetClientRect(h, &mut rc);
        let (w, ht) = (rc.right, rc.bottom);
        let fill = |left: i32, top: i32, right: i32, bottom: i32, color: u32| {
            let brush = CreateSolidBrush(color);
            let r = RECT {
                left,
                top,
                right,
                bottom,
            };
            FillRect(hdc, &r, brush);
            DeleteObject(brush);
        };
        fill(0, 0, w, ht, COLOR_BACKGROUND);
        fill(w / 2 - 1, 0, w / 2 + 1, ht, COLOR_CROSS);
        fill(0, ht / 2 - 1, w, ht / 2 + 1, COLOR_CROSS);
        fill(0, 0, w, BORDER, COLOR_BORDER);
        fill(0, ht - BORDER, w, ht, COLOR_BORDER);
        fill(0, 0, BORDER, ht, COLOR_BORDER);
        fill(w - BORDER, 0, w, ht, COLOR_BORDER);
        fill(
            BORDER,
            BORDER,
            BORDER + MARKER,
            BORDER + MARKER,
            COLOR_ORIGIN,
        );
        EndPaint(h, &ps);
    }
}

unsafe extern "system" fn wndproc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        // WM_PAINT draws every pixel, so skip the erase to avoid flicker.
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            // SAFETY: called on the window's own thread from its wndproc.
            unsafe { paint(h) };
            0
        }
        // SAFETY: default handling for everything else.
        _ => unsafe { DefWindowProcW(h, m, wp, lp) },
    }
}

fn register_class() -> io::Result<()> {
    let class = wide(CLASS_NAME);
    // SAFETY: all pointers are valid for the duration of the call; `class` outlives it.
    unsafe {
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: GetModuleHandleW(null()),
            hIcon: 0,
            hCursor: 0,
            hbrBackground: 0,
            lpszMenuName: null(),
            lpszClassName: class.as_ptr(),
            hIconSm: 0,
        };
        if RegisterClassExW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

unsafe extern "system" fn collect_children(h: HWND, l: LPARAM) -> i32 {
    // SAFETY: `l` is the address of a live `Vec<HWND>` owned by the caller; the callback
    // only runs during `EnumChildWindows`.
    unsafe { (*(l as *mut Vec<HWND>)).push(h) };
    1
}

/// A wallpaper window attached to the desktop host. Destroyed on drop.
pub struct WallpaperWindow {
    hwnd: HWND,
    host: HWND,
}

impl WallpaperWindow {
    /// Create the window and attach it to `host`, covering `monitor`
    /// (virtual-desktop physical pixels; the origin may be negative).
    pub fn create(host: &Host, monitor: RectI32) -> io::Result<Self> {
        register_class()?;
        let class = wide(CLASS_NAME);
        // SAFETY: `class` is a valid NUL-terminated UTF-16 string for the call's duration.
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class.as_ptr(),
                class.as_ptr(),
                WS_POPUP | WS_CLIPSIBLINGS | WS_CLIPCHILDREN,
                monitor.left,
                monitor.top,
                100,
                100,
                0,
                0,
                GetModuleHandleW(null()),
                null_mut(),
            )
        };
        if hwnd == 0 {
            return Err(io::Error::last_os_error());
        }
        // The struct owns the window from here on: early returns destroy it via Drop.
        let window = Self {
            hwnd,
            host: host.hwnd,
        };
        window.attach(host, monitor)?;
        Ok(window)
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    fn attach(&self, host: &Host, monitor: RectI32) -> io::Result<()> {
        // SAFETY: both handles are live windows and every out-pointer is valid.
        unsafe {
            let style = GetWindowLongPtrW(self.hwnd, GWL_STYLE) as u32;
            let child_style = (style & !WS_POPUP) | WS_CHILD | WS_VISIBLE;
            SetWindowLongPtrW(self.hwnd, GWL_STYLE, child_style as isize);
            SetParent(self.hwnd, host.hwnd);
            // SetParent returns NULL both on failure and for a top-level previous parent,
            // so verify the result instead of trusting the return value.
            if GetParent(self.hwnd) != host.hwnd {
                return Err(io::Error::other(
                    "SetParent into the wallpaper host did not take effect",
                ));
            }
            // The host spans the virtual desktop; its top-left is the parent origin
            // (same approach as the M0 spike, which worked with negative origins).
            let mut parent = RECT::default();
            if GetWindowRect(host.hwnd, &mut parent) == 0 {
                return Err(io::Error::last_os_error());
            }
            let rel = to_parent_relative(monitor, (parent.left, parent.top));
            log::debug!(
                "attach: monitor={monitor:?} parent_origin=({},{}) child={rel:?}",
                parent.left,
                parent.top
            );
            let ok = SetWindowPos(
                self.hwnd,
                HWND_BOTTOM,
                rel.left,
                rel.top,
                rel.width() as i32,
                rel.height() as i32,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        let actual = self.screen_rect()?;
        if actual == monitor {
            log::info!("wallpaper window 0x{:X} placed on {monitor:?}", self.hwnd);
        } else {
            log::warn!(
                "placement mismatch for 0x{:X}: expected {monitor:?}, actual {actual:?}",
                self.hwnd
            );
        }
        Ok(())
    }

    /// Move/resize to cover `monitor` (virtual-desktop physical pixels). Reads the host's
    /// current origin every time, so it also follows a host that Explorer moved or resized
    /// after a display change. Visibility is left unchanged.
    pub fn move_to(&self, host: &Host, monitor: RectI32) -> io::Result<()> {
        // SAFETY: live window handles and a valid out-pointer.
        unsafe {
            let mut parent = RECT::default();
            if GetWindowRect(host.hwnd, &mut parent) == 0 {
                return Err(io::Error::last_os_error());
            }
            let rel = to_parent_relative(monitor, (parent.left, parent.top));
            let ok = SetWindowPos(
                self.hwnd,
                HWND_BOTTOM,
                rel.left,
                rel.top,
                rel.width() as i32,
                rel.height() as i32,
                SWP_NOACTIVATE,
            );
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        self.fit_children();
        let actual = self.screen_rect()?;
        if actual == monitor {
            log::info!("wallpaper window 0x{:X} moved to {monitor:?}", self.hwnd);
        } else {
            log::warn!(
                "placement mismatch for 0x{:X}: expected {monitor:?}, actual {actual:?}",
                self.hwnd
            );
        }
        Ok(())
    }

    /// Make mpv's own child window (created inside this one via `wid`) fill our client
    /// area again after a resize; harmless if mpv already did it.
    fn fit_children(&self) {
        let mut kids: Vec<HWND> = Vec::new();
        let mut client = RECT::default();
        // SAFETY: live handle, valid out-pointers; the callback runs only during the call.
        unsafe {
            if GetClientRect(self.hwnd, &mut client) == 0 {
                return;
            }
            EnumChildWindows(
                self.hwnd,
                collect_children,
                &mut kids as *mut Vec<HWND> as LPARAM,
            );
            for k in kids {
                if GetParent(k) == self.hwnd {
                    SetWindowPos(
                        k,
                        0,
                        0,
                        0,
                        client.right,
                        client.bottom,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
            }
        }
    }

    /// Window rectangle in virtual-desktop coordinates.
    fn screen_rect(&self) -> io::Result<RectI32> {
        let mut r = RECT::default();
        // SAFETY: live window handle and valid out-pointer.
        if unsafe { GetWindowRect(self.hwnd, &mut r) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(RectI32::new(r.left, r.top, r.right, r.bottom))
    }
}

/// Show or hide a wallpaper window without activating it (F-08 failure path).
/// A stale handle just makes the call fail harmlessly.
pub fn set_visible(hwnd: HWND, visible: bool) {
    // SAFETY: plain handle + constant; no pointers involved.
    unsafe { ShowWindow(hwnd, if visible { SW_SHOWNA } else { SW_HIDE }) };
}

impl Drop for WallpaperWindow {
    fn drop(&mut self) {
        // SAFETY: as before; RedrawWindow on a stale host just fails harmlessly.
        unsafe {
            DestroyWindow(self.hwnd);
            RedrawWindow(
                self.host,
                std::ptr::null(),
                0,
                RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW,
            );
            super::desktop::refresh_wallpaper();
        }
        log::debug!("wallpaper window 0x{:X} destroyed", self.hwnd);
    }
}
