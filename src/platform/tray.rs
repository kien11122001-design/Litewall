//! Notification-area icon and its popup menu (raw `Shell_NotifyIconW`, decision D8).
//! The menu *model* comes from `core::menu`; this file only draws it and reports the choice.
use super::sys::*;
use super::WM_APP_TRAY;
use crate::core::menu::MenuEntry;
use std::io;
use std::ptr::null;

const ICON_ID: u32 = 1;

pub struct Tray {
    hwnd: HWND,
    icon: isize,
    tip: String,
}

/// Copy `text` into a fixed UTF-16 buffer, truncating and keeping the final NUL.
fn copy_text(dst: &mut [u16], text: &str) {
    let max = dst.len().saturating_sub(1);
    for (slot, unit) in dst.iter_mut().zip(text.encode_utf16().take(max)) {
        *slot = unit;
    }
}

fn data(hwnd: HWND, icon: isize, tip: &str, flags: u32) -> NOTIFYICONDATAW {
    // SAFETY: NOTIFYICONDATAW is plain old data; all-zero is a valid value.
    let mut n: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    n.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    n.hWnd = hwnd;
    n.uID = ICON_ID;
    n.uFlags = flags;
    n.uCallbackMessage = WM_APP_TRAY;
    n.hIcon = icon;
    copy_text(&mut n.szTip, tip);
    n
}

impl Tray {
    /// `hwnd` receives `WM_APP_TRAY`. The icon is the stock application icon until D10
    /// (final icon) is decided.
    pub fn new(hwnd: HWND, tip: &str) -> Self {
        // SAFETY: a predefined system icon id passed as MAKEINTRESOURCE; the handle is
        // shared and never destroyed.
        let icon = unsafe { LoadIconW(0, IDI_APPLICATION as *const u16) };
        Self {
            hwnd,
            icon,
            tip: tip.to_string(),
        }
    }

    /// Add the icon. Call again after Explorer restarts (`TaskbarCreated`).
    pub fn add(&self) -> io::Result<()> {
        let mut n = data(
            self.hwnd,
            self.icon,
            &self.tip,
            NIF_MESSAGE | NIF_ICON | NIF_TIP,
        );
        // SAFETY: `n` is fully initialised with the right cbSize.
        if unsafe { Shell_NotifyIconW(NIM_ADD, &mut n) } == 0 {
            return Err(io::Error::other("Shell_NotifyIconW(NIM_ADD) failed"));
        }
        Ok(())
    }

    pub fn set_tip(&self, tip: &str) {
        let mut n = data(self.hwnd, self.icon, tip, NIF_TIP);
        // SAFETY: as above.
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &mut n) };
    }

    /// Balloon / toast notification from the tray icon.
    pub fn notify(&self, title: &str, text: &str, warning: bool) {
        let mut n = data(self.hwnd, self.icon, &self.tip, NIF_INFO);
        copy_text(&mut n.szInfoTitle, title);
        copy_text(&mut n.szInfo, text);
        n.dwInfoFlags = if warning { NIIF_WARNING } else { NIIF_INFO };
        // SAFETY: as above.
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &mut n) };
    }

    /// Remove the icon (safe to call twice).
    pub fn remove(&self) {
        let mut n = data(self.hwnd, self.icon, &self.tip, 0);
        // SAFETY: as above.
        unsafe { Shell_NotifyIconW(NIM_DELETE, &mut n) };
    }

    /// Show the menu at the cursor and return the chosen item id (`None` = dismissed).
    /// Runs a nested message loop until the menu closes.
    pub fn show_menu(&self, entries: &[MenuEntry]) -> Option<u32> {
        // SAFETY: the standard popup-menu sequence on the owner window's thread.
        unsafe {
            let menu = CreatePopupMenu();
            if menu == 0 {
                return None;
            }
            fill_menu(menu, entries);
            let mut pt = POINT { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            // Without this the menu does not close when the user clicks elsewhere.
            SetForegroundWindow(self.hwnd);
            let cmd = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                pt.x,
                pt.y,
                0,
                self.hwnd,
                null(),
            );
            PostMessageW(self.hwnd, WM_NULL, 0, 0);
            DestroyMenu(menu); // also destroys the submenus
            (cmd > 0).then_some(cmd as u32)
        }
    }
}

fn fill_menu(menu: isize, entries: &[MenuEntry]) {
    for entry in entries {
        match entry {
            MenuEntry::Item {
                id,
                label,
                checked,
                enabled,
            } => {
                let mut flags = MF_STRING;
                if *checked {
                    flags |= MF_CHECKED;
                }
                if !*enabled {
                    flags |= MF_GRAYED;
                }
                let text = wide(label);
                // SAFETY: valid menu handle and NUL-terminated text for the call.
                unsafe { AppendMenuW(menu, flags, *id as usize, text.as_ptr()) };
            }
            MenuEntry::Submenu { label, entries } => {
                // SAFETY: as above; the popup handle is owned by the parent menu afterwards.
                unsafe {
                    let sub = CreatePopupMenu();
                    if sub == 0 {
                        continue;
                    }
                    fill_menu(sub, entries);
                    let text = wide(label);
                    AppendMenuW(menu, MF_POPUP, sub as usize, text.as_ptr());
                }
            }
            MenuEntry::Separator => {
                // SAFETY: separators take no text.
                unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, null()) };
            }
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        self.remove();
    }
}
