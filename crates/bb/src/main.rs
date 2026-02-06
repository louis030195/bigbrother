//! bb - BigBrother CLI
//!
//! Cross-platform desktop automation and workflow recording for AI agents.
//!
//! Currently supported: macOS
//! Coming soon: Linux, Windows

use anyhow::Result;
use clap::{Parser, Subcommand};
use serde::Serialize;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use bigbrother::prelude::*;
use bigbrother::input;
use bigbrother::error::{Error, ErrorCode};

#[derive(Parser)]
#[command(name = "bb")]
#[command(about = "BigBrother - macOS desktop automation and workflow recording")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    // === Recording Commands ===
    /// Start recording user interactions
    Record {
        /// Workflow name
        #[arg(short, long, default_value = "workflow")]
        name: String,

        /// Skip element context capture (faster)
        #[arg(long)]
        no_context: bool,

        /// Mouse move threshold in pixels
        #[arg(long, default_value = "5")]
        threshold: f64,
    },

    /// Replay a recorded workflow
    Replay {
        /// Workflow file
        file: String,

        /// Playback speed (1.0 = realtime, 2.0 = 2x)
        #[arg(short, long, default_value = "1.0")]
        speed: f64,
    },

    /// List saved workflows
    List,

    /// Show workflow info
    Show {
        /// Workflow file
        file: String,

        /// Show all events
        #[arg(long)]
        all: bool,
    },

    /// Delete a workflow
    Delete {
        /// Workflow file
        file: String,
    },

    /// Check/request permissions
    Permissions {
        /// Request if not granted
        #[arg(long)]
        request: bool,
    },

    // === Automation Commands ===
    /// List running applications
    Apps,

    /// Find a browser
    Browser,

    /// Get accessibility tree for an app
    Tree {
        /// Application name
        #[arg(long)]
        app: String,

        /// Maximum tree depth
        #[arg(long, default_value = "15")]
        depth: usize,
    },

    /// Find elements matching selector
    Find {
        /// Selector (e.g., "role:Button AND name:Submit")
        selector: String,

        /// Application name
        #[arg(long)]
        app: Option<String>,

        /// Timeout in milliseconds
        #[arg(long, default_value = "5000")]
        timeout: u64,
    },

    /// Click an element
    Click {
        /// Selector or index (e.g., "role:Button" or "index:42")
        selector: String,

        /// Application name
        #[arg(long)]
        app: Option<String>,
    },

    /// Type text (optionally into a specific element)
    Type {
        /// Text to type
        text: String,

        /// Selector to focus first
        #[arg(long)]
        selector: Option<String>,

        /// Application name
        #[arg(long)]
        app: Option<String>,
    },

    /// Scroll up or down
    Scroll {
        /// Direction: up or down
        #[arg(long, default_value = "down")]
        direction: String,

        /// Number of pages
        #[arg(long, default_value = "1")]
        pages: u32,

        /// Application to activate first
        #[arg(long)]
        app: Option<String>,
    },

    /// Press a key
    Press {
        /// Key name (PageUp, PageDown, Return, Tab, Escape, etc.)
        key: String,

        /// Repeat count
        #[arg(long, default_value = "1")]
        repeat: u32,

        /// Delay between presses in ms
        #[arg(long, default_value = "100")]
        delay: u64,
    },

    /// Open a URL
    Open {
        /// URL to open
        url: String,
    },

    /// Wait for idle or element
    Wait {
        /// Milliseconds to wait
        #[arg(long)]
        idle: Option<u64>,

        /// Selector to wait for
        #[arg(long)]
        selector: Option<String>,

        /// Application name
        #[arg(long)]
        app: Option<String>,

        /// Timeout for selector wait
        #[arg(long, default_value = "10000")]
        timeout: u64,
    },

    /// Take a screenshot and save to file
    Screenshot {
        /// Output file path (png)
        #[arg(short, long, default_value = "screenshot.png")]
        output: String,
    },

    /// Scrape text from an app
    Scrape {
        /// Application name
        #[arg(long)]
        app: String,

        /// Maximum depth
        #[arg(long, default_value = "20")]
        depth: usize,
    },

    /// Keyboard shortcut (e.g., cmd+c)
    Shortcut {
        /// Key (e.g., "c" for cmd+c)
        key: String,

        /// Modifiers: cmd, ctrl, alt, shift (comma-separated for multiple)
        #[arg(long, default_value = "cmd")]
        modifiers: String,
    },

    /// Activate (focus) an application
    Activate {
        /// Application name
        app: String,
    },

    /// Click at screen coordinates
    ClickAt {
        /// X coordinate
        x: i32,

        /// Y coordinate
        y: i32,

        /// Click type: left, right, double
        #[arg(long, default_value = "left")]
        button: String,
    },

    /// Send text to an app (activate, type, press enter)
    Send {
        /// Text to send
        text: String,

        /// Application name
        #[arg(long)]
        app: String,

        /// Don't press enter after typing
        #[arg(long)]
        no_enter: bool,
    },

    /// WezTerm pane control
    Wezterm {
        #[command(subcommand)]
        action: WeztermAction,
    },
}

