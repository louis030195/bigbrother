//! Simplified event types optimized for AI consumption
//!
//! Events are stored as simple structs that serialize to compact JSON lines.

use serde::{Deserialize, Serialize};

/// A recorded workflow - just a list of events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedWorkflow {
    pub name: String,
    pub events: Vec<Event>,
}

impl RecordedWorkflow {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            events: Vec::new(),
        }
    }
}

/// Single event - flat structure for efficiency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Milliseconds since recording start
    pub t: u64,
    /// Event type and data
    #[serde(flatten)]
    pub data: EventData,
}

/// Event data - simple tagged union
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "e")]
pub enum EventData {
    /// Mouse click: x, y, button (0=left, 1=right, 2=middle), clicks (1=single, 2=double)
    #[serde(rename = "c")]
    Click { x: i32, y: i32, b: u8, n: u8, m: u8 },

    /// Mouse move: x, y
    #[serde(rename = "m")]
    Move { x: i32, y: i32 },

    /// Mouse scroll: x, y, dx, dy
    #[serde(rename = "s")]
    Scroll { x: i32, y: i32, dx: i16, dy: i16 },

    /// Key down: keycode, modifiers
    #[serde(rename = "k")]
    Key { k: u16, m: u8 },

    /// Text input (aggregated keystrokes)
    #[serde(rename = "t")]
    Text { s: String },

    /// App activated: name, pid
    #[serde(rename = "a")]
    App { n: String, p: i32 },

    /// Clipboard changed: operation (c=copy, x=cut, v=paste), content preview
    #[serde(rename = "p")]
    Paste { o: char, s: String },

    /// Element context at last click position
    #[serde(rename = "x")]
    Context {
        r: String, // role
        #[serde(skip_serializing_if = "Option::is_none")]
        n: Option<String>, // name/title
        #[serde(skip_serializing_if = "Option::is_none")]
        v: Option<String>, // value
    },
}

/// Modifier flags packed into a single byte
/// Bit 0: shift, 1: ctrl, 2: option/alt, 3: command, 4: capslock, 5: fn
#[derive(Debug, Clone, Copy, Default)]
pub struct Modifiers(pub u8);

impl Modifiers {
    pub const SHIFT: u8 = 1 << 0;
    pub const CTRL: u8 = 1 << 1;
    pub const OPT: u8 = 1 << 2;
    pub const CMD: u8 = 1 << 3;
    pub const CAPS: u8 = 1 << 4;
    pub const FN: u8 = 1 << 5;

    pub fn from_cg_flags(flags: u64) -> Self {
        let mut m = 0u8;
        if flags & 0x20000 != 0 { m |= Self::SHIFT; }
        if flags & 0x40000 != 0 { m |= Self::CTRL; }
        if flags & 0x80000 != 0 { m |= Self::OPT; }
        if flags & 0x100000 != 0 { m |= Self::CMD; }
        if flags & 0x10000 != 0 { m |= Self::CAPS; }
        if flags & 0x800000 != 0 { m |= Self::FN; }
        Self(m)
    }

    pub fn has_cmd(&self) -> bool { self.0 & Self::CMD != 0 }
    pub fn has_ctrl(&self) -> bool { self.0 & Self::CTRL != 0 }
    pub fn any_modifier(&self) -> bool { self.0 & (Self::CMD | Self::CTRL) != 0 }
}
