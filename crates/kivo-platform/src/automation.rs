//! UI Automation and simulated input (TOOLS_AND_CONTROL §4, §8).

use crate::error::PlatformResult;
use crate::types::{Point, Rect, WindowId};
use serde::{Deserialize, Serialize};

/// A reference to a UI element, valid while the element exists.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ElementRef(pub String);

/// How to find an element: any combination; empty fields match anything.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementQuery {
    pub window: Option<WindowId>,
    /// Matched fuzzily against the element's accessible name.
    pub name: Option<String>,
    /// The control type, e.g. "Button", "Edit", "ListItem".
    pub role: Option<String>,
    pub automation_id: Option<String>,
}

/// One node of a pruned, depth-limited element tree, compact enough to give a model.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiNode {
    pub element: ElementRef,
    pub role: String,
    pub name: String,
    pub bounds: Option<Rect>,
    /// Password fields are never typed into (SECURITY §1.1 hard limit).
    pub is_password: bool,
    pub children: Vec<UiNode>,
}

pub trait UiAutomation: Send + Sync {
    fn find(&self, query: &ElementQuery) -> PlatformResult<Vec<ElementRef>>;
    fn tree(&self, window: WindowId, max_depth: u8) -> PlatformResult<UiNode>;
    fn invoke(&self, element: &ElementRef) -> PlatformResult<()>;
    fn set_value(&self, element: &ElementRef, value: &str) -> PlatformResult<()>;
    fn bounds(&self, element: &ElementRef) -> PlatformResult<Rect>;
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
