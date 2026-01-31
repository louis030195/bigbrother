//! superintelligence - macOS desktop automation for AI agents
//!
//! Deterministic Rust primitives with structured JSON output.
//! When things fail, AI writes more Rust to recover.

pub mod accessibility;
pub mod apps;
pub mod desktop;
pub mod element;
pub mod error;
pub mod input;
pub mod locator;
pub mod selector;

pub use desktop::Desktop;
pub use element::UIElement;
pub use error::{Error, ErrorCode, Result};
pub use locator::Locator;
pub use selector::Selector;

pub mod prelude {
    pub use crate::desktop::Desktop;
    pub use crate::element::UIElement;
    pub use crate::error::{Error, ErrorCode, Result};
    pub use crate::locator::Locator;
    pub use crate::selector::Selector;
}

use cidre::ax;

pub fn ensure_accessibility() -> Result<()> {
    if ax::is_process_trusted() {
        return Ok(());
    }
    ax::is_process_trusted_with_prompt(true);
    Err(Error::permission_denied(
        "Accessibility permissions required. Enable in System Settings > Privacy & Security > Accessibility"
    ))
}

pub fn has_accessibility() -> bool {
    ax::is_process_trusted()
}
