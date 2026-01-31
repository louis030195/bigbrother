//! Windows UI Automation wrapper
//!
//! Provides access to the Windows accessibility tree.

use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTreeWalker,
    TreeScope_Children, TreeScope_Subtree, UIA_NamePropertyId, UIA_ControlTypePropertyId,
    UIA_BoundingRectanglePropertyId, UIA_ProcessIdPropertyId,
};
use windows::Win32::System::Com::CoCreateInstance;
use windows::Win32::System::Com::CLSCTX_INPROC_SERVER;
use windows::core::BSTR;

use crate::{Error, ErrorCode, Result};

/// Windows UI Automation instance
pub struct Automation {
    inner: IUIAutomation,
}

impl Automation {
    /// Create a new UI Automation instance
    pub fn new() -> Result<Self> {
        super::init_com()?;

        let automation: IUIAutomation = unsafe {
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| Error::new(ErrorCode::Unknown, format!("Failed to create UIAutomation: {:?}", e)))?
        };

        Ok(Self { inner: automation })
    }

    /// Get the root element (desktop)
    pub fn root(&self) -> Result<Element> {
        let root = unsafe {
            self.inner.GetRootElement()
                .map_err(|e| Error::new(ErrorCode::Unknown, format!("Failed to get root: {:?}", e)))?
        };
        Ok(Element { inner: root })
    }

    /// Get the focused element
    pub fn focused(&self) -> Result<Element> {
        let focused = unsafe {
            self.inner.GetFocusedElement()
                .map_err(|e| Error::new(ErrorCode::Unknown, format!("Failed to get focused: {:?}", e)))?
        };
        Ok(Element { inner: focused })
    }

    /// Get element at point
    pub fn element_at(&self, x: i32, y: i32) -> Result<Element> {
        use windows::Win32::Foundation::POINT;

        let point = POINT { x, y };
        let element = unsafe {
            self.inner.ElementFromPoint(point)
                .map_err(|e| Error::new(ErrorCode::Unknown, format!("Failed to get element at point: {:?}", e)))?
        };
        Ok(Element { inner: element })
    }

    /// Get the tree walker for traversing elements
    pub fn tree_walker(&self) -> Result<TreeWalker> {
        let walker = unsafe {
            self.inner.ControlViewWalker()
                .map_err(|e| Error::new(ErrorCode::Unknown, format!("Failed to get tree walker: {:?}", e)))?
        };
        Ok(TreeWalker { inner: walker })
    }
}

/// A UI element
pub struct Element {
    inner: IUIAutomationElement,
}

impl Element {
    /// Get the element's name
    pub fn name(&self) -> Option<String> {
        unsafe {
            self.inner.CurrentName().ok().map(|s| s.to_string())
        }
    }

    /// Get the control type ID
    pub fn control_type(&self) -> i32 {
        unsafe {
            self.inner.CurrentControlType().unwrap_or(0)
        }
    }

