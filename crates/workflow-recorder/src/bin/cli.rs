//! Workflow Recorder CLI

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use workflow_recorder::prelude::*;

#[derive(Parser)]
#[command(name = "wr")]
#[command(about = "macOS Workflow Recorder - Record and replay user interactions")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start recording
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

    /// Replay a workflow
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

    /// Check permissions
    Permissions {
        /// Request if not granted
        #[arg(long)]
        request: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Record { name, no_context, threshold } => {
            record(&name, !no_context, threshold)?;
        }
        Commands::Replay { file, speed } => {
            replay(&file, speed)?;
        }
        Commands::List => {
            list()?;
        }
        Commands::Show { file, all } => {
            show(&file, all)?;
        }
        Commands::Delete { file } => {
            delete(&file)?;
        }
        Commands::Permissions { request } => {
            permissions(request)?;
        }
    }

    Ok(())
}

fn record(name: &str, capture_context: bool, threshold: f64) -> Result<()> {
    let config = RecorderConfig {
        capture_context,
        mouse_move_threshold: threshold,
        ..Default::default()
    };

    let recorder = WorkflowRecorder::with_config(config);

    // Check permissions
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

    // Ctrl+C handler
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })?;

    // Progress display
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

    // Save
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

    // Count event types
    let mut clicks = 0;
    let mut moves = 0;
    let mut scrolls = 0;
    let mut keys = 0;
    let mut text = 0;
    let mut apps = 0;
    let mut pastes = 0;

    for e in &workflow.events {
        match &e.data {
            EventData::Click { .. } => clicks += 1,
            EventData::Move { .. } => moves += 1,
            EventData::Scroll { .. } => scrolls += 1,
            EventData::Key { .. } => keys += 1,
            EventData::Text { .. } => text += 1,
            EventData::App { .. } => apps += 1,
            EventData::Paste { .. } => pastes += 1,
            EventData::Context { .. } => {}
        }
    }

    println!("\nSummary:");
    println!("  Clicks: {}", clicks);
    println!("  Moves: {}", moves);
    println!("  Scrolls: {}", scrolls);
    println!("  Keys: {}", keys);
    println!("  Text inputs: {}", text);
    println!("  App switches: {}", apps);
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
