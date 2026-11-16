//! Raw Win32 declarations (hand-written, proven in the M0 spike; no binding-crate drift).
#![allow(non_snake_case, non_camel_case_types, clippy::upper_case_acronyms)]

pub type HWND = isize;
pub type HANDLE = isize;
pub type WPARAM = usize;
pub type LPARAM = isize;
pub type LRESULT = isize;

pub const WM_DESTROY: u32 = 0x0002;
pub const WM_CLOSE: u32 = 0x0010;
pub const WM_APP: u32 = 0x8000;
pub const WS_POPUP: u32 = 0x8000_0000;
pub const WS_EX_TOOLWINDOW: u32 = 0x80;
pub const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
pub const ERROR_ALREADY_EXISTS: u32 = 183;
pub const SPI_SETDESKWALLPAPER: u32 = 0x0014;

pub const SW_HIDE: i32 = 0;
pub const SW_SHOWNA: i32 = 8;

pub type WndProc = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;

#[repr(C)]
pub struct WNDCLASSEXW {
    pub cbSize: u32,
    pub style: u32,
    pub lpfnWndProc: Option<WndProc>,
    pub cbClsExtra: i32,
    pub cbWndExtra: i32,
    pub hInstance: isize,
    pub hIcon: isize,
    pub hCursor: isize,
    pub hbrBackground: isize,
    pub lpszMenuName: *const u16,
    pub lpszClassName: *const u16,
    pub hIconSm: isize,
}

#[repr(C)]
pub struct MSG {
    pub hwnd: HWND,
    pub message: u32,
    pub wParam: WPARAM,
    pub lParam: LPARAM,
    pub time: u32,
    pub pt_x: i32,
    pub pt_y: i32,
    pub lPrivate: u32,
}

