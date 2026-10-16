// screenpipe — AI that knows everything you've seen, said, and heard
// https://screenpipe.com
//! UI Element representation with structured output

use crate::accessibility::*;
use crate::error::{Error, Result};
use cidre::arc::R;
use cidre::ax;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct UIElement {
    inner: R<ax::UiElement>,
    pub index: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub success: bool,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element: Option<ElementInfo>,
    pub timing_ms: u64,
}

impl UIElement {
    pub fn new(inner: R<ax::UiElement>) -> Self {
        Self { inner, index: None }
    }

    pub fn with_index(mut self, index: usize) -> Self {
        self.index = Some(index);
        self
    }

    pub fn raw(&self) -> &ax::UiElement {
        &self.inner
    }

    pub fn role(&self) -> Option<String> {
        get_role(&self.inner)
    }

    pub fn name(&self) -> Option<String> {
        get_title(&self.inner).or_else(|| get_description(&self.inner))
    }

    pub fn title(&self) -> Option<String> {
        get_title(&self.inner)
    }

    pub fn value(&self) -> Option<String> {
        get_value(&self.inner)
    }

    pub fn description(&self) -> Option<String> {
        get_description(&self.inner)
    }

    pub fn text(&self) -> Option<String> {
        self.value()
            .or_else(|| self.title())
            .or_else(|| self.description())
            .or_else(|| self.name())
    }

    pub fn bounds(&self) -> Option<Bounds> {
        let p = self.inner.pos().ok()?.cg_point()?;
        let s = self.inner.size().ok()?.cg_size()?;
        Some(Bounds {
            x: p.x,
            y: p.y,
            width: s.width,
            height: s.height,
        })
    }

    pub fn info(&self) -> ElementInfo {
        ElementInfo {
            index: self.index,
            role: self.role().unwrap_or_else(|| "Unknown".to_string()),
            name: self.name(),
            title: self.title(),
            value: self.value(),
            description: self.description(),
            bounds: self.bounds(),
        }
    }

    pub fn children(&self) -> Vec<UIElement> {
        get_children(&self.inner)
            .into_iter()
            .map(UIElement::new)
            .collect()
    }

    pub fn click(&self) -> Result<ActionResult> {
        let start = std::time::Instant::now();

        // Try to perform AX press action
        if let Err(e) = self.inner.perform_action(ax::action::press()) {
            return Err(Error::action_failed("click", &format!("{:?}", e)));
        }

        Ok(ActionResult {
            success: true,
            action: "click".to_string(),
            element: Some(self.info()),
            timing_ms: start.elapsed().as_millis() as u64,
        })
    }

    pub fn set_value(&self, text: &str) -> Result<ActionResult> {
        let start = std::time::Instant::now();

        let mut element = self.inner.clone();
        if !element.is_settable(ax::attr::value()).unwrap_or(false) {
            return Err(Error::action_failed(
                "set_value",
                "Element does not expose a writable AXValue",
            ));
        }
        let value = cidre::cf::String::from_str(text);
        element
            .set_attr(ax::attr::value(), &value)
            .map_err(|e| Error::action_failed("set_value", &format!("{e:?}")))?;

        Ok(ActionResult {
            success: true,
            action: "set_value".to_string(),
            element: Some(self.info()),
            timing_ms: start.elapsed().as_millis() as u64,
        })
    }
}

impl std::fmt::Debug for UIElement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UIElement")
            .field("role", &self.role())
            .field("name", &self.name())
            .field("title", &self.title())
            .finish()
    }
}
