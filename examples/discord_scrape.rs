//! Discord channel scraper using superintelligence
//!
//! Run with: cargo run --example discord_scrape -- <channel_url> [scroll_count]

use superintelligence::prelude::*;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let url = args.get(1).map(|s| s.as_str());
    let scroll_count: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);

    let desktop = Desktop::new()?;

    // Open URL if provided
    if let Some(url) = url {
        println!("Opening: {}", url);
        desktop.open_url(url)?;
        desktop.wait_idle(5000)?;
    }

    // Find browser
    let browser = desktop.browser()?;
    println!("Using browser: {} (PID: {})", browser.name, browser.pid);

    // Activate browser
    desktop.activate(&browser.name)?;
    desktop.wait_idle(1000)?;

    // Scroll and scrape loop
    let mut all_messages: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for i in 0..=scroll_count {
        if i > 0 {
            println!("Scrolling up ({}/{})", i, scroll_count);
            desktop.scroll_up(5)?;
            desktop.wait_idle(1000)?;
        }

        // Scrape current view
        let result = desktop.scrape(&browser.name, 25)?;

        let mut new_count = 0;
        for item in result.items {
            if !seen.contains(&item.text) {
                seen.insert(item.text.clone());
                all_messages.push(item.text);
                new_count += 1;
            }
        }

        println!("  Found {} new items (total: {})", new_count, all_messages.len());

        if i > 0 && new_count == 0 {
            println!("No new content, stopping");
            break;
        }
    }

    // Output as JSON
    println!("\n--- Results ---");
    println!("{}", serde_json::to_string_pretty(&all_messages).unwrap());

    Ok(())
}
