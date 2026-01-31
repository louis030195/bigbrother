//! Windows recording implementation
//!
//! Uses rdev for global event capture and Windows APIs for context.

mod recorder;
mod replay;

pub use recorder::*;
pub use replay::*;

use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW,
    PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::Foundation::HWND;

/// Get the currently focused window title
pub fn get_focused_window_title() -> Option<String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0 == 0 {
            return None;
        }

        let mut buffer = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut buffer);
        if len > 0 {
            Some(String::from_utf16_lossy(&buffer[..len as usize]))
        } else {
            None
        }
    }
}

/// Get the PID of the currently focused window
pub fn get_focused_window_pid() -> Option<u32> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0 == 0 {
            return None;
        }

        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid > 0 {
            Some(pid)
        } else {
            None
        }
    }
}

/// Get process name by PID
pub fn get_process_name(pid: u32) -> Option<String> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                if entry.th32ProcessID == pid {
                    let name = String::from_utf16_lossy(
                        &entry.szExeFile[..entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len())]
                    );
                    return Some(name);
                }

                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        None
    }
}

/// Get current focused app info
pub fn get_focused_app() -> Option<(String, u32, Option<String>)> {
    let pid = get_focused_window_pid()?;
    let name = get_process_name(pid)?;
    let title = get_focused_window_title();
    Some((name, pid, title))
}
