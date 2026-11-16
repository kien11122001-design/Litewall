//! LiteWall M0 feasibility spike (Windows 10, Classic WorkerW). THROWAWAY CODE.
//!
//! Goal: prove `mpv --wid` embedding into the WorkerW behind desktop icons.
//! Raw Win32 declarations are hand-written (no `windows` crate) so nothing here
//! depends on binding-version drift. Everything is `unsafe`; real code later
//! confines it to `platform/` and `mpv/ffi.rs`.
//!
//! Usage (put libmpv-2.dll next to the exe):
//!   litewall-spike.exe <video> [--monitor N] [--seconds N] [--vo gpu-next|gpu]
//!                      [--hwdec auto-safe|d3d11va-copy|no] [--spawn-variant 0|1]
//!                      [--normal] [--dump] [--rewid]
//! Logs: spike.log (this program), spike-mpv.log (mpv) in the current directory.

#[cfg(not(windows))]
fn main() {
    eprintln!("litewall-spike only runs on Windows.");
}

#[cfg(windows)]
fn main() {
    app::run();
}

#[cfg(windows)]
#[allow(non_snake_case, non_camel_case_types, clippy::upper_case_acronyms)]
mod app {
    use libloading::Library;
    use std::ffi::{c_char, c_int, c_void, CStr, CString};
    use std::io::Write;
    use std::ptr::{null, null_mut};

    type HWND = isize;
    type WPARAM = usize;
    type LPARAM = isize;
    type LRESULT = isize;

    #[repr(C)]
    #[derive(Clone, Copy, Default, Debug)]
    struct RECT {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[repr(C)]
    struct MONITORINFO {
        cbSize: u32,
        rcMonitor: RECT,
        rcWork: RECT,
        dwFlags: u32,
    }
    #[repr(C)]
    struct MSG {
        hwnd: HWND,
        message: u32,
        wParam: WPARAM,
        lParam: LPARAM,
        time: u32,
        pt_x: i32,
        pt_y: i32,
        lPrivate: u32,
    }
    type WndProc = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;
    #[repr(C)]
    struct WNDCLASSEXW {
        cbSize: u32,
        style: u32,
        lpfnWndProc: Option<WndProc>,
        cbClsExtra: i32,
        cbWndExtra: i32,
        hInstance: isize,
        hIcon: isize,
        hCursor: isize,
        hbrBackground: isize,
        lpszMenuName: *const u16,
        lpszClassName: *const u16,
        hIconSm: isize,
    }
    #[repr(C)]
    struct PROCESS_MEMORY_COUNTERS_EX {
        cb: u32,
        PageFaultCount: u32,
        PeakWorkingSetSize: usize,
        WorkingSetSize: usize,
        QuotaPeakPagedPoolUsage: usize,
        QuotaPagedPoolUsage: usize,
        QuotaPeakNonPagedPoolUsage: usize,
        QuotaNonPagedPoolUsage: usize,
        PagefileUsage: usize,
        PeakPagefileUsage: usize,
        PrivateUsage: usize,
    }

