//! Windows input injection
//!
//! Uses SendInput for keyboard and mouse events.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, MOUSEINPUT,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_WHEEL, MOUSEEVENTF_MOVE, MOUSEEVENTF_ABSOLUTE,
    VIRTUAL_KEY, VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    SetCursorPos, GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN,
};

use crate::{Error, ErrorCode, Result};

/// Move the mouse to absolute coordinates
pub fn move_mouse(x: i32, y: i32) -> Result<()> {
    unsafe {
        SetCursorPos(x, y)
            .map_err(|e| Error::new(ErrorCode::ActionFailed, format!("Failed to move mouse: {:?}", e)))?;
    }
    Ok(())
}

/// Click at the current position
pub fn click() -> Result<()> {
    let inputs = [
        make_mouse_input(MOUSEEVENTF_LEFTDOWN, 0, 0, 0),
        make_mouse_input(MOUSEEVENTF_LEFTUP, 0, 0, 0),
    ];
    send_inputs(&inputs)
}

/// Click at specific coordinates
pub fn click_at(x: i32, y: i32) -> Result<()> {
    move_mouse(x, y)?;
    std::thread::sleep(std::time::Duration::from_millis(10));
    click()
}

/// Double click at current position
pub fn double_click() -> Result<()> {
    click()?;
    std::thread::sleep(std::time::Duration::from_millis(50));
    click()
}

/// Right click at current position
pub fn right_click() -> Result<()> {
    let inputs = [
        make_mouse_input(MOUSEEVENTF_RIGHTDOWN, 0, 0, 0),
        make_mouse_input(MOUSEEVENTF_RIGHTUP, 0, 0, 0),
    ];
    send_inputs(&inputs)
}

/// Middle click at current position
pub fn middle_click() -> Result<()> {
    let inputs = [
        make_mouse_input(MOUSEEVENTF_MIDDLEDOWN, 0, 0, 0),
        make_mouse_input(MOUSEEVENTF_MIDDLEUP, 0, 0, 0),
    ];
    send_inputs(&inputs)
}

/// Scroll the mouse wheel
/// Positive delta = scroll up, negative = scroll down
pub fn scroll(delta: i32) -> Result<()> {
    let inputs = [make_mouse_input(MOUSEEVENTF_WHEEL, 0, 0, delta * 120)];
    send_inputs(&inputs)
}

/// Press and release a virtual key
pub fn press_key(vk: u16) -> Result<()> {
    let inputs = [
        make_key_input(vk, false),
        make_key_input(vk, true),
    ];
    send_inputs(&inputs)
}

/// Hold a key down
pub fn key_down(vk: u16) -> Result<()> {
    let inputs = [make_key_input(vk, false)];
    send_inputs(&inputs)
}

/// Release a key
pub fn key_up(vk: u16) -> Result<()> {
    let inputs = [make_key_input(vk, true)];
    send_inputs(&inputs)
}

/// Type a string using Unicode input
pub fn type_text(text: &str) -> Result<()> {
    let mut inputs = Vec::new();

    for c in text.chars() {
        let code = c as u16;
        // Key down
        inputs.push(make_unicode_input(code, false));
        // Key up
        inputs.push(make_unicode_input(code, true));
    }

    send_inputs(&inputs)
}

/// Execute a keyboard shortcut (e.g., Ctrl+C)
pub fn shortcut(key: u16, modifiers: &[u16]) -> Result<()> {
    let mut inputs = Vec::new();

    // Press modifiers
    for &modifier in modifiers {
        inputs.push(make_key_input(modifier, false));
    }

    // Press and release key
    inputs.push(make_key_input(key, false));
    inputs.push(make_key_input(key, true));

    // Release modifiers (in reverse order)
    for &modifier in modifiers.iter().rev() {
        inputs.push(make_key_input(modifier, true));
    }

    send_inputs(&inputs)
}

// Helper functions

fn make_mouse_input(flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS, dx: i32, dy: i32, data: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn make_key_input(vk: u16, key_up: bool) -> INPUT {
    let flags = if key_up {
        KEYEVENTF_KEYUP
    } else {
        windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0)
    };

    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn make_unicode_input(char_code: u16, key_up: bool) -> INPUT {
    let mut flags = KEYEVENTF_UNICODE;
    if key_up {
        flags |= KEYEVENTF_KEYUP;
    }

    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: char_code,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send_inputs(inputs: &[INPUT]) -> Result<()> {
    let sent = unsafe {
        SendInput(inputs, std::mem::size_of::<INPUT>() as i32)
    };

    if sent as usize != inputs.len() {
        return Err(Error::new(
            ErrorCode::ActionFailed,
            format!("SendInput failed: sent {} of {} inputs", sent, inputs.len()),
        ));
    }

    Ok(())
}

/// Common virtual key codes
pub mod vk {
    pub const BACKSPACE: u16 = 0x08;
    pub const TAB: u16 = 0x09;
    pub const RETURN: u16 = 0x0D;
    pub const SHIFT: u16 = 0x10;
    pub const CONTROL: u16 = 0x11;
    pub const ALT: u16 = 0x12;
    pub const ESCAPE: u16 = 0x1B;
    pub const SPACE: u16 = 0x20;
    pub const PAGE_UP: u16 = 0x21;
    pub const PAGE_DOWN: u16 = 0x22;
    pub const END: u16 = 0x23;
    pub const HOME: u16 = 0x24;
    pub const LEFT: u16 = 0x25;
    pub const UP: u16 = 0x26;
    pub const RIGHT: u16 = 0x27;
    pub const DOWN: u16 = 0x28;
    pub const DELETE: u16 = 0x2E;

    // Letters A-Z are 0x41-0x5A
    pub const A: u16 = 0x41;
    pub const C: u16 = 0x43;
    pub const V: u16 = 0x56;
    pub const X: u16 = 0x58;
    pub const Z: u16 = 0x5A;

    // Function keys
    pub const F1: u16 = 0x70;
    pub const F12: u16 = 0x7B;

    // Windows key
    pub const LWIN: u16 = 0x5B;
}
