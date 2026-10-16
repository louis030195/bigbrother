// screenpipe — AI that knows everything you've seen, said, and heard
// https://screenpipe.com
use super::{Action, Backend};
use crate::{
    accessibility as ax_helpers, apps,
    observation::{Limits, Node, Rect, Snapshot},
    Error, Result, UIElement,
};
use cidre::{ax, cf};
use std::collections::{HashMap, HashSet};

pub struct NativeBackend {
    root: UIElement,
    pid: i32,
    app: String,
    handles: HashMap<String, UIElement>,
    next: u64,
}
impl NativeBackend {
    pub fn new(app: &str) -> Result<Self> {
        if !crate::has_accessibility() {
            return Err(Error::permission_denied(
                "Enable Accessibility for bb before starting a native session",
            ));
        }
        let pid = apps::find_app_pid(app).map_err(Error::from)?;
        let root = UIElement::new(apps::get_app_element(pid).map_err(Error::from)?);
        root.raw()
            .set_messaging_timeout_secs(0.25)
            .map_err(|e| Error::action_failed("observe", &format!("{e:?}")))?;
        Ok(Self {
            root,
            pid,
            app: app.into(),
            handles: HashMap::new(),
            next: 0,
        })
    }
}
fn bool_attr(element: &UIElement, attr: &ax::Attr) -> Option<bool> {
    element.raw().attr_value(attr).ok().and_then(|v| {
        if v.get_type_id() == cf::Boolean::type_id() {
            let b: &cf::Boolean = unsafe { std::mem::transmute(&*v) };
            Some(b.value())
        } else {
            None
        }
    })
}
impl Backend for NativeBackend {
    fn capture(&mut self, limits: Limits) -> Result<Snapshot> {
        self.root.raw().role().map_err(|e| {
            Error::action_failed("observe", &format!("Target is unavailable: {e:?}"))
        })?;
        let mut nodes = Vec::new();
        let mut warnings = Vec::new();
        let mut handles = HashMap::new();
        let mut seen = HashSet::new();
        let started = std::time::Instant::now();
        let mut pending = vec![(self.root.clone(), None, 0)];
        while let Some((element, parent, depth)) = pending.pop() {
            if started.elapsed() > std::time::Duration::from_secs(5) {
                warnings.push("capture time limit reached".into());
                break;
            }
            if nodes.len() >= limits.max_nodes {
                warnings.push("node limit reached".into());
                break;
            }
            let key = self
                .handles
                .iter()
                .chain(handles.iter())
                .find(|(_, v)| v.raw().equal(element.raw()))
                .map(|(k, _)| k.clone())
                .unwrap_or_else(|| {
                    self.next += 1;
                    format!("{}:{}", self.pid, self.next)
                });
            if !seen.insert(key.clone()) {
                warnings.push("cycle skipped".into());
                continue;
            }
            let role = element.role().unwrap_or_else(|| "unknown".into());
            let subrole =
                ax_helpers::get_string_attr(element.raw(), ax::attr::subrole()).unwrap_or_default();
            let protected = subrole.contains("Secure") || role.contains("Secure");
            let mut actions = Vec::new();
            if element
                .raw()
                .actions()
                .map(|a| a.iter().any(|v| v.equal(ax::action::press())))
                .unwrap_or(false)
            {
                actions.push("click".into());
            }
            if element
                .raw()
                .is_settable(ax::attr::value())
                .unwrap_or(false)
            {
                actions.push("set_value".into());
            }
            actions.sort();
            let mut states = Vec::new();
            if bool_attr(&element, ax::attr::enabled()) == Some(false) {
                states.push("disabled".into());
            }
            if bool_attr(&element, ax::attr::focused()) == Some(true) {
                states.push("focused".into());
            }
            states.sort();
            let bounds = element
                .raw()
                .pos()
                .ok()
                .and_then(|p| p.cg_point())
                .zip(element.raw().size().ok().and_then(|s| s.cg_size()))
                .map(|(p, s)| Rect {
                    x: p.x,
                    y: p.y,
                    width: s.width,
                    height: s.height,
                });
            nodes.push(Node {
                key: key.clone(),
                parent,
                role,
                name: element
                    .title()
                    .filter(|s| !s.is_empty())
                    .or_else(|| element.description())
                    .unwrap_or_default(),
                value: if protected {
                    String::new()
                } else {
                    element.value().unwrap_or_default()
                },
                states,
                actions,
                bounds,
                protected,
            });
            let children = element.children();
            if depth >= limits.max_depth {
                if !children.is_empty() {
                    warnings.push("depth limit reached".into());
                }
            } else {
                if children.len() > limits.max_children {
                    warnings.push("child limit reached".into());
                }
                for child in children
                    .into_iter()
                    .take(limits.max_children)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                {
                    pending.push((child, Some(key.clone()), depth + 1));
                }
            }
            handles.insert(key, element);
        }
        self.handles = handles;
        warnings.sort();
        warnings.dedup();
        Ok(Snapshot {
            target: format!("{}:{}", self.app, self.pid),
            source: "macos_ax".into(),
            nodes,
            warnings,
        })
    }
    fn act(&mut self, key: &str, action: &Action) -> Result<()> {
        let el = self
            .handles
            .get(key)
            .ok_or_else(|| Error::element_not_found(key))?;
        if bool_attr(el, ax::attr::enabled()) == Some(false) {
            return Err(Error::action_failed(action.name(), "Element is disabled"));
        }
        match action {
            Action::Click => {
                el.click()?;
            }
            Action::SetValue { value } => {
                let mut raw = el.raw().retained();
                let value = cf::String::from_str(value);
                raw.set_attr(ax::attr::value(), &value)
                    .map_err(|e| Error::action_failed("set_value", &format!("{e:?}")))?;
            }
            _ => return Err(Error::action_failed(action.name(), "Unsupported AX action")),
        }
        Ok(())
    }
}
