//! libmpv client API, loaded at run time (D4: hand-written FFI + `libloading`).
//!
//! All `unsafe` for libmpv lives in this file. Callers get a safe [`Handle`].
//! Only the ~13 functions LiteWall needs are resolved from `libmpv-2.dll`.
//!
//! Threading: a `Handle` must be used from the thread that created it (the message
//! thread). The one exception is mpv's wakeup callback, which runs on an mpv thread and
//! does nothing but `PostMessageW` (see [`Wake`]).
use crate::platform::sys::{PostMessageW, HWND};
use libloading::Library;
use std::ffi::{c_char, c_int, c_ulong, c_void, CStr, CString};
use std::fmt;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

pub const DLL_NAME: &str = "libmpv-2.dll";

const MPV_FORMAT_STRING: c_int = 1;

const EVENT_SHUTDOWN: c_int = 1;
const EVENT_LOG_MESSAGE: c_int = 2;
const EVENT_START_FILE: c_int = 6;
const EVENT_END_FILE: c_int = 7;
const EVENT_FILE_LOADED: c_int = 8;
const EVENT_PROPERTY_CHANGE: c_int = 22;

const END_FILE_ERROR: c_int = 4;

// ------------------------------------------------------------------ C layouts

#[repr(C)]
struct RawEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

#[repr(C)]
struct RawEndFile {
    reason: c_int,
    error: c_int,
    // playlist_entry_id etc. follow in the C struct; we never read them.
}

#[repr(C)]
struct RawLogMessage {
    prefix: *const c_char,
    level: *const c_char,
    text: *const c_char,
    log_level: c_int,
}

#[repr(C)]
struct RawProperty {
    name: *const c_char,
    format: c_int,
    data: *mut c_void,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<RawEvent>() == 24);

type FnClientApiVersion = unsafe extern "C" fn() -> c_ulong;
type FnCreate = unsafe extern "C" fn() -> *mut c_void;
type FnInitialize = unsafe extern "C" fn(*mut c_void) -> c_int;
type FnTerminateDestroy = unsafe extern "C" fn(*mut c_void);
type FnSetStr = unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int;
type FnCommand = unsafe extern "C" fn(*mut c_void, *mut *const c_char) -> c_int;
type FnGetPropertyString = unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_char;
type FnFree = unsafe extern "C" fn(*mut c_void);
type FnErrorString = unsafe extern "C" fn(c_int) -> *const c_char;
type FnWaitEvent = unsafe extern "C" fn(*mut c_void, f64) -> *mut RawEvent;
type WakeupCb = unsafe extern "C" fn(*mut c_void);
type FnSetWakeup = unsafe extern "C" fn(*mut c_void, Option<WakeupCb>, *mut c_void);
type FnRequestLog = unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int;
type FnObserve = unsafe extern "C" fn(*mut c_void, u64, *const c_char, c_int) -> c_int;

// ------------------------------------------------------------------ errors

#[derive(Debug)]
pub enum LoadError {
    /// The DLL was not found or could not be loaded (path tried is included).
    Library(String, String),
    /// A required export is missing: wrong or too old a libmpv.
    Symbol(&'static str, String),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Library(path, e) => {
                write!(f, "cannot load {DLL_NAME} ({path}): {e}")
            }
            LoadError::Symbol(name, e) => {
                write!(
                    f,
                    "{DLL_NAME} lacks `{name}` (wrong or too old build?): {e}"
                )
            }
        }
    }
}
impl std::error::Error for LoadError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MpvError {
    pub code: i32,
    pub what: String,
}

impl fmt::Display for MpvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (mpv error {})", self.what, self.code)
    }
}
impl std::error::Error for MpvError {}

// ------------------------------------------------------------------ events

/// Events LiteWall cares about; everything else is `Other(id)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    None,
    Shutdown,
    Log {
        level: String,
        text: String,
    },
    StartFile,
    FileLoaded,
    EndFile {
        error: Option<i32>,
    },
    /// `value` is `None` when the property became unavailable.
    Property {
        name: String,
        value: Option<String>,
    },
    Other(i32),
}

// ------------------------------------------------------------------ API table

