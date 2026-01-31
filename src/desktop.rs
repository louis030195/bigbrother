//! Desktop - main entry point for automation

use crate::apps;
use crate::element::UIElement;
use crate::error::{Error, Result};
use crate::input;
use crate::locator::Locator;
use crate::selector::Selector;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub struct Desktop {
    app_filter: Option<String>,
    tree_cache: Vec<UIElement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub pid: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub index: usize,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub depth: usize,
    pub children_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeResult {
    pub app: String,
    pub element_count: usize,
    pub nodes: Vec<TreeNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeResult {
    pub app: String,
    pub items: Vec<ScrapeItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeItem {
    pub index: usize,
    pub role: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

impl Desktop {
    pub fn new() -> Result<Self> {
        crate::ensure_accessibility()?;
        Ok(Self {
            app_filter: None,
            tree_cache: Vec::new(),
        })
    }

    pub fn in_app(mut self, app: &str) -> Self {
        self.app_filter = Some(app.to_string());
        self
    }

    // Discovery

    pub fn apps(&self) -> Result<Vec<AppInfo>> {
        let names = apps::list_running_apps().map_err(|e| Error::from(e))?;
        let mut result = Vec::new();

        for name in names {
            if let Ok(pid) = apps::find_app_pid(&name) {
                result.push(AppInfo { name, pid });
            }
        }

        Ok(result)
    }

    pub fn find_app(&self, name: &str) -> Result<AppInfo> {
        let pid = apps::find_app_pid(name).map_err(|_| Error::app_not_running(name))?;
        Ok(AppInfo {
            name: name.to_string(),
            pid,
        })
    }

    pub fn browser(&self) -> Result<AppInfo> {
        let (name, pid) = apps::find_browser().map_err(|e| Error::from(e))?;
        Ok(AppInfo { name, pid })
    }

    // Element finding

    pub fn locator(&self, selector: &str) -> Result<Locator> {
        let mut loc = Locator::parse(selector)?;
        if let Some(ref app) = self.app_filter {
            let root = self.app_root(app)?;
            loc = loc.with_root(root);
        }
        Ok(loc)
    }

    pub fn locator_selector(&self, selector: Selector) -> Locator {
        let mut loc = Locator::new(selector);
        if let Some(ref app) = self.app_filter {
            if let Ok(root) = self.app_root(app) {
                loc = loc.with_root(root);
            }
        }
        loc
    }

    fn app_root(&self, app: &str) -> Result<UIElement> {
        let element = apps::get_app_by_name(app).map_err(|_| Error::app_not_running(app))?;
        Ok(UIElement::new(element))
    }

    // Tree inspection

    pub fn tree(&mut self, app: &str, max_depth: usize) -> Result<TreeResult> {
        let root = self.app_root(app)?;
        let mut nodes = Vec::new();
        let mut index = 0;

        self.tree_cache.clear();
        self.build_tree(&root, 0, max_depth, &mut nodes, &mut index);

        Ok(TreeResult {
            app: app.to_string(),
            element_count: nodes.len(),
            nodes,
        })
    }

    fn build_tree(
        &mut self,
        element: &UIElement,
        depth: usize,
        max_depth: usize,
        nodes: &mut Vec<TreeNode>,
        index: &mut usize,
    ) {
        if depth > max_depth {
            return;
        }

        let children = element.children();
        let node = TreeNode {
            index: *index,
            role: element.role().unwrap_or_else(|| "Unknown".to_string()),
            name: element.name(),
            title: element.title(),
            value: element.value().map(|v| {
                if v.len() > 100 {
                    format!("{}...", &v[..100])
                } else {
                    v
                }
            }),
            depth,
            children_count: children.len(),
        };

        nodes.push(node);
        self.tree_cache.push(element.clone().with_index(*index));
        *index += 1;

        for child in children {
            self.build_tree(&child, depth + 1, max_depth, nodes, index);
        }
    }

    pub fn element_by_index(&self, index: usize) -> Result<UIElement> {
        self.tree_cache
            .get(index)
            .cloned()
            .ok_or_else(|| Error::element_not_found(&format!("index:{}", index)))
    }

    // Scraping

    pub fn scrape(&self, app: &str, max_depth: usize) -> Result<ScrapeResult> {
        let root = self.app_root(app)?;
        let mut items = Vec::new();
        let mut seen = std::collections::HashSet::new();

        self.scrape_recursive(&root, max_depth, 0, &mut items, &mut seen);

        Ok(ScrapeResult {
            app: app.to_string(),
            items,
        })
    }

    fn scrape_recursive(
        &self,
        element: &UIElement,
        max_depth: usize,
        depth: usize,
        items: &mut Vec<ScrapeItem>,
        seen: &mut std::collections::HashSet<String>,
    ) {
        if depth > max_depth {
            return;
        }

        if let Some(text) = element.text() {
            if text.len() > 2 && !seen.contains(&text) {
                seen.insert(text.clone());
                items.push(ScrapeItem {
                    index: items.len(),
                    role: element.role().unwrap_or_else(|| "Unknown".to_string()),
                    text,
                    context: element.name(),
                });
            }
        }

        for child in element.children() {
            self.scrape_recursive(&child, max_depth, depth + 1, items, seen);
        }
    }

    // Actions

    pub fn open_url(&self, url: &str) -> Result<()> {
        apps::open_url(url).map_err(|e| Error::from(e))
    }

    pub fn activate(&self, app: &str) -> Result<()> {
        apps::activate_app(app).map_err(|e| Error::from(e))
    }

    pub fn wait_idle(&self, ms: u64) -> Result<()> {
        std::thread::sleep(Duration::from_millis(ms));
        Ok(())
    }

    pub fn scroll_up(&self, pages: u32) -> Result<()> {
        input::scroll_up(pages).map_err(|e| Error::from(e))
    }

    pub fn scroll_down(&self, pages: u32) -> Result<()> {
        input::scroll_down(pages).map_err(|e| Error::from(e))
    }

    pub fn press_key(&self, key_code: u8) -> Result<()> {
        input::press_key(key_code).map_err(|e| Error::from(e))
    }

    pub fn type_text(&self, text: &str) -> Result<()> {
        input::type_text(text).map_err(|e| Error::from(e))
    }

    pub fn cmd(&self, key: &str) -> Result<()> {
        input::cmd(key).map_err(|e| Error::from(e))
    }
}

impl Default for Desktop {
    fn default() -> Self {
        Self {
            app_filter: None,
            tree_cache: Vec::new(),
        }
    }
}
