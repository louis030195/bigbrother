use anyhow::{Context, Result};
use chrono::Local;
use cidre::ax;
use csv::Writer;
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone)]
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

fn find_browser_pid() -> Result<i32> {
    let browsers = [
        "Arc",
        "Google Chrome",
        "Safari",
        "Firefox",
        "Brave Browser",
        "Microsoft Edge",
        "Opera",
        "Vivaldi",
    ];

    for browser in browsers {
        let output = Command::new("pgrep")
            .arg("-x")
            .arg(browser)
            .output()
            .context("Failed to run pgrep")?;

        if output.status.success() {
            let pid_str = String::from_utf8_lossy(&output.stdout);
            if let Some(first_line) = pid_str.lines().next() {
                if let Ok(pid) = first_line.trim().parse::<i32>() {
                    println!("Found browser: {} (PID: {})", browser, pid);
                    return Ok(pid);
                }
            }
        }
    }

    anyhow::bail!("No browser found running. Please open a browser first.")
}

fn get_browser_name() -> Option<String> {
    let browsers = [
        "Arc",
        "Google Chrome",
        "Safari",
        "Firefox",
        "Brave Browser",
        "Microsoft Edge",
        "Opera",
        "Vivaldi",
    ];

    for browser in browsers {
        let output = Command::new("pgrep")
            .arg("-x")
            .arg(browser)
            .output()
            .ok()?;

        if output.status.success() {
            return Some(browser.to_string());
        }
    }
    None
}

fn find_browser_app() -> Result<cidre::arc::R<ax::UiElement>> {
    let pid = find_browser_pid()?;
    let app = ax::UiElement::with_app_pid(pid);

    let role = app.role().context("Failed to access browser - check accessibility permissions")?;
    println!("Browser role: {}", extract_role_name(&role));

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

fn extract_role_name(role: &cidre::arc::R<ax::Role>) -> String {
    let debug = format!("{:?}", role);
    if let Some(start) = debug.find("AX") {
        let rest = &debug[start..];
        let end = rest.find(|c| c == ')' || c == '"' || c == '}').unwrap_or(rest.len());
        return rest[..end].to_string();
    }
    "Unknown".to_string()
}

fn scrape_messages_recursive(
    element: &ax::UiElement,
    messages: &mut Vec<DiscordMessage>,
    depth: usize,
) {
    if depth > 30 {
        return;
    }

    let role = element.role().ok().map(|r| extract_role_name(&r));
    let role_desc = element.role_desc().ok().map(|s| s.to_string());

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

    if let Ok(children) = element.children() {
        for child in children.iter() {
            scrape_messages_recursive(child, messages, depth + 1);
        }
    }
}

fn scroll_up_in_browser(browser_name: &str, times: u32) -> Result<()> {
    // Use AppleScript to send Page Up key to the browser
    let script = format!(
        r#"
        tell application "{}"
            activate
        end tell
        delay 0.3
        tell application "System Events"
            repeat {} times
                key code 116 -- Page Up
                delay 0.5
            end repeat
        end tell
        "#,
        browser_name, times
    );

    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .context("Failed to execute scroll script")?;

    // Wait for Discord to load more messages
    thread::sleep(Duration::from_millis(800));
    Ok(())
}

fn scrape_browser() -> Result<Vec<DiscordMessage>> {
    let app = find_browser_app()?;
    let mut messages = Vec::new();

    println!("Scraping browser UI...");
    scrape_messages_recursive(&app, &mut messages, 0);

    messages.dedup_by(|a, b| a.content == b.content);

    Ok(messages)
}

fn scrape_with_scrolling(scroll_iterations: u32) -> Result<Vec<DiscordMessage>> {
    let browser_name = get_browser_name().context("No browser found")?;
    let mut all_messages: Vec<DiscordMessage> = Vec::new();
    let mut seen_content: HashSet<String> = HashSet::new();

    println!("Will scroll {} times to load more messages...", scroll_iterations);

    for i in 0..=scroll_iterations {
        if i > 0 {
            print!("Scrolling up ({}/{})... ", i, scroll_iterations);
            std::io::Write::flush(&mut std::io::stdout())?;
            scroll_up_in_browser(&browser_name, 5)?; // 5 page-ups per iteration
            println!("done");
        }

        let messages = scrape_browser()?;
        let mut new_count = 0;

        for msg in messages {
            if !seen_content.contains(&msg.content) {
                seen_content.insert(msg.content.clone());
                all_messages.push(msg);
                new_count += 1;
            }
        }

        println!("  Found {} new elements (total: {})", new_count, all_messages.len());

        // If no new messages found, we might have reached the top
        if i > 0 && new_count == 0 {
            println!("No new messages found, may have reached channel start.");
            break;
        }
    }

    Ok(all_messages)
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

    if !ax::is_process_trusted() {
        anyhow::bail!("Accessibility permissions still not granted. Please enable and try again.");
    }

    println!("✓ Accessibility permissions granted");
    Ok(())
}

fn print_usage() {
    println!("Usage: scrape-cidre [OPTIONS] <discord-url>");
    println!();
    println!("Options:");
    println!("  --scrolls <N>  Number of scroll iterations (default: 0)");
    println!("                 Each iteration scrolls up ~5 pages");
    println!("  --days <N>     Approximate days of history (1 day ≈ 3 scrolls)");
    println!();
    println!("Examples:");
    println!("  scrape-cidre https://discord.com/channels/123/456");
    println!("  scrape-cidre --scrolls 10 https://discord.com/channels/123/456");
    println!("  scrape-cidre --days 7 https://discord.com/channels/123/456");
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    println!("Discord Scraper using macOS Accessibility APIs");
    println!("===============================================");
    println!();

    // Parse arguments
    let mut scroll_iterations: u32 = 0;
    let mut url: Option<String> = None;
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "--scrolls" => {
                i += 1;
                if i < args.len() {
                    scroll_iterations = args[i].parse().unwrap_or(0);
                }
            }
            "--days" => {
                i += 1;
                if i < args.len() {
                    let days: u32 = args[i].parse().unwrap_or(1);
                    scroll_iterations = days * 3; // ~3 scroll iterations per day
                }
            }
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            arg if arg.starts_with("https://discord.com/") => {
                url = Some(arg.to_string());
            }
            _ => {}
        }
        i += 1;
    }

    // Check permissions first
    ensure_accessibility_permissions()?;

    if let Some(ref discord_url) = url {
        open_url(discord_url)?;
        println!("Waiting for browser to load...");
        thread::sleep(Duration::from_secs(5));
    } else {
        print_usage();
        println!();
        println!("Running without URL - will scrape current browser window.");
        println!();
    }

    let messages = if scroll_iterations > 0 {
        scrape_with_scrolling(scroll_iterations)?
    } else {
        scrape_browser()?
    };

    println!("Found {} total text elements", messages.len());

    if messages.is_empty() {
        println!("No messages found. Make sure Discord is visible in the browser.");
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
