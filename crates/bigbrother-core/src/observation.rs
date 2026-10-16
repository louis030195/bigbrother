// screenpipe — AI that knows everything you've seen, said, and heard
// https://screenpipe.com
//! Platform-independent, bounded accessibility observations and revision rendering.
use crate::{Error, ErrorCode, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub key: String,
    pub parent: Option<String>,
    pub role: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub states: Vec<String>,
    #[serde(default)]
    pub actions: Vec<String>,
    pub bounds: Option<Rect>,
    #[serde(default)]
    pub protected: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Limits {
    pub max_nodes: usize,
    pub max_depth: usize,
    pub max_children: usize,
    pub max_text_chars: usize,
    pub max_output_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_nodes: 2000,
            max_depth: 32,
            max_children: 200,
            max_text_chars: 500,
            max_output_bytes: 32000,
        }
    }
}
impl Limits {
    pub fn validate(&self) -> Result<()> {
        if self.max_nodes == 0
            || self.max_nodes > 10000
            || self.max_depth == 0
            || self.max_depth > 128
            || self.max_children == 0
            || self.max_children > 1000
            || self.max_text_chars == 0
            || self.max_text_chars > 10000
            || self.max_output_bytes < 256
            || self.max_output_bytes > 1_000_000
        {
            return Err(Error::new(
                ErrorCode::InvalidRequest,
                "Observation limits are outside supported bounds",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub target: String,
    pub source: String,
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub revision: u64,
    pub base_revision: Option<u64>,
    pub target: String,
    pub source: String,
    pub mode: String,
    pub text: String,
    pub node_count: usize,
    pub truncated: bool,
    pub warnings: Vec<String>,
}

/// A renderer is owned by one session. References are never recycled.
#[derive(Default)]
pub struct Renderer {
    revision: u64,
    next_id: u64,
    ids: HashMap<String, String>,
    previous: Vec<(String, String)>,
    target: String,
    current: HashMap<String, Node>,
    valid: bool,
    limits: Limits,
}

pub fn clipped(text: &str, max: usize) -> String {
    let mut chars = text.chars();
    let prefix: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}
fn clean(text: &str, max: usize) -> String {
    clipped(&text.split_whitespace().collect::<Vec<_>>().join(" "), max)
}
fn generic(role: &str) -> bool {
    matches!(
        role.to_ascii_lowercase().trim_start_matches("ax"),
        "group" | "generic" | "pane" | "unknown" | "ignored"
    )
}

impl Renderer {
    pub fn invalidate(&mut self) {
        self.valid = false;
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn limits(&self) -> Limits {
        self.limits
    }
    pub fn resolve(&self, revision: u64, reference: &str) -> Result<&Node> {
        if !self.valid || revision != self.revision {
            return Err(Error::new(
                ErrorCode::StaleObservation,
                "Observe again before acting: this revision is no longer current",
            ));
        }
        self.current
            .get(reference)
            .ok_or_else(|| Error::element_not_found(reference))
    }
    pub fn render(
        &mut self,
        snapshot: Snapshot,
        limits: Limits,
        full: bool,
    ) -> Result<Observation> {
        limits.validate()?;
        self.valid = false;
        if self.target != snapshot.target {
            self.ids.clear();
            self.previous.clear();
            self.target = snapshot.target.clone();
        }
        self.current.clear();
        let mut warnings = snapshot.warnings;
        let mut truncated = !warnings.is_empty();
        let mut nodes = Vec::new();
        let mut seen = HashSet::new();
        let mut parents: HashMap<String, Option<String>> = HashMap::new();
        if snapshot.nodes.len() > limits.max_nodes {
            truncated = true;
        }
        for mut node in snapshot.nodes.into_iter().take(limits.max_nodes) {
            if nodes.len() >= limits.max_nodes {
                truncated = true;
                break;
            }
            if !seen.insert(node.key.clone()) {
                warnings.push("Duplicate/cyclic element skipped".into());
                truncated = true;
                continue;
            }
            // Parents must precede children. Unknown parents are made explicit roots.
            node.parent = node.parent.and_then(|p| parents.get(&p).cloned().flatten());
            if node.protected {
                node.value.clear();
                node.states.push("protected".into());
            }
            node.states.sort();
            node.states.dedup();
            node.actions.sort();
            node.actions.dedup();
            if generic(&node.role)
                && node.name.trim().is_empty()
                && node.value.trim().is_empty()
                && node.actions.is_empty()
                && node.states.is_empty()
            {
                parents.insert(node.key, node.parent);
                continue;
            }
            parents.insert(node.key.clone(), Some(node.key.clone()));
            nodes.push(node);
        }
        let mut lines = Vec::new();
        let mut bytes = 0;
        for node in nodes {
            let id = self
                .ids
                .entry(node.key.clone())
                .or_insert_with(|| {
                    self.next_id += 1;
                    format!("e{}", self.next_id)
                })
                .clone();
            let parent = node
                .parent
                .as_ref()
                .and_then(|p| self.ids.get(p))
                .map(|p| format!(" parent={p}"))
                .unwrap_or_default();
            let mut line = format!("[{id}]{parent} {}", clean(&node.role, 80));
            if !node.name.trim().is_empty() {
                line.push_str(&format!(
                    " {}",
                    serde_json::to_string(&clean(&node.name, limits.max_text_chars)).unwrap()
                ));
            }
            if !node.value.trim().is_empty() && node.value != node.name {
                line.push_str(&format!(
                    " value={}",
                    serde_json::to_string(&clean(&node.value, limits.max_text_chars)).unwrap()
                ));
            }
            if !node.states.is_empty() {
                line.push_str(&format!(" ({})", node.states.join(",")));
            }
            if !node.actions.is_empty() {
                line.push_str(&format!(" actions={}", node.actions.join(",")));
            }
            if bytes + line.len() + 1 > limits.max_output_bytes.saturating_sub(64) {
                truncated = true;
                break;
            }
            bytes += line.len() + 1;
            self.current.insert(id.clone(), node);
            lines.push((id, line));
        }
        let base = self.revision;
        self.revision += 1;
        let mut text = lines
            .iter()
            .map(|(_, l)| l.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if truncated {
            text.push_str("\n[truncated: narrow the target or increase observation limits]");
        }
        if lines.is_empty() && !truncated {
            text = "[no accessible elements]".into();
        }
        let mut mode = "full";
        let mut base_revision = None;
        // Structural changes force a full snapshot; a textual delta never loses ordering.
        if !full
            && base > 0
            && lines
                .iter()
                .map(|p| &p.0)
                .eq(self.previous.iter().map(|p| &p.0))
            && !truncated
        {
            let changes: Vec<_> = lines
                .iter()
                .zip(&self.previous)
                .filter(|(a, b)| a.1 != b.1)
                .map(|(a, _)| format!("~ {}", a.1))
                .collect();
            let delta = if changes.is_empty() {
                "[unchanged]".to_string()
            } else {
                changes.join("\n")
            };
            if delta.len() < text.len() {
                text = delta;
                mode = if changes.is_empty() {
                    "unchanged"
                } else {
                    "diff"
                };
                base_revision = Some(base);
            }
        }
        // Retain only current identities; monotonic counters prevent reference reuse.
        let active: HashSet<_> = self.current.values().map(|n| n.key.clone()).collect();
        self.ids.retain(|k, _| active.contains(k));
        self.previous = lines;
        self.limits = limits;
        self.valid = true;
        Ok(Observation {
            revision: self.revision,
            base_revision,
            target: snapshot.target,
            source: snapshot.source,
            mode: mode.into(),
            text,
            node_count: self.current.len(),
            truncated,
            warnings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(key: &str, name: &str) -> Node {
        Node {
            key: key.into(),
            parent: None,
            role: "button".into(),
            name: name.into(),
            value: String::new(),
            actions: vec!["click".into()],
            states: vec![],
            bounds: None,
            protected: false,
        }
    }
    fn snap(nodes: Vec<Node>) -> Snapshot {
        Snapshot {
            target: "test:1".into(),
            source: "fixture".into(),
            nodes,
            warnings: vec![],
        }
    }
    #[test]
    fn revisions_reject_stale_actions_and_compact_changes() {
        let mut r = Renderer::default();
        let nodes: Vec<_> = (0..100).map(|i| node(&i.to_string(), "Save")).collect();
        let first = r
            .render(snap(nodes.clone()), Limits::default(), false)
            .unwrap();
        let unchanged = r
            .render(snap(nodes.clone()), Limits::default(), false)
            .unwrap();
        assert_eq!(unchanged.mode, "unchanged");
        assert!(r.resolve(first.revision, "e1").is_err());
        let mut changed = nodes;
        changed[0].name = "Saved".into();
        let diff = r.render(snap(changed), Limits::default(), false).unwrap();
        assert_eq!(diff.mode, "diff");
        assert_eq!(diff.base_revision, Some(2));
        assert!(diff.text.len() < first.text.len() / 10);
        r.invalidate();
        assert!(r.resolve(diff.revision, "e1").is_err());
    }
    #[test]
    fn structural_changes_return_full_and_do_not_reuse_refs() {
        let mut r = Renderer::default();
        r.render(
            snap(vec![node("a", "A"), node("b", "B")]),
            Limits::default(),
            false,
        )
        .unwrap();
        let next = r
            .render(
                snap(vec![node("b", "B"), node("c", "C")]),
                Limits::default(),
                false,
            )
            .unwrap();
        assert_eq!(next.mode, "full");
        assert!(next.text.contains("[e2]"));
        assert!(next.text.contains("[e3]"));
        assert!(r.resolve(2, "e1").is_err());
    }
    #[test]
    fn budget_unicode_passwords_and_pruned_parents() {
        let mut r = Renderer::default();
        let mut group = node("root", "");
        group.role = "group".into();
        group.actions.clear();
        let mut secret = node("input", "密码🙂");
        secret.parent = Some("root".into());
        secret.value = "never-print-this".into();
        secret.protected = true;
        let out = r
            .render(snap(vec![group, secret]), Limits::default(), false)
            .unwrap();
        assert_eq!(out.node_count, 1);
        assert!(!out.text.contains("never-print-this"));
        assert!(out.text.contains("密码🙂"));
        assert!(!out.text.contains("parent="));
        assert_eq!(clipped("🙂你好a", 3), "🙂你好…");
        let many = (0..100)
            .map(|i| node(&i.to_string(), "a".repeat(300).as_str()))
            .collect();
        let out = r
            .render(
                snap(many),
                Limits {
                    max_output_bytes: 512,
                    ..Limits::default()
                },
                true,
            )
            .unwrap();
        assert!(out.truncated);
        assert!(out.text.len() <= 512);
    }
}