/// Resolved libmpv entry points. Share it between players with `Rc<Api>`.
pub struct Api {
    // Keeps the DLL mapped for as long as any function pointer below can be called.
    _lib: Library,
    client_api_version: FnClientApiVersion,
    create: FnCreate,
    initialize: FnInitialize,
    terminate_destroy: FnTerminateDestroy,
    set_option_string: FnSetStr,
    set_property_string: FnSetStr,
    command: FnCommand,
    get_property_string: FnGetPropertyString,
    free: FnFree,
    error_string: FnErrorString,
    wait_event: FnWaitEvent,
    set_wakeup_callback: FnSetWakeup,
    request_log_messages: FnRequestLog,
    observe_property: FnObserve,
}

macro_rules! sym {
    ($lib:expr, $name:literal, $ty:ty) => {{
        // SAFETY: `$ty` is the documented C signature of this libmpv export, and the
        // library is stored in `Api::_lib`, so the pointer never outlives the mapping.
        let s = unsafe { $lib.get::<$ty>(concat!($name, "\0").as_bytes()) }
            .map_err(|e| LoadError::Symbol($name, e.to_string()))?;
        *s
    }};
}

impl Api {
    /// Load `libmpv-2.dll`, preferring the copy next to the executable.
    pub fn load() -> Result<Rc<Api>, LoadError> {
        let beside_exe = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join(DLL_NAME)))
            .filter(|p| p.is_file());
        let (target, shown) = match beside_exe {
            Some(p) => {
                let shown = p.display().to_string();
                (p.into_os_string(), shown)
            }
            None => (
                DLL_NAME.into(),
                format!("{DLL_NAME} via the default DLL search"),
            ),
        };
        // SAFETY: loading a DLL runs its initialisers; libmpv-2.dll is the library the
        // user (or the release zip) put next to the exe, which is the documented setup.
        let lib = unsafe { Library::new(&target) }
            .map_err(|e| LoadError::Library(shown, e.to_string()))?;
        Ok(Rc::new(Api {
            client_api_version: sym!(lib, "mpv_client_api_version", FnClientApiVersion),
            create: sym!(lib, "mpv_create", FnCreate),
            initialize: sym!(lib, "mpv_initialize", FnInitialize),
            terminate_destroy: sym!(lib, "mpv_terminate_destroy", FnTerminateDestroy),
            set_option_string: sym!(lib, "mpv_set_option_string", FnSetStr),
            set_property_string: sym!(lib, "mpv_set_property_string", FnSetStr),
            command: sym!(lib, "mpv_command", FnCommand),
            get_property_string: sym!(lib, "mpv_get_property_string", FnGetPropertyString),
            free: sym!(lib, "mpv_free", FnFree),
            error_string: sym!(lib, "mpv_error_string", FnErrorString),
            wait_event: sym!(lib, "mpv_wait_event", FnWaitEvent),
            set_wakeup_callback: sym!(lib, "mpv_set_wakeup_callback", FnSetWakeup),
            request_log_messages: sym!(lib, "mpv_request_log_messages", FnRequestLog),
            observe_property: sym!(lib, "mpv_observe_property", FnObserve),
            _lib: lib,
        }))
    }

    /// `(major, minor)` of the client API, e.g. `(2, 5)`.
    pub fn client_api_version(&self) -> (u32, u32) {
        // SAFETY: no arguments, no preconditions.
        // `c_ulong` is u32 on Windows but u64 elsewhere, hence the cast.
        #[allow(clippy::unnecessary_cast)]
        let v = unsafe { (self.client_api_version)() } as u32;
        (v >> 16, v & 0xFFFF)
    }

    fn err(&self, code: c_int) -> MpvError {
        // SAFETY: mpv_error_string accepts any code and returns a static string.
        let p = unsafe { (self.error_string)(code) };
        let what = if p.is_null() {
            "unknown error".to_string()
        } else {
            // SAFETY: non-null, NUL-terminated, static lifetime.
            unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
        };
        MpvError { code, what }
    }

    fn check(&self, code: c_int) -> Result<(), MpvError> {
        if code < 0 {
            Err(self.err(code))
        } else {
            Ok(())
        }
    }
}

// ------------------------------------------------------------------ wakeup

