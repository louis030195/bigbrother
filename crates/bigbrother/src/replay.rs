//! Workflow replay using CGEvent injection

use crate::events::*;
use anyhow::Result;
use std::time::Duration;

use cidre::cg;

// Raw FFI for CGEventPost (not exposed by cidre)
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventPost(tap: u32, event: *const std::ffi::c_void);
}

/// Post an event to the system
fn post_event(event: &cg::Event, location: u32) {
    unsafe {
        CGEventPost(location, event as *const _ as *const std::ffi::c_void);
    }
}

const HID_EVENT_TAP: u32 = 0;

/// Replay recorded workflows
pub struct Replayer {
    speed: f64,
}

impl Replayer {
    pub fn new() -> Self {
        Self { speed: 1.0 }
    }

    /// Set playback speed (1.0 = real-time, 2.0 = 2x speed)
    pub fn speed(mut self, speed: f64) -> Self {
        self.speed = speed;
        self
    }

    /// Replay a workflow
    pub fn play(&self, workflow: &RecordedWorkflow) -> Result<ReplayStats> {
        let mut stats = ReplayStats::default();
        let mut last_t = 0u64;

        for event in &workflow.events {
            // Wait for the right time
            if event.t > last_t {
                let delay_ms = ((event.t - last_t) as f64 / self.speed) as u64;
                if delay_ms > 0 {
                    std::thread::sleep(Duration::from_millis(delay_ms));
                }
            }
            last_t = event.t;

            // Replay the event
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
                    self.scroll(*x, *y, *dx, *dy)?;
                    stats.scrolls += 1;
                }
                EventData::Key { k, m } => {
                    self.key(*k, *m)?;
                    stats.keys += 1;
                }
                EventData::Text { s } => {
                    self.type_text(s)?;
                    stats.text_chars += s.len();
                }
                // Context, App, Paste events are informational - skip during replay
                _ => {}
            }
        }

        Ok(stats)
    }

    fn click(&self, x: i32, y: i32, button: u8, clicks: u8) -> Result<()> {
        let pos = cg::Point { x: x as f64, y: y as f64 };
        let btn = match button {
            0 => cg::MouseButton::Left,
            1 => cg::MouseButton::Right,
            _ => cg::MouseButton::Center,
        };

        let down_type = match button {
            0 => cg::EventType::LEFT_MOUSE_DOWN,
            1 => cg::EventType::RIGHT_MOUSE_DOWN,
            _ => cg::EventType::OHTER_MOUSE_DOWN,
        };
        let up_type = match button {
            0 => cg::EventType::LEFT_MOUSE_UP,
            1 => cg::EventType::RIGHT_MOUSE_UP,
            _ => cg::EventType::OHTER_MOUSE_UP,
        };

        for _ in 0..clicks.max(1) {
            // Mouse down
            if let Some(evt) = cg::Event::mouse(None, down_type, pos, btn) {
                post_event(&evt, HID_EVENT_TAP);
            }
            std::thread::sleep(Duration::from_millis(10));
            // Mouse up
            if let Some(evt) = cg::Event::mouse(None, up_type, pos, btn) {
                post_event(&evt, HID_EVENT_TAP);
            }
            if clicks > 1 {
                std::thread::sleep(Duration::from_millis(50));
            }
        }

        Ok(())
    }

    fn move_to(&self, x: i32, y: i32) -> Result<()> {
        let pos = cg::Point { x: x as f64, y: y as f64 };
        if let Some(evt) = cg::Event::mouse(None, cg::EventType::MOUSE_MOVED, pos, cg::MouseButton::Left) {
            post_event(&evt, HID_EVENT_TAP);
        }
        Ok(())
    }

    fn scroll(&self, x: i32, y: i32, dx: i16, dy: i16) -> Result<()> {
        // Move to position first
        self.move_to(x, y)?;

        // Create scroll event
        if let Some(evt) = cg::Event::wheel_2(
            None,
            cg::ScrollEventUnit::Line,
            dy.unsigned_abs() as u32,
            dx.unsigned_abs() as u32,
        ) {
            post_event(&evt, HID_EVENT_TAP);
        }
        Ok(())
    }

    fn key(&self, keycode: u16, modifiers: u8) -> Result<()> {
        // Build flags
        let mut flags = cg::EventFlags(0);
        if modifiers & Modifiers::SHIFT != 0 { flags.0 |= 0x20000; }
        if modifiers & Modifiers::CTRL != 0 { flags.0 |= 0x40000; }
        if modifiers & Modifiers::OPT != 0 { flags.0 |= 0x80000; }
        if modifiers & Modifiers::CMD != 0 { flags.0 |= 0x100000; }

        // Key down
        if let Some(mut evt) = cg::Event::keyboard(None, keycode, true) {
            evt.set_flags(flags);
            post_event(&evt, HID_EVENT_TAP);
        }

        std::thread::sleep(Duration::from_millis(10));

        // Key up
        if let Some(mut evt) = cg::Event::keyboard(None, keycode, false) {
            evt.set_flags(flags);
            post_event(&evt, HID_EVENT_TAP);
        }

        Ok(())
    }

    fn type_text(&self, text: &str) -> Result<()> {
        for c in text.chars() {
            if let Some((keycode, shift)) = char_to_keycode(c) {
                let mods = if shift { Modifiers::SHIFT } else { 0 };
                self.key(keycode, mods)?;
                std::thread::sleep(Duration::from_millis(20));
            }
        }
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

/// Convert char to (keycode, needs_shift)
fn char_to_keycode(c: char) -> Option<(u16, bool)> {
    Some(match c {
        'a' | 'A' => (0, c.is_uppercase()),
        'b' | 'B' => (11, c.is_uppercase()),
        'c' | 'C' => (8, c.is_uppercase()),
        'd' | 'D' => (2, c.is_uppercase()),
        'e' | 'E' => (14, c.is_uppercase()),
        'f' | 'F' => (3, c.is_uppercase()),
        'g' | 'G' => (5, c.is_uppercase()),
        'h' | 'H' => (4, c.is_uppercase()),
        'i' | 'I' => (34, c.is_uppercase()),
        'j' | 'J' => (38, c.is_uppercase()),
        'k' | 'K' => (40, c.is_uppercase()),
        'l' | 'L' => (37, c.is_uppercase()),
        'm' | 'M' => (46, c.is_uppercase()),
        'n' | 'N' => (45, c.is_uppercase()),
        'o' | 'O' => (31, c.is_uppercase()),
        'p' | 'P' => (35, c.is_uppercase()),
        'q' | 'Q' => (12, c.is_uppercase()),
        'r' | 'R' => (15, c.is_uppercase()),
        's' | 'S' => (1, c.is_uppercase()),
        't' | 'T' => (17, c.is_uppercase()),
        'u' | 'U' => (32, c.is_uppercase()),
        'v' | 'V' => (9, c.is_uppercase()),
        'w' | 'W' => (13, c.is_uppercase()),
        'x' | 'X' => (7, c.is_uppercase()),
        'y' | 'Y' => (16, c.is_uppercase()),
        'z' | 'Z' => (6, c.is_uppercase()),
        '0' | ')' => (29, c == ')'),
        '1' | '!' => (18, c == '!'),
        '2' | '@' => (19, c == '@'),
        '3' | '#' => (20, c == '#'),
        '4' | '$' => (21, c == '$'),
        '5' | '%' => (23, c == '%'),
        '6' | '^' => (22, c == '^'),
        '7' | '&' => (26, c == '&'),
        '8' | '*' => (28, c == '*'),
        '9' | '(' => (25, c == '('),
        ' ' => (49, false),
        '\n' => (36, false),
        '\t' => (48, false),
        '\x08' => (51, false), // backspace
        '-' | '_' => (27, c == '_'),
        '=' | '+' => (24, c == '+'),
        '[' | '{' => (33, c == '{'),
        ']' | '}' => (30, c == '}'),
        '\\' | '|' => (42, c == '|'),
        ';' | ':' => (41, c == ':'),
        '\'' | '"' => (39, c == '"'),
        ',' | '<' => (43, c == '<'),
        '.' | '>' => (47, c == '>'),
        '/' | '?' => (44, c == '?'),
        '`' | '~' => (50, c == '~'),
        _ => return None,
    })
}
