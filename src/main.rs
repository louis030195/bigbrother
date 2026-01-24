use anyhow::{Context, Result};
use chrono::Local;
use cidre::ax;
use csv::Writer;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[derive(Debug)]
struct DiscordMessage {
    timestamp: String,
    username: String,
    content: String,
}

fn open_url(url: &str) -> Result<()> {
    println!("Opening URL: {}", url);
    Command::new("open")
        .arg(url)
        .spawn()
        .context("Failed to open URL")?;
    Ok(())
}

fn find_browser_app() -> Result<cidre::arc::R<ax::UiElement>> {
    let system_wide = ax::UiElement::sys_wide();
    let app = system_wide
        .focused_app()
        .context("Failed to get focused application")?;

    let role_desc = app.role_desc().ok().map(|s| s.to_string());
    println!("Focused app: {:?}", role_desc);

    Ok(app)
}

fn get_string_attr(element: &ax::UiElement, attr: &ax::Attr) -> Option<String> {
    element
        .attr_value(attr)
        .ok()
        .and_then(|v| {
            if v.get_type_id() == cidre::cf::String::type_id() {
                let cf_str: &cidre::cf::String = unsafe { std::mem::transmute(&*v) };
                Some(cf_str.to_string())
            } else {
                None
            }
        })
}

fn scrape_messages_recursive(
    element: &ax::UiElement,
    messages: &mut Vec<DiscordMessage>,
    depth: usize,
) {
    if depth > 30 {
        return;
    }

    let role = element.role().ok().map(|r| format!("{:?}", r));
    let role_desc = element.role_desc().ok().map(|s| s.to_string());

    // Try to get value/content from the element
    if let Some(text) = get_string_attr(element, ax::attr::value()) {
        if !text.is_empty() && text.len() > 2 {
            let msg = DiscordMessage {
                timestamp: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                username: role_desc.clone().unwrap_or_else(|| "Unknown".to_string()),
                content: text,
            };
            messages.push(msg);
        }
    }

    // Also try title attribute
    if let Some(text) = get_string_attr(element, ax::attr::title()) {
        if !text.is_empty() && text.len() > 2 {
            let msg = DiscordMessage {
                timestamp: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                username: role.clone().unwrap_or_else(|| "Unknown".to_string()),
                content: text,
            };
            messages.push(msg);
        }
    }

    // Try description attribute
    if let Some(text) = get_string_attr(element, ax::attr::desc()) {
        if !text.is_empty() && text.len() > 2 {
            let msg = DiscordMessage {
                timestamp: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                username: role.clone().unwrap_or_else(|| "Unknown".to_string()),
                content: text,
            };
            messages.push(msg);
        }
    }

    // Recurse into children
    if let Ok(children) = element.children() {
        for child in children.iter() {
            scrape_messages_recursive(child, messages, depth + 1);
        }
    }
}

fn scrape_focused_app() -> Result<Vec<DiscordMessage>> {
    // Retry a few times in case app isn't focused yet
    let mut app = None;
    for attempt in 1..=5 {
        match find_browser_app() {
            Ok(a) => {
                app = Some(a);
                break;
            }
            Err(e) => {
                if attempt < 5 {
                    println!("Attempt {}/5: Waiting for focused app... ({:?})", attempt, e);
                    thread::sleep(Duration::from_secs(1));
                } else {
                    return Err(e);
                }
            }
        }
    }

    let app = app.context("Failed to find focused application")?;
    let mut messages = Vec::new();

    println!("Scraping UI...");
    scrape_messages_recursive(&app, &mut messages, 0);

    // Deduplicate messages
    messages.dedup_by(|a, b| a.content == b.content);

    Ok(messages)
}

fn save_to_csv(messages: &[DiscordMessage], path: &PathBuf) -> Result<()> {
    let mut writer = Writer::from_path(path)?;
    writer.write_record(["timestamp", "username", "content"])?;

    for msg in messages {
        writer.write_record([&msg.timestamp, &msg.username, &msg.content])?;
    }

    writer.flush()?;
    Ok(())
}

fn ensure_accessibility_permissions() -> Result<()> {
    if ax::is_process_trusted() {
        println!("✓ Accessibility permissions granted");
        return Ok(());
    }

    println!("⚠ Accessibility permissions required!");
    println!();
    println!("Opening System Settings...");
    ax::is_process_trusted_with_prompt(true);

    println!("Please enable accessibility for this terminal in:");
    println!("  System Settings > Privacy & Security > Accessibility");
    println!();
    println!("After granting permission, press Enter to continue...");

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

    // Check again
    if !ax::is_process_trusted() {
        anyhow::bail!("Accessibility permissions still not granted. Please enable and try again.");
    }

    println!("✓ Accessibility permissions granted");
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    println!("Discord Scraper using macOS Accessibility APIs");
    println!("===============================================");
    println!();

    // Check permissions first
    ensure_accessibility_permissions()?;

    if args.len() > 1 {
        let url = &args[1];

        if !url.starts_with("https://discord.com/") {
            anyhow::bail!("URL must be a Discord URL (https://discord.com/...)");
        }

        open_url(url)?;

        println!("Waiting for browser to load...");
        thread::sleep(Duration::from_secs(5));

        println!("Make sure the Discord tab is focused in your browser.");
        println!("Press Enter when ready to scrape...");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
    } else {
        println!("Usage: scrape-cidre <discord-url>");
        println!("Example: scrape-cidre https://discord.com/channels/1255148501793243136/1255148503081156642");
        println!();
        println!("Or make sure Discord (app or browser) is focused and run without arguments.");
        println!();
    }

    let messages = scrape_focused_app()?;
    println!("Found {} text elements", messages.len());

    if messages.is_empty() {
        println!("No messages found. Make sure Discord is visible.");
        return Ok(());
    }

    let output_path = PathBuf::from("discord_messages.csv");
    save_to_csv(&messages, &output_path)?;
    println!("Saved to: {}", output_path.display());

    println!("\nPreview of scraped content:");
    for (i, msg) in messages.iter().take(15).enumerate() {
        let preview = if msg.content.len() > 80 {
            format!("{}...", &msg.content[..80])
        } else {
            msg.content.clone()
        };
        println!("{}. [{}] {}", i + 1, msg.username, preview);
    }

    Ok(())
}