    /// Get the control type as a string
    pub fn control_type_name(&self) -> &'static str {
        match self.control_type() {
            50000 => "Button",
            50001 => "Calendar",
            50002 => "CheckBox",
            50003 => "ComboBox",
            50004 => "Edit",
            50005 => "Hyperlink",
            50006 => "Image",
            50007 => "ListItem",
            50008 => "List",
            50009 => "Menu",
            50010 => "MenuBar",
            50011 => "MenuItem",
            50012 => "ProgressBar",
            50013 => "RadioButton",
            50014 => "ScrollBar",
            50015 => "Slider",
            50016 => "Spinner",
            50017 => "StatusBar",
            50018 => "Tab",
            50019 => "TabItem",
            50020 => "Text",
            50021 => "ToolBar",
            50022 => "ToolTip",
            50023 => "Tree",
            50024 => "TreeItem",
            50025 => "Custom",
            50026 => "Group",
            50027 => "Thumb",
            50028 => "DataGrid",
            50029 => "DataItem",
            50030 => "Document",
            50031 => "SplitButton",
            50032 => "Window",
            50033 => "Pane",
            50034 => "Header",
            50035 => "HeaderItem",
            50036 => "Table",
            50037 => "TitleBar",
            50038 => "Separator",
            _ => "Unknown",
        }
    }

    /// Get the bounding rectangle
    pub fn bounds(&self) -> Option<(i32, i32, i32, i32)> {
        unsafe {
            self.inner.CurrentBoundingRectangle().ok().map(|r| {
                (r.left, r.top, r.right - r.left, r.bottom - r.top)
            })
        }
    }

    /// Get the process ID
    pub fn process_id(&self) -> i32 {
        unsafe {
            self.inner.CurrentProcessId().unwrap_or(0)
        }
    }

    /// Get the automation ID
    pub fn automation_id(&self) -> Option<String> {
        unsafe {
            self.inner.CurrentAutomationId().ok().map(|s| s.to_string())
        }
    }

    /// Get the class name
    pub fn class_name(&self) -> Option<String> {
        unsafe {
            self.inner.CurrentClassName().ok().map(|s| s.to_string())
        }
    }

    /// Check if element is enabled
    pub fn is_enabled(&self) -> bool {
        unsafe {
            self.inner.CurrentIsEnabled().unwrap_or(false.into()).as_bool()
        }
    }

    /// Check if element is offscreen
    pub fn is_offscreen(&self) -> bool {
        unsafe {
            self.inner.CurrentIsOffscreen().unwrap_or(true.into()).as_bool()
        }
    }

    /// Get the clickable point
    pub fn clickable_point(&self) -> Option<(i32, i32)> {
        use windows::Win32::Foundation::POINT;

        let mut point = POINT::default();
        let mut got_point = false.into();

        unsafe {
            if self.inner.GetClickablePoint(&mut point, &mut got_point).is_ok() && got_point.as_bool() {
                Some((point.x, point.y))
            } else {
                // Fallback to center of bounding rect
                self.bounds().map(|(x, y, w, h)| (x + w / 2, y + h / 2))
            }
        }
    }

    /// Get the inner IUIAutomationElement (for advanced usage)
    pub fn raw(&self) -> &IUIAutomationElement {
        &self.inner
    }
}

/// Tree walker for traversing the UI tree
pub struct TreeWalker {
    inner: IUIAutomationTreeWalker,
}

impl TreeWalker {
    /// Get the first child of an element
    pub fn first_child(&self, element: &Element) -> Option<Element> {
        unsafe {
            self.inner.GetFirstChildElement(&element.inner)
                .ok()
                .map(|e| Element { inner: e })
        }
    }

    /// Get the next sibling of an element
    pub fn next_sibling(&self, element: &Element) -> Option<Element> {
        unsafe {
            self.inner.GetNextSiblingElement(&element.inner)
                .ok()
                .map(|e| Element { inner: e })
        }
    }

    /// Get the parent of an element
    pub fn parent(&self, element: &Element) -> Option<Element> {
        unsafe {
            self.inner.GetParentElement(&element.inner)
                .ok()
                .map(|e| Element { inner: e })
        }
    }
}

/// Get all running windows
pub fn get_windows() -> Result<Vec<Element>> {
    let automation = Automation::new()?;
    let root = automation.root()?;
    let walker = automation.tree_walker()?;

    let mut windows = Vec::new();
    let mut child = walker.first_child(&root);

    while let Some(element) = child {
        if element.control_type() == 50032 {
            // Window control type
            windows.push(element);
        }
        child = walker.next_sibling(&windows.last().unwrap_or(&root));
    }

    Ok(windows)
}

/// Find a window by name (partial match)
pub fn find_window(name: &str) -> Result<Option<Element>> {
    let automation = Automation::new()?;
    let root = automation.root()?;
    let walker = automation.tree_walker()?;

    let name_lower = name.to_lowercase();
    let mut child = walker.first_child(&root);

    while let Some(element) = child {
        if let Some(window_name) = element.name() {
            if window_name.to_lowercase().contains(&name_lower) {
                return Ok(Some(element));
            }
        }
        child = walker.next_sibling(&element);
    }

    Ok(None)
}
