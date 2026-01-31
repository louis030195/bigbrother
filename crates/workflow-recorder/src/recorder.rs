//! Efficient workflow recorder using CGEventTap + NSWorkspace notifications
//!
//! Optimized for minimal CPU/memory usage while capturing everything.

use crate::events::*;
use anyhow::Result;
use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use cidre::{cf, cg, ns};
use cidre::cg::event::access as cg_access;

// Keycodes for clipboard operations
const KEY_C: u16 = 8;
const KEY_X: u16 = 7;
const KEY_V: u16 = 9;

/// Recorder configuration
#[derive(Debug, Clone)]
pub struct RecorderConfig {
    /// Mouse move sampling - record every N pixels moved
    pub mouse_move_threshold: f64,
    /// Text aggregation timeout in ms
    pub text_timeout_ms: u64,
    /// Max events before auto-flush to disk
    pub max_buffer: usize,
    /// Capture element context on clicks (slower but richer)
    pub capture_context: bool,
}

impl Default for RecorderConfig {
    fn default() -> Self {
        Self {
            mouse_move_threshold: 5.0,
            text_timeout_ms: 300,
            max_buffer: 10000,
            capture_context: true,
        }
    }
}

/// Recording handle
pub struct RecordingHandle {
    stop: Arc<AtomicBool>,
    events_rx: Receiver<Event>,
    threads: Vec<thread::JoinHandle<()>>,
}

impl RecordingHandle {
    pub fn stop(self, workflow: &mut RecordedWorkflow) {
        self.stop.store(true, Ordering::SeqCst);
        // Drain remaining events
        while let Ok(e) = self.events_rx.try_recv() {
            workflow.events.push(e);
        }
        for t in self.threads {
            let _ = t.join();
        }
    }

    pub fn drain(&self, workflow: &mut RecordedWorkflow) {
        while let Ok(e) = self.events_rx.try_recv() {
            workflow.events.push(e);
        }
    }

    pub fn is_running(&self) -> bool {
        !self.stop.load(Ordering::Relaxed)
    }
}

/// Permission status
#[derive(Debug, Clone)]
pub struct PermissionStatus {
    pub accessibility: bool,
    pub input_monitoring: bool,
}

impl PermissionStatus {
    pub fn all_granted(&self) -> bool {
        self.accessibility && self.input_monitoring
    }
}

/// The recorder
pub struct WorkflowRecorder {
    config: RecorderConfig,
}

impl WorkflowRecorder {
    pub fn new() -> Self {
        Self::with_config(RecorderConfig::default())
    }

    pub fn with_config(config: RecorderConfig) -> Self {
        Self { config }
    }

    pub fn check_permissions(&self) -> PermissionStatus {
        PermissionStatus {
            accessibility: cidre::ax::is_process_trusted(),
            input_monitoring: cg_access::listen_preflight(),
        }
    }

    pub fn request_permissions(&self) -> PermissionStatus {
        PermissionStatus {
            accessibility: cidre::ax::is_process_trusted_with_prompt(true),
            input_monitoring: cg_access::listen_request(),
        }
    }

    pub fn start(&self, name: impl Into<String>) -> Result<(RecordedWorkflow, RecordingHandle)> {
        let workflow = RecordedWorkflow::new(name);
        let (tx, rx) = bounded::<Event>(self.config.max_buffer);
        let stop = Arc::new(AtomicBool::new(false));
        let start_time = Instant::now();

        let mut threads = Vec::new();

        // Thread 1: CGEventTap for input events (includes clipboard via Cmd+C/X/V)
        let tx1 = tx.clone();
        let stop1 = stop.clone();
        let config1 = self.config.clone();
        threads.push(thread::spawn(move || {
            run_event_tap(tx1, stop1, start_time, config1);
        }));

        // Thread 2: App switch notifications
        let tx2 = tx.clone();
        let stop2 = stop.clone();
        threads.push(thread::spawn(move || {
            run_app_observer(tx2, stop2, start_time);
        }));

        let handle = RecordingHandle {
            stop,
            events_rx: rx,
            threads,
        };

        Ok((workflow, handle))
    }
}

impl Default for WorkflowRecorder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Event Tap Thread
// ============================================================================

struct TapState {
    tx: Sender<Event>,
    start: Instant,
    config: RecorderConfig,
    last_mouse: Mutex<(f64, f64)>,
    text_buf: Mutex<TextBuffer>,
}

struct TextBuffer {
    chars: String,
    first_time: Option<Instant>,
    last_time: Option<Instant>,
    timeout_ms: u64,
}