#[derive(Subcommand)]
enum WeztermAction {
    /// List all panes
    List,

    /// Send text to a specific pane
    Send {
        /// Pane ID
        pane_id: u32,

        /// Text to send
        text: String,

        /// Don't press enter after typing
        #[arg(long)]
        no_enter: bool,
    },

    /// Activate a pane
    Focus {
        /// Pane ID
        pane_id: u32,
    },
}

#[derive(Serialize)]
struct Output<T: Serialize> {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Error>,
}

impl<T: Serialize> Output<T> {
    fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    fn err(e: Error) -> Output<()> {
        Output {
            success: false,
            data: None,
            error: Some(e),
        }
    }
}

fn print_json<T: Serialize>(output: &T) {
    println!("{}", serde_json::to_string_pretty(output).unwrap());
}

fn key_name_to_code(name: &str) -> Option<u8> {
    match name.to_lowercase().as_str() {
        "pageup" | "page_up" => Some(input::key_codes::PAGE_UP),
        "pagedown" | "page_down" => Some(input::key_codes::PAGE_DOWN),
        "return" | "enter" => Some(input::key_codes::RETURN),
        "tab" => Some(input::key_codes::TAB),
        "escape" | "esc" => Some(input::key_codes::ESCAPE),
        "space" => Some(input::key_codes::SPACE),
        "delete" | "backspace" => Some(input::key_codes::DELETE),
        "up" | "arrow_up" => Some(input::key_codes::ARROW_UP),
        "down" | "arrow_down" => Some(input::key_codes::ARROW_DOWN),
        "left" | "arrow_left" => Some(input::key_codes::ARROW_LEFT),
        "right" | "arrow_right" => Some(input::key_codes::ARROW_RIGHT),
        "home" => Some(input::key_codes::HOME),
        "end" => Some(input::key_codes::END),
        _ => None,
    }
}

