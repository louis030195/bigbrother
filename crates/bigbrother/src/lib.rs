//! macOS Workflow Recorder
//!
//! Efficient recording of user interactions with UI element context.
//! Optimized for AI consumption.

pub mod events;
pub mod recorder;
pub mod replay;
pub mod storage;

pub use events::*;
pub use recorder::{WorkflowRecorder, RecorderConfig, PermissionStatus, RecordingHandle, EventStream, Receiver, Sender};
pub use replay::Replayer;
pub use storage::WorkflowStorage;

pub mod prelude {
    pub use crate::events::*;
    pub use crate::recorder::{WorkflowRecorder, RecorderConfig, PermissionStatus, RecordingHandle, EventStream, Receiver, Sender};
    pub use crate::replay::Replayer;
    pub use crate::storage::WorkflowStorage;
}
