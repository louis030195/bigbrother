use anyhow::{Context, Result};
use chrono::Local;
use cidre::ax;
use csv::Writer;
use std::path::PathBuf;

#[derive(Debug)]
struct DiscordMessage {
    timestamp: String,
    username: String,
    content: String,
}

fn find_discord_app() -> Result<cidre::arc::R<ax::UiElement>> {
    let system_wide = ax::UiElement::sys_wide();
    let app = system_wide
        .focused_app()
        .context("Failed to get focused application")?;

    let role_desc = app.role_desc().ok();
    println!("Focused app role: {:?}", role_desc);

    Ok(app)
}

fn get_string_attr(element: &ax::UiElement, attr: &ax::Attr) -> Option<String> {
    element
        .attr_value(attr)
        .ok()
        .and_then(|v| {
            // Try to downcast to cf::String
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
    if depth > 25 {
        return;
    }

    // Get role for this element
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

fn scrape_discord_messages() -> Result<Vec<DiscordMessage>> {
    // Check if we have accessibility permissions
    if !ax::is_process_trusted() {
        println!("Requesting accessibility permissions...");
        ax::is_process_trusted_with_prompt(true);
        anyhow::bail!("Please grant accessibility permissions in System Settings > Privacy & Security > Accessibility, then try again.");
    }

    let app = find_discord_app()?;
    let mut messages = Vec::new();

    println!("Scraping Discord UI...");
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

fn main() -> Result<()> {
    println!("Discord Scraper using macOS Accessibility APIs");
    println!("===============================================");
    println!();
    println!("Make sure:");
    println!("1. Discord is open and focused");
    println!("2. Terminal has Accessibility permissions in System Settings");
    println!();

    let messages = scrape_discord_messages()?;
    println!("Found {} text elements", messages.len());

    if messages.is_empty() {
        println!("No messages found. Make sure Discord chat is visible.");
        return Ok(());
    }

    let output_path = PathBuf::from("discord_messages.csv");
    save_to_csv(&messages, &output_path)?;
    println!("Saved to: {}", output_path.display());

    // Print first few messages as preview
    println!("\nPreview of scraped content:");
    for (i, msg) in messages.iter().take(10).enumerate() {
        let preview = if msg.content.len() > 60 {
            format!("{}...", &msg.content[..60])
        } else {
            msg.content.clone()
        };
        println!("{}. [{}] {}", i + 1, msg.username, preview);
    }

    Ok(())
}
