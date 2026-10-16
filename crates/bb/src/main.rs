// screenpipe — AI that knows everything you've seen, said, and heard
// https://screenpipe.com
//! bb - BigBrother CLI
//!
//! Cross-platform desktop automation and workflow recording for AI agents.
//!
//! Supported: macOS, Windows

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod serve;

use anyhow::Result;
use clap::{Parser, Subcommand};
use serde::Serialize;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use bigbrother::prelude::*;
use bigbrother::error::{Error, ErrorCode};

// macOS-only imports
#[cfg(target_os = "macos")]
use bigbrother::input;

#[derive(Parser)]
#[command(name = "bb")]
#[command(about = "BigBrother - cross-platform desktop automation and workflow recording")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compact, bounded native accessibility observation
    Observe { #[arg(long)] app: String, #[arg(long, default_value = "32")] depth: usize },
    /// Stateful native observation/action protocol over newline-delimited JSON
    Serve { #[arg(long)] app: String },
    // === Recording Commands ===
    /// Start recording user interactions
    Record {
        #[arg(short, long, default_value = "workflow")]
        name: String,
        #[arg(long)]
        no_context: bool,
        #[arg(long, default_value = "5")]
        threshold: f64,
    },
    /// Replay a recorded workflow
    Replay {
        file: String,
        #[arg(short, long, default_value = "1.0")]
        speed: f64,
    },
    /// List saved workflows
    List,
    /// Show workflow info
    Show {
        file: String,
        #[arg(long)]
        all: bool,
    },
    /// Delete a workflow
    Delete {
        file: String,
    },
    /// Check/request permissions
    Permissions {
        #[arg(long)]
        request: bool,
    },

    // === Automation Commands ===
    /// List running applications
    Apps,
    /// Find a running browser window
    Browser,
    /// Browser sessions and observations (same interface as the npm wrapper)
    #[command(disable_help_flag = true)]
    Web {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Get accessibility tree for an app
    Tree {
        /// Legacy platform-specific JSON instead of a compact observation
        #[arg(long)]
        raw: bool,
        #[arg(long)]
        app: String,
        #[arg(long, default_value = "15")]
        depth: usize,
    },
    /// Find elements matching selector
    Find {
        selector: String,
        #[arg(long)]
        app: Option<String>,
        #[arg(long, default_value = "5000")]
        timeout: u64,
    },
    /// Click an element
    Click {
        selector: String,
        #[arg(long)]
        app: Option<String>,
    },
    /// Type text
    Type {
        text: String,
        #[arg(long)]
        selector: Option<String>,
        #[arg(long)]
        app: Option<String>,
    },
    /// Scroll up or down
    Scroll {
        #[arg(long, default_value = "down")]
        direction: String,
        #[arg(long, default_value = "1")]
        pages: u32,
        #[arg(long)]
        app: Option<String>,
    },
    /// Press a key
    Press {
        key: String,
        #[arg(long, default_value = "1")]
        repeat: u32,
        #[arg(long, default_value = "100")]
        delay: u64,
    },
    /// Open a URL
    Open {
        url: String,
        /// Open in background without focusing the browser window
        #[arg(long)]
        background: bool,
    },
    /// Wait for idle or element
    Wait {
        #[arg(long)]
        idle: Option<u64>,
        #[arg(long)]
        selector: Option<String>,
        #[arg(long)]
        app: Option<String>,
        #[arg(long, default_value = "10000")]
        timeout: u64,
    },
    /// Take a screenshot
    Screenshot {
        #[arg(short, long, default_value = "screenshot.png")]
        output: String,
    },
    /// Scrape text from an app
    Scrape {
        #[arg(long)]
        app: String,
        #[arg(long, default_value = "20")]
        depth: usize,
    },
    /// Keyboard shortcut
    Shortcut {
        key: String,
        #[arg(long, default_value = "cmd")]
        modifiers: String,
    },
    /// Activate (focus) an application
    Activate {
        app: String,
    },
    /// Click at screen coordinates
    ClickAt {
        x: i32,
        y: i32,
        #[arg(long, default_value = "left")]
        button: String,
    },
    /// Send text to an app
    Send {
        text: String,
        #[arg(long)]
        app: String,
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
    List,
    Send {
        pane_id: u32,
        text: String,
        #[arg(long)]
        no_enter: bool,
    },
    Focus {
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
        Self { success: true, data: Some(data), error: None }
    }
    fn err(e: Error) -> Output<()> {
        Output { success: false, data: None, error: Some(e) }
    }
}

fn print_json<T: Serialize>(output: &T) {
    println!("{}", serde_json::to_string_pretty(output).unwrap());
}

// ── macOS key code mapping ──────────────────────────────────────────────────

#[cfg(target_os = "macos")]
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

// ── Windows key code mapping ────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn key_name_to_vk(name: &str) -> Option<u16> {
    match name.to_lowercase().as_str() {
        "pageup" | "page_up" => Some(vk::PAGE_UP),
        "pagedown" | "page_down" => Some(vk::PAGE_DOWN),
        "return" | "enter" => Some(vk::RETURN),
        "tab" => Some(vk::TAB),
        "escape" | "esc" => Some(vk::ESCAPE),
        "space" => Some(vk::SPACE),
        "delete" | "backspace" => Some(vk::BACKSPACE),
        "up" | "arrow_up" => Some(vk::UP),
        "down" | "arrow_down" => Some(vk::DOWN),
        "left" | "arrow_left" => Some(vk::LEFT),
        "right" | "arrow_right" => Some(vk::RIGHT),
        "home" => Some(vk::HOME),
        "end" => Some(vk::END),
        "f1" => Some(vk::F1),
        "f4" => Some(0x73), // VK_F4
        "f12" => Some(vk::F12),
        // Single letter keys
        k if k.len() == 1 => {
            let c = k.chars().next().unwrap().to_ascii_uppercase();
            if c.is_ascii_alphabetic() {
                Some(c as u16)
            } else if c.is_ascii_digit() {
                Some(c as u16)
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(target_os = "windows")]
fn modifier_name_to_vk(name: &str) -> u16 {
    match name.trim().to_lowercase().as_str() {
        "ctrl" | "control" => vk::CONTROL,
        "alt" | "option" | "menu" => vk::ALT,
        "shift" => vk::SHIFT,
        "win" | "super" | "cmd" | "command" => vk::LWIN,
        _ => vk::CONTROL,
    }
}

// ── Windows element helpers ─────────────────────────────────────────────────

#[cfg(target_os = "windows")]
#[derive(Serialize)]
struct ElementInfo {
    name: Option<String>,
    control_type: &'static str,
    control_type_id: i32,
    bounds: Option<(i32, i32, i32, i32)>,
    is_enabled: bool,
    class_name: Option<String>,
    automation_id: Option<String>,
}

#[cfg(target_os = "windows")]
impl ElementInfo {
    fn from_element(el: &Element) -> Self {
        Self {
            name: el.name(),
            control_type: el.control_type_name(),
            control_type_id: el.control_type(),
            bounds: el.bounds(),
            is_enabled: el.is_enabled(),
            class_name: el.class_name(),
            automation_id: el.automation_id(),
        }
    }
}

#[cfg(target_os = "windows")]
#[derive(Serialize)]
struct TreeNode {
    name: Option<String>,
    role: &'static str,
    children: Vec<TreeNode>,
}

#[cfg(target_os = "windows")]
fn build_tree(walker: &TreeWalker, element: &Element, depth: usize, max_depth: usize) -> TreeNode {
    let mut children = Vec::new();
    if depth < max_depth {
        let mut child = walker.first_child(element);
        while let Some(c) = child {
            children.push(build_tree(walker, &c, depth + 1, max_depth));
            child = walker.next_sibling(&c);
        }
    }
    TreeNode {
        name: element.name(),
        role: element.control_type_name(),
        children,
    }
}

#[cfg(target_os = "windows")]
fn collect_text(walker: &TreeWalker, element: &Element, depth: usize, max_depth: usize, items: &mut Vec<serde_json::Value>) {
    if let Some(name) = element.name() {
        if !name.is_empty() {
            items.push(serde_json::json!({
                "text": name,
                "role": element.control_type_name(),
            }));
        }
    }
    if depth < max_depth {
        let mut child = walker.first_child(element);
        while let Some(c) = child {
            collect_text(walker, &c, depth + 1, max_depth, items);
            child = walker.next_sibling(&c);
        }
    }
}

/// Parse a selector string like "role:Button AND name~:Submit"
/// Returns matching elements from the tree
#[cfg(target_os = "windows")]
fn find_elements_matching(
    walker: &TreeWalker,
    element: &Element,
    selector: &str,
    max_depth: usize,
    results: &mut Vec<ElementInfo>,
    depth: usize,
) {
    if depth > max_depth { return; }

    if matches_selector(element, selector) {
        results.push(ElementInfo::from_element(element));
    }

    let mut child = walker.first_child(element);
    while let Some(c) = child {
        find_elements_matching(walker, &c, selector, max_depth, results, depth + 1);
        child = walker.next_sibling(&c);
    }
}

#[cfg(target_os = "windows")]
fn matches_selector(element: &Element, selector: &str) -> bool {
    // Parse AND-separated conditions
    let conditions: Vec<&str> = selector.split(" AND ").collect();

    for cond in conditions {
        let cond = cond.trim();

        if cond.starts_with("role:") {
            let expected_role = &cond[5..];
            if !element.control_type_name().eq_ignore_ascii_case(expected_role) {
                return false;
            }
        } else if cond.starts_with("name~:") || cond.starts_with("title~:") {
            // Partial match
            let needle = if cond.starts_with("name~:") { &cond[6..] } else { &cond[7..] };
            let name = element.name().unwrap_or_default();
            if !name.to_lowercase().contains(&needle.to_lowercase()) {
                return false;
            }
        } else if cond.starts_with("name:") || cond.starts_with("title:") {
            // Exact/substring match
            let needle = if cond.starts_with("name:") { &cond[5..] } else { &cond[6..] };
            let name = element.name().unwrap_or_default();
            if !name.to_lowercase().contains(&needle.to_lowercase()) {
                return false;
            }
        }
    }

    true
}

#[cfg(target_os = "windows")]
fn find_app_window(app_name: &str) -> Result<Element> {
    find_window(app_name)?
        .ok_or_else(|| anyhow::anyhow!("Window not found: {}", app_name))
}

// ── Main ────────────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();

    let result: Result<(), anyhow::Error> = match cli.command {
        Commands::Observe { app, depth } => serve::observe(&app, depth),
        Commands::Serve { app } => serve::serve(&app),
        Commands::Record { name, no_context, threshold } => record(&name, !no_context, threshold),
        Commands::Replay { file, speed } => replay(&file, speed),
        Commands::List => list(),
        Commands::Show { file, all } => show(&file, all),
        Commands::Delete { file } => delete(&file),
        Commands::Permissions { request } => permissions(request),

        // ── Automation (platform-dispatched) ──────────────────────────────
        Commands::Apps => run_automation(cmd_apps),
        Commands::Browser => run_automation(cmd_browser),
        Commands::Tree { app, depth, raw } => if raw { run_automation(move || cmd_tree(&app, depth)) } else { serve::observe(&app, depth) },
        Commands::Find { selector, app, timeout } => run_automation(move || cmd_find(&selector, app.as_deref(), timeout)),
        Commands::Click { selector, app } => run_automation(move || cmd_click(&selector, app.as_deref())),
        Commands::Type { text, selector, app } => run_automation(move || cmd_type(&text, selector.as_deref(), app.as_deref())),
        Commands::Scroll { direction, pages, app } => run_automation(move || cmd_scroll(&direction, pages, app.as_deref())),
        Commands::Press { key, repeat, delay } => run_automation(move || cmd_press(&key, repeat, delay)),
        Commands::Open { url, background } => run_automation(move || cmd_open(&url, background)),
        Commands::Wait { idle, selector, app, timeout } => run_automation(move || cmd_wait(idle, selector.as_deref(), app.as_deref(), timeout)),
        Commands::Screenshot { output } => run_automation(move || cmd_screenshot(&output)),
        Commands::Scrape { app, depth } => run_automation(move || cmd_scrape(&app, depth)),
        Commands::Shortcut { key, modifiers } => run_automation(move || cmd_shortcut(&key, &modifiers)),
        Commands::Activate { app } => run_automation(move || cmd_activate(&app)),
        Commands::ClickAt { x, y, button } => run_automation(move || cmd_click_at(x, y, &button)),
        Commands::Send { text, app, no_enter } => run_automation(move || cmd_send(&text, &app, no_enter)),
        Commands::Web { args } => cmd_web(args),
        Commands::Wezterm { action } => cmd_wezterm(action),
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run_automation<F>(f: F) -> Result<(), anyhow::Error>
where F: FnOnce() -> Result<(), anyhow::Error> {
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

// ══════════════════════════════════════════════════════════════════════════════
//  macOS automation commands
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(target_os = "macos")]
fn cmd_apps() -> Result<()> {
    let desktop = Desktop::new()?;
    let apps = desktop.apps()?;
    print_json(&Output::ok(apps));
    Ok(())
}

#[cfg(target_os = "macos")]
fn cmd_browser() -> Result<()> {
    let desktop = Desktop::new()?;
    let browser = desktop.browser()?;
    print_json(&Output::ok(browser));
    Ok(())
}

#[cfg(target_os = "macos")]
fn cmd_tree(app: &str, depth: usize) -> Result<()> {
    let mut desktop = Desktop::new()?;
    let tree = desktop.tree(app, depth)?;
    print_json(&Output::ok(tree));
    Ok(())
}

#[cfg(target_os = "macos")]
fn cmd_find(selector: &str, app: Option<&str>, timeout: u64) -> Result<()> {
    let desktop = Desktop::new()?;
    let desktop = match app {
        Some(a) => desktop.in_app(a),
        None => desktop,
    };
    let loc = desktop.locator(selector)?.timeout(timeout);
    let elements = loc.find_all()?;
    let infos: Vec<_> = elements.iter().map(|e| e.info()).collect();
    print_json(&Output::ok(infos));
    Ok(())
}

#[cfg(target_os = "macos")]
fn cmd_click(selector: &str, app: Option<&str>) -> Result<()> {
    let desktop = Desktop::new()?;
    let desktop = match app {
        Some(a) => desktop.in_app(a),
        None => desktop,
    };
    let result = desktop.locator(selector)?.click()?;
    print_json(&Output::ok(result));
    Ok(())
}

#[cfg(target_os = "macos")]
fn cmd_type(text: &str, selector: Option<&str>, app: Option<&str>) -> Result<()> {
    let desktop = Desktop::new()?;
    if let Some(sel) = selector {
        let desktop = match app {
            Some(a) => desktop.in_app(a),
            None => desktop,
        };
        let result = desktop.locator(sel)?.type_text(text)?;
        print_json(&Output::ok(result));
    } else {
        desktop.type_text(text)?;
        print_json(&Output::ok(serde_json::json!({"typed": text})));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn cmd_scroll(direction: &str, pages: u32, app: Option<&str>) -> Result<()> {
    let desktop = Desktop::new()?;
    if let Some(a) = app {
        desktop.activate(a)?;
        desktop.wait_idle(300)?;
    }
    match direction.to_lowercase().as_str() {
        "up" => desktop.scroll_up(pages)?,
        "down" => desktop.scroll_down(pages)?,
        _ => return Err(Error::new(ErrorCode::Unknown, format!("Unknown direction: {}", direction)).into()),
    }
    print_json(&Output::ok(serde_json::json!({"direction": direction, "pages": pages})));
    Ok(())
}

#[cfg(target_os = "macos")]
fn cmd_press(key: &str, repeat: u32, delay: u64) -> Result<()> {
    let code = key_name_to_code(key).ok_or_else(|| Error::new(ErrorCode::Unknown, format!("Unknown key: {}", key)))?;
    for i in 0..repeat {
        input::press_key(code).map_err(Error::from)?;
        if i < repeat - 1 {
            std::thread::sleep(std::time::Duration::from_millis(delay));
        }
    }
    print_json(&Output::ok(serde_json::json!({"key": key, "repeat": repeat})));
    Ok(())
}

#[cfg(target_os = "macos")]
fn cmd_open(url: &str, background: bool) -> Result<()> {
    if background {
        // Save the currently focused app, open URL, then re-focus the original app
        let save_focus = std::process::Command::new("osascript")
            .args(["-e", r#"tell application "System Events" to get name of first application process whose frontmost is true"#])
            .output()
            .ok()
            .and_then(|o| if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            });

        std::process::Command::new("open")
            .arg("-g")
            .arg(url)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to open URL: {}", e))?;

        // Re-focus the original app so user sees no change
        if let Some(app) = save_focus {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let _ = std::process::Command::new("osascript")
                .args(["-e", &format!(r#"tell application "{}" to activate"#, app)])
                .output();
        }
    } else {
        let desktop = Desktop::new()?;
        desktop.open_url(url)?;
    }
    print_json(&Output::ok(serde_json::json!({"opened": url, "background": background})));
    Ok(())
}

#[cfg(target_os = "macos")]
fn cmd_wait(idle: Option<u64>, selector: Option<&str>, app: Option<&str>, timeout: u64) -> Result<()> {
    let desktop = Desktop::new()?;
    let desktop = match app {
        Some(a) => desktop.in_app(a),
        None => desktop,
    };
    if let Some(ms) = idle {
        desktop.wait_idle(ms)?;
        print_json(&Output::ok(serde_json::json!({"waited_ms": ms})));
    } else if let Some(sel) = selector {
        let element = desktop.locator(sel)?.timeout(timeout).wait()?;
        print_json(&Output::ok(element.info()));
    } else {
        print_json(&Output::ok(serde_json::json!({"waited_ms": 0})));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn cmd_screenshot(output: &str) -> Result<()> {
    let status = std::process::Command::new("screencapture")
        .args(["-x", output])
        .status()?;
    if !status.success() { anyhow::bail!("screencapture failed"); }
    print_json(&Output::ok(serde_json::json!({"path": output})));
    Ok(())
}

#[cfg(target_os = "macos")]
fn cmd_scrape(app: &str, depth: usize) -> Result<()> {
    let desktop = Desktop::new()?;
    let result = desktop.scrape(app, depth)?;
    print_json(&Output::ok(result));
    Ok(())
}

#[cfg(target_os = "macos")]
fn cmd_shortcut(key: &str, modifiers: &str) -> Result<()> {
    let mods: Vec<&str> = modifiers.split(',').map(|m| match m.trim().to_lowercase().as_str() {
        "cmd" | "command" => "command",
        "ctrl" | "control" => "control",
        "alt" | "option" => "option",
        "shift" => "shift",
        _ => "command",
    }).collect();
    input::shortcut(key, &mods).map_err(Error::from)?;
    print_json(&Output::ok(serde_json::json!({"key": key, "modifiers": modifiers})));
    Ok(())
}

#[cfg(target_os = "macos")]
fn cmd_activate(app: &str) -> Result<()> {
    let desktop = Desktop::new()?;
    desktop.activate(app)?;
    print_json(&Output::ok(serde_json::json!({"activated": app})));
    Ok(())
}

#[cfg(target_os = "macos")]
fn cmd_click_at(x: i32, y: i32, button: &str) -> Result<()> {
    input::click_at(x, y, button).map_err(Error::from)?;
    print_json(&Output::ok(serde_json::json!({"clicked": {"x": x, "y": y, "button": button}})));
    Ok(())
}

#[cfg(target_os = "macos")]
fn cmd_send(text: &str, app: &str, no_enter: bool) -> Result<()> {
    let desktop = Desktop::new()?;
    desktop.activate(app)?;
    std::thread::sleep(std::time::Duration::from_millis(300));
    desktop.type_text(text)?;
    if !no_enter {
        input::press_key(input::key_codes::RETURN).map_err(Error::from)?;
    }
    print_json(&Output::ok(serde_json::json!({"sent": text, "app": app, "enter": !no_enter})));
    Ok(())
}

// ══════════════════════════════════════════════════════════════════════════════
//  Windows automation commands
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(target_os = "windows")]
fn cmd_apps() -> Result<()> {
    let windows = get_windows()?;
    let apps: Vec<_> = windows.iter().filter_map(|w| {
        w.name().map(|n| serde_json::json!({"name": n, "pid": w.process_id()}))
    }).collect();
    print_json(&Output::ok(apps));
    Ok(())
}

#[cfg(target_os = "windows")]
fn cmd_browser() -> Result<()> {
    let browsers = ["chrome", "firefox", "msedge", "brave", "opera"];
    for b in &browsers {
        if let Ok(Some(w)) = find_window(b) {
            print_json(&Output::ok(serde_json::json!({
                "name": w.name(),
                "browser": b,
                "pid": w.process_id(),
            })));
            return Ok(());
        }
    }
    print_json(&Output::ok(serde_json::json!({"browser": serde_json::Value::Null})));
    Ok(())
}

#[cfg(target_os = "windows")]
fn cmd_tree(app: &str, depth: usize) -> Result<()> {
    let automation = Automation::new()?;
    let window = find_app_window(app)?;
    let walker = automation.tree_walker()?;
    let tree = build_tree(&walker, &window, 0, depth);
    let element_count = count_nodes(&tree);
    print_json(&Output::ok(serde_json::json!({
        "tree": tree,
        "element_count": element_count,
    })));
    Ok(())
}

#[cfg(target_os = "windows")]
fn count_nodes(node: &TreeNode) -> usize {
    1 + node.children.iter().map(count_nodes).sum::<usize>()
}

#[cfg(target_os = "windows")]
fn cmd_find(selector: &str, app: Option<&str>, _timeout: u64) -> Result<()> {
    let automation = Automation::new()?;
    let root = if let Some(a) = app {
        find_app_window(a)?
    } else {
        automation.root()?
    };
    let walker = automation.tree_walker()?;
    let mut results = Vec::new();
    find_elements_matching(&walker, &root, selector, 30, &mut results, 0);
    print_json(&Output::ok(results));
    Ok(())
}

#[cfg(target_os = "windows")]
fn cmd_click(selector: &str, app: Option<&str>) -> Result<()> {
    let automation = Automation::new()?;
    let root = if let Some(a) = app {
        find_app_window(a)?
    } else {
        automation.root()?
    };
    let walker = automation.tree_walker()?;
    let mut results = Vec::new();
    find_elements_matching(&walker, &root, selector, 30, &mut results, 0);

    if results.is_empty() {
        return Err(Error::new(ErrorCode::ElementNotFound, format!("Element not found: {}", selector)).into());
    }

    // Use the first match's clickable point or center of bounds
    let info = &results[0];
    if let Some((x, y, w, h)) = info.bounds {
        click_at(x + w / 2, y + h / 2)?;
        print_json(&Output::ok(serde_json::json!({"clicked": info.name, "at": [x + w/2, y + h/2]})));
    } else {
        return Err(Error::new(ErrorCode::ActionFailed, "Element has no bounds".to_string()).into());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn cmd_type(text: &str, _selector: Option<&str>, _app: Option<&str>) -> Result<()> {
    type_text(text)?;
    print_json(&Output::ok(serde_json::json!({"typed": text})));
    Ok(())
}

#[cfg(target_os = "windows")]
fn cmd_scroll(direction: &str, pages: u32, app: Option<&str>) -> Result<()> {
    if let Some(a) = app {
        cmd_activate(a)?;
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    let delta = match direction.to_lowercase().as_str() {
        "up" => pages as i32,
        "down" => -(pages as i32),
        _ => return Err(Error::new(ErrorCode::Unknown, format!("Unknown direction: {}", direction)).into()),
    };
    scroll(delta)?;
    print_json(&Output::ok(serde_json::json!({"direction": direction, "pages": pages})));
    Ok(())
}

#[cfg(target_os = "windows")]
fn cmd_press(key: &str, repeat: u32, delay: u64) -> Result<()> {
    let vk_code = key_name_to_vk(key).ok_or_else(|| Error::new(ErrorCode::Unknown, format!("Unknown key: {}", key)))?;
    for i in 0..repeat {
        press_key(vk_code)?;
        if i < repeat - 1 {
            std::thread::sleep(std::time::Duration::from_millis(delay));
        }
    }
    print_json(&Output::ok(serde_json::json!({"key": key, "repeat": repeat})));
    Ok(())
}

#[cfg(target_os = "windows")]
fn cmd_open(url: &str, background: bool) -> Result<()> {
    std::process::Command::new("cmd")
        .args(["/c", "start", url])
        .spawn()?;
    print_json(&Output::ok(serde_json::json!({"opened": url, "background": background})));
    Ok(())
}

#[cfg(target_os = "windows")]
fn cmd_wait(idle: Option<u64>, selector: Option<&str>, app: Option<&str>, timeout: u64) -> Result<()> {
    if let Some(ms) = idle {
        std::thread::sleep(std::time::Duration::from_millis(ms));
        print_json(&Output::ok(serde_json::json!({"waited_ms": ms})));
        return Ok(());
    }

    if let Some(sel) = selector {
        let automation = Automation::new()?;
        let start = std::time::Instant::now();
        loop {
            let root = if let Some(a) = app {
                find_app_window(a)?
            } else {
                automation.root()?
            };
            let walker = automation.tree_walker()?;
            let mut results = Vec::new();
            find_elements_matching(&walker, &root, sel, 30, &mut results, 0);

            if !results.is_empty() {
                print_json(&Output::ok(serde_json::json!({
                    "found": results.first(),
                    "waited_ms": start.elapsed().as_millis(),
                })));
                return Ok(());
            }

            if start.elapsed().as_millis() > timeout as u128 {
                return Err(Error::new(ErrorCode::Timeout, format!("Timed out waiting for: {}", sel)).into());
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }

    print_json(&Output::ok(serde_json::json!({"waited_ms": 0})));
    Ok(())
}

#[cfg(target_os = "windows")]
fn cmd_screenshot(output: &str) -> Result<()> {
    // Use PowerShell to take a screenshot on Windows
    let ps_script = format!(
        r#"Add-Type -AssemblyName System.Windows.Forms; $screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds; $bitmap = New-Object System.Drawing.Bitmap($screen.Width, $screen.Height); $graphics = [System.Drawing.Graphics]::FromImage($bitmap); $graphics.CopyFromScreen($screen.Location, [System.Drawing.Point]::Empty, $screen.Size); $bitmap.Save('{}')"#,
        output.replace('\'', "''")
    );
    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps_script])
        .status()?;
    if !status.success() {
        anyhow::bail!("screenshot capture failed");
    }
    print_json(&Output::ok(serde_json::json!({"path": output})));
    Ok(())
}

#[cfg(target_os = "windows")]
fn cmd_scrape(app: &str, depth: usize) -> Result<()> {
    let automation = Automation::new()?;
    let window = find_app_window(app)?;
    let walker = automation.tree_walker()?;
    let mut items = Vec::new();
    collect_text(&walker, &window, 0, depth, &mut items);
    print_json(&Output::ok(serde_json::json!({"items": items})));
    Ok(())
}

#[cfg(target_os = "windows")]
fn cmd_shortcut(key: &str, modifiers: &str) -> Result<()> {
    let key_vk = key_name_to_vk(key).ok_or_else(|| Error::new(ErrorCode::Unknown, format!("Unknown key: {}", key)))?;
    let mod_vks: Vec<u16> = modifiers.split(',').map(|m| modifier_name_to_vk(m)).collect();
    shortcut(key_vk, &mod_vks)?;
    print_json(&Output::ok(serde_json::json!({"key": key, "modifiers": modifiers})));
    Ok(())
}

#[cfg(target_os = "windows")]
fn cmd_activate(app: &str) -> Result<()> {
    use windows::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_RESTORE};

    let window = find_app_window(app)?;
    if let Some((x, y, _, _)) = window.bounds() {
        // Get the HWND by finding the window via UI Automation
        // Use SetForegroundWindow via element's native handle
        // Fallback: click the window center
        if let Some((cx, cy)) = window.clickable_point() {
            // Use Windows API to bring window to front
            unsafe {
                // Get the window handle from the element's process
                let pid = window.process_id() as u32;
                use windows::Win32::UI::WindowsAndMessaging::{
                    EnumWindows, GetWindowThreadProcessId, IsWindowVisible, WNDENUMPROC,
                };
                use windows::Win32::Foundation::{BOOL, HWND, LPARAM};

                // Find HWND by PID
                static mut TARGET_PID: u32 = 0;
                static mut FOUND_HWND: isize = 0;
                TARGET_PID = pid;
                FOUND_HWND = 0;

                unsafe extern "system" fn enum_callback(hwnd: HWND, _: LPARAM) -> BOOL {
                    let mut proc_pid: u32 = 0;
                    GetWindowThreadProcessId(hwnd, Some(&mut proc_pid));
                    if proc_pid == TARGET_PID && IsWindowVisible(hwnd).as_bool() {
                        FOUND_HWND = hwnd.0 as isize;
                        return BOOL(0); // stop enumerating
                    }
                    BOOL(1)
                }

                let _ = EnumWindows(Some(enum_callback), LPARAM(0));

                if FOUND_HWND != 0 {
                    let hwnd = HWND(FOUND_HWND as *mut _);
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                    let _ = SetForegroundWindow(hwnd);
                }
            }
        }
    }
    print_json(&Output::ok(serde_json::json!({"activated": app})));
    Ok(())
}

#[cfg(target_os = "windows")]
fn cmd_click_at(x: i32, y: i32, button: &str) -> Result<()> {
    move_mouse(x, y)?;
    std::thread::sleep(std::time::Duration::from_millis(10));
    match button {
        "right" => right_click()?,
        "double" => double_click()?,
        "middle" => middle_click()?,
        _ => click()?,
    }
    print_json(&Output::ok(serde_json::json!({"clicked": {"x": x, "y": y, "button": button}})));
    Ok(())
}

#[cfg(target_os = "windows")]
fn cmd_send(text: &str, app: &str, no_enter: bool) -> Result<()> {
    cmd_activate(app)?;
    std::thread::sleep(std::time::Duration::from_millis(300));
    type_text(text)?;
    if !no_enter {
        press_key(vk::RETURN)?;
    }
    print_json(&Output::ok(serde_json::json!({"sent": text, "app": app, "enter": !no_enter})));
    Ok(())
}

// ── Web (browser automation) ────────────────────────────────────────────────

fn cmd_web(args: Vec<String>) -> Result<()> {
    let browser_cli = find_browser_module().ok_or_else(|| anyhow::anyhow!(
        "Browser module not found. Run from a checkout with browser/src, install bbctl, or set BB_BROWSER_CLI."
    ))?;
    let status = std::process::Command::new("bun")
        .arg("run").arg(browser_cli).args(args)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status().map_err(|e| anyhow::anyhow!("Could not run the browser module with Bun: {e}"))?;
    if !status.success() { std::process::exit(status.code().unwrap_or(1)); }
    Ok(())
}

fn find_browser_module() -> Option<std::path::PathBuf> {
    if let Some(explicit) = std::env::var_os("BB_BROWSER_CLI") {
        let path = std::path::PathBuf::from(explicit);
        return path.is_file().then_some(path);
    }
    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors().skip(1).take(5) {
            let path = ancestor.join("browser/src/cli.ts");
            if path.is_file() { return Some(path); }
        }
    }
    let cwd = std::path::PathBuf::from("browser/src/cli.ts");
    if cwd.is_file() { return Some(cwd); }
    if let Ok(output) = std::process::Command::new("node")
        .args(["-e", "console.log(require('path').join(require('path').dirname(require.resolve('bbctl/package.json')), 'browser/cli.ts'))"])
        .output() {
        if output.status.success() {
            let path = std::path::PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
            if path.is_file() { return Some(path); }
        }
    }
    None
}

#[cfg(test)]
mod cli_tests {
    use super::*;
    #[test]
    fn web_forwards_the_browser_cli_without_a_second_command_schema() {
        let cli = Cli::try_parse_from(["bb", "web", "snap", "demo", "--full"]).unwrap();
        match cli.command { Commands::Web { args } => assert_eq!(args, ["snap", "demo", "--full"]), _ => panic!("wrong command") }
        let cli = Cli::try_parse_from(["bb", "web", "--help"]).unwrap();
        match cli.command { Commands::Web { args } => assert_eq!(args, ["--help"]), _ => panic!("wrong command") }
    }
}

// ── WezTerm (macOS-only for now) ────────────────────────────────────────────

fn cmd_wezterm(action: WeztermAction) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let wezterm = "/Applications/WezTerm.app/Contents/MacOS/wezterm";
        match action {
            WeztermAction::List => {
                let output = std::process::Command::new(wezterm)
                    .args(["cli", "list", "--format", "json"])
                    .output();
                match output {
                    Ok(out) => {
                        let json: serde_json::Value = serde_json::from_slice(&out.stdout)
                            .unwrap_or(serde_json::json!({"raw": String::from_utf8_lossy(&out.stdout)}));
                        print_json(&Output::ok(json));
                    }
                    Err(e) => print_json(&Output::<()>::err(Error::new(ErrorCode::Unknown, format!("{}", e)))),
                }
            }
            WeztermAction::Send { pane_id, text, no_enter } => {
                return run_automation(move || {
                    std::process::Command::new(wezterm)
                        .args(["cli", "activate-pane", "--pane-id", &pane_id.to_string()])
                        .output()?;
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    let desktop = Desktop::new()?;
                    desktop.type_text(&text)?;
                    if !no_enter {
                        input::press_key(input::key_codes::RETURN).map_err(Error::from)?;
                    }
                    print_json(&Output::ok(serde_json::json!({"pane_id": pane_id, "sent": text})));
                    Ok(())
                });
            }
            WeztermAction::Focus { pane_id } => {
                match std::process::Command::new(wezterm)
                    .args(["cli", "activate-pane", "--pane-id", &pane_id.to_string()])
                    .output()
                {
                    Ok(_) => print_json(&Output::ok(serde_json::json!({"focused": pane_id}))),
                    Err(e) => print_json(&Output::<()>::err(Error::new(ErrorCode::Unknown, format!("{}", e)))),
                }
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = action;
        print_json(&Output::<()>::err(Error::new(ErrorCode::Unknown, "wezterm command is macOS-only".to_string())));
    }
    Ok(())
}

// ── Recording Functions (cross-platform) ────────────────────────────────────

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
    ctrlc::set_handler(move || { r.store(false, Ordering::SeqCst); })?;
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
    if files.is_empty() { println!("No workflows saved."); } else { for f in files { println!("{}", f); } }
    Ok(())
}

fn show(file: &str, all: bool) -> Result<()> {
    let storage = WorkflowStorage::new()?;
    let workflow = storage.load(file)?;
    println!("Name: {}", workflow.name);
    println!("Events: {}", workflow.events.len());
    let (mut clicks, mut moves, mut scrolls, mut keys, mut text, mut apps, mut windows, mut pastes) = (0,0,0,0,0,0,0,0);
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
            _ => {}
        }
    }
    println!("\nSummary: {} clicks, {} moves, {} scrolls, {} keys, {} text, {} apps, {} windows, {} clipboard", clicks, moves, scrolls, keys, text, apps, windows, pastes);
    if all { for (i, e) in workflow.events.iter().enumerate() { println!("{}: {:?}", i, e); } }
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
    let perms = if request { recorder.request_permissions() } else { recorder.check_permissions() };
    println!("Accessibility: {}", if perms.accessibility { "OK" } else { "DENIED" });
    println!("Input Monitoring: {}", if perms.input_monitoring { "OK" } else { "DENIED" });
    if !perms.all_granted() && !request { println!("\nRun with --request to request permissions"); }
    Ok(())
}
