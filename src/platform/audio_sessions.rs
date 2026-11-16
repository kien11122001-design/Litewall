//! Is another application playing audio on the default output device? (PRD F-44)
//!
//! Core Audio session enumeration (`IAudioSessionManager2`), written as raw COM vtable calls
//! because the project has no COM binding crate. A session counts when it is *active*, belongs
//! to another process, is not the system-sounds session and its meter shows a signal, so a
//! paused player, a muted browser tab or an idle app does not. All calls run on the message
//! thread, once every couple of seconds and only while LiteWall itself is playing sound.
use super::sys::*;
use crate::core::audio::SessionInfo;
use std::ffi::c_void;
use std::io;
use std::ptr::{null, null_mut};
use std::sync::Once;

/// Read by COM through a pointer, not by Rust.
#[repr(C)]
#[allow(dead_code)]
struct Guid {
    d1: u32,
    d2: u16,
    d3: u16,
    d4: [u8; 8],
}

/// `CLSID_MMDeviceEnumerator` {BCDE0395-E52F-467C-8E3D-C4579291692E}
const CLSID_MM_DEVICE_ENUMERATOR: Guid = Guid {
    d1: 0xBCDE_0395,
    d2: 0xE52F,
    d3: 0x467C,
    d4: [0x8E, 0x3D, 0xC4, 0x57, 0x92, 0x91, 0x69, 0x2E],
};
/// `IID_IMMDeviceEnumerator` {A95664D2-9614-4F35-A746-DE8DB63617E6}
const IID_MM_DEVICE_ENUMERATOR: Guid = Guid {
    d1: 0xA956_64D2,
    d2: 0x9614,
    d3: 0x4F35,
    d4: [0xA7, 0x46, 0xDE, 0x8D, 0xB6, 0x36, 0x17, 0xE6],
};
/// `IID_IAudioSessionManager2` {77AA99A0-1BD6-484F-8BC7-2C654C9A9B6F}
const IID_AUDIO_SESSION_MANAGER2: Guid = Guid {
    d1: 0x77AA_99A0,
    d2: 0x1BD6,
    d3: 0x484F,
    d4: [0x8B, 0xC7, 0x2C, 0x65, 0x4C, 0x9A, 0x9B, 0x6F],
};
/// `IID_IAudioSessionControl2` {BFB7FF88-7239-4FC9-8FA2-07C950BE9C6D}
const IID_AUDIO_SESSION_CONTROL2: Guid = Guid {
    d1: 0xBFB7_FF88,
    d2: 0x7239,
    d3: 0x4FC9,
    d4: [0x8F, 0xA2, 0x07, 0xC9, 0x50, 0xBE, 0x9C, 0x6D],
};
/// `IID_IAudioMeterInformation` {C02216F6-8C67-4B5B-9D00-D008E73E0064}
const IID_AUDIO_METER_INFORMATION: Guid = Guid {
    d1: 0xC022_16F6,
    d2: 0x8C67,
    d3: 0x4B5B,
    d4: [0x9D, 0x00, 0xD0, 0x08, 0xE7, 0x3E, 0x00, 0x64],
};

const CLSCTX_ALL: u32 = 0x17;
const E_RENDER: i32 = 0;
const E_MULTIMEDIA: i32 = 1;
/// `AudioSessionStateActive`.
const SESSION_ACTIVE: i32 = 1;

#[link(name = "ole32")]
extern "system" {
    fn CoCreateInstance(
        clsid: *const Guid,
        outer: *mut c_void,
        context: u32,
        iid: *const Guid,
        out: *mut *mut c_void,
    ) -> i32;
}

// vtable slots (IUnknown = 0..=2).
const QUERY_INTERFACE: usize = 0;
const RELEASE: usize = 2;
const ENUM_GET_DEFAULT_ENDPOINT: usize = 4; // IMMDeviceEnumerator
const DEVICE_ACTIVATE: usize = 3; // IMMDevice
const MANAGER_GET_SESSION_ENUMERATOR: usize = 5; // IAudioSessionManager2
const SESSIONS_GET_COUNT: usize = 3; // IAudioSessionEnumerator
const SESSIONS_GET_SESSION: usize = 4;
const CONTROL_GET_STATE: usize = 3; // IAudioSessionControl
const CONTROL2_GET_PROCESS_ID: usize = 14; // IAudioSessionControl2
const CONTROL2_IS_SYSTEM_SOUNDS: usize = 15;
const METER_GET_PEAK_VALUE: usize = 3; // IAudioMeterInformation

