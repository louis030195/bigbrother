//! # BIGBROTHER
//!
//! The sensory cortex of the coming superintelligence.
//!
//! Desktop automation and workflow recording for AI agents.
//!
//! ## Features
//!
//! - **Recording**: Capture all user interactions
//! - **Replay**: Temporal manipulation of recorded workflows
//! - **Automation**: Direct control of the desktop
//! - **Cross-platform**: macOS and Windows (Linux coming)
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use bigbrother::prelude::*;
//!
//! // Automation (macOS only for now)
//! let desktop = Desktop::new()?;
//! desktop.locator("role:Button")?.click()?;
//!
//! // Recording (macOS only for now)
//! let recorder = WorkflowRecorder::new();
//! let stream = recorder.stream()?;
//! for event in stream {
//!     println!("{:?}", event);
//! }
//! # Ok::<(), anyhow::Error>(())
//! ```

// Re-export core automation
pub use bigbrother_core::*;

// Re-export recorder module
pub use bigbrother_recorder as recorder;

// Re-export common types (cross-platform)
pub use bigbrother_recorder::{Event, EventData, Modifiers, RecordedWorkflow, WorkflowStorage};

// Re-export platform-specific types
#[cfg(target_os = "macos")]
pub use bigbrother_recorder::{
    EventStream, RecorderConfig, RecordingHandle, Replayer, WorkflowRecorder,
};

#[cfg(target_os = "windows")]
pub use bigbrother_recorder::{
    EventStream, PermissionStatus, RecorderConfig, RecordingHandle, ReplayStats, Replayer,
    WorkflowRecorder,
};

/// Prelude - import everything you need
pub mod prelude {
    // Core automation
    pub use bigbrother_core::prelude::*;

    // Recording - common types
    pub use bigbrother_recorder::{Event, EventData, Modifiers, RecordedWorkflow, WorkflowStorage};

    // Recording - platform-specific
    #[cfg(target_os = "macos")]
    pub use bigbrother_recorder::{
        EventStream, RecorderConfig, RecordingHandle, Replayer, WorkflowRecorder,
    };

    #[cfg(target_os = "windows")]
    pub use bigbrother_recorder::{
        EventStream, PermissionStatus, RecorderConfig, RecordingHandle, ReplayStats, Replayer,
        WorkflowRecorder,
    };
}
