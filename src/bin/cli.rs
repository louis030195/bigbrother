//! si - superintelligence CLI
//!
//! macOS desktop automation for AI agents.
//! All commands output JSON for structured parsing.

use clap::{Parser, Subcommand};
use serde::Serialize;
use superintelligence::prelude::*;
use superintelligence::input;

#[derive(Parser)]
#[command(name = "si")]
#[command(about = "superintelligence - macOS desktop automation for AI agents")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output as JSON (default: true)
    #[arg(long, default_value = "true")]
    json: bool,
}

#[derive(Subcommand)]
enum Commands {
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
        #[arg(long, default_value = "up")]
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

        /// Timeout for selector wait
        #[arg(long, default_value = "10000")]
        timeout: u64,
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

        /// Modifier: cmd, ctrl, alt, shift
        #[arg(long, default_value = "cmd")]
        modifier: String,
    },

    /// Activate (focus) an application
    Activate {
        /// Application name
        app: String,
    },
}

#[derive(Serialize)]
struct Output<T: Serialize> {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<superintelligence::error::Error>,
}

impl<T: Serialize> Output<T> {
    fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    fn err(e: superintelligence::error::Error) -> Output<()> {
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

    let result: std::result::Result<(), superintelligence::error::Error> = (|| {
        match cli.command {
            Commands::Apps => {
                let desktop = Desktop::new()?;
                let apps = desktop.apps()?;
                print_json(&Output::ok(apps));
            }

            Commands::Browser => {
                let desktop = Desktop::new()?;
                let browser = desktop.browser()?;
                print_json(&Output::ok(browser));
            }

            Commands::Tree { app, depth } => {
                let mut desktop = Desktop::new()?;
                let tree = desktop.tree(&app, depth)?;
                print_json(&Output::ok(tree));
            }

            Commands::Find {
                selector,
                app,
                timeout,
            } => {
                let desktop = Desktop::new()?;
                let desktop = match app {
                    Some(ref a) => desktop.in_app(a),
                    None => desktop,
                };
                let loc = desktop.locator(&selector)?.timeout(timeout);
                let elements = loc.find_all()?;
                let infos: Vec<_> = elements.iter().map(|e| e.info()).collect();
                print_json(&Output::ok(infos));
            }

            Commands::Click { selector, app } => {
                let desktop = Desktop::new()?;
                let desktop = match app {
                    Some(ref a) => desktop.in_app(a),
                    None => desktop,
                };
                let result = desktop.locator(&selector)?.click()?;
                print_json(&Output::ok(result));
            }

            Commands::Type {
                text,
                selector,
                app,
            } => {
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
            }

            Commands::Scroll {
                direction,
                pages,
                app,
            } => {
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
                        ))
                    }
                }
                print_json(&Output::ok(serde_json::json!({
                    "direction": direction,
                    "pages": pages
                })));
            }

            Commands::Press { key, repeat, delay } => {
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
            }

            Commands::Open { url } => {
                let desktop = Desktop::new()?;
                desktop.open_url(&url)?;
                print_json(&Output::ok(serde_json::json!({"opened": url})));
            }

            Commands::Wait {
                idle,
                selector,
                timeout,
            } => {
                let desktop = Desktop::new()?;
                if let Some(ms) = idle {
                    desktop.wait_idle(ms)?;
                    print_json(&Output::ok(serde_json::json!({"waited_ms": ms})));
                } else if let Some(sel) = selector {
                    let element = desktop.locator(&sel)?.timeout(timeout).wait()?;
                    print_json(&Output::ok(element.info()));
                } else {
                    print_json(&Output::ok(serde_json::json!({"waited_ms": 0})));
                }
            }

            Commands::Scrape { app, depth } => {
                let desktop = Desktop::new()?;
                let result = desktop.scrape(&app, depth)?;
                print_json(&Output::ok(result));
            }

            Commands::Shortcut { key, modifier } => {
                let mods: Vec<&str> = match modifier.to_lowercase().as_str() {
                    "cmd" | "command" => vec!["command"],
                    "ctrl" | "control" => vec!["control"],
                    "alt" | "option" => vec!["option"],
                    "shift" => vec!["shift"],
                    _ => vec!["command"],
                };
                input::shortcut(&key, &mods).map_err(Error::from)?;
                print_json(&Output::ok(serde_json::json!({
                    "key": key,
                    "modifier": modifier
                })));
            }

            Commands::Activate { app } => {
                let desktop = Desktop::new()?;
                desktop.activate(&app)?;
                print_json(&Output::ok(serde_json::json!({"activated": app})));
            }
        }
        Ok(())
    })();

    if let Err(e) = result {
        print_json(&Output::<()>::err(e));
        std::process::exit(1);
    }
}