impl TextBuffer {
    fn new(timeout_ms: u64) -> Self {
        Self {
            chars: String::new(),
            first_time: None,
            last_time: None,
            timeout_ms,
        }
    }

    fn push(&mut self, c: char) {
        let now = Instant::now();
        if self.chars.is_empty() {
            self.first_time = Some(now);
        }
        self.chars.push(c);
        self.last_time = Some(now);
    }

    fn flush(&mut self) -> Option<String> {
        if self.chars.is_empty() {
            return None;
        }
        let s = std::mem::take(&mut self.chars);
        self.first_time = None;
        self.last_time = None;
        Some(s)
    }

    fn should_flush(&self) -> bool {
        if let Some(last) = self.last_time {
            last.elapsed().as_millis() as u64 >= self.timeout_ms
        } else {
            false
        }
    }
}

fn run_event_tap(tx: Sender<Event>, stop: Arc<AtomicBool>, start: Instant, config: RecorderConfig) {
    // Build event mask - capture everything
    let mask = cg::EventType::LEFT_MOUSE_DOWN.mask()
        | cg::EventType::LEFT_MOUSE_UP.mask()
        | cg::EventType::RIGHT_MOUSE_DOWN.mask()
        | cg::EventType::RIGHT_MOUSE_UP.mask()
        | cg::EventType::MOUSE_MOVED.mask()
        | cg::EventType::LEFT_MOUSE_DRAGGED.mask()
        | cg::EventType::RIGHT_MOUSE_DRAGGED.mask()
        | cg::EventType::KEY_DOWN.mask()
        | cg::EventType::SCROLL_WHEEL.mask();

    let state = Box::leak(Box::new(TapState {
        tx,
        start,
        config: config.clone(),
        last_mouse: Mutex::new((0.0, 0.0)),
        text_buf: Mutex::new(TextBuffer::new(config.text_timeout_ms)),
    }));

    let tap = cg::EventTap::new(
        cg::EventTapLocation::Session,
        cg::EventTapPlacement::TailAppend,
        cg::EventTapOpts::LISTEN_ONLY,
        mask,
        tap_callback,
        state as *mut TapState,
    );

    let Some(tap) = tap else {
        eprintln!("Failed to create event tap");
        return;
    };

    let Some(src) = cf::MachPort::run_loop_src(&tap, 0) else {
        eprintln!("Failed to create run loop source");
        return;
    };

    let rl = cf::RunLoop::current();
    rl.add_src(&src, cf::RunLoopMode::default());

    while !stop.load(Ordering::Relaxed) {
        cf::RunLoop::run_in_mode(cf::RunLoopMode::default(), 0.05, true);

        // Check text buffer timeout
        let mut buf = state.text_buf.lock();
        if buf.should_flush() {
            if let Some(s) = buf.flush() {
                let _ = state.tx.try_send(Event {
                    t: state.start.elapsed().as_millis() as u64,
                    data: EventData::Text { s },
                });
            }
        }
    }

    // Final flush
    let mut buf = state.text_buf.lock();
    if let Some(s) = buf.flush() {
        let _ = state.tx.try_send(Event {
            t: state.start.elapsed().as_millis() as u64,
            data: EventData::Text { s },
        });
    }

    rl.remove_src(&src, cf::RunLoopMode::default());
}

