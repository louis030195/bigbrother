//! Locator - fluent API for finding and interacting with elements

use crate::accessibility::*;
use crate::element::{ActionResult, UIElement};
use crate::error::{Error, Result};
use crate::selector::{Attribute, Selector};
use cidre::ax;
use std::time::{Duration, Instant};

pub struct Locator {
    selector: Selector,
    root: Option<UIElement>,
    timeout_ms: u64,
    max_depth: usize,
}

impl Locator {
    pub fn new(selector: Selector) -> Self {
        Self {
            selector,
            root: None,
            timeout_ms: 5000,
            max_depth: 30,
        }
    }

    pub fn parse(selector: &str) -> Result<Self> {
        Ok(Self::new(Selector::parse(selector)?))
    }

    pub fn with_root(mut self, root: UIElement) -> Self {
        self.root = Some(root);
        self
    }

    pub fn timeout(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }

    pub fn depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth;
        self
    }

    pub fn find(&self) -> Result<UIElement> {
        let elements = self.find_all()?;

        if elements.is_empty() {
            return Err(Error::element_not_found(&self.selector.to_string()));
        }

        if elements.len() > 1 {
            return Err(Error::multiple_matches(&self.selector.to_string(), elements.len())
                .with_suggestions(vec![
                    "Add more conditions to narrow the match".to_string(),
                    format!("Use index:0 through index:{} to pick one", elements.len() - 1),
                ])
                .with_context(serde_json::json!({
                    "matches": elements.iter().map(|e| e.info()).collect::<Vec<_>>()
                })));
        }

        Ok(elements.into_iter().next().unwrap())
    }

    pub fn find_all(&self) -> Result<Vec<UIElement>> {
        let root = match &self.root {
            Some(r) => r.clone(),
            None => {
                // Get system-wide element as root
                let sys = ax::UiElement::sys_wide();
                UIElement::new(sys)
            }
        };

        let mut results = Vec::new();
        self.find_recursive(root.raw(), 0, &mut results);

        // Add indices
        let results: Vec<UIElement> = results
            .into_iter()
            .enumerate()
            .map(|(i, e)| e.with_index(i))
            .collect();

        Ok(results)
    }

    fn find_recursive(&self, element: &ax::UiElement, depth: usize, results: &mut Vec<UIElement>) {
        if depth > self.max_depth {
            return;
        }

        if self.matches(element) {
            results.push(UIElement::new(element.retained()));
        }

        for child in get_children(element) {
            self.find_recursive(&child, depth + 1, results);
        }
    }

    fn matches(&self, element: &ax::UiElement) -> bool {
        let role = get_role(element);
        let name = get_role_desc(element);
        let title = get_title(element);
        let value = get_value(element);
        let desc = get_description(element);

        for cond in &self.selector.conditions {
            if cond.attr == Attribute::Index {
                continue; // Index handled separately
            }
            if !cond.matches(
                role.as_deref(),
                name.as_deref(),
                title.as_deref(),
                value.as_deref(),
                desc.as_deref(),
            ) {
                return false;
            }
        }
        true
    }

    pub fn exists(&self) -> bool {
        self.find_all().map(|v| !v.is_empty()).unwrap_or(false)
    }

    pub fn wait(&self) -> Result<UIElement> {
        let start = Instant::now();
        let timeout = Duration::from_millis(self.timeout_ms);

        loop {
            match self.find() {
                Ok(e) => return Ok(e),
                Err(_) if start.elapsed() < timeout => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(_) => {
                    return Err(Error::timeout(&self.selector.to_string(), self.timeout_ms));
                }
            }
        }
    }

    pub fn wait_gone(&self) -> Result<()> {
        let start = Instant::now();
        let timeout = Duration::from_millis(self.timeout_ms);

        loop {
            if !self.exists() {
                return Ok(());
            }
            if start.elapsed() >= timeout {
                return Err(Error::timeout(
                    &format!("{} to disappear", self.selector),
                    self.timeout_ms,
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    // Actions - find then act

    pub fn click(&self) -> Result<ActionResult> {
        self.find()?.click()
    }

    pub fn type_text(&self, text: &str) -> Result<ActionResult> {
        let element = self.find()?;
        element.click()?;
        std::thread::sleep(Duration::from_millis(100));
        element.set_value(text)
    }
}
