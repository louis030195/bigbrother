//! bigbrother-recorder - Cross-platform workflow recording
//!
//! Efficient recording of user interactions with UI element context.
//! Optimized for AI consumption.
//!
//! ## Platform Support
//!
//! - **macOS**: Full support via CGEventTap
//! - **Windows**: Full support via rdev + SendInput
//! - **Linux**: Coming soon (libevdev)

pub mod events;
pub mod platform;
pub mod storage;

#[cfg(target_os = "macos")]
pub mod recorder;
#[cfg(target_os = "macos")]
pub mod replay;

pub use events::*;

// macOS exports
#[cfg(target_os = "macos")]
pub use recorder::{
    EventStream, PermissionStatus, RecorderConfig, RecordingHandle, Receiver, Sender,
    WorkflowRecorder,
};
#[cfg(target_os = "macos")]
pub use replay::Replayer;

// Windows exports
#[cfg(target_os = "windows")]
pub use platform::windows::{
    EventStream, PermissionStatus, RecorderConfig, RecordingHandle, ReplayStats, Replayer,
    WorkflowRecorder,
};

pub use storage::WorkflowStorage;

pub mod prelude {
    pub use crate::events::*;
    pub use crate::storage::WorkflowStorage;

    #[cfg(target_os = "macos")]
    pub use crate::recorder::{
        EventStream, PermissionStatus, RecorderConfig, RecordingHandle, Receiver, Sender,
        WorkflowRecorder,
    };
    #[cfg(target_os = "macos")]
    pub use crate::replay::Replayer;

    #[cfg(target_os = "windows")]
    pub use crate::platform::windows::{
        EventStream, PermissionStatus, RecorderConfig, RecordingHandle, ReplayStats, Replayer,
        WorkflowRecorder,
    };
}