extern "C" fn tap_callback(
    _proxy: *mut cg::EventTapProxy,
    event_type: cg::EventType,
    event: &mut cg::Event,
    user_info: *mut TapState,
) -> Option<&cg::Event> {
    let state = unsafe { &*user_info };
    let t = state.start.elapsed().as_millis() as u64;
    let loc = event.location();
    let flags = event.flags().0;
    let mods = Modifiers::from_cg_flags(flags);

    match event_type {
        cg::EventType::LEFT_MOUSE_DOWN | cg::EventType::RIGHT_MOUSE_DOWN => {
            let btn = if event_type == cg::EventType::LEFT_MOUSE_DOWN { 0 } else { 1 };
            let clicks = event.field_i64(cg::EventField::MOUSE_EVENT_CLICK_STATE) as u8;

            let _ = state.tx.try_send(Event {
                t,
                data: EventData::Click {
                    x: loc.x as i32,
                    y: loc.y as i32,
                    b: btn,
                    n: clicks,
                    m: mods.0,
                },
            });

            // Capture element context in background (non-blocking)
            if state.config.capture_context {
                let tx = state.tx.clone();
                let x = loc.x;
                let y = loc.y;
                let start = state.start;
                std::thread::spawn(move || {
                    if let Some(ctx) = get_element_context(x, y) {
                        let _ = tx.try_send(Event {
                            t: start.elapsed().as_millis() as u64,
                            data: ctx,
                        });
                    }
                });
            }
        }

        cg::EventType::MOUSE_MOVED
        | cg::EventType::LEFT_MOUSE_DRAGGED
        | cg::EventType::RIGHT_MOUSE_DRAGGED => {
            let mut last = state.last_mouse.lock();
            let dx = loc.x - last.0;
            let dy = loc.y - last.1;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist >= state.config.mouse_move_threshold {
                *last = (loc.x, loc.y);
                let _ = state.tx.try_send(Event {
                    t,
                    data: EventData::Move {
                        x: loc.x as i32,
                        y: loc.y as i32,
                    },
                });
            }
        }

        cg::EventType::SCROLL_WHEEL => {
            let dy = event.field_i64(cg::EventField::SCROLL_WHEEL_EVENT_DELTA_AXIS1) as i16;
            let dx = event.field_i64(cg::EventField::SCROLL_WHEEL_EVENT_DELTA_AXIS2) as i16;
            if dx != 0 || dy != 0 {
                let _ = state.tx.try_send(Event {
                    t,
                    data: EventData::Scroll {
                        x: loc.x as i32,
                        y: loc.y as i32,
                        dx,
                        dy,
                    },
                });
            }
        }

        cg::EventType::KEY_DOWN => {
            let keycode = event.field_i64(cg::EventField::KEYBOARD_EVENT_KEYCODE) as u16;

            // Check for clipboard operations (Cmd+C, Cmd+X, Cmd+V)
            if mods.has_cmd() && !mods.has_ctrl() {
                match keycode {
                    KEY_C => {
                        // Copy - capture clipboard after a short delay
                        let tx = state.tx.clone();
                        let start = state.start;
                        std::thread::spawn(move || {
                            // Wait for clipboard to be populated
                            std::thread::sleep(std::time::Duration::from_millis(50));
                            if let Some(content) = get_clipboard() {
                                let _ = tx.try_send(Event {
                                    t: start.elapsed().as_millis() as u64,
                                    data: EventData::Paste { o: 'c', s: truncate(&content, 100) },
                                });
                            }
                        });
                        // Also record the key event
                        let _ = state.tx.try_send(Event {
                            t,
                            data: EventData::Key { k: keycode, m: mods.0 },
                        });
                    }
                    KEY_X => {
                        // Cut - capture clipboard after a short delay
                        let tx = state.tx.clone();
                        let start = state.start;
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(50));
                            if let Some(content) = get_clipboard() {
                                let _ = tx.try_send(Event {
                                    t: start.elapsed().as_millis() as u64,
                                    data: EventData::Paste { o: 'x', s: truncate(&content, 100) },
                                });
                            }
                        });
                        let _ = state.tx.try_send(Event {
                            t,
                            data: EventData::Key { k: keycode, m: mods.0 },
                        });
                    }
                    KEY_V => {
                        // Paste - capture what's being pasted
                        if let Some(content) = get_clipboard() {
                            let _ = state.tx.try_send(Event {
                                t,
                                data: EventData::Paste { o: 'v', s: truncate(&content, 100) },
                            });
                        }
                        let _ = state.tx.try_send(Event {
                            t,
                            data: EventData::Key { k: keycode, m: mods.0 },
                        });
                    }
                    _ => {
                        // Other Cmd combo
                        let _ = state.tx.try_send(Event {
                            t,
                            data: EventData::Key { k: keycode, m: mods.0 },
                        });
                    }
                }
            } else if mods.any_modifier() {
                // Other modifier combo
                let _ = state.tx.try_send(Event {
                    t,
                    data: EventData::Key { k: keycode, m: mods.0 },
                });
            } else if let Some(c) = keycode_to_char(keycode, mods) {
                // Aggregate into text buffer
                state.text_buf.lock().push(c);
            } else {
                // Unknown key, record as key event
                let _ = state.tx.try_send(Event {
                    t,
                    data: EventData::Key { k: keycode, m: mods.0 },
                });
            }
        }

        _ => {}
    }

    Some(event)
}

/// Get clipboard content via pbpaste
fn get_clipboard() -> Option<String> {
    std::process::Command::new("pbpaste")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .filter(|s| !s.is_empty())
}