type FnQuery = unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32;
type FnRelease = unsafe extern "system" fn(*mut c_void) -> u32;
type FnDefaultEndpoint = unsafe extern "system" fn(*mut c_void, i32, i32, *mut *mut c_void) -> i32;
type FnActivate = unsafe extern "system" fn(
    *mut c_void,
    *const Guid,
    u32,
    *const c_void,
    *mut *mut c_void,
) -> i32;
type FnOut = unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32;
type FnCount = unsafe extern "system" fn(*mut c_void, *mut i32) -> i32;
type FnGetSession = unsafe extern "system" fn(*mut c_void, i32, *mut *mut c_void) -> i32;
type FnGetU32 = unsafe extern "system" fn(*mut c_void, *mut u32) -> i32;
type FnGetI32 = unsafe extern "system" fn(*mut c_void, *mut i32) -> i32;
type FnGetF32 = unsafe extern "system" fn(*mut c_void, *mut f32) -> i32;
type FnNoArgs = unsafe extern "system" fn(*mut c_void) -> i32;

/// One COM interface pointer; `Release` runs on drop.
struct Com(*mut c_void);

impl Com {
    /// Function pointer in vtable slot `slot`.
    fn slot(&self, slot: usize) -> *const c_void {
        // SAFETY: `self.0` is a live COM object, whose first word points to its vtable; every
        // slot index used in this file is within the documented interface.
        unsafe { *(*(self.0 as *const *const *const c_void)).add(slot) }
    }

    fn from_hr(hr: i32, p: *mut c_void) -> io::Result<Com> {
        if hr < 0 || p.is_null() {
            Err(io::Error::from_raw_os_error(hr))
        } else {
            Ok(Com(p))
        }
    }

    fn query(&self, iid: &Guid) -> io::Result<Com> {
        let mut out: *mut c_void = null_mut();
        // SAFETY: slot 0 of every COM interface is QueryInterface with this signature.
        let hr = unsafe {
            std::mem::transmute::<*const c_void, FnQuery>(self.slot(QUERY_INTERFACE))(
                self.0, iid, &mut out,
            )
        };
        Com::from_hr(hr, out)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        // SAFETY: slot 2 is Release; the pointer is released exactly once.
        unsafe {
            std::mem::transmute::<*const c_void, FnRelease>(self.slot(RELEASE))(self.0);
        }
    }
}

fn init_com() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: plain initialisation of this thread's COM apartment. S_FALSE (already
        // initialised) and RPC_E_CHANGED_MODE are fine for our purpose and ignored.
        unsafe { CoInitializeEx(null(), COINIT_APARTMENTTHREADED) };
    });
}

