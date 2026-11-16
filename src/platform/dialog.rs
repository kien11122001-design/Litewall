//! Native "open file" dialog (`GetOpenFileNameW`) on its own thread, so the message thread
//! never blocks. The choice comes back through a queue plus `WM_APP_FILE_PICKED`.
//! (Decision D9 was `IFileOpenDialog`; the classic common dialog gives the same Explorer-style
//! window for far less unsafe code.)
use super::sys::*;
use super::WM_APP_FILE_PICKED;
use crate::core::app::Target;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::ptr::null;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static PICKED: Mutex<Vec<(Target, PathBuf)>> = Mutex::new(Vec::new());
static OPEN: AtomicBool = AtomicBool::new(false);

/// Everything chosen since the last call (message thread, on `AppEvent::FilePicked`).
pub fn take_picked() -> Vec<(Target, PathBuf)> {
    let mut q = PICKED.lock().unwrap_or_else(|p| p.into_inner());
    std::mem::take(&mut *q)
}

/// Show the dialog without blocking. Does nothing if one is already open.
pub fn pick_file_async(owner: HWND, target: Target, filters: Vec<(String, String)>) {
    if OPEN.swap(true, Ordering::SeqCst) {
        log::info!("the file dialog is already open");
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("file-dialog".into())
        .spawn(move || {
            // SAFETY: COM is initialised and released on this dedicated thread only.
            unsafe { CoInitializeEx(null(), COINIT_APARTMENTTHREADED) };
            let choice = open_dialog(owner, &filters);
            // SAFETY: pairs with the CoInitializeEx above.
            unsafe { CoUninitialize() };
            if let Some(path) = choice {
                PICKED
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .push((target, path));
                // SAFETY: PostMessageW is thread-safe; a dead window just fails.
                unsafe { PostMessageW(owner, WM_APP_FILE_PICKED, 0, 0) };
            }
            OPEN.store(false, Ordering::SeqCst);
        });
    if let Err(e) = spawned {
        log::error!("cannot start the file dialog thread: {e}");
        OPEN.store(false, Ordering::SeqCst);
    }
}

fn open_dialog(owner: HWND, filters: &[(String, String)]) -> Option<PathBuf> {
    // "Name\0*.a;*.b\0Name2\0*.c\0\0"
    let mut filter: Vec<u16> = Vec::new();
    for (name, patterns) in filters {
        filter.extend(name.encode_utf16());
        filter.push(0);
        filter.extend(patterns.encode_utf16());
        filter.push(0);
    }
    filter.push(0);
    let title = wide("Choose a wallpaper (video, GIF or image)");
    let mut file = vec![0u16; 32_768];

    // SAFETY: OPENFILENAMEW is plain old data; all-zero is a valid value.
    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = owner;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.nFilterIndex = 1;
    ofn.lpstrFile = file.as_mut_ptr();
    ofn.nMaxFile = file.len() as u32;
    ofn.lpstrTitle = title.as_ptr();
    ofn.Flags =
        OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_HIDEREADONLY | OFN_NOCHANGEDIR;

    // SAFETY: every pointer in `ofn` stays valid for the duration of the modal call.
    let ok = unsafe { GetOpenFileNameW(&mut ofn) };
    if ok == 0 {
        return None; // cancelled
    }
    let end = file.iter().position(|&c| c == 0).unwrap_or(file.len());
    Some(PathBuf::from(OsString::from_wide(&file[..end])))
}
