//! Structured errors for AI parsing

use serde::{Deserialize, Serialize};
use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    ElementNotFound,
    Timeout,
    PermissionDenied,
    AppNotRunning,
    ActionFailed,
    SelectorInvalid,
    MultipleMatches,
    Unknown,
}

impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            suggestions: Vec::new(),
            context: None,
        }
    }

    pub fn with_suggestions(mut self, suggestions: Vec<String>) -> Self {
        self.suggestions = suggestions;
        self
    }

    pub fn with_context(mut self, context: serde_json::Value) -> Self {
        self.context = Some(context);
        self
    }

    pub fn element_not_found(selector: &str) -> Self {
        Self::new(
            ErrorCode::ElementNotFound,
            format!("No element matching: {}", selector),
        )
    }

    pub fn timeout(selector: &str, timeout_ms: u64) -> Self {
        Self::new(
            ErrorCode::Timeout,
            format!("Timeout after {}ms waiting for: {}", timeout_ms, selector),
        )
    }

    pub fn permission_denied(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::PermissionDenied, message)
    }

    pub fn app_not_running(app: &str) -> Self {
        Self::new(ErrorCode::AppNotRunning, format!("App not running: {}", app))
    }

    pub fn action_failed(action: &str, reason: &str) -> Self {
        Self::new(
            ErrorCode::ActionFailed,
            format!("{} failed: {}", action, reason),
        )
    }

    pub fn selector_invalid(selector: &str, reason: &str) -> Self {
        Self::new(
            ErrorCode::SelectorInvalid,
            format!("Invalid selector '{}': {}", selector, reason),
        )
    }

    pub fn multiple_matches(selector: &str, count: usize) -> Self {
        Self::new(
            ErrorCode::MultipleMatches,
            format!("Selector '{}' matched {} elements, expected 1", selector, count),
        )
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}] {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Self::new(ErrorCode::Unknown, e.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::new(ErrorCode::Unknown, e.to_string())
    }
}