/// Every audio session on the default output device (the criteria live in
/// `core::audio::counts_as_other_audio`). `Err` when Core Audio is unavailable (no output
/// device, service stopped).
pub fn list_sessions() -> io::Result<Vec<SessionInfo>> {
    init_com();
    let mut raw: *mut c_void = null_mut();
    // SAFETY: valid GUIDs and out-pointer; the result is wrapped in `Com` right away.
    let hr = unsafe {
        CoCreateInstance(
            &CLSID_MM_DEVICE_ENUMERATOR,
            null_mut(),
            CLSCTX_ALL,
            &IID_MM_DEVICE_ENUMERATOR,
            &mut raw,
        )
    };
    let enumerator = Com::from_hr(hr, raw)?;

    let mut raw: *mut c_void = null_mut();
    // SAFETY: slot 4 of IMMDeviceEnumerator is GetDefaultAudioEndpoint.
    let hr = unsafe {
        std::mem::transmute::<*const c_void, FnDefaultEndpoint>(
            enumerator.slot(ENUM_GET_DEFAULT_ENDPOINT),
        )(enumerator.0, E_RENDER, E_MULTIMEDIA, &mut raw)
    };
    let device = Com::from_hr(hr, raw)?;

    let mut raw: *mut c_void = null_mut();
    // SAFETY: slot 3 of IMMDevice is Activate.
    let hr = unsafe {
        std::mem::transmute::<*const c_void, FnActivate>(device.slot(DEVICE_ACTIVATE))(
            device.0,
            &IID_AUDIO_SESSION_MANAGER2,
            CLSCTX_ALL,
            null(),
            &mut raw,
        )
    };
    let manager = Com::from_hr(hr, raw)?;

    let mut raw: *mut c_void = null_mut();
    // SAFETY: slot 5 of IAudioSessionManager2 is GetSessionEnumerator.
    let hr = unsafe {
        std::mem::transmute::<*const c_void, FnOut>(manager.slot(MANAGER_GET_SESSION_ENUMERATOR))(
            manager.0, &mut raw,
        )
    };
    let sessions = Com::from_hr(hr, raw)?;

    let mut count = 0i32;
    // SAFETY: slot 3 of IAudioSessionEnumerator is GetCount.
    let hr = unsafe {
        std::mem::transmute::<*const c_void, FnCount>(sessions.slot(SESSIONS_GET_COUNT))(
            sessions.0, &mut count,
        )
    };
    if hr < 0 {
        return Err(io::Error::from_raw_os_error(hr));
    }
    let mut out = Vec::new();
    for i in 0..count.clamp(0, 256) {
        let mut raw: *mut c_void = null_mut();
        // SAFETY: slot 4 of IAudioSessionEnumerator is GetSession.
        let hr = unsafe {
            std::mem::transmute::<*const c_void, FnGetSession>(sessions.slot(SESSIONS_GET_SESSION))(
                sessions.0, i, &mut raw,
            )
        };
        let Ok(control) = Com::from_hr(hr, raw) else {
            continue;
        };
        if let Some(info) = session_info(&control) {
            out.push(info);
        }
    }
    Ok(out)
}

/// State, process id, system-sounds flag and (for active sessions) the peak meter of one session.
fn session_info(control: &Com) -> Option<SessionInfo> {
    let control2 = control.query(&IID_AUDIO_SESSION_CONTROL2).ok()?;

    let mut state = 0i32;
    // SAFETY: slot 3 of IAudioSessionControl is GetState (IAudioSessionControl2 extends it).
    let hr = unsafe {
        std::mem::transmute::<*const c_void, FnGetI32>(control2.slot(CONTROL_GET_STATE))(
            control2.0, &mut state,
        )
    };
    if hr < 0 {
        return None;
    }
    let active = state == SESSION_ACTIVE;

    let mut pid = 0u32;
    // SAFETY: slot 14 of IAudioSessionControl2 is GetProcessId.
    let hr = unsafe {
        std::mem::transmute::<*const c_void, FnGetU32>(control2.slot(CONTROL2_GET_PROCESS_ID))(
            control2.0, &mut pid,
        )
    };
    if hr < 0 {
        return None;
    }

    // SAFETY: slot 15 of IAudioSessionControl2 is IsSystemSoundsSession (S_OK = yes).
    let system = unsafe {
        std::mem::transmute::<*const c_void, FnNoArgs>(control2.slot(CONTROL2_IS_SYSTEM_SOUNDS))(
            control2.0,
        )
    } == 0;

    // The meter is only read for active sessions; if it cannot be read the session is judged
    // by its state alone.
    let peak = if active {
        control
            .query(&IID_AUDIO_METER_INFORMATION)
            .ok()
            .and_then(|meter| {
                let mut peak = 0f32;
                // SAFETY: slot 3 of IAudioMeterInformation is GetPeakValue.
                let hr = unsafe {
                    std::mem::transmute::<*const c_void, FnGetF32>(
                        meter.slot(METER_GET_PEAK_VALUE),
                    )(meter.0, &mut peak)
                };
                (hr >= 0).then_some(peak)
            })
    } else {
        None
    };
    Some(SessionInfo {
        pid,
        active,
        system,
        peak,
    })
}
