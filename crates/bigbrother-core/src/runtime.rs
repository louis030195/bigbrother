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
    target: Option<String>,
}
impl<B: Backend> Runtime<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            renderer: Renderer::default(),
            target: None,
        }
    }
    pub fn observe(&mut self, limits: Limits, full: bool) -> Result<Observation> {
        limits.validate()?;
        self.renderer.invalidate();
        let snapshot = self.backend.capture(limits)?;
        let observation = self.renderer.render(snapshot, limits, full)?;
        self.target = Some(observation.target.clone());
        Ok(observation)
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
        if self.target.as_deref() != Some(current.target.as_str())
            || !current
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
    a.parent == b.parent
        && a.role == b.role
        && a.name == b.name
        && (a.protected || a.value == b.value)
        && a.bounds == b.bounds
        && a.protected == b.protected
        && canonical(&a.actions, false) == canonical(&b.actions, false)
        && canonical(&a.states, true) == canonical(&b.states, true)
}
fn canonical(values: &[String], omit_protected: bool) -> Vec<&str> {
    let mut values: Vec<_> = values
        .iter()
        .map(String::as_str)
        .filter(|s| !omit_protected || *s != "protected")
        .collect();
    values.sort_unstable();
    values.dedup();
    values
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
        target: Rc<RefCell<String>>,
    }
    impl Backend for Fake {
        fn capture(&mut self, _: Limits) -> Result<Snapshot> {
            Ok(Snapshot {
                target: self.target.borrow().clone(),
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
            target: Rc::new(RefCell::new("test".into())),
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
    fn fixture() -> (
        Runtime<Fake>,
        Rc<RefCell<Node>>,
        Rc<RefCell<usize>>,
        Rc<RefCell<String>>,
    ) {
        let node = Rc::new(RefCell::new(Node {
            key: "button".into(),
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
        let target = Rc::new(RefCell::new("window-a".into()));
        (
            Runtime::new(Fake {
                node: node.clone(),
                actions: actions.clone(),
                target: target.clone(),
            }),
            node,
            actions,
            target,
        )
    }
    #[test]
    fn replaced_target_cannot_reuse_an_element_identity() {
        let (mut runtime, _, actions, target) = fixture();
        let observation = runtime.observe(Limits::default(), false).unwrap();
        *target.borrow_mut() = "window-b".into();
        let error = runtime
            .act(observation.revision, "e1", Action::Click)
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::StaleObservation);
        assert_eq!(*actions.borrow(), 0);
    }
    #[test]
    fn reparented_control_requires_another_observation() {
        let (mut runtime, node, actions, _) = fixture();
        node.borrow_mut().parent = Some("group-a".into());
        let observation = runtime.observe(Limits::default(), false).unwrap();
        // Flattened/unknown parents must still participate in action validation.
        node.borrow_mut().parent = Some("group-b".into());
        let error = runtime
            .act(observation.revision, "e1", Action::Click)
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::StaleObservation);
        assert_eq!(*actions.borrow(), 0);
        let fresh = runtime.observe(Limits::default(), false).unwrap();
        runtime.act(fresh.revision, "e1", Action::Click).unwrap();
        assert_eq!(*actions.borrow(), 1);
    }
    #[test]
    fn provider_state_order_and_duplicates_do_not_make_a_control_stale() {
        let (mut runtime, node, actions, _) = fixture();
        node.borrow_mut().states = vec!["selected".into(), "focused".into(), "focused".into()];
        node.borrow_mut().actions = vec!["set_value".into(), "click".into(), "click".into()];
        let observation = runtime.observe(Limits::default(), false).unwrap();
        runtime
            .act(observation.revision, "e1", Action::Click)
            .unwrap();
        assert_eq!(*actions.borrow(), 1);
    }
}