/// What mpv's wakeup callback does: post one window message to the message thread.
/// Coalesced: while a wakeup is still unhandled no further message is posted.
pub struct Wake {
    hwnd: HWND,
    msg: u32,
    /// Sent as `wParam`, so the window procedure can tell players apart.
    token: usize,
    pending: AtomicBool,
}

impl Wake {
    pub fn new(hwnd: HWND, msg: u32, token: usize) -> Box<Wake> {
        Box::new(Wake {
            hwnd,
            msg,
            token,
            pending: AtomicBool::new(false),
        })
    }
}

unsafe extern "C" fn wake_trampoline(ctx: *mut c_void) {
    // SAFETY: `ctx` is the `Wake` boxed in `Handle`, which outlives the mpv handle (the
    // callback is cleared and the handle destroyed before the box is dropped). Only
    // atomics and a thread-safe PostMessageW are touched; the call never panics.
    unsafe {
        let wake = &*(ctx as *const Wake);
        if !wake.pending.swap(true, Ordering::AcqRel) {
            PostMessageW(wake.hwnd, wake.msg, wake.token, 0);
        }
    }
}

// ------------------------------------------------------------------ handle

/// One mpv instance. Destroyed on drop.
pub struct Handle {
    api: Rc<Api>,
    ctx: *mut c_void,
    wake: Option<Box<Wake>>,
}

fn cstr(s: &str) -> Result<CString, MpvError> {
    CString::new(s).map_err(|_| MpvError {
        code: -1,
        what: format!("string contains a NUL byte: {s:?}"),
    })
}

