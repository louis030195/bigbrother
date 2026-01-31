//! Windows workflow replay
//!
//! Uses SendInput for input injection.

use crate::events::*;
use anyhow::Result;
use std::time::Duration;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, MOUSEINPUT,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_WHEEL, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;

/// Replay recorded workflows
pub struct Replayer {
    speed: f64,
}

impl Replayer {
    pub fn new() -> Self {
        Self { speed: 1.0 }
    }

    pub fn speed(mut self, speed: f64) -> Self {
        self.speed = speed;
        self
    }

    pub fn play(&self, workflow: &RecordedWorkflow) -> Result<ReplayStats> {
        let mut stats = ReplayStats::default();
        let mut last_t = 0u64;

        for event in &workflow.events {
            if event.t > last_t {
                let delay_ms = ((event.t - last_t) as f64 / self.speed) as u64;
                if delay_ms > 0 {
                    std::thread::sleep(Duration::from_millis(delay_ms));
                }
            }
            last_t = event.t;

            match &event.data {
                EventData::Click { x, y, b, n, .. } => {
                    self.click(*x, *y, *b, *n)?;
                    stats.clicks += 1;
                }
                EventData::Move { x, y } => {
                    self.move_to(*x, *y)?;
                    stats.moves += 1;
                }
                EventData::Scroll { x, y, dx, dy } => {
                    self.scroll(*x, *y, *dy)?;
                    stats.scrolls += 1;
                }
                EventData::Key { k, m } => {
                    self.key(*k)?;
                    stats.keys += 1;
                }
                EventData::Text { s } => {
                    self.type_text(s)?;
                    stats.text_chars += s.len();
                }
                _ => {}
            }
        }

        Ok(stats)
    }

    fn click(&self, x: i32, y: i32, button: u8, clicks: u8) -> Result<()> {
        self.move_to(x, y)?;
        std::thread::sleep(Duration::from_millis(10));

        let (down_flags, up_flags) = match button {
            0 => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
            1 => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
            _ => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
        };

        for _ in 0..clicks.max(1) {
            let inputs = [
                make_mouse_input(down_flags, 0),
                make_mouse_input(up_flags, 0),
            ];
            send_inputs(&inputs)?;

            if clicks > 1 {
                std::thread::sleep(Duration::from_millis(50));
            }
        }

        Ok(())
    }

    fn move_to(&self, x: i32, y: i32) -> Result<()> {
        unsafe {
            SetCursorPos(x, y)
                .map_err(|e| anyhow::anyhow!("Failed to move cursor: {:?}", e))?;
        }
        Ok(())
    }

    fn scroll(&self, x: i32, y: i32, dy: i16) -> Result<()> {
        self.move_to(x, y)?;
        let inputs = [make_mouse_input(MOUSEEVENTF_WHEEL, dy as i32 * 120)];
        send_inputs(&inputs)
    }

    fn key(&self, keycode: u16) -> Result<()> {
        let inputs = [
            make_key_input(keycode, false),
            make_key_input(keycode, true),
        ];
        send_inputs(&inputs)?;
        std::thread::sleep(Duration::from_millis(10));
        Ok(())
    }

    fn type_text(&self, text: &str) -> Result<()> {
        let mut inputs = Vec::new();

        for c in text.chars() {
            let code = c as u16;
            inputs.push(make_unicode_input(code, false));
            inputs.push(make_unicode_input(code, true));
        }

        send_inputs(&inputs)?;
        Ok(())
    }
}

impl Default for Replayer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default)]
pub struct ReplayStats {
    pub clicks: usize,
    pub moves: usize,
    pub scrolls: usize,
    pub keys: usize,
    pub text_chars: usize,
}

// Helper functions

fn make_mouse_input(flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS, data: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
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
        return Err(anyhow::anyhow!("SendInput failed: sent {} of {}", sent, inputs.len()));
    }

    Ok(())
}