fn get_element_context(x: f64, y: f64) -> Option<EventData> {
    use cidre::ax;

    let sys = ax::UiElement::sys_wide();
    let elem = sys.element_at_pos(x as f32, y as f32).ok()?;

    let role = elem.role().ok().map(|r| {
        let s = format!("{:?}", r);
        s.find("AX").map(|i| {
            let rest = &s[i..];
            rest.find(|c| c == ')' || c == '"').map(|j| rest[..j].to_string()).unwrap_or(rest.to_string())
        }).unwrap_or_else(|| "?".to_string())
    })?;

    let name = get_str_attr(&elem, ax::attr::title())
        .or_else(|| get_str_attr(&elem, ax::attr::desc()));
    let value = get_str_attr(&elem, ax::attr::value());

    Some(EventData::Context {
        r: role,
        n: name.map(|s| truncate(&s, 50)),
        v: value.map(|s| truncate(&s, 50)),
    })
}

fn get_str_attr(elem: &cidre::ax::UiElement, attr: &cidre::ax::Attr) -> Option<String> {
    elem.attr_value(attr).ok().and_then(|v| {
        if v.get_type_id() == cidre::cf::String::type_id() {
            let s: &cidre::cf::String = unsafe { std::mem::transmute(&*v) };
            Some(s.to_string())
        } else {
            None
        }
    })
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() } else { format!("{}...", &s[..max-3]) }
}

// ============================================================================
// App Observer Thread
// ============================================================================

fn run_app_observer(tx: Sender<Event>, stop: Arc<AtomicBool>, start: Instant) {
    let workspace = ns::Workspace::shared();
    let mut nc = workspace.notification_center();

    let tx_clone = tx.clone();
    let start_clone = start;

    let _guard = nc.add_observer_guard(
        ns::workspace::notification::did_activate_app(),
        None,
        None,
        move |notif| {
            // Extract app info from notification
            if let Some(user_info) = notif.user_info() {
                let key = ns::workspace::notification::app_key();
                if let Some(app) = user_info.get(key.as_ref()) {
                    // app is NSRunningApplication
                    let app: &ns::RunningApp = unsafe { std::mem::transmute(app) };
                    let name = app.localized_name()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "?".to_string());
                    let pid = app.pid();

                    let _ = tx_clone.try_send(Event {
                        t: start_clone.elapsed().as_millis() as u64,
                        data: EventData::App { n: name, p: pid },
                    });
                }
            }
        },
    );

    // Run loop to receive notifications
    let _rl = cf::RunLoop::current();
    while !stop.load(Ordering::Relaxed) {
        cf::RunLoop::run_in_mode(cf::RunLoopMode::default(), 0.1, true);
    }
}

// ============================================================================
// Keycode Mapping
// ============================================================================

fn keycode_to_char(keycode: u16, mods: Modifiers) -> Option<char> {
    let shift = mods.0 & Modifiers::SHIFT != 0 || mods.0 & Modifiers::CAPS != 0;

    let c = match keycode {
        // Letters
        0 => 'a', 1 => 's', 2 => 'd', 3 => 'f', 4 => 'h', 5 => 'g', 6 => 'z', 7 => 'x',
        8 => 'c', 9 => 'v', 11 => 'b', 12 => 'q', 13 => 'w', 14 => 'e', 15 => 'r',
        16 => 'y', 17 => 't', 31 => 'o', 32 => 'u', 34 => 'i', 35 => 'p', 37 => 'l',
        38 => 'j', 40 => 'k', 45 => 'n', 46 => 'm',
        // Numbers
        18 => if shift { '!' } else { '1' },
        19 => if shift { '@' } else { '2' },
        20 => if shift { '#' } else { '3' },
        21 => if shift { '$' } else { '4' },
        22 => if shift { '^' } else { '6' },
        23 => if shift { '%' } else { '5' },
        24 => if shift { '+' } else { '=' },
        25 => if shift { '(' } else { '9' },
        26 => if shift { '&' } else { '7' },
        27 => if shift { '_' } else { '-' },
        28 => if shift { '*' } else { '8' },
        29 => if shift { ')' } else { '0' },
        // Punctuation
        30 => if shift { '}' } else { ']' },
        33 => if shift { '{' } else { '[' },
        39 => if shift { '"' } else { '\'' },
        41 => if shift { ':' } else { ';' },
        42 => if shift { '|' } else { '\\' },
        43 => if shift { '<' } else { ',' },
        44 => if shift { '?' } else { '/' },
        47 => if shift { '>' } else { '.' },
        50 => if shift { '~' } else { '`' },
        // Whitespace
        36 => '\n',
        48 => '\t',
        49 => ' ',
        // Backspace - special handling
        51 => '\x08',
        _ => return None,
    };

    // Handle shift for letters
    if shift && c.is_ascii_lowercase() {
        Some(c.to_ascii_uppercase())
    } else {
        Some(c)
    }
}
