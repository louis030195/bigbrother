//! Workflow storage - JSON lines format for efficiency

use crate::events::{RecordedWorkflow, Event};
use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

pub struct WorkflowStorage {
    dir: PathBuf,
}

impl WorkflowStorage {
    pub fn new() -> Result<Self> {
        let home = std::env::var("HOME").context("HOME not set")?;
        let dir = PathBuf::from(home).join(".workflow-recorder");
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    pub fn with_dir(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    /// Save workflow as JSON lines (one event per line for streaming)
    pub fn save(&self, workflow: &RecordedWorkflow) -> Result<PathBuf> {
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let name = sanitize(&workflow.name);
        let filename = format!("{}_{}.jsonl", name, ts);
        let path = self.dir.join(&filename);

        let file = File::create(&path)?;
        let mut w = BufWriter::new(file);

        // First line: metadata
        writeln!(w, r#"{{"name":"{}","events":{}}}"#, workflow.name, workflow.events.len())?;

        // Remaining lines: events
        for e in &workflow.events {
            serde_json::to_writer(&mut w, e)?;
            writeln!(w)?;
        }

        w.flush()?;
        Ok(path)
    }

    /// Load workflow from JSON lines
    pub fn load(&self, filename: &str) -> Result<RecordedWorkflow> {
        let path = self.dir.join(filename);
        let file = File::open(&path)?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        // First line: metadata
        let meta_line = lines.next().context("Empty file")??;
        let meta: serde_json::Value = serde_json::from_str(&meta_line)?;
        let name = meta["name"].as_str().unwrap_or("unknown").to_string();

        // Remaining lines: events
        let mut events = Vec::new();
        for line in lines {
            let line = line?;
            if !line.is_empty() {
                let e: Event = serde_json::from_str(&line)?;
                events.push(e);
            }
        }

        Ok(RecordedWorkflow { name, events })
    }

    /// List all workflows
    pub fn list(&self) -> Result<Vec<String>> {
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let name = entry.file_name();
            if let Some(s) = name.to_str() {
                if s.ends_with(".jsonl") {
                    files.push(s.to_string());
                }
            }
        }
        files.sort();
        Ok(files)
    }

    pub fn delete(&self, filename: &str) -> Result<()> {
        let path = self.dir.join(filename);
        fs::remove_file(path)?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.dir
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}
