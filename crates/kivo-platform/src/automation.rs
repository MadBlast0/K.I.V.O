//! UI Automation and simulated input (TOOLS_AND_CONTROL §4, §8).

use crate::error::PlatformResult;
use crate::types::{Point, Rect, WindowId};
use serde::{Deserialize, Serialize};

/// A reference to a UI element, valid while the element exists. It starts with the top-level
/// window it lives in (`"<window>:<runtime id>"`), so a caller can check that window's app against
/// the allow and block lists before acting (CAP-07).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct ElementRef(pub String);

impl ElementRef {
    pub fn new(window: WindowId, runtime_id: &[i32]) -> Self {
        let id: Vec<String> = runtime_id.iter().map(i32::to_string).collect();
        Self(format!("{}:{}", window.0, id.join(".")))
    }

    /// The top-level window this element belongs to.
    pub fn window(&self) -> Option<WindowId> {
        self.0.split_once(':')?.0.parse().ok().map(WindowId)
    }
}

/// How to find an element: any combination; empty fields match anything.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementQuery {
    /// The window to search; the foreground window when absent (never the whole desktop).
    pub window: Option<WindowId>,
    /// Matched fuzzily against the element's accessible name.
    pub name: Option<String>,
    /// The control type, e.g. "Button", "Edit", "ListItem".
    pub role: Option<String>,
    pub automation_id: Option<String>,
}

/// What can be done to an element, from the UIA patterns it supports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UiAction {
    Invoke,
    SetValue,
    Toggle,
    Select,
    Expand,
    ScrollIntoView,
}

/// One node of a pruned, depth-limited element tree, compact enough to give a model.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiNode {
    pub element: ElementRef,
    pub role: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub automation_id: String,
    /// The current value (text fields, combo boxes). Never read from password fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// On/off for check boxes and toggle buttons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toggled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expanded: Option<bool>,
    pub enabled: bool,
    pub bounds: Option<Rect>,
    /// Password fields are never typed into (SECURITY §1.1 hard limit).
    pub is_password: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<UiAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<UiNode>,
}

impl UiNode {
    /// How many nodes this subtree holds.
    pub fn count(&self) -> usize {
        1 + self.children.iter().map(Self::count).sum::<usize>()
    }
}

/// UI events KIVO can subscribe to on demand (TOOL-21). Nothing is subscribed at idle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UiEventKind {
    FocusChanged,
    /// Children added, removed or replaced; always scoped to one window.
    StructureChanged,
    WindowOpened,
    WindowClosed,
    /// Name, value or toggle state changes; always scoped to one window.
    PropertyChanged,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSubscription {
    pub kind: UiEventKind,
    /// Required for the scoped kinds (StructureChanged, PropertyChanged); optional for the rest.
    pub window: Option<WindowId>,
}

/// A UI event as it arrives. Names and values are the app's content: `Untrusted`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum UiEvent {
    FocusChanged {
        element: ElementRef,
        role: String,
        name: String,
        is_password: bool,
    },
    StructureChanged {
        window: WindowId,
    },
    WindowOpened {
        element: ElementRef,
        name: String,
    },
    WindowClosed {
        element: Option<ElementRef>,
    },
    PropertyChanged {
        element: ElementRef,
        property: String,
        value: String,
    },
}

pub type UiEventSink = Box<dyn Fn(UiEvent) + Send + Sync>;

pub trait UiAutomation: Send + Sync {
    /// Elements matching `query`, best name match first, at most `limit`.
    fn find(&self, query: &ElementQuery, limit: usize) -> PlatformResult<Vec<UiNode>>;
    /// The window's element tree, depth-limited and pruned (no unnamed, empty containers), with
    /// at most `max_nodes` nodes.
    fn tree(&self, window: WindowId, max_depth: u8, max_nodes: usize) -> PlatformResult<UiNode>;
    /// The element with keyboard focus.
    fn focused(&self) -> PlatformResult<Option<UiNode>>;
    /// The text selected in the focused element (UIA TextPattern), if any. Never from a password
    /// field.
    fn selected_text(&self) -> PlatformResult<Option<String>>;
    /// A fresh look at one element (its state now).
    fn describe(&self, element: &ElementRef) -> PlatformResult<UiNode>;
    fn invoke(&self, element: &ElementRef) -> PlatformResult<()>;
    /// Refuses password fields (`AccessDenied`).
    fn set_value(&self, element: &ElementRef, value: &str) -> PlatformResult<()>;
    /// Toggles and returns the new state (on = true).
    fn toggle(&self, element: &ElementRef) -> PlatformResult<bool>;
    fn select(&self, element: &ElementRef) -> PlatformResult<()>;
    /// Expands (`true`) or collapses (`false`).
    fn expand(&self, element: &ElementRef, expand: bool) -> PlatformResult<()>;
    fn scroll_into_view(&self, element: &ElementRef) -> PlatformResult<()>;
    fn bounds(&self, element: &ElementRef) -> PlatformResult<Rect>;
    /// Starts delivering `subscription`'s events to `sink`; returns an id for `unsubscribe`.
    fn subscribe(&self, subscription: &UiSubscription, sink: UiEventSink) -> PlatformResult<u64>;
    fn unsubscribe(&self, id: u64) -> PlatformResult<()>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Simulated mouse and keyboard: the last tier of the capability ladder.
pub trait Input: Send + Sync {
    fn click(&self, at: Point, button: MouseButton, count: u8) -> PlatformResult<()>;
    fn type_text(&self, text: &str) -> PlatformResult<()>;
    /// Presses a key combination, e.g. `["Ctrl", "S"]`.
    fn press(&self, keys: &[String]) -> PlatformResult<()>;
    fn scroll(&self, at: Point, delta_y: i32) -> PlatformResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_refs_carry_their_window() {
        let r = ElementRef::new(WindowId(0x1234), &[42, 7, -3]);
        assert_eq!(r.0, "4660:42.7.-3");
        assert_eq!(r.window(), Some(WindowId(0x1234)));
        assert_eq!(ElementRef("junk".into()).window(), None);
    }

    #[test]
    fn nodes_count_their_subtree() {
        let leaf = UiNode::default();
        let node = UiNode {
            children: vec![
                leaf.clone(),
                UiNode {
                    children: vec![leaf],
                    ..UiNode::default()
                },
            ],
            ..UiNode::default()
        };
        assert_eq!(node.count(), 4);
    }
}