    #[link(name = "user32")]
    extern "system" {
        fn FindWindowW(class: *const u16, title: *const u16) -> HWND;
        fn FindWindowExW(parent: HWND, after: HWND, class: *const u16, title: *const u16) -> HWND;
        fn SendMessageTimeoutW(
            h: HWND,
            msg: u32,
            w: WPARAM,
            l: LPARAM,
            flags: u32,
            timeout: u32,
            result: *mut usize,
        ) -> isize;
        fn EnumWindows(cb: unsafe extern "system" fn(HWND, LPARAM) -> i32, l: LPARAM) -> i32;
        fn EnumChildWindows(p: HWND, cb: unsafe extern "system" fn(HWND, LPARAM) -> i32, l: LPARAM) -> i32;
        fn GetClassNameW(h: HWND, buf: *mut u16, max: i32) -> i32;
        fn GetWindowRect(h: HWND, r: *mut RECT) -> i32;
        fn IsWindowVisible(h: HWND) -> i32;
        fn GetWindowLongPtrW(h: HWND, idx: i32) -> isize;
        fn SetWindowLongPtrW(h: HWND, idx: i32, v: isize) -> isize;
        fn SetParent(child: HWND, parent: HWND) -> HWND;
        fn SetWindowPos(h: HWND, after: HWND, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
        fn CreateWindowExW(
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
            param: *mut c_void,
        ) -> HWND;
        fn RegisterClassExW(wc: *const WNDCLASSEXW) -> u16;
        fn DefWindowProcW(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT;
        fn GetMessageW(m: *mut MSG, h: HWND, min: u32, max: u32) -> i32;
        fn TranslateMessage(m: *const MSG) -> i32;
        fn DispatchMessageW(m: *const MSG) -> LRESULT;
        fn PostQuitMessage(code: i32);
        fn SetTimer(h: HWND, id: usize, ms: u32, f: *const c_void) -> usize;
        fn KillTimer(h: HWND, id: usize) -> i32;
        fn EnumDisplayMonitors(
            hdc: isize,
            clip: *const RECT,
            cb: unsafe extern "system" fn(isize, isize, *mut RECT, isize) -> i32,
            data: isize,
        ) -> i32;
        fn GetMonitorInfoW(h: isize, mi: *mut MONITORINFO) -> i32;
        fn DestroyWindow(h: HWND) -> i32;
        fn LoadCursorW(inst: isize, name: *const u16) -> isize;
        fn SetProcessDpiAwarenessContext(ctx: isize) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleW(name: *const u16) -> isize;
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(p: isize, c: *mut PROCESS_MEMORY_COUNTERS_EX, cb: u32) -> i32;
    }

    const WS_POPUP: u32 = 0x8000_0000;
    const WS_CHILD: u32 = 0x4000_0000;
    const WS_VISIBLE: u32 = 0x1000_0000;
    const WS_CLIPSIBLINGS: u32 = 0x0400_0000;
    const WS_CLIPCHILDREN: u32 = 0x0200_0000;
    const WS_OVERLAPPEDWINDOW_VISIBLE: u32 = 0x00CF_0000 | WS_VISIBLE;
    const WS_EX_TOOLWINDOW: u32 = 0x80;
    const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
    const GWL_STYLE: i32 = -16;
    const GWL_EXSTYLE: i32 = -20;
    const SWP_NOACTIVATE: u32 = 0x10;
    const SWP_SHOWWINDOW: u32 = 0x40;
    const HWND_BOTTOM: HWND = 1;
    const WM_TIMER: u32 = 0x0113;
    const MSG_SPAWN_WORKERW: u32 = 0x052C;
    const DPI_PER_MONITOR_V2: isize = -4;
    const IDC_ARROW: usize = 32512;

    // ---------------------------------------------------------------- logging
    fn log(msg: &str) {
        println!("{msg}");
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("spike.log") {
            let _ = writeln!(f, "{msg}");
        }
    }

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn class_name(h: HWND) -> String {
        let mut buf = [0u16; 256];
        let n = unsafe { GetClassNameW(h, buf.as_mut_ptr(), 256) };
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }

    unsafe fn rect_of(h: HWND) -> RECT {
        let mut r = RECT::default();
        unsafe { GetWindowRect(h, &mut r) };
        r
    }

    // ---------------------------------------------------------- window tree dump
    unsafe extern "system" fn collect_cb(h: HWND, l: LPARAM) -> i32 {
        unsafe { (*(l as *mut Vec<HWND>)).push(h) };
        1
    }

    unsafe fn describe(h: HWND, indent: &str) -> String {
        let (cls, vis, r) = unsafe { (class_name(h), IsWindowVisible(h) != 0, rect_of(h)) };
        let (st, ex) = unsafe { (GetWindowLongPtrW(h, GWL_STYLE), GetWindowLongPtrW(h, GWL_EXSTYLE)) };
        format!(
            "{indent}hwnd=0x{h:X} class={cls} visible={vis} rect=({},{})-({},{}) style=0x{:X} ex=0x{:X}",
            r.left, r.top, r.right, r.bottom, st as u32, ex as u32
        )
    }

    fn dump_tree(title: &str) {
        log(&format!("--- window tree: {title} ---"));
        unsafe {
            let mut tops: Vec<HWND> = Vec::new();
            EnumWindows(collect_cb, &mut tops as *mut _ as LPARAM);
            for &t in &tops {
                let cls = class_name(t);
                if cls != "Progman" && cls != "WorkerW" {
                    continue;
                }
                log(&describe(t, ""));
                let mut kids: Vec<HWND> = Vec::new();
                EnumChildWindows(t, collect_cb, &mut kids as *mut _ as LPARAM);
                for k in kids {
                    log(&describe(k, "    "));
                }
            }
        }
        log("--- end tree ---");
    }

    // ------------------------------------------------------------- desktop host
    struct Host {
        layout: &'static str,
        workerw: HWND,
    }

    unsafe extern "system" fn find_workerw_cb(h: HWND, l: LPARAM) -> i32 {
        let out = l as *mut HWND;
        let shell = unsafe { FindWindowExW(h, 0, w("SHELLDLL_DefView").as_ptr(), null()) };
        if shell != 0 {
            // The WorkerW *after* the one that hosts the icons is the one behind them.
            let next = unsafe { FindWindowExW(0, h, w("WorkerW").as_ptr(), null()) };
            if next != 0 {
                unsafe { *out = next };
            }
        }
        1
    }

    fn find_host(variant: u32) -> Option<Host> {
        unsafe {
            let progman = FindWindowW(w("Progman").as_ptr(), null());
            log(&format!("Progman = 0x{progman:X}"));
            if progman == 0 {
                return None;
            }
            let (wp, lp) = if variant == 0 { (0xD, 1) } else { (0, 0) };
            let mut res = 0usize;
            let ok = SendMessageTimeoutW(progman, MSG_SPAWN_WORKERW, wp, lp, 0, 1000, &mut res);
            log(&format!("0x052C(wParam={wp:#X}, lParam={lp}) -> ret={ok} res={res}"));
            dump_tree("after 0x052C");
            let mut worker: HWND = 0;
            EnumWindows(find_workerw_cb, &mut worker as *mut _ as LPARAM);
            let defview_in_progman = FindWindowExW(progman, 0, w("SHELLDLL_DefView").as_ptr(), null()) != 0;
            log(&format!("workerw(behind icons)=0x{worker:X} defview_in_progman={defview_in_progman}"));
            if worker != 0 {
                Some(Host { layout: "Classic", workerw: worker })
            } else if defview_in_progman {
                log("Layout looks RAISED (Win11 24H2+): out of scope for this spike (extended).");
                None
            } else {
                None
            }
        }
    }

    // ----------------------------------------------------------------- monitors
    unsafe extern "system" fn monitor_cb(hmon: isize, _dc: isize, _r: *mut RECT, data: isize) -> i32 {
        let v = data as *mut Vec<(RECT, bool)>;
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            rcMonitor: RECT::default(),
            rcWork: RECT::default(),
            dwFlags: 0,
        };
        if unsafe { GetMonitorInfoW(hmon, &mut mi) } != 0 {
            unsafe { (*v).push((mi.rcMonitor, mi.dwFlags & 1 != 0)) };
        }
        1
    }

    fn monitors() -> Vec<RECT> {
        let mut v: Vec<(RECT, bool)> = Vec::new();
        unsafe { EnumDisplayMonitors(0, null(), monitor_cb, &mut v as *mut _ as isize) };
        // Primary first, then the rest in enumeration order (stable enough for a spike).
        v.sort_by_key(|(_, primary)| !*primary);
        for (i, (r, p)) in v.iter().enumerate() {
            log(&format!("monitor[{i}] rect=({},{})-({},{}) primary={p}", r.left, r.top, r.right, r.bottom));
        }
        v.into_iter().map(|(r, _)| r).collect()
    }

    // ------------------------------------------------------------------ windows
    unsafe extern "system" fn wndproc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        unsafe { DefWindowProcW(h, m, wp, lp) }
    }

