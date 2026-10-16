// screenpipe — AI that knows everything you've seen, said, and heard
// https://screenpipe.com
//! bigbrother-core - Cross-platform desktop automation for AI agents
//!
//! Deterministic Rust primitives with structured JSON output.
//! When things fail, AI writes more Rust to recover.
//!
//! ## Platform Support
//!
//! - **macOS**: Full support via Accessibility API
//! - **Windows**: Full support via UI Automation + SendInput
//! - **Linux**: Coming soon (AT-SPI2)

pub mod error;
pub mod observation;
pub mod platform;
pub mod runtime;

#[cfg(target_os = "macos")]
pub mod accessibility;
#[cfg(target_os = "macos")]
pub mod apps;
#[cfg(target_os = "macos")]
pub mod desktop;
#[cfg(target_os = "macos")]
pub mod element;
#[cfg(target_os = "macos")]
pub mod input;
#[cfg(target_os = "macos")]
pub mod locator;
#[cfg(target_os = "macos")]
pub mod selector;

// macOS exports
#[cfg(target_os = "macos")]
pub use desktop::Desktop;
#[cfg(target_os = "macos")]
pub use element::UIElement;
pub use error::{Error, ErrorCode, Result};
#[cfg(target_os = "macos")]
pub use locator::Locator;
#[cfg(target_os = "macos")]
pub use selector::Selector;

// Windows exports
#[cfg(target_os = "windows")]
pub use platform::windows::{
    click, click_at, double_click, find_window, get_windows, key_down, key_up, middle_click,
    move_mouse, press_key, right_click, scroll, shortcut, type_text, vk, Automation, Element,
    TreeWalker,
};

pub mod prelude {
    #[cfg(target_os = "macos")]
    pub use crate::desktop::Desktop;
    #[cfg(target_os = "macos")]
    pub use crate::element::UIElement;
    pub use crate::error::{Error, ErrorCode, Result};
    #[cfg(target_os = "macos")]
    pub use crate::locator::Locator;
    #[cfg(target_os = "macos")]
    pub use crate::selector::Selector;

    #[cfg(target_os = "windows")]
    pub use crate::platform::windows::{
        click, click_at, double_click, find_window, get_windows, key_down, key_up, middle_click,
        move_mouse, press_key, right_click, scroll, shortcut, type_text, vk, Automation, Element,
        TreeWalker,
    };
}

/// Check if the process has accessibility permissions
pub fn has_accessibility() -> bool {
    platform::current::has_accessibility()
}

/// Ensure accessibility permissions are granted
pub fn ensure_accessibility() -> Result<()> {
    platform::current::ensure_accessibility()
}
