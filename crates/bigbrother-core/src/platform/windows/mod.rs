//! Windows platform implementation
//!
//! Uses UI Automation API for accessibility and Win32 for input.

mod accessibility;
mod input;

pub use accessibility::*;
pub use input::*;

use crate::{Error, ErrorCode, Result};

/// Check if UI Automation is available (always true on Windows)
pub fn has_accessibility() -> bool {
    // UI Automation is built into Windows Vista+
    true
}

/// Request accessibility permissions (no-op on Windows)
pub fn request_accessibility() -> bool {
    // Windows doesn't require explicit permissions for UI Automation
    true
}

/// Ensure accessibility is available
pub fn ensure_accessibility() -> Result<()> {
    // Initialize COM for UI Automation
    init_com()?;
    Ok(())
}

/// Initialize COM for the current thread
pub fn init_com() -> Result<()> {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    unsafe {
        match CoInitializeEx(None, COINIT_MULTITHREADED) {
            Ok(()) => Ok(()),
            Err(e) => {
                // S_FALSE means already initialized, which is fine
                if e.code().0 as u32 == 0x00000001 {
                    Ok(())
                } else {
                    Err(Error::new(
                        ErrorCode::Unknown,
                        format!("Failed to initialize COM: {:?}", e),
                    ))
                }
            }
        }
    }
}