fn owned(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        // SAFETY: callers pass pointers from mpv that are NUL-terminated and valid
        // until the next mpv_wait_event call; the contents are copied right away.
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

impl Handle {
    /// `mpv_create`. Options must be set before [`Handle::initialize`].
    pub fn create(api: &Rc<Api>) -> Result<Handle, MpvError> {
        // SAFETY: no preconditions; returns NULL on failure.
        let ctx = unsafe { (api.create)() };
        if ctx.is_null() {
            return Err(MpvError {
                code: -1,
                what: "mpv_create returned NULL".into(),
            });
        }
        Ok(Handle {
            api: Rc::clone(api),
            ctx,
            wake: None,
        })
    }

    pub fn initialize(&mut self) -> Result<(), MpvError> {
        // SAFETY: `ctx` is a live handle owned by `self`.
        let rc = unsafe { (self.api.initialize)(self.ctx) };
        self.api.check(rc)
    }

    /// Route mpv's wakeups to `wake` (see [`Wake`]).
    pub fn set_wake(&mut self, wake: Box<Wake>) {
        let ptr = &*wake as *const Wake as *mut c_void;
        self.wake = Some(wake);
        // SAFETY: `ptr` points into the box stored in `self.wake`; the box is only
        // dropped after the callback is cleared and the handle destroyed (see Drop).
        unsafe { (self.api.set_wakeup_callback)(self.ctx, Some(wake_trampoline), ptr) };
    }

    /// Allow the next wakeup to post a message again. Call before draining events.
    pub fn rearm_wake(&self) {
        if let Some(w) = &self.wake {
            w.pending.store(false, Ordering::Release);
        }
    }

    pub fn set_option(&mut self, name: &str, value: &str) -> Result<(), MpvError> {
        let (n, v) = (cstr(name)?, cstr(value)?);
        // SAFETY: valid handle and NUL-terminated strings for the call's duration.
        let rc = unsafe { (self.api.set_option_string)(self.ctx, n.as_ptr(), v.as_ptr()) };
        self.api.check(rc)
    }

    pub fn set_property(&mut self, name: &str, value: &str) -> Result<(), MpvError> {
        let (n, v) = (cstr(name)?, cstr(value)?);
        // SAFETY: as above.
        let rc = unsafe { (self.api.set_property_string)(self.ctx, n.as_ptr(), v.as_ptr()) };
        self.api.check(rc)
    }

    pub fn command(&mut self, args: &[&str]) -> Result<(), MpvError> {
        let strings: Vec<CString> = args.iter().map(|a| cstr(a)).collect::<Result<_, _>>()?;
        let mut ptrs: Vec<*const c_char> = strings.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        // SAFETY: `ptrs` is a NULL-terminated array of valid C strings kept alive by `strings`.
        let rc = unsafe { (self.api.command)(self.ctx, ptrs.as_mut_ptr()) };
        self.api.check(rc)
    }

    pub fn get_property(&self, name: &str) -> Option<String> {
        let n = cstr(name).ok()?;
        // SAFETY: valid handle and string; the result is mpv-allocated and freed below.
        let p = unsafe { (self.api.get_property_string)(self.ctx, n.as_ptr()) };
        if p.is_null() {
            return None;
        }
        let s = owned(p);
        // SAFETY: `p` came from mpv_get_property_string and is freed exactly once.
        unsafe { (self.api.free)(p as *mut c_void) };
        Some(s)
    }

    /// Ask for `warn`-level (or `v`, `info`, ...) log lines as [`Event::Log`].
    pub fn request_log_messages(&mut self, min_level: &str) -> Result<(), MpvError> {
        let l = cstr(min_level)?;
        // SAFETY: valid handle and string.
        let rc = unsafe { (self.api.request_log_messages)(self.ctx, l.as_ptr()) };
        self.api.check(rc)
    }

    /// Deliver [`Event::Property`] whenever `name` changes (string format).
    pub fn observe_property(&mut self, name: &str) -> Result<(), MpvError> {
        let n = cstr(name)?;
        // SAFETY: valid handle and string.
        let rc = unsafe { (self.api.observe_property)(self.ctx, 0, n.as_ptr(), MPV_FORMAT_STRING) };
        self.api.check(rc)
    }

    /// Non-blocking: returns [`Event::None`] when the queue is empty.
    pub fn poll_event(&mut self) -> Event {
        // SAFETY: valid handle; timeout 0 never blocks. The returned pointer is valid
        // until the next call, and everything needed is copied before returning.
        let ev = unsafe { (self.api.wait_event)(self.ctx, 0.0) };
        if ev.is_null() {
            return Event::None;
        }
        // SAFETY: non-null event owned by mpv, see above.
        let (id, error, data) = unsafe { ((*ev).event_id, (*ev).error, (*ev).data) };
        match id {
            0 => Event::None,
            EVENT_SHUTDOWN => Event::Shutdown,
            EVENT_START_FILE => Event::StartFile,
            EVENT_FILE_LOADED => Event::FileLoaded,
            EVENT_END_FILE if !data.is_null() => {
                // SAFETY: for END_FILE, `data` points to an mpv_event_end_file.
                let f = unsafe { &*(data as *const RawEndFile) };
                Event::EndFile {
                    error: (f.reason == END_FILE_ERROR).then_some(f.error),
                }
            }
            EVENT_END_FILE => Event::EndFile {
                error: (error < 0).then_some(error),
            },
            EVENT_LOG_MESSAGE if !data.is_null() => {
                // SAFETY: for LOG_MESSAGE, `data` points to an mpv_event_log_message.
                let m = unsafe { &*(data as *const RawLogMessage) };
                Event::Log {
                    level: owned(m.level),
                    text: owned(m.text).trim_end().to_string(),
                }
            }
            EVENT_PROPERTY_CHANGE if !data.is_null() => {
                // SAFETY: for PROPERTY_CHANGE, `data` points to an mpv_event_property;
                // with MPV_FORMAT_STRING its `data` is a `char*` (NULL if unavailable).
                let p = unsafe { &*(data as *const RawProperty) };
                let value = if p.format == MPV_FORMAT_STRING && !p.data.is_null() {
                    // SAFETY: format STRING => `p.data` points to a `char*`.
                    let s = unsafe { *(p.data as *const *const c_char) };
                    (!s.is_null()).then(|| owned(s))
                } else {
                    None
                };
                Event::Property {
                    name: owned(p.name),
                    value,
                }
            }
            other => Event::Other(other),
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: valid handle destroyed exactly once. Clearing the callback first, then
        // destroying, guarantees no wakeup can run after `self.wake` is dropped below.
        unsafe {
            (self.api.set_wakeup_callback)(self.ctx, None, std::ptr::null_mut());
            (self.api.terminate_destroy)(self.ctx);
        }
        self.wake = None;
    }
}
