//! Selector parsing for finding UI elements
//!
//! Syntax:
//!   role:Button              - exact role match
//!   name:Submit              - exact name match
//!   name~:screenpipe         - name contains
//!   title:Login              - exact title match
//!   value~:hello             - value contains
//!   index:42                 - element by index from last tree
//!   role:Button AND name:Sub - compound selector

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Selector {
    pub conditions: Vec<Condition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    pub attr: Attribute,
    pub op: MatchOp,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Attribute {
    Role,
    Name,
    Title,
    Value,
    Description,
    Index,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchOp {
    Equals,
    Contains,
}

impl Selector {
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        if s.is_empty() {
            return Err(Error::selector_invalid(s, "empty selector"));
        }

        let parts: Vec<&str> = s.split(" AND ").collect();
        let mut conditions = Vec::new();

        for part in parts {
            conditions.push(Condition::parse(part.trim())?);
        }

        Ok(Self { conditions })
    }

    pub fn role(role: &str) -> Self {
        Self {
            conditions: vec![Condition {
                attr: Attribute::Role,
                op: MatchOp::Equals,
                value: role.to_string(),
            }],
        }
    }

    pub fn name(name: &str) -> Self {
        Self {
            conditions: vec![Condition {
                attr: Attribute::Name,
                op: MatchOp::Equals,
                value: name.to_string(),
            }],
        }
    }

    pub fn name_contains(text: &str) -> Self {
        Self {
            conditions: vec![Condition {
                attr: Attribute::Name,
                op: MatchOp::Contains,
                value: text.to_string(),
            }],
        }
    }

    pub fn index(idx: usize) -> Self {
        Self {
            conditions: vec![Condition {
                attr: Attribute::Index,
                op: MatchOp::Equals,
                value: idx.to_string(),
            }],
        }
    }

    pub fn and(mut self, other: Selector) -> Self {
        self.conditions.extend(other.conditions);
        self
    }
}

impl Condition {
    pub fn parse(s: &str) -> Result<Self> {
        let (attr_str, rest) = s.split_once(':').ok_or_else(|| {
            Error::selector_invalid(s, "expected format 'attr:value' or 'attr~:value'")
        })?;

        let (attr, op) = if attr_str.ends_with('~') {
            (&attr_str[..attr_str.len() - 1], MatchOp::Contains)
        } else {
            (attr_str, MatchOp::Equals)
        };

        let attr = match attr.to_lowercase().as_str() {
            "role" => Attribute::Role,
            "name" => Attribute::Name,
            "title" => Attribute::Title,
            "value" => Attribute::Value,
            "desc" | "description" => Attribute::Description,
            "index" | "idx" => Attribute::Index,
            _ => {
                return Err(Error::selector_invalid(
                    s,
                    &format!("unknown attribute '{}'", attr),
                ))
            }
        };

        Ok(Self {
            attr,
            op,
            value: rest.to_string(),
        })
    }

    pub fn matches(&self, role: Option<&str>, name: Option<&str>, title: Option<&str>, value: Option<&str>, desc: Option<&str>) -> bool {
        let target = match self.attr {
            Attribute::Role => role,
            Attribute::Name => name,
            Attribute::Title => title,
            Attribute::Value => value,
            Attribute::Description => desc,
            Attribute::Index => return false, // handled separately
        };

        match (target, &self.op) {
            (Some(t), MatchOp::Equals) => t == self.value,
            (Some(t), MatchOp::Contains) => t.to_lowercase().contains(&self.value.to_lowercase()),
            (None, _) => false,
        }
    }
}

impl std::fmt::Display for Selector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let parts: Vec<String> = self.conditions.iter().map(|c| c.to_string()).collect();
        write!(f, "{}", parts.join(" AND "))
    }
}

impl std::fmt::Display for Condition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let op = match self.op {
            MatchOp::Equals => ":",
            MatchOp::Contains => "~:",
        };
        write!(f, "{:?}{}{}", self.attr, op, self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        let s = Selector::parse("role:Button").unwrap();
        assert_eq!(s.conditions.len(), 1);
        assert_eq!(s.conditions[0].attr, Attribute::Role);
        assert_eq!(s.conditions[0].value, "Button");
    }

    #[test]
    fn parse_contains() {
        let s = Selector::parse("name~:screenpipe").unwrap();
        assert_eq!(s.conditions[0].op, MatchOp::Contains);
    }

    #[test]
    fn parse_compound() {
        let s = Selector::parse("role:Button AND name:Submit").unwrap();
        assert_eq!(s.conditions.len(), 2);
    }
}
