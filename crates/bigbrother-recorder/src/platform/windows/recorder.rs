//! Windows event recorder using rdev
//!
//! Captures global keyboard and mouse events.

use crate::events::*;
use anyhow::Result;
use crossbeam_channel::{bounded, Receiver, Sender};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

/// Recorder configuration
#[derive(Debug, Clone)]
pub struct RecorderConfig {
    /// Mouse move sampling - record every N pixels moved
    pub mouse_move_threshold: f64,
    /// Text aggregation timeout in ms
    pub text_timeout_ms: u64,
    /// Max events before auto-flush
    pub max_buffer: usize,
    /// Capture element context on clicks
    pub capture_context: bool,
}

impl Default for RecorderConfig {
    fn default() -> Self {
        Self {
            mouse_move_threshold: 5.0,
            text_timeout_ms: 300,
            max_buffer: 10000,
            capture_context: false, // Disabled by default on Windows for now
        }
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

/// Recording handle
pub struct RecordingHandle {
    stop: Arc<AtomicBool>,
    events_rx: Receiver<Event>,
    threads: Vec<thread::JoinHandle<()>>,
}

impl RecordingHandle {
    pub fn stop(self, workflow: &mut RecordedWorkflow) {
        self.stop.store(true, Ordering::SeqCst);
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

    pub fn receiver(&self) -> &Receiver<Event> {
        &self.events_rx
    }

    pub fn try_recv(&self) -> Option<Event> {
        self.events_rx.try_recv().ok()
    }

    pub fn recv(&self) -> Option<Event> {
        self.events_rx.recv().ok()
    }

    pub fn recv_timeout(&self, timeout: std::time::Duration) -> Option<Event> {
        self.events_rx.recv_timeout(timeout).ok()
    }
}

/// Event stream for consuming events
pub struct EventStream {
    stop: Arc<AtomicBool>,
    events_rx: Receiver<Event>,
    threads: Vec<thread::JoinHandle<()>>,
}

impl EventStream {
    pub fn stop(self) {
        self.stop.store(true, Ordering::SeqCst);
        for t in self.threads {
            let _ = t.join();
        }
    }

    pub fn is_running(&self) -> bool {
        !self.stop.load(Ordering::Relaxed)
    }

    pub fn receiver(&self) -> &Receiver<Event> {
        &self.events_rx
    }

    pub fn try_recv(&self) -> Option<Event> {
        self.events_rx.try_recv().ok()
    }

    pub fn recv(&self) -> Option<Event> {
        self.events_rx.recv().ok()
    }

    pub fn recv_timeout(&self, timeout: std::time::Duration) -> Option<Event> {
        self.events_rx.recv_timeout(timeout).ok()
    }
}

impl Iterator for EventStream {
    type Item = Event;

    fn next(&mut self) -> Option<Self::Item> {
        if self.stop.load(Ordering::Relaxed) {
            return None;
        }
        self.events_rx.recv().ok()
    }
}

/// Workflow recorder
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
        // Windows doesn't require explicit permissions
        PermissionStatus {
            accessibility: true,
            input_monitoring: true,
        }
    }

    pub fn request_permissions(&self) -> PermissionStatus {
        self.check_permissions()
    }

    pub fn start(&self, name: impl Into<String>) -> Result<(RecordedWorkflow, RecordingHandle)> {
        let workflow = RecordedWorkflow::new(name);
        let (internals, rx) = self.start_capture()?;

        let handle = RecordingHandle {
            stop: internals.1,
            events_rx: rx,
            threads: internals.0,
        };

        Ok((workflow, handle))
    }

    pub fn stream(&self) -> Result<EventStream> {
        let (internals, rx) = self.start_capture()?;

        Ok(EventStream {
            stop: internals.1,
            events_rx: rx,
            threads: internals.0,
        })
    }

    fn start_capture(&self) -> Result<((Vec<thread::JoinHandle<()>>, Arc<AtomicBool>), Receiver<Event>)> {
        let (tx, rx) = bounded::<Event>(self.config.max_buffer);
        let stop = Arc::new(AtomicBool::new(false));
        let start_time = Instant::now();

        let mut threads = Vec::new();

        // Thread 1: rdev event listener
        let tx1 = tx.clone();
        let stop1 = stop.clone();
        let config1 = self.config.clone();
        threads.push(thread::spawn(move || {
            run_rdev_listener(tx1, stop1, start_time, config1);
        }));

        // Thread 2: App/window observer
        let tx2 = tx.clone();
        let stop2 = stop.clone();
        threads.push(thread::spawn(move || {
            run_app_observer(tx2, stop2, start_time);
        }));

        Ok(((threads, stop), rx))
    }
}

impl Default for WorkflowRecorder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// rdev Event Listener
// ============================================================================

fn run_rdev_listener(tx: Sender<Event>, stop: Arc<AtomicBool>, start: Instant, config: RecorderConfig) {
    use rdev::{listen, Event as RdevEvent, EventType};
    use parking_lot::Mutex;

    struct State {
        tx: Sender<Event>,
        start: Instant,
        config: RecorderConfig,
        last_mouse: (f64, f64),
        text_buf: String,
        last_text_time: Option<Instant>,
    }

    let state = Arc::new(Mutex::new(State {
        tx,
        start,
        config,
        last_mouse: (0.0, 0.0),
        text_buf: String::new(),
        last_text_time: None,
    }));

    let state_clone = state.clone();
    let stop_clone = stop.clone();

    // rdev::listen blocks, so we need to handle stop differently
    let callback = move |event: RdevEvent| {
        if stop_clone.load(Ordering::Relaxed) {
            return;
        }

        let mut s = state_clone.lock();
        let t = s.start.elapsed().as_millis() as u64;

        match event.event_type {
            EventType::ButtonPress(button) => {
                let (x, y) = s.last_mouse;
                let b = match button {
                    rdev::Button::Left => 0,
                    rdev::Button::Right => 1,
                    rdev::Button::Middle => 2,
                    _ => 0,
                };
                let _ = s.tx.try_send(Event {
                    t,
                    data: EventData::Click {
                        x: x as i32,
                        y: y as i32,
                        b,
                        n: 1,
                        m: 0,
                    },
                });
            }
            EventType::MouseMove { x, y } => {
                let dx = x - s.last_mouse.0;
                let dy = y - s.last_mouse.1;
                let dist = (dx * dx + dy * dy).sqrt();

                if dist >= s.config.mouse_move_threshold {
                    s.last_mouse = (x, y);
                    let _ = s.tx.try_send(Event {
                        t,
                        data: EventData::Move {
                            x: x as i32,
                            y: y as i32,
                        },
                    });
                }
            }
            EventType::Wheel { delta_x, delta_y } => {
                let (x, y) = s.last_mouse;
                let _ = s.tx.try_send(Event {
                    t,
                    data: EventData::Scroll {
                        x: x as i32,
                        y: y as i32,
                        dx: delta_x as i16,
                        dy: delta_y as i16,
                    },
                });
            }
            EventType::KeyPress(key) => {
                let keycode = key_to_code(&key);

                // Check for Ctrl+C/X/V
                // For now, just record key events
                let _ = s.tx.try_send(Event {
                    t,
                    data: EventData::Key { k: keycode, m: 0 },
                });

                // Try to get character for text aggregation
                if let Some(c) = key_to_char(&key) {
                    s.text_buf.push(c);
                    s.last_text_time = Some(Instant::now());
                }
            }
            _ => {}
        }

        // Check text buffer timeout
        if let Some(last_time) = s.last_text_time {
            if last_time.elapsed().as_millis() as u64 >= s.config.text_timeout_ms && !s.text_buf.is_empty() {
                let text = std::mem::take(&mut s.text_buf);
                let _ = s.tx.try_send(Event {
                    t,
                    data: EventData::Text { s: text },
                });
                s.last_text_time = None;
            }
        }
    };

    // rdev::listen blocks forever, but we need to respect the stop flag
    // Unfortunately rdev doesn't have a clean way to stop
    // We'll just let the thread run until the process exits
    if let Err(e) = listen(callback) {
        eprintln!("rdev listen error: {:?}", e);
    }
}

fn key_to_code(key: &rdev::Key) -> u16 {
    use rdev::Key;
    match key {
        Key::Alt => 0x12,
        Key::AltGr => 0x12,
        Key::Backspace => 0x08,
        Key::CapsLock => 0x14,
        Key::ControlLeft | Key::ControlRight => 0x11,
        Key::Delete => 0x2E,
        Key::DownArrow => 0x28,
        Key::End => 0x23,
        Key::Escape => 0x1B,
        Key::F1 => 0x70,
        Key::F2 => 0x71,
        Key::F3 => 0x72,
        Key::F4 => 0x73,
        Key::F5 => 0x74,
        Key::F6 => 0x75,
        Key::F7 => 0x76,
        Key::F8 => 0x77,
        Key::F9 => 0x78,
        Key::F10 => 0x79,
        Key::F11 => 0x7A,
        Key::F12 => 0x7B,
        Key::Home => 0x24,
        Key::LeftArrow => 0x25,
        Key::MetaLeft | Key::MetaRight => 0x5B,
        Key::PageDown => 0x22,
        Key::PageUp => 0x21,
        Key::Return => 0x0D,
        Key::RightArrow => 0x27,
        Key::ShiftLeft | Key::ShiftRight => 0x10,
        Key::Space => 0x20,
        Key::Tab => 0x09,
        Key::UpArrow => 0x26,
        Key::Num0 => 0x30,
        Key::Num1 => 0x31,
        Key::Num2 => 0x32,
        Key::Num3 => 0x33,
        Key::Num4 => 0x34,
        Key::Num5 => 0x35,
        Key::Num6 => 0x36,
        Key::Num7 => 0x37,
        Key::Num8 => 0x38,
        Key::Num9 => 0x39,
        Key::KeyA => 0x41,
        Key::KeyB => 0x42,
        Key::KeyC => 0x43,
        Key::KeyD => 0x44,
        Key::KeyE => 0x45,
        Key::KeyF => 0x46,
        Key::KeyG => 0x47,
        Key::KeyH => 0x48,
        Key::KeyI => 0x49,
        Key::KeyJ => 0x4A,
        Key::KeyK => 0x4B,
        Key::KeyL => 0x4C,
        Key::KeyM => 0x4D,
        Key::KeyN => 0x4E,
        Key::KeyO => 0x4F,
        Key::KeyP => 0x50,
        Key::KeyQ => 0x51,
        Key::KeyR => 0x52,
        Key::KeyS => 0x53,
        Key::KeyT => 0x54,
        Key::KeyU => 0x55,
        Key::KeyV => 0x56,
        Key::KeyW => 0x57,
        Key::KeyX => 0x58,
        Key::KeyY => 0x59,
        Key::KeyZ => 0x5A,
        _ => 0,
    }
}

fn key_to_char(key: &rdev::Key) -> Option<char> {
    use rdev::Key;
    match key {
        Key::KeyA => Some('a'),
        Key::KeyB => Some('b'),
        Key::KeyC => Some('c'),
        Key::KeyD => Some('d'),
        Key::KeyE => Some('e'),
        Key::KeyF => Some('f'),
        Key::KeyG => Some('g'),
        Key::KeyH => Some('h'),
        Key::KeyI => Some('i'),
        Key::KeyJ => Some('j'),
        Key::KeyK => Some('k'),
        Key::KeyL => Some('l'),
        Key::KeyM => Some('m'),
        Key::KeyN => Some('n'),
        Key::KeyO => Some('o'),
        Key::KeyP => Some('p'),
        Key::KeyQ => Some('q'),
        Key::KeyR => Some('r'),
        Key::KeyS => Some('s'),
        Key::KeyT => Some('t'),
        Key::KeyU => Some('u'),
        Key::KeyV => Some('v'),
        Key::KeyW => Some('w'),
        Key::KeyX => Some('x'),
        Key::KeyY => Some('y'),
        Key::KeyZ => Some('z'),
        Key::Num0 => Some('0'),
        Key::Num1 => Some('1'),
        Key::Num2 => Some('2'),
        Key::Num3 => Some('3'),
        Key::Num4 => Some('4'),
        Key::Num5 => Some('5'),
        Key::Num6 => Some('6'),
        Key::Num7 => Some('7'),
        Key::Num8 => Some('8'),
        Key::Num9 => Some('9'),
        Key::Space => Some(' '),
        Key::Return => Some('\n'),
        Key::Tab => Some('\t'),
        _ => None,
    }
}

// ============================================================================
// App Observer
// ============================================================================

fn run_app_observer(tx: Sender<Event>, stop: Arc<AtomicBool>, start: Instant) {
    let mut last_app: Option<String> = None;
    let mut last_pid: u32 = 0;
    let mut last_window: Option<String> = None;

    while !stop.load(Ordering::Relaxed) {
        if let Some((name, pid, title)) = super::get_focused_app() {
            let app_changed = last_app.as_ref() != Some(&name) || last_pid != pid;

            if app_changed {
                let _ = tx.try_send(Event {
                    t: start.elapsed().as_millis() as u64,
                    data: EventData::App { n: name.clone(), p: pid as i32 },
                });
                last_app = Some(name.clone());
                last_pid = pid;
            }

            if title != last_window || app_changed {
                let _ = tx.try_send(Event {
                    t: start.elapsed().as_millis() as u64,
                    data: EventData::Window {
                        a: name,
                        w: title.clone(),
                    },
                });
                last_window = title;
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
