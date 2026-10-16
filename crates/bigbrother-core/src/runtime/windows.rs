// screenpipe — AI that knows everything you've seen, said, and heard
// https://screenpipe.com
use super::{Action, Backend};
use crate::{
    observation::{Limits, Node, Rect, Snapshot},
    Error, ErrorCode, Result,
};
use std::collections::{HashMap, HashSet};
use windows::{
    core::{Interface, BSTR},
    Win32::{
        System::{
            Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
            Ole::*,
        },
        UI::Accessibility::*,
    },
};

pub struct NativeBackend {
    automation: IUIAutomation,
    root: IUIAutomationElement,
    cache: IUIAutomationCacheRequest,
    handles: HashMap<String, IUIAutomationElement>,
    target: String,
}
fn error(e: windows::core::Error) -> Error {
    Error::action_failed("UI Automation", &e.to_string())
}
fn identity(el: &IUIAutomationElement) -> Result<String> {
    unsafe {
        let array = el.GetRuntimeId().map_err(error)?;
        if array.is_null() {
            return Err(Error::action_failed(
                "observe",
                "UIA runtime identity unavailable",
            ));
        }
        let result = (|| {
            let low = SafeArrayGetLBound(array, 1).map_err(error)?;
            let high = SafeArrayGetUBound(array, 1).map_err(error)?;
            if high - low > 128 {
                return Err(Error::action_failed(
                    "observe",
                    "Invalid UIA identity length",
                ));
            }
            let mut values = Vec::new();
            for index in low..=high {
                let mut v = 0i32;
                SafeArrayGetElement(array, &index, &mut v as *mut _ as _).map_err(error)?;
                values.push(v.to_string());
            }
            Ok(format!(
                "{}:{}",
                el.CurrentProcessId().map_err(error)?,
                values.join(".")
            ))
        })();
        let _ = SafeArrayDestroy(array);
        result
    }
}
impl NativeBackend {
    pub fn new(app: &str) -> Result<Self> {
        crate::platform::windows::init_com()?;
        unsafe {
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).map_err(error)?;
            if let Ok(options) = automation.cast::<IUIAutomation2>() {
                options.SetConnectionTimeout(500).map_err(error)?;
                options.SetTransactionTimeout(500).map_err(error)?;
            }
            let candidates = crate::platform::windows::get_windows()?;
            let pid = app.strip_prefix("pid:").and_then(|s| s.parse::<i32>().ok());
            let mut matches: Vec<_> = candidates
                .into_iter()
                .filter(|e| {
                    pid.map(|p| e.process_id() == p).unwrap_or_else(|| {
                        e.name()
                            .unwrap_or_default()
                            .to_lowercase()
                            .contains(&app.to_lowercase())
                    })
                })
                .collect();
            if matches.len() != 1 {
                return Err(if matches.is_empty() {
                    Error::app_not_running(app)
                } else {
                    Error::multiple_matches(app, matches.len())
                });
            }
            let root = matches.remove(0).raw().clone();
            let target = format!("windows:{}", identity(&root)?);
            let cache = automation.CreateCacheRequest().map_err(error)?;
            cache.SetTreeScope(TreeScope_Element).map_err(error)?;
            for prop in [
                UIA_NamePropertyId,
                UIA_ControlTypePropertyId,
                UIA_BoundingRectanglePropertyId,
                UIA_IsEnabledPropertyId,
                UIA_HasKeyboardFocusPropertyId,
                UIA_IsPasswordPropertyId,
            ] {
                cache.AddProperty(prop).map_err(error)?;
            }
            Ok(Self {
                automation,
                root,
                cache,
                handles: HashMap::new(),
                target,
            })
        }
    }
}
impl Backend for NativeBackend {
    fn capture(&mut self, limits: Limits) -> Result<Snapshot> {
        unsafe {
            let walker = self.automation.ControlViewWalker().map_err(error)?;
            let root = self.root.BuildUpdatedCache(&self.cache).map_err(error)?;
            let mut pending = vec![(root, None, 0)];
            let mut nodes = Vec::new();
            let mut handles = HashMap::new();
            let mut seen = HashSet::new();
            let mut warnings = Vec::new();
            let started = std::time::Instant::now();
            while let Some((el, parent, depth)) = pending.pop() {
                if started.elapsed() > std::time::Duration::from_secs(5) {
                    warnings.push("capture time limit reached".into());
                    break;
                }
                if nodes.len() >= limits.max_nodes {
                    warnings.push("node limit reached".into());
                    break;
                }
                let key = identity(&el)?;
                if !seen.insert(key.clone()) {
                    warnings.push("cycle skipped".into());
                    continue;
                }
                let protected = el.CachedIsPassword().map_err(error)?.as_bool();
                let mut actions = Vec::new();
                let mut value = String::new();
                if el
                    .GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
                    .is_ok()
                {
                    actions.push("click".into());
                }
                if let Ok(pattern) =
                    el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                {
                    if !pattern.CurrentIsReadOnly().map_err(error)?.as_bool() {
                        actions.push("set_value".into());
                    }
                    if !protected {
                        value = pattern.CurrentValue().map_err(error)?.to_string();
                    }
                }
                let mut states = Vec::new();
                if !el.CachedIsEnabled().map_err(error)?.as_bool() {
                    states.push("disabled".into());
                }
                if el.CachedHasKeyboardFocus().map_err(error)?.as_bool() {
                    states.push("focused".into());
                }
                if let Ok(p) = el.GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(
                    UIA_ExpandCollapsePatternId,
                ) {
                    let state = p.CurrentExpandCollapseState().map_err(error)?;
                    if state == ExpandCollapseState_Collapsed {
                        states.push("collapsed".into());
                        actions.push("expand".into());
                    }
                    if state == ExpandCollapseState_Expanded {
                        states.push("expanded".into());
                        actions.push("collapse".into());
                    }
                }
                actions.sort();
                states.sort();
                let b = el.CachedBoundingRectangle().map_err(error)?;
                nodes.push(Node {
                    key: key.clone(),
                    parent,
                    role: role(el.CachedControlType().map_err(error)?.0).into(),
                    name: el.CachedName().map_err(error)?.to_string(),
                    value,
                    states,
                    actions,
                    bounds: Some(Rect {
                        x: b.left as f64,
                        y: b.top as f64,
                        width: (b.right - b.left) as f64,
                        height: (b.bottom - b.top) as f64,
                    }),
                    protected,
                });
                let mut child = walker.GetFirstChildElementBuildCache(&el, &self.cache).ok();
                if depth >= limits.max_depth {
                    if child.is_some() {
                        warnings.push("depth limit reached".into());
                    }
                } else {
                    let mut children = Vec::new();
                    let mut sibling_ids = HashSet::new();
                    while let Some(c) = child {
                        if started.elapsed() > std::time::Duration::from_secs(5) {
                            warnings.push("capture time limit reached".into());
                            break;
                        }
                        if children.len() >= limits.max_children {
                            warnings.push("child limit reached".into());
                            break;
                        }
                        if !sibling_ids.insert(identity(&c)?) {
                            warnings.push("sibling cycle skipped".into());
                            break;
                        }
                        child = walker.GetNextSiblingElementBuildCache(&c, &self.cache).ok();
                        children.push(c);
                    }
                    for c in children.into_iter().rev() {
                        pending.push((c, Some(key.clone()), depth + 1));
                    }
                }
                handles.insert(key, el);
            }
            self.handles = handles;
            warnings.sort();
            warnings.dedup();
            Ok(Snapshot {
                target: self.target.clone(),
                source: "windows_uia".into(),
                nodes,
                warnings,
            })
        }
    }
    fn act(&mut self, key: &str, action: &Action) -> Result<()> {
        let el = self
            .handles
            .get(key)
            .ok_or_else(|| Error::element_not_found(key))?;
        unsafe {
            if !el.CurrentIsEnabled().map_err(error)?.as_bool() {
                return Err(Error::new(ErrorCode::ActionFailed, "Element is disabled"));
            }
            match action {
                Action::Click => el
                    .GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
                    .map_err(error)?
                    .Invoke()
                    .map_err(error),
                Action::SetValue { value } => el
                    .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                    .map_err(error)?
                    .SetValue(&BSTR::from(value))
                    .map_err(error),
                Action::Expand => el
                    .GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(
                        UIA_ExpandCollapsePatternId,
                    )
                    .map_err(error)?
                    .Expand()
                    .map_err(error),
                Action::Collapse => el
                    .GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(
                        UIA_ExpandCollapsePatternId,
                    )
                    .map_err(error)?
                    .Collapse()
                    .map_err(error),
            }
        }
    }
}
fn role(id: i32) -> &'static str {
    match id {
        50000 => "button",
        50002 => "checkbox",
        50003 => "combobox",
        50004 => "textbox",
        50005 => "link",
        50006 => "image",
        50007 => "listitem",
        50008 => "list",
        50009 => "menu",
        50010 => "menubar",
        50011 => "menuitem",
        50012 => "progressbar",
        50013 => "radio",
        50014 => "scrollbar",
        50015 => "slider",
        50018 => "tablist",
        50019 => "tab",
        50020 => "text",
        50021 => "toolbar",
        50023 => "tree",
        50024 => "treeitem",
        50026 => "group",
        50028 => "grid",
        50029 => "cell",
        50030 => "document",
        50031 => "button",
        50032 => "window",
        50033 => "pane",
        50036 => "table",
        _ => "element",
    }
}
