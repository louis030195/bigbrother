//! Keyboard and mouse input simulation

use anyhow::{Context, Result};
use std::process::Command;
use std::thread;
use std::time::Duration;

/// Common key codes for macOS
pub mod key_codes {
    pub const RETURN: u8 = 36;
    pub const TAB: u8 = 48;
    pub const SPACE: u8 = 49;
    pub const DELETE: u8 = 51;
    pub const ESCAPE: u8 = 53;
    pub const COMMAND: u8 = 55;
    pub const SHIFT: u8 = 56;
    pub const CAPS_LOCK: u8 = 57;
    pub const OPTION: u8 = 58;
    pub const CONTROL: u8 = 59;
    pub const ARROW_LEFT: u8 = 123;
    pub const ARROW_RIGHT: u8 = 124;
    pub const ARROW_DOWN: u8 = 125;
    pub const ARROW_UP: u8 = 126;
    pub const PAGE_UP: u8 = 116;
    pub const PAGE_DOWN: u8 = 121;
    pub const HOME: u8 = 115;
    pub const END: u8 = 119;
    pub const F1: u8 = 122;
    pub const F2: u8 = 120;
    pub const F3: u8 = 99;
    pub const F4: u8 = 118;
    pub const F5: u8 = 96;
    pub const F6: u8 = 97;
    pub const F7: u8 = 98;
    pub const F8: u8 = 100;
    pub const F9: u8 = 101;
    pub const F10: u8 = 109;
    pub const F11: u8 = 103;
    pub const F12: u8 = 111;
}

/// Press a key by key code
pub fn press_key(key_code: u8) -> Result<()> {
    let script = format!(
        r#"tell application "System Events" to key code {}"#,
        key_code
    );

    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .context("Failed to press key")?;

    Ok(())
}

/// Press a key multiple times with delay
pub fn press_key_repeat(key_code: u8, times: u32, delay_ms: u64) -> Result<()> {
    let script = format!(
        r#"
        tell application "System Events"
            repeat {} times
                key code {}
                delay {}
            end repeat
        end tell
        "#,
        times,
        key_code,
        delay_ms as f64 / 1000.0
    );

    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .context("Failed to press key")?;

    Ok(())
}

/// Type text using keystroke
pub fn type_text(text: &str) -> Result<()> {
    let escaped = text.replace("\\", "\\\\").replace("\"", "\\\"");
    let script = format!(
        r#"tell application "System Events" to keystroke "{}""#,
        escaped
    );

    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .context("Failed to type text")?;

    Ok(())
}

/// Press a keyboard shortcut (e.g., Cmd+C)
pub fn shortcut(key: &str, modifiers: &[&str]) -> Result<()> {
    let modifier_str = modifiers
        .iter()
        .map(|m| format!("{} down", m))
        .collect::<Vec<_>>()
        .join(", ");

    let script = format!(
        r#"tell application "System Events" to keystroke "{}" using {{{}}}"#,
        key, modifier_str
    );

    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .context("Failed to execute shortcut")?;

    Ok(())
}

/// Press Cmd+key shortcut
pub fn cmd(key: &str) -> Result<()> {
    shortcut(key, &["command"])
}

/// Scroll up in the frontmost application
pub fn scroll_up(pages: u32) -> Result<()> {
    press_key_repeat(key_codes::PAGE_UP, pages, 300)
}

/// Scroll down in the frontmost application
pub fn scroll_down(pages: u32) -> Result<()> {
    press_key_repeat(key_codes::PAGE_DOWN, pages, 300)
}

/// Scroll up in a specific application
pub fn scroll_up_in_app(app_name: &str, pages: u32, delay_ms: u64) -> Result<()> {
    let script = format!(
        r#"
        tell application "{}"
            activate
        end tell
        delay 0.3
        tell application "System Events"
            repeat {} times
                key code {}
                delay {}
            end repeat
        end tell
        "#,
        app_name,
        pages,
        key_codes::PAGE_UP,
        delay_ms as f64 / 1000.0
    );

    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .context("Failed to scroll")?;

    thread::sleep(Duration::from_millis(500));
    Ok(())
}

/// Click at screen coordinates
/// button: "left", "right", or "double"
pub fn click_at(x: i32, y: i32, button: &str) -> Result<()> {
    let click_cmd = match button.to_lowercase().as_str() {
        "right" => format!("rc:{},{}", x, y),
        "double" => format!("dc:{},{}", x, y),
        _ => format!("c:{},{}", x, y), // left click default
    };
    
    let script = format!(
        r#"do shell script "cliclick {}""#,
        click_cmd
    );

    // Note: requires cliclick to be installed (brew install cliclick)
    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .context("Failed to click (requires cliclick: brew install cliclick)")?;

    Ok(())
}

/// Move mouse to screen coordinates
pub fn move_mouse(x: i32, y: i32) -> Result<()> {
    let script = format!(
        r#"
        do shell script "cliclick m:{},{}"
        "#,
        x, y
    );

    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .context("Failed to move mouse (requires cliclick: brew install cliclick)")?;

    Ok(())
}