#[link(name = "user32")]
extern "system" {
    pub fn RegisterClassExW(wc: *const WNDCLASSEXW) -> u16;
    pub fn CreateWindowExW(
        ex: u32,
        class: *const u16,
        name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: HWND,
        menu: isize,
        inst: isize,
        param: *mut core::ffi::c_void,
    ) -> HWND;
    pub fn DestroyWindow(h: HWND) -> i32;
    pub fn DefWindowProcW(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT;
    pub fn GetMessageW(m: *mut MSG, h: HWND, min: u32, max: u32) -> i32;
    pub fn TranslateMessage(m: *const MSG) -> i32;
    pub fn DispatchMessageW(m: *const MSG) -> LRESULT;
    pub fn PostMessageW(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> i32;
    pub fn PostQuitMessage(code: i32);
    pub fn FindWindowW(class: *const u16, title: *const u16) -> HWND;
    pub fn SystemParametersInfoW(
        action: u32,
        param: u32,
        pv: *mut core::ffi::c_void,
        win_ini: u32,
    ) -> i32;
    pub fn ShowWindow(h: HWND, cmd: i32) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    pub fn GetModuleHandleW(name: *const u16) -> isize;
    pub fn CreateMutexW(
        attrs: *const core::ffi::c_void,
        initial_owner: i32,
        name: *const u16,
    ) -> HANDLE;
    pub fn GetLastError() -> u32;
    pub fn CloseHandle(h: HANDLE) -> i32;
}

/// NUL-terminated UTF-16.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// ---------------------------------------------------------------------------
// M2 additions: desktop host discovery, wallpaper window, GDI test pattern,
// primary monitor, DPI awareness check.
// ---------------------------------------------------------------------------

pub const WS_CHILD: u32 = 0x4000_0000;
pub const WS_VISIBLE: u32 = 0x1000_0000;
pub const WS_CLIPSIBLINGS: u32 = 0x0400_0000;
pub const WS_CLIPCHILDREN: u32 = 0x0200_0000;
pub const GWL_STYLE: i32 = -16;
pub const GWL_EXSTYLE: i32 = -20;
pub const SWP_NOACTIVATE: u32 = 0x0010;
pub const SWP_SHOWWINDOW: u32 = 0x0040;
pub const HWND_BOTTOM: HWND = 1;
pub const WM_PAINT: u32 = 0x000F;
pub const RDW_INVALIDATE: u32 = 0x0001;
pub const RDW_ERASE: u32 = 0x0004;
pub const RDW_ALLCHILDREN: u32 = 0x0080;
pub const RDW_UPDATENOW: u32 = 0x0100;
pub const WM_ERASEBKGND: u32 = 0x0014;
pub const ERROR_CLASS_ALREADY_EXISTS: u32 = 1410;
/// `DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2`.
pub const DPI_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct RECT {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[repr(C)]
pub struct MONITORINFO {
    pub cbSize: u32,
    pub rcMonitor: RECT,
    pub rcWork: RECT,
    pub dwFlags: u32,
}

#[repr(C)]
pub struct PAINTSTRUCT {
    pub hdc: isize,
    pub fErase: i32,
    pub rcPaint: RECT,
    pub fRestore: i32,
    pub fIncUpdate: i32,
    pub rgbReserved: [u8; 32],
}

// Layout guards (x64): a wrong struct size would corrupt memory silently.
#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    std::mem::size_of::<RECT>() == 16
        && std::mem::size_of::<MONITORINFO>() == 40
        && std::mem::size_of::<PAINTSTRUCT>() == 72
        && std::mem::size_of::<WNDCLASSEXW>() == 80
        && std::mem::size_of::<MSG>() == 48
);

pub type WindowEnumProc = unsafe extern "system" fn(HWND, LPARAM) -> i32;
pub type MonitorEnumProc = unsafe extern "system" fn(isize, isize, *mut RECT, isize) -> i32;

#[link(name = "user32")]
extern "system" {
    pub fn FindWindowExW(parent: HWND, after: HWND, class: *const u16, title: *const u16) -> HWND;
    pub fn SendMessageTimeoutW(
        h: HWND,
        msg: u32,
        w: WPARAM,
        l: LPARAM,
        flags: u32,
        timeout: u32,
        result: *mut usize,
    ) -> LRESULT;
    pub fn EnumWindows(cb: WindowEnumProc, l: LPARAM) -> i32;
    pub fn EnumChildWindows(parent: HWND, cb: WindowEnumProc, l: LPARAM) -> i32;
    pub fn GetClassNameW(h: HWND, buf: *mut u16, max: i32) -> i32;
    pub fn GetWindowRect(h: HWND, r: *mut RECT) -> i32;
    pub fn GetClientRect(h: HWND, r: *mut RECT) -> i32;
    pub fn IsWindowVisible(h: HWND) -> i32;
    pub fn GetWindowLongPtrW(h: HWND, idx: i32) -> isize;
    pub fn SetWindowLongPtrW(h: HWND, idx: i32, v: isize) -> isize;
    pub fn SetParent(child: HWND, parent: HWND) -> HWND;
    pub fn GetParent(h: HWND) -> HWND;
    pub fn SetWindowPos(h: HWND, after: HWND, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    pub fn BeginPaint(h: HWND, ps: *mut PAINTSTRUCT) -> isize;
    pub fn EndPaint(h: HWND, ps: *const PAINTSTRUCT) -> i32;
    pub fn FillRect(hdc: isize, r: *const RECT, brush: isize) -> i32;
    pub fn EnumDisplayMonitors(
        hdc: isize,
        clip: *const RECT,
        cb: MonitorEnumProc,
        data: isize,
    ) -> i32;
    pub fn GetMonitorInfoW(mon: isize, mi: *mut MONITORINFO) -> i32;
    pub fn GetThreadDpiAwarenessContext() -> isize;
    pub fn AreDpiAwarenessContextsEqual(a: isize, b: isize) -> i32;
    pub fn RedrawWindow(h: HWND, rect: *const RECT, rgn: isize, flags: u32) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    pub fn CreateSolidBrush(color: u32) -> isize;
    pub fn DeleteObject(obj: isize) -> i32;
}

// ---------------------------------------------------------------------------
// M4 additions: monitor enumeration (stable id, DPI, refresh rate).
// ---------------------------------------------------------------------------

pub const EDD_GET_DEVICE_INTERFACE_NAME: u32 = 1;
pub const DISPLAY_DEVICE_ACTIVE: u32 = 1;
/// `ENUM_CURRENT_SETTINGS` is `(DWORD)-1`.
pub const ENUM_CURRENT_SETTINGS: u32 = 0xFFFF_FFFF;
/// `MDT_EFFECTIVE_DPI`.
pub const MDT_EFFECTIVE_DPI: i32 = 0;

/// `MONITORINFO` followed by the GDI device name (`\\.\DISPLAYn`).
#[repr(C)]
pub struct MONITORINFOEXW {
    pub cbSize: u32,
    pub rcMonitor: RECT,
    pub rcWork: RECT,
    pub dwFlags: u32,
    pub szDevice: [u16; 32],
}

#[repr(C)]
pub struct DISPLAY_DEVICEW {
    pub cb: u32,
    pub DeviceName: [u16; 32],
    pub DeviceString: [u16; 128],
    pub StateFlags: u32,
    pub DeviceID: [u16; 128],
    pub DeviceKey: [u16; 128],
}

/// Only `dmSize` and `dmDisplayFrequency` are used; the printer/position union is kept as
/// opaque padding so the total size and every offset still match the C struct (220 bytes).
#[repr(C)]
pub struct DEVMODEW {
    pub dmDeviceName: [u16; 32],
    pub dmSpecVersion: u16,
    pub dmDriverVersion: u16,
    pub dmSize: u16,
    pub dmDriverExtra: u16,
    pub dmFields: u32,
    pub dmUnion: [i32; 4],
    pub dmColor: i16,
    pub dmDuplex: i16,
    pub dmYResolution: i16,
    pub dmTTOption: i16,
    pub dmCollate: i16,
    pub dmFormName: [u16; 32],
    pub dmLogPixels: u16,
    pub dmBitsPerPel: u32,
    pub dmPelsWidth: u32,
    pub dmPelsHeight: u32,
    pub dmDisplayFlags: u32,
    pub dmDisplayFrequency: u32,
    pub dmICMMethod: u32,
    pub dmICMIntent: u32,
    pub dmMediaType: u32,
    pub dmDitherType: u32,
    pub dmReserved1: u32,
    pub dmReserved2: u32,
    pub dmPanningWidth: u32,
    pub dmPanningHeight: u32,
}

const _: () = assert!(
    std::mem::size_of::<MONITORINFOEXW>() == 104
        && std::mem::size_of::<DISPLAY_DEVICEW>() == 840
        && std::mem::size_of::<DEVMODEW>() == 220
);

#[link(name = "user32")]
extern "system" {
    pub fn EnumDisplayDevicesW(
        device: *const u16,
        index: u32,
        dd: *mut DISPLAY_DEVICEW,
        flags: u32,
    ) -> i32;
    pub fn EnumDisplaySettingsW(device: *const u16, mode: u32, dm: *mut DEVMODEW) -> i32;
}

#[link(name = "shcore")]
extern "system" {
    /// Returns an HRESULT (>= 0 on success).
    pub fn GetDpiForMonitor(mon: isize, kind: i32, x: *mut u32, y: *mut u32) -> i32;
}

// ---------------------------------------------------------------------------
// M4 additions: display-change messages and the one-shot debounce timer.
// ---------------------------------------------------------------------------

pub const WM_TIMER: u32 = 0x0113;
pub const WM_DISPLAYCHANGE: u32 = 0x007E;
pub const WM_DPICHANGED: u32 = 0x02E0;
pub const SWP_NOZORDER: u32 = 0x0004;

pub type TimerProc = unsafe extern "system" fn(HWND, u32, usize, u32);

#[link(name = "user32")]
extern "system" {
    /// With a non-NULL `h`, `id` is honoured and re-arming the same id restarts the timer.
    pub fn SetTimer(h: HWND, id: usize, ms: u32, callback: Option<TimerProc>) -> usize;
    pub fn KillTimer(h: HWND, id: usize) -> i32;
    pub fn RegisterWindowMessageW(name: *const u16) -> u32;
}

// ---------------------------------------------------------------------------
// Diagnostics: kernel handle count of this process (leak probe).
// ---------------------------------------------------------------------------

#[link(name = "kernel32")]
extern "system" {
    pub fn GetCurrentProcess() -> HANDLE;
    pub fn GetProcessHandleCount(process: HANDLE, count: *mut u32) -> i32;
}

// ---------------------------------------------------------------------------
// M5 additions: notification-area icon, popup menu, ShellExecute.
// ---------------------------------------------------------------------------

pub const NIM_ADD: u32 = 0;
pub const NIM_MODIFY: u32 = 1;
pub const NIM_DELETE: u32 = 2;
pub const NIF_MESSAGE: u32 = 0x01;
pub const NIF_ICON: u32 = 0x02;
pub const NIF_TIP: u32 = 0x04;

pub const MF_STRING: u32 = 0x0000;
pub const MF_GRAYED: u32 = 0x0001;
pub const MF_CHECKED: u32 = 0x0008;
pub const MF_POPUP: u32 = 0x0010;
pub const MF_SEPARATOR: u32 = 0x0800;
pub const TPM_RIGHTBUTTON: u32 = 0x0002;
pub const TPM_NONOTIFY: u32 = 0x0080;
pub const TPM_RETURNCMD: u32 = 0x0100;

pub const WM_NULL: u32 = 0x0000;
pub const WM_CONTEXTMENU: u32 = 0x007B;
pub const WM_LBUTTONUP: u32 = 0x0202;
pub const WM_RBUTTONUP: u32 = 0x0205;
/// `MAKEINTRESOURCE(IDI_APPLICATION)`.
pub const IDI_APPLICATION: usize = 32512;
pub const SW_SHOWNORMAL: i32 = 1;

#[repr(C)]
pub struct POINT {
    pub x: i32,
    pub y: i32,
}

/// `NOTIFYICONDATAW` (Vista+ layout; the version/timeout union is one `u32`, the GUID is
/// kept as bytes). 976 bytes on x64.
#[repr(C)]
pub struct NOTIFYICONDATAW {
    pub cbSize: u32,
    pub hWnd: HWND,
    pub uID: u32,
    pub uFlags: u32,
    pub uCallbackMessage: u32,
    pub hIcon: isize,
    pub szTip: [u16; 128],
    pub dwState: u32,
    pub dwStateMask: u32,
    pub szInfo: [u16; 256],
    pub uVersion: u32,
    pub szInfoTitle: [u16; 64],
    pub dwInfoFlags: u32,
    pub guidItem: [u8; 16],
    pub hBalloonIcon: isize,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<NOTIFYICONDATAW>() == 976);

#[link(name = "user32")]
extern "system" {
    pub fn CreatePopupMenu() -> isize;
    pub fn AppendMenuW(menu: isize, flags: u32, id: usize, text: *const u16) -> i32;
    pub fn DestroyMenu(menu: isize) -> i32;
    pub fn TrackPopupMenu(
        menu: isize,
        flags: u32,
        x: i32,
        y: i32,
        reserved: i32,
        hwnd: HWND,
        rect: *const RECT,
    ) -> i32;
    pub fn SetForegroundWindow(h: HWND) -> i32;
    pub fn GetCursorPos(p: *mut POINT) -> i32;
    pub fn LoadIconW(inst: isize, name: *const u16) -> isize;
}

#[link(name = "shell32")]
extern "system" {
    pub fn Shell_NotifyIconW(message: u32, data: *mut NOTIFYICONDATAW) -> i32;
    pub fn ShellExecuteW(
        hwnd: HWND,
        operation: *const u16,
        file: *const u16,
        params: *const u16,
        dir: *const u16,
        show: i32,
    ) -> isize;
}

// ---------------------------------------------------------------------------
// M5 step 2 additions: file dialog, COM init, Run-key registry access, balloon text.
// ---------------------------------------------------------------------------

pub const NIF_INFO: u32 = 0x10;
pub const NIIF_INFO: u32 = 0x01;
pub const NIIF_WARNING: u32 = 0x02;

pub const OFN_HIDEREADONLY: u32 = 0x0000_0004;
pub const OFN_NOCHANGEDIR: u32 = 0x0000_0008;
pub const OFN_PATHMUSTEXIST: u32 = 0x0000_0800;
pub const OFN_FILEMUSTEXIST: u32 = 0x0000_1000;
pub const OFN_EXPLORER: u32 = 0x0008_0000;
pub const COINIT_APARTMENTTHREADED: u32 = 0x2;

/// `OPENFILENAMEW`, 152 bytes on x64.
#[repr(C)]
pub struct OPENFILENAMEW {
    pub lStructSize: u32,
    pub hwndOwner: HWND,
    pub hInstance: isize,
    pub lpstrFilter: *const u16,
    pub lpstrCustomFilter: *mut u16,
    pub nMaxCustFilter: u32,
    pub nFilterIndex: u32,
    pub lpstrFile: *mut u16,
    pub nMaxFile: u32,
    pub lpstrFileTitle: *mut u16,
    pub nMaxFileTitle: u32,
    pub lpstrInitialDir: *const u16,
    pub lpstrTitle: *const u16,
    pub Flags: u32,
    pub nFileOffset: u16,
    pub nFileExtension: u16,
    pub lpstrDefExt: *const u16,
    pub lCustData: isize,
    pub lpfnHook: isize,
    pub lpTemplateName: *const u16,
    pub pvReserved: *mut core::ffi::c_void,
    pub dwReserved: u32,
    pub FlagsEx: u32,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<OPENFILENAMEW>() == 152);

/// `HKEY_CURRENT_USER` (0x80000001, sign-extended like the C macro).
pub const HKEY_CURRENT_USER: isize = 0x8000_0001u32 as i32 as isize;
pub const KEY_QUERY_VALUE: u32 = 0x0001;
pub const KEY_SET_VALUE: u32 = 0x0002;
pub const REG_SZ: u32 = 1;
pub const ERROR_FILE_NOT_FOUND: i32 = 2;

#[link(name = "comdlg32")]
extern "system" {
    pub fn GetOpenFileNameW(ofn: *mut OPENFILENAMEW) -> i32;
}

#[link(name = "ole32")]
extern "system" {
    pub fn CoInitializeEx(reserved: *const core::ffi::c_void, coinit: u32) -> i32;
    pub fn CoUninitialize();
}

#[link(name = "advapi32")]
extern "system" {
    pub fn RegCreateKeyExW(
        key: isize,
        subkey: *const u16,
        reserved: u32,
        class: *const u16,
        options: u32,
        sam: u32,
        security: *const core::ffi::c_void,
        result: *mut isize,
        disposition: *mut u32,
    ) -> i32;
    pub fn RegOpenKeyExW(
        key: isize,
        subkey: *const u16,
        options: u32,
        sam: u32,
        result: *mut isize,
    ) -> i32;
    pub fn RegSetValueExW(
        key: isize,
        name: *const u16,
        reserved: u32,
        kind: u32,
        data: *const u8,
        len: u32,
    ) -> i32;
    pub fn RegDeleteValueW(key: isize, name: *const u16) -> i32;
    pub fn RegCloseKey(key: isize) -> i32;
}

// ---------------------------------------------------------------------------
// M6 additions: window event hooks, window filtering (cloaked / layered / owner).
// ---------------------------------------------------------------------------

pub const WS_CAPTION: u32 = 0x00C0_0000;
pub const WS_EX_TRANSPARENT: u32 = 0x20;
pub const WS_EX_LAYERED: u32 = 0x0008_0000;
pub const LWA_ALPHA: u32 = 2;
pub const DWMWA_CLOAKED: u32 = 14;
pub const WINEVENT_OUTOFCONTEXT: u32 = 0;
pub const WINEVENT_SKIPOWNPROCESS: u32 = 2;

pub type WinEventProc = unsafe extern "system" fn(isize, u32, HWND, i32, i32, u32, u32);

#[link(name = "user32")]
extern "system" {
    pub fn SetWinEventHook(
        min: u32,
        max: u32,
        hmod: isize,
        proc_: Option<WinEventProc>,
        pid: u32,
        tid: u32,
        flags: u32,
    ) -> isize;
    pub fn UnhookWinEvent(hook: isize) -> i32;
    pub fn GetForegroundWindow() -> HWND;
    pub fn IsIconic(h: HWND) -> i32;
    pub fn GetWindowThreadProcessId(h: HWND, pid: *mut u32) -> u32;
    pub fn GetLayeredWindowAttributes(
        h: HWND,
        key: *mut u32,
        alpha: *mut u8,
        flags: *mut u32,
    ) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    pub fn GetCurrentProcessId() -> u32;
}

#[link(name = "dwmapi")]
extern "system" {
    pub fn DwmGetWindowAttribute(h: HWND, attr: u32, pv: *mut core::ffi::c_void, cb: u32) -> i32;
}

// ---------------------------------------------------------------------------
// M6 step 3 additions: session (lock / remote) and power notifications.
// ---------------------------------------------------------------------------

pub const WM_WTSSESSION_CHANGE: u32 = 0x02B1;
pub const WM_POWERBROADCAST: u32 = 0x0218;
pub const NOTIFY_FOR_THIS_SESSION: u32 = 0;
pub const SM_REMOTESESSION: i32 = 0x1000;

#[link(name = "wtsapi32")]
extern "system" {
    pub fn WTSRegisterSessionNotification(hwnd: HWND, flags: u32) -> i32;
    pub fn WTSUnRegisterSessionNotification(hwnd: HWND) -> i32;
}

#[link(name = "user32")]
extern "system" {
    pub fn GetSystemMetrics(index: i32) -> i32;
}

// ---------------------------------------------------------------------------
// M6 step 3b additions: screen on/off notification (works with Modern Standby too).
// ---------------------------------------------------------------------------

pub const PBT_POWERSETTINGCHANGE: usize = 0x8013;
pub const DEVICE_NOTIFY_WINDOW_HANDLE: u32 = 0;

/// `GUID_CONSOLE_DISPLAY_STATE` {6fe69556-704a-47a0-8f24-c28d936fda47} in memory layout.
pub const GUID_CONSOLE_DISPLAY_STATE: [u8; 16] = [
    0x56, 0x95, 0xe6, 0x6f, 0x4a, 0x70, 0xa0, 0x47, 0x8f, 0x24, 0xc2, 0x8d, 0x93, 0x6f, 0xda, 0x47,
];

/// `POWERBROADCAST_SETTING`: the GUID, the data length and the data (a DWORD here).
#[repr(C)]
pub struct POWERBROADCAST_SETTING {
    pub PowerSetting: [u8; 16],
    pub DataLength: u32,
    pub Data: [u8; 1],
}

#[link(name = "user32")]
extern "system" {
    pub fn RegisterPowerSettingNotification(
        recipient: HANDLE,
        setting: *const u8,
        flags: u32,
    ) -> isize;
    pub fn UnregisterPowerSettingNotification(handle: isize) -> i32;
}

// ---------------------------------------------------------------------------
// M6 step 4 additions: user notification state (exclusive fullscreen / presentation).
// ---------------------------------------------------------------------------

#[link(name = "shell32")]
extern "system" {
    /// Returns an HRESULT; `state` is a `QUERY_USER_NOTIFICATION_STATE`.
    pub fn SHQueryUserNotificationState(state: *mut i32) -> i32;
}