fn main() {
    let cli = Cli::parse();

    let result: Result<(), anyhow::Error> = match cli.command {
        // === Recording Commands ===
        Commands::Record { name, no_context, threshold } => {
            record(&name, !no_context, threshold)
        }
        Commands::Replay { file, speed } => {
            replay(&file, speed)
        }
        Commands::List => {
            list()
        }
        Commands::Show { file, all } => {
            show(&file, all)
        }
        Commands::Delete { file } => {
            delete(&file)
        }
        Commands::Permissions { request } => {
            permissions(request)
        }

        // === Automation Commands ===
        Commands::Apps => {
            run_automation(|| {
                let desktop = Desktop::new()?;
                let apps = desktop.apps()?;
                print_json(&Output::ok(apps));
                Ok(())
            })
        }
        Commands::Browser => {
            run_automation(|| {
                let desktop = Desktop::new()?;
                let browser = desktop.browser()?;
                print_json(&Output::ok(browser));
                Ok(())
            })
        }
        Commands::Tree { app, depth } => {
            run_automation(|| {
                let mut desktop = Desktop::new()?;
                let tree = desktop.tree(&app, depth)?;
                print_json(&Output::ok(tree));
                Ok(())
            })
        }
        Commands::Find { selector, app, timeout } => {
            run_automation(|| {
                let desktop = Desktop::new()?;
                let desktop = match app {
                    Some(ref a) => desktop.in_app(a),
                    None => desktop,
                };
                let loc = desktop.locator(&selector)?.timeout(timeout);
                let elements = loc.find_all()?;
                let infos: Vec<_> = elements.iter().map(|e| e.info()).collect();
                print_json(&Output::ok(infos));
                Ok(())
            })
        }
        Commands::Click { selector, app } => {
            run_automation(|| {
                let desktop = Desktop::new()?;
                let desktop = match app {
                    Some(ref a) => desktop.in_app(a),
                    None => desktop,
                };
                let result = desktop.locator(&selector)?.click()?;
                print_json(&Output::ok(result));
                Ok(())
            })
        }
        Commands::Type { text, selector, app } => {
            run_automation(|| {
                let desktop = Desktop::new()?;
                if let Some(sel) = selector {
                    let desktop = match app {
                        Some(ref a) => desktop.in_app(a),
                        None => desktop,
                    };
                    let result = desktop.locator(&sel)?.type_text(&text)?;
                    print_json(&Output::ok(result));
                } else {
                    desktop.type_text(&text)?;
                    print_json(&Output::ok(serde_json::json!({"typed": text})));
                }
                Ok(())
            })
        }
        Commands::Scroll { direction, pages, app } => {
            run_automation(|| {
                let desktop = Desktop::new()?;
                if let Some(ref a) = app {
                    desktop.activate(a)?;
                    desktop.wait_idle(300)?;
                }
                match direction.to_lowercase().as_str() {
                    "up" => desktop.scroll_up(pages)?,
                    "down" => desktop.scroll_down(pages)?,
                    _ => {
                        return Err(Error::new(
                            ErrorCode::Unknown,
                            format!("Unknown direction: {}", direction),
                        ).into())
                    }
                }
                print_json(&Output::ok(serde_json::json!({
                    "direction": direction,
                    "pages": pages
                })));
                Ok(())
            })
        }
        Commands::Press { key, repeat, delay } => {
            run_automation(|| {
                let code = key_name_to_code(&key).ok_or_else(|| {
                    Error::new(ErrorCode::Unknown, format!("Unknown key: {}", key))
                })?;
                for i in 0..repeat {
                    input::press_key(code).map_err(Error::from)?;
                    if i < repeat - 1 {
                        std::thread::sleep(std::time::Duration::from_millis(delay));
                    }
                }
                print_json(&Output::ok(serde_json::json!({
                    "key": key,
                    "repeat": repeat
                })));
                Ok(())
            })
        }
        Commands::Open { url } => {
            run_automation(|| {
                let desktop = Desktop::new()?;
                desktop.open_url(&url)?;
                print_json(&Output::ok(serde_json::json!({"opened": url})));
                Ok(())
            })
        }
        Commands::Wait { idle, selector, app, timeout } => {
            run_automation(|| {
                let desktop = Desktop::new()?;
                let desktop = match app {
                    Some(ref a) => desktop.in_app(a),
                    None => desktop,
                };
                if let Some(ms) = idle {
                    desktop.wait_idle(ms)?;
                    print_json(&Output::ok(serde_json::json!({"waited_ms": ms})));
                } else if let Some(sel) = selector {
                    let element = desktop.locator(&sel)?.timeout(timeout).wait()?;
                    print_json(&Output::ok(element.info()));
                } else {
                    print_json(&Output::ok(serde_json::json!({"waited_ms": 0})));
                }
                Ok(())
            })
        }
        Commands::Screenshot { output } => {
            run_automation(|| {
                use std::process::Command as Cmd;
                let status = Cmd::new("screencapture")
                    .args(["-x", &output])
                    .status()
                    .map_err(|e| anyhow::anyhow!("screencapture failed: {}", e))?;
                if !status.success() {
                    anyhow::bail!("screencapture exited with error");
                }
                print_json(&Output::ok(serde_json::json!({"path": output})));
                Ok(())
            })
        }
        Commands::Scrape { app, depth } => {
            run_automation(|| {
                let desktop = Desktop::new()?;
                let result = desktop.scrape(&app, depth)?;
                print_json(&Output::ok(result));
                Ok(())
            })
        }
        Commands::Shortcut { key, modifiers } => {
            run_automation(|| {
                let mods: Vec<&str> = modifiers
                    .split(',')
                    .map(|m| match m.trim().to_lowercase().as_str() {
                        "cmd" | "command" => "command",
                        "ctrl" | "control" => "control",
                        "alt" | "option" => "option",
                        "shift" => "shift",
                        _ => "command",
                    })
                    .collect();
                input::shortcut(&key, &mods).map_err(Error::from)?;
                print_json(&Output::ok(serde_json::json!({
                    "key": key,
                    "modifiers": modifiers
                })));
                Ok(())
            })
        }
        Commands::Activate { app } => {
            run_automation(|| {
                let desktop = Desktop::new()?;
                desktop.activate(&app)?;
                print_json(&Output::ok(serde_json::json!({"activated": app})));
                Ok(())
            })
        }
        Commands::ClickAt { x, y, button } => {
            run_automation(|| {
                input::click_at(x, y, &button).map_err(Error::from)?;
                print_json(&Output::ok(serde_json::json!({
                    "clicked": {"x": x, "y": y, "button": button}
                })));
                Ok(())
            })
        }
        Commands::Send { text, app, no_enter } => {
            run_automation(|| {
                let desktop = Desktop::new()?;
                desktop.activate(&app)?;
                std::thread::sleep(std::time::Duration::from_millis(300));
                desktop.type_text(&text)?;
                if !no_enter {
                    input::press_key(input::key_codes::RETURN).map_err(Error::from)?;
                }
                print_json(&Output::ok(serde_json::json!({
                    "sent": text,
                    "app": app,
                    "enter": !no_enter
                })));
                Ok(())
            })
        }
        Commands::Wezterm { action } => {
            match action {
                WeztermAction::List => {
                    let output = std::process::Command::new("/Applications/WezTerm.app/Contents/MacOS/wezterm")
                        .args(["cli", "list", "--format", "json"])
                        .output();
                    match output {
                        Ok(out) => {
                            let json: serde_json::Value = serde_json::from_slice(&out.stdout)
                                .unwrap_or(serde_json::json!({"raw": String::from_utf8_lossy(&out.stdout)}));
                            print_json(&Output::ok(json));
                        }
                        Err(e) => {
                            print_json(&Output::<()>::err(Error::new(ErrorCode::Unknown, format!("Failed to run wezterm cli: {}", e))));
                        }
                    }
                    Ok(())
                }
                WeztermAction::Send { pane_id, text, no_enter } => {
                    run_automation(|| {
                        // Activate pane
                        std::process::Command::new("/Applications/WezTerm.app/Contents/MacOS/wezterm")
                            .args(["cli", "activate-pane", "--pane-id", &pane_id.to_string()])
                            .output()
                            .map_err(|e| Error::new(ErrorCode::Unknown, format!("Failed to activate pane: {}", e)))?;
                        
                        std::thread::sleep(std::time::Duration::from_millis(300));
                        
                        // Type text
                        let desktop = Desktop::new()?;
                        desktop.type_text(&text)?;
                        
                        if !no_enter {
                            input::press_key(input::key_codes::RETURN).map_err(Error::from)?;
                        }
                        
                        print_json(&Output::ok(serde_json::json!({
                            "pane_id": pane_id,
                            "sent": text,
                            "enter": !no_enter
                        })));
                        Ok(())
                    })
                }
                WeztermAction::Focus { pane_id } => {
                    let output = std::process::Command::new("/Applications/WezTerm.app/Contents/MacOS/wezterm")
                        .args(["cli", "activate-pane", "--pane-id", &pane_id.to_string()])
                        .output();
                    match output {
                        Ok(_) => {
                            print_json(&Output::ok(serde_json::json!({"focused": pane_id})));
                        }
                        Err(e) => {
                            print_json(&Output::<()>::err(Error::new(ErrorCode::Unknown, format!("Failed to focus pane: {}", e))));
                        }
                    }
                    Ok(())
                }
            }
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run_automation<F>(f: F) -> Result<(), anyhow::Error>
where
    F: FnOnce() -> Result<(), anyhow::Error>,
{
    match f() {
        Ok(()) => Ok(()),
        Err(e) => {
            if let Some(err) = e.downcast_ref::<Error>() {
                print_json(&Output::<()>::err(err.clone()));
            }
            Err(e)
        }
    }
}

// === Recording Functions ===

fn record(name: &str, capture_context: bool, threshold: f64) -> Result<()> {
    let config = RecorderConfig {
        capture_context,
        mouse_move_threshold: threshold,
        ..Default::default()
    };

    let recorder = WorkflowRecorder::with_config(config);

    let perms = recorder.check_permissions();
    if !perms.accessibility {
        eprintln!("Accessibility permission required.");
        recorder.request_permissions();
        return Ok(());
    }
    if !perms.input_monitoring {
        eprintln!("Input Monitoring permission required.");
        recorder.request_permissions();
        return Ok(());
    }

    println!("Recording: {} (Ctrl+C to stop)", name);

    let (mut workflow, handle) = recorder.start(name)?;

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })?;

    let mut count = 0;
    while running.load(Ordering::SeqCst) && handle.is_running() {
        handle.drain(&mut workflow);
        if workflow.events.len() != count {
            count = workflow.events.len();
            print!("\r{} events", count);
            io::stdout().flush()?;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    handle.stop(&mut workflow);
    println!("\n{} events recorded", workflow.events.len());

    let storage = WorkflowStorage::new()?;
    let path = storage.save(&workflow)?;
    println!("Saved: {}", path.display());

    Ok(())
}

fn replay(file: &str, speed: f64) -> Result<()> {
    let storage = WorkflowStorage::new()?;
    let workflow = storage.load(file)?;

    println!("Replaying {} ({} events) at {}x speed...", workflow.name, workflow.events.len(), speed);
    println!("Starting in 2 seconds...");
    std::thread::sleep(std::time::Duration::from_secs(2));

    let replayer = Replayer::new().speed(speed);
    let stats = replayer.play(&workflow)?;

    println!("Done! {} clicks, {} keys, {} chars typed", stats.clicks, stats.keys, stats.text_chars);

    Ok(())
}

fn list() -> Result<()> {
    let storage = WorkflowStorage::new()?;
    let files = storage.list()?;

    if files.is_empty() {
        println!("No workflows saved.");
    } else {
        for f in files {
            println!("{}", f);
        }
    }

    Ok(())
}

fn show(file: &str, all: bool) -> Result<()> {
    let storage = WorkflowStorage::new()?;
    let workflow = storage.load(file)?;

    println!("Name: {}", workflow.name);
    println!("Events: {}", workflow.events.len());

    let mut clicks = 0;
    let mut moves = 0;
    let mut scrolls = 0;
    let mut keys = 0;
    let mut text = 0;
    let mut apps = 0;
    let mut windows = 0;
    let mut pastes = 0;

    for e in &workflow.events {
        match &e.data {
            bigbrother::EventData::Click { .. } => clicks += 1,
            bigbrother::EventData::Move { .. } => moves += 1,
            bigbrother::EventData::Scroll { .. } => scrolls += 1,
            bigbrother::EventData::Key { .. } => keys += 1,
            bigbrother::EventData::Text { .. } => text += 1,
            bigbrother::EventData::App { .. } => apps += 1,
            bigbrother::EventData::Window { .. } => windows += 1,
            bigbrother::EventData::Paste { .. } => pastes += 1,
            bigbrother::EventData::Context { .. } => {}
        }
    }

    println!("\nSummary:");
    println!("  Clicks: {}", clicks);
    println!("  Moves: {}", moves);
    println!("  Scrolls: {}", scrolls);
    println!("  Keys: {}", keys);
    println!("  Text inputs: {}", text);
    println!("  App switches: {}", apps);
    println!("  Window switches: {}", windows);
    println!("  Clipboard: {}", pastes);

    if all {
        println!("\nEvents:");
        for (i, e) in workflow.events.iter().enumerate() {
            println!("{}: {:?}", i, e);
        }
    }

    Ok(())
}

fn delete(file: &str) -> Result<()> {
    let storage = WorkflowStorage::new()?;
    storage.delete(file)?;
    println!("Deleted: {}", file);
    Ok(())
}

fn permissions(request: bool) -> Result<()> {
    let recorder = WorkflowRecorder::new();
    let perms = if request {
        recorder.request_permissions()
    } else {
        recorder.check_permissions()
    };

    println!("Accessibility: {}", if perms.accessibility { "OK" } else { "DENIED" });
    println!("Input Monitoring: {}", if perms.input_monitoring { "OK" } else { "DENIED" });

    if !perms.all_granted() && !request {
        println!("\nRun with --request to request permissions");
    }

    Ok(())
}
