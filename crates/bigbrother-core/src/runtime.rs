// screenpipe — AI that knows everything you've seen, said, and heard
// https://screenpipe.com
//! Session-owned observations and actions. Native handles never cross the protocol.
use crate::observation::{Limits, Node, Observation, Renderer, Snapshot};
use crate::{Error, ErrorCode, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    Click,
    SetValue { value: String },
    Expand,
    Collapse,
}
impl Action {
    pub fn name(&self) -> &str {
        match self {
            Self::Click => "click",
            Self::SetValue { .. } => "set_value",
            Self::Expand => "expand",
            Self::Collapse => "collapse",
        }
    }
}

pub trait Backend {
    fn capture(&mut self, limits: Limits) -> Result<Snapshot>;
    fn act(&mut self, key: &str, action: &Action) -> Result<()>;
}

pub struct Runtime<B: Backend> {
    backend: B,
    renderer: Renderer,
}
impl<B: Backend> Runtime<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            renderer: Renderer::default(),
        }
    }
    pub fn observe(&mut self, limits: Limits, full: bool) -> Result<Observation> {
        limits.validate()?;
        self.renderer.invalidate();
        let snapshot = self.backend.capture(limits)?;
        self.renderer.render(snapshot, limits, full)
    }
    pub fn act(&mut self, revision: u64, reference: &str, action: Action) -> Result<()> {
        let expected: Node = self.renderer.resolve(revision, reference)?.clone();
        self.renderer.invalidate();
        if !expected.actions.iter().any(|a| a == action.name()) {
            return Err(Error::action_failed(
                action.name(),
                "Action was not advertised for this element",
            ));
        }
        let current = self.backend.capture(self.renderer.limits())?;
        // Re-resolve against a fresh provider snapshot. A recycled index, moved control,
        // changed label/value or replaced window cannot silently redirect an action.
        if !current
            .nodes
            .iter()
            .any(|n| n.key == expected.key && equivalent(n, &expected))
        {
            return Err(Error::new(
                ErrorCode::StaleObservation,
                "Element changed or disappeared; observe again",
            ));
        }
        self.backend.act(&expected.key, &action)
    }
}
fn equivalent(a: &Node, b: &Node) -> bool {
    a.role == b.role
        && a.name == b.name
        && (a.protected || a.value == b.value)
        && a.bounds == b.bounds
        && a.protected == b.protected
        && a.actions == b.actions
        && a.states
            .iter()
            .filter(|s| s.as_str() != "protected")
            .eq(b.states.iter().filter(|s| s.as_str() != "protected"))
}

#[cfg(target_os = "macos")]
#[path = "runtime/macos.rs"]
mod native;
#[cfg(target_os = "windows")]
#[path = "runtime/windows.rs"]
mod native;
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use native::NativeBackend;

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};
    struct Fake {
        node: Rc<RefCell<Node>>,
        actions: Rc<RefCell<usize>>,
    }
    impl Backend for Fake {
        fn capture(&mut self, _: Limits) -> Result<Snapshot> {
            Ok(Snapshot {
                target: "test".into(),
                source: "fake".into(),
                nodes: vec![self.node.borrow().clone()],
                warnings: vec![],
            })
        }
        fn act(&mut self, _: &str, _: &Action) -> Result<()> {
            *self.actions.borrow_mut() += 1;
            Ok(())
        }
    }
    #[test]
    fn changed_target_and_consumed_revision_never_act() {
        let node = Rc::new(RefCell::new(Node {
            key: "1".into(),
            parent: None,
            role: "button".into(),
            name: "Save".into(),
            value: String::new(),
            states: vec![],
            actions: vec!["click".into()],
            bounds: None,
            protected: false,
        }));
        let actions = Rc::new(RefCell::new(0));
        let mut runtime = Runtime::new(Fake {
            node: node.clone(),
            actions: actions.clone(),
        });
        runtime.observe(Limits::default(), false).unwrap();
        node.borrow_mut().name = "Delete".into();
        assert!(runtime.act(1, "e1", Action::Click).is_err());
        assert_eq!(*actions.borrow(), 0);
        runtime.observe(Limits::default(), false).unwrap();
        runtime.act(2, "e1", Action::Click).unwrap();
        assert!(runtime.act(2, "e1", Action::Click).is_err());
        assert_eq!(*actions.borrow(), 1);
    }
}