    fn register_class() -> Vec<u16> {
        let name = w("LiteWallSpike");
        unsafe {
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: 0,
                lpfnWndProc: Some(wndproc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: GetModuleHandleW(null()),
                hIcon: 0,
                hCursor: LoadCursorW(0, IDC_ARROW as *const u16),
                hbrBackground: 0,
                lpszMenuName: null(),
                lpszClassName: name.as_ptr(),
                hIconSm: 0,
            };
            if RegisterClassExW(&wc) == 0 {
                log("RegisterClassExW failed (already registered?)");
            }
        }
        name
    }

    /// Normal top-level window (sanity check that libmpv + wid work at all).
    fn create_normal_window(class: &[u16]) -> HWND {
        unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                w("LiteWall spike (normal)").as_ptr(),
                WS_OVERLAPPEDWINDOW_VISIBLE,
                100,
                100,
                960,
                540,
                0,
                0,
                GetModuleHandleW(null()),
                null_mut(),
            )
        }
    }

    /// Wallpaper window: create as POPUP, then convert to CHILD and reparent into
    /// the WorkerW (never WS_POPUP|WS_CHILD together).
    fn create_wallpaper_window(class: &[u16], host: &Host, mon: RECT) -> HWND {
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class.as_ptr(),
                w("LiteWall spike").as_ptr(),
                WS_POPUP | WS_CLIPSIBLINGS | WS_CLIPCHILDREN,
                mon.left,
                mon.top,
                100,
                100,
                0,
                0,
                GetModuleHandleW(null()),
                null_mut(),
            );
            if hwnd == 0 {
                log("CreateWindowExW failed");
                return 0;
            }
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
            let new_style = (style & !WS_POPUP) | WS_CHILD | WS_VISIBLE;
            SetWindowLongPtrW(hwnd, GWL_STYLE, new_style as isize);
            let prev = SetParent(hwnd, host.workerw);
            log(&format!("SetParent -> previous parent 0x{prev:X} (0 may mean failure)"));
            // Parent-relative coordinates: monitor rect minus the parent's screen origin
            // (same math as core::geometry::to_parent_relative).
            let pr = rect_of(host.workerw);
            let (x, y) = (mon.left - pr.left, mon.top - pr.top);
            let (cw, ch) = (mon.right - mon.left, mon.bottom - mon.top);
            log(&format!("parent origin=({},{}) child=({x},{y}) size={cw}x{ch}", pr.left, pr.top));
            let ok = SetWindowPos(hwnd, HWND_BOTTOM, x, y, cw, ch, SWP_NOACTIVATE | SWP_SHOWWINDOW);
            log(&format!("SetWindowPos ok={} hwnd=0x{hwnd:X}", ok != 0));
            log(&describe(hwnd, "wallpaper: "));
            hwnd
        }
    }

    // --------------------------------------------------------------------- mpv
    struct Mpv {
        _lib: Library,
        h: *mut c_void,
        set_option: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
        set_property: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
        get_property: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_char,
        free: unsafe extern "C" fn(*mut c_void),
        command: unsafe extern "C" fn(*mut c_void, *const *const c_char) -> c_int,
        destroy: unsafe extern "C" fn(*mut c_void),
        err_str: unsafe extern "C" fn(c_int) -> *const c_char,
    }

    impl Mpv {
        fn load() -> Result<Mpv, String> {
            unsafe {
                let lib =
                    Library::new("libmpv-2.dll").map_err(|e| format!("cannot load libmpv-2.dll: {e}"))?;
                macro_rules! sym {
                    ($n:literal, $t:ty) => {
                        *lib.get::<$t>(concat!($n, "\0").as_bytes()).map_err(|e| format!("{}: {e}", $n))?
                    };
                }
                let create: unsafe extern "C" fn() -> *mut c_void =
                    sym!("mpv_create", unsafe extern "C" fn() -> *mut c_void);
                let m = Mpv {
                    h: create(),
                    set_option: sym!(
                        "mpv_set_option_string",
                        unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int
                    ),
                    set_property: sym!(
                        "mpv_set_property_string",
                        unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int
                    ),
                    get_property: sym!(
                        "mpv_get_property_string",
                        unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_char
                    ),
                    free: sym!("mpv_free", unsafe extern "C" fn(*mut c_void)),
                    command: sym!(
                        "mpv_command",
                        unsafe extern "C" fn(*mut c_void, *const *const c_char) -> c_int
                    ),
                    destroy: sym!("mpv_terminate_destroy", unsafe extern "C" fn(*mut c_void)),
                    err_str: sym!("mpv_error_string", unsafe extern "C" fn(c_int) -> *const c_char),
                    _lib: lib,
                };
                if m.h.is_null() {
                    return Err("mpv_create returned NULL".into());
                }
                Ok(m)
            }
        }
        fn err(&self, rc: c_int) -> String {
            unsafe { CStr::from_ptr((self.err_str)(rc)).to_string_lossy().into_owned() }
        }
        fn option(&self, k: &str, v: &str) {
            let (ck, cv) = (CString::new(k).unwrap(), CString::new(v).unwrap());
            let rc = unsafe { (self.set_option)(self.h, ck.as_ptr(), cv.as_ptr()) };
            if rc < 0 {
                log(&format!("  option {k}={v} FAILED: {} ({rc})", self.err(rc)));
            }
        }
        fn set(&self, k: &str, v: &str) -> c_int {
            let (ck, cv) = (CString::new(k).unwrap(), CString::new(v).unwrap());
            unsafe { (self.set_property)(self.h, ck.as_ptr(), cv.as_ptr()) }
        }
        fn get(&self, k: &str) -> String {
            let ck = CString::new(k).unwrap();
            unsafe {
                let p = (self.get_property)(self.h, ck.as_ptr());
                if p.is_null() {
                    return "<unavailable>".into();
                }
                let s = CStr::from_ptr(p).to_string_lossy().into_owned();
                (self.free)(p as *mut c_void);
                s
            }
        }
        fn command(&self, args: &[&str]) -> c_int {
            let owned: Vec<CString> = args.iter().map(|a| CString::new(*a).unwrap()).collect();
            let mut ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
            ptrs.push(null());
            unsafe { (self.command)(self.h, ptrs.as_ptr()) }
        }
    }

    // Initialising is split out because mpv_initialize is resolved separately.
    fn mpv_initialize(m: &Mpv) -> c_int {
        unsafe {
            let f: libloading::Symbol<unsafe extern "C" fn(*mut c_void) -> c_int> =
                match m._lib.get(b"mpv_initialize\0") {
                    Ok(f) => f,
                    Err(e) => {
                        log(&format!("mpv_initialize symbol missing: {e}"));
                        return -1;
                    }
                };
            f(m.h)
        }
    }

    struct Args {
        video: String,
        monitor: usize,
        seconds: u32,
        vo: String,
        hwdec: String,
        variant: u32,
        normal: bool,
        dump: bool,
        rewid: bool,
    }

    fn parse_args() -> Option<Args> {
        let mut a = Args {
            video: String::new(),
            monitor: 0,
            seconds: 0,
            vo: "gpu-next".into(),
            hwdec: "auto-safe".into(),
            variant: 0,
            normal: false,
            dump: false,
            rewid: false,
        };
        let mut it = std::env::args().skip(1);
        while let Some(x) = it.next() {
            match x.as_str() {
                "--monitor" => a.monitor = it.next()?.parse().ok()?,
                "--seconds" => a.seconds = it.next()?.parse().ok()?,
                "--vo" => a.vo = it.next()?,
                "--hwdec" => a.hwdec = it.next()?,
                "--spawn-variant" => a.variant = it.next()?.parse().ok()?,
                "--normal" => a.normal = true,
                "--dump" => a.dump = true,
                "--rewid" => a.rewid = true,
                _ if !x.starts_with("--") => a.video = x,
                _ => return None,
            }
        }
        if a.video.is_empty() && !a.dump {
            return None;
        }
        Some(a)
    }

    fn mem_report(tag: &str) {
        unsafe {
            let mut c: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
            c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
            if K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) != 0 {
                log(&format!(
                    "[{tag}] private={} MB working_set={} MB (whole process incl. mpv)",
                    c.PrivateUsage >> 20,
                    c.WorkingSetSize >> 20
                ));
            }
        }
    }

    pub fn run() {
        let Some(args) = parse_args() else {
            eprintln!("usage: litewall-spike <video> [--monitor N] [--seconds N] [--vo gpu-next|gpu] [--hwdec auto-safe|d3d11va-copy|no] [--spawn-variant 0|1] [--normal] [--dump] [--rewid]");
            return;
        };
        unsafe { SetProcessDpiAwarenessContext(DPI_PER_MONITOR_V2) };
        log("===== litewall spike start =====");

        if args.dump {
            dump_tree("before");
            let _ = find_host(args.variant);
            return;
        }

        let class = register_class();
        let mons = monitors();
        let host = if args.normal { None } else { find_host(args.variant) };
        if !args.normal && host.is_none() {
            log("FAIL: no WorkerW host found. See window tree above; try --spawn-variant 1.");
            return;
        }
        if let Some(h) = &host {
            log(&format!("layout={} host=0x{:X}", h.layout, h.workerw));
        }
        let Some(mon) = mons.get(args.monitor).copied() else {
            log("FAIL: --monitor index out of range");
            return;
        };

        let make_window = || match &host {
            Some(h) => create_wallpaper_window(&class, h, mon),
            None => create_normal_window(&class),
        };
        let hwnd = make_window();
        if hwnd == 0 {
            return;
        }

        let mpv = match Mpv::load() {
            Ok(m) => m,
            Err(e) => {
                log(&format!("FAIL: {e}"));
                return;
            }
        };
        log("--- applying mpv options (PRD §7 profile) ---");
        mpv.option("wid", &(hwnd as i64).to_string()); // must be before initialize
        for (k, v) in [
            ("config", "no"),
            ("load-scripts", "no"),
            ("ytdl", "no"),
            ("osc", "no"),
            ("osd-level", "0"),
            ("input-default-bindings", "no"),
            ("input-vo-keyboard", "no"),
            ("input-cursor", "no"),
            ("cursor-autohide", "always"),
            ("terminal", "no"),
            ("idle", "yes"),
            ("force-window", "yes"),
            ("stop-screensaver", "no"),
            ("keep-open", "no"),
            ("loop-file", "inf"),
            ("hwdec", args.hwdec.as_str()),
            ("vo", args.vo.as_str()),
            ("gpu-api", "d3d11"),
            ("gpu-context", "d3d11"),
            ("video-sync", "display-desync"),
            ("framedrop", "decoder"),
            ("interpolation", "no"),
            ("deband", "no"),
            ("dither-depth", "no"),
            ("scale", "bilinear"),
            ("cscale", "bilinear"),
            ("dscale", "bilinear"),
            ("correct-downscaling", "no"),
            ("gpu-shader-cache", "no"),
            ("cache", "yes"),
            ("demuxer-max-bytes", "32MiB"),
            ("demuxer-max-back-bytes", "8MiB"),
            ("aid", "no"),
            ("mute", "yes"),
            ("msg-level", "all=v"),
            ("log-file", "spike-mpv.log"),
            ("keepaspect", "yes"),
            ("panscan", "1.0"),
        ] {
            mpv.option(k, v);
        }
        let rc = mpv_initialize(&mpv);
        log(&format!("mpv_initialize rc={rc} ({})", mpv.err(rc)));
        if rc < 0 {
            return;
        }
        match std::fs::canonicalize(&args.video) {
            Ok(p) => log(&format!("video file OK: {} ({} bytes)", p.display(), std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0))),
            Err(e) => log(&format!("!!! VIDEO FILE NOT FOUND/UNREADABLE: {} ({e}) -- cwd={:?}", args.video, std::env::current_dir())),
        }
        let rc = mpv.command(&["loadfile", &args.video]);
        log(&format!("loadfile rc={rc} ({})", mpv.err(rc)));
        mem_report("after loadfile");

        // Thread timers (hwnd = 0). With hwnd == NULL Windows IGNORES the id we pass and
        // returns a system-assigned one, so we must keep the returned ids and compare those.
        let (t_status3, t_status30, t_rewid, t_exit) = unsafe {
            (
                SetTimer(0, 0, 3000, null()),
                SetTimer(0, 0, 30000, null()),
                if args.rewid { SetTimer(0, 0, 8000, null()) } else { usize::MAX },
                if args.seconds > 0 { SetTimer(0, 0, args.seconds * 1000, null()) } else { usize::MAX },
            )
        };
        log(&format!("timers: status3={t_status3} status30={t_status30} rewid={t_rewid} exit={t_exit}"));
        log("running; Ctrl+C or --seconds to stop. CHECK: video behind icons? icons clickable? not in taskbar/Alt-Tab?");

        let mut current = hwnd;
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, 0, 0, 0) > 0 {
                if msg.message == WM_TIMER && msg.hwnd == 0 {
                    let id = msg.wParam;
                    match id {
                        _ if id == t_status3 || id == t_status30 => {
                            log(&format!(
                                "[status] hwdec-current={} vo={} size={}x{} fps={} dropped={}",
                                mpv.get("hwdec-current"),
                                mpv.get("current-vo"),
                                mpv.get("video-params/w"),
                                mpv.get("video-params/h"),
                                mpv.get("container-fps"),
                                mpv.get("frame-drop-count")
                            ));
                            log(&format!(
                                "[status] path={} file-format={} video-format={} idle-active={} core-idle={}",
                                mpv.get("path"),
                                mpv.get("file-format"),
                                mpv.get("video-format"),
                                mpv.get("idle-active"),
                                mpv.get("core-idle")
                            ));
                            mem_report("status");
                            if id == t_status3 {
                                KillTimer(0, id);
                            }
                        }
                        _ if id == t_rewid => {
                            KillTimer(0, id);
                            let new = make_window();
                            let rc = mpv.set("wid", &(new as i64).to_string());
                            log(&format!("runtime wid change -> rc={rc} ({}) new=0x{new:X}", mpv.err(rc)));
                            if rc >= 0 {
                                DestroyWindow(current);
                                current = new;
                            }
                        }
                        _ if id == t_exit => PostQuitMessage(0),
                        _ => {}
                    }
                    continue;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        log("exiting; destroying mpv");
        unsafe { (mpv.destroy)(mpv.h) };
        unsafe { DestroyWindow(current) };
    }
}
