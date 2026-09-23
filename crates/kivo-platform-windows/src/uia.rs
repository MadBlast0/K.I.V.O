//! UI Automation (TOOLS_AND_CONTROL §4, TOOL-19–22): one dedicated thread in the COM
//! multithreaded apartment owns the `IUIAutomation` client and every element KIVO holds. Callers
//! send it jobs and wait for the answer with a deadline, so a hung app can't hang the runtime.
//!
//! - Element references are `"<window>:<runtime id>"`. Recently seen elements are cached; an
//!   evicted one is found again by searching its window for the runtime id.
//! - Trees are walked one level per round trip with the properties cached in bulk, depth-limited
//!   and pruned (unnamed containers with nothing inside are dropped).
//! - Windows that run as administrator are refused up front with `Elevated`: UIPI would only
//!   show their frame, and KIVO never elevates itself (TOOL-22).
//! - Events are subscribed only on request and removed on `unsubscribe`; there is no polling.

use crate::com::Com;
use kivo_platform::{
    ElementQuery, ElementRef, PlatformError, PlatformResult, Rect, UiAction, UiAutomation, UiEvent,
    UiEventKind, UiEventSink, UiNode, UiSubscription, WindowId,
};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::Duration;
use uiautomation::UIElement;
use uiautomation::core::{UIAutomation, UICacheRequest, UICondition};
use uiautomation::events::{
    UIEventHandler, UIEventType, UIFocusChangedEventHandler, UIPropertyChangedEventHandler,
    UIStructureChangeEventHandler,
};
use uiautomation::patterns::{
    UIExpandCollapsePattern, UIInvokePattern, UIScrollItemPattern, UISelectionItemPattern,
    UITogglePattern, UIValuePattern,
};
use uiautomation::types::{ExpandCollapseState, Handle, TreeScope, UIProperty};
use uiautomation::variants::Variant;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GetAncestor, GetWindowTextW, GetWindowThreadProcessId, IsWindow,
};

/// How long a caller waits for the UIA thread (a hung app answers with `Timeout`).
const DEADLINE: Duration = Duration::from_secs(12);
/// UIA's own limits for talking to a provider (IUIAutomation2).
const CONNECTION_TIMEOUT_MS: u32 = 2_000;
const TRANSACTION_TIMEOUT_MS: u32 = 8_000;
/// Elements kept for quick lookup.
const CACHE: usize = 4_096;
/// Longest text returned for a value (a big document isn't a tree node).
const MAX_VALUE: usize = 200;

type Job = Box<dyn FnOnce(&mut Worker) + Send>;

/// The Windows `UiAutomation`: a handle to the UIA thread.
pub struct WindowsUiAutomation {
    jobs: mpsc::Sender<Job>,
    next_sub: AtomicU64,
}

impl WindowsUiAutomation {
    /// Starts the UIA thread. Fails if UI Automation isn't available.
    pub fn new() -> PlatformResult<Self> {
        let (jobs, rx) = mpsc::channel::<Job>();
        let (ready_tx, ready_rx) = mpsc::sync_channel::<PlatformResult<()>>(1);
        std::thread::Builder::new()
            .name("kivo-uia".into())
            .spawn(move || {
                let com = match Com::init() {
                    Ok(com) => com,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let mut worker = match Worker::new() {
                    Ok(w) => w,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(()));
                while let Ok(job) = rx.recv() {
                    job(&mut worker);
                }
                worker.remove_all();
                drop(worker);
                drop(com);
            })
            .map_err(|e| PlatformError::Os {
                code: 0,
                message: e.to_string(),
            })?;
        ready_rx.recv().map_err(|_| PlatformError::Unsupported)??;
        Ok(Self {
            jobs,
            next_sub: AtomicU64::new(1),
        })
    }

    /// Runs `f` on the UIA thread and waits for its answer.
    fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Worker) -> PlatformResult<T> + Send + 'static,
    ) -> PlatformResult<T> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.jobs
            .send(Box::new(move |w: &mut Worker| {
                let _ = tx.send(f(w));
            }))
            .map_err(|_| PlatformError::Unsupported)?;
        match rx.recv_timeout(DEADLINE) {
            Ok(result) => result,
            Err(_) => Err(PlatformError::Timeout),
        }
    }
}

impl UiAutomation for WindowsUiAutomation {
    fn find(&self, query: &ElementQuery, limit: usize) -> PlatformResult<Vec<UiNode>> {
        let query = query.clone();
        self.call(move |w| w.find(&query, limit))
    }

    fn tree(&self, window: WindowId, max_depth: u8, max_nodes: usize) -> PlatformResult<UiNode> {
        self.call(move |w| w.tree(window, max_depth, max_nodes))
    }

    fn focused(&self) -> PlatformResult<Option<UiNode>> {
        self.call(Worker::focused)
    }

    fn selected_text(&self) -> PlatformResult<Option<String>> {
        self.call(|w| {
            let Ok(el) = w.auto.get_focused_element() else {
                return Ok(None);
            };
            if let Some(window) = top_window(&w.auto, &el)
                && !w.self_elevated
                && window_elevated(hwnd(window))
            {
                return Err(PlatformError::Elevated(window_title(hwnd(window))));
            }
            if el.is_password().unwrap_or(true) {
                return Ok(None);
            }
            let Ok(pattern) = el.get_pattern::<uiautomation::patterns::UITextPattern>() else {
                return Ok(None);
            };
            let ranges = pattern.get_selection().map_err(|e| uia_error(&e))?;
            let text: Vec<String> = ranges
                .iter()
                .filter_map(|r| r.get_text(20_000).ok())
                .filter(|t| !t.is_empty())
                .collect();
            Ok((!text.is_empty()).then(|| text.join("\n")))
        })
    }

    fn describe(&self, element: &ElementRef) -> PlatformResult<UiNode> {
        let element = element.clone();
        self.call(move |w| {
            let el = w.resolve(&element)?;
            w.node_live(&el, window_or_zero(&element))
        })
    }

    fn invoke(&self, element: &ElementRef) -> PlatformResult<()> {
        let element = element.clone();
        self.call(move |w| {
            let el = w.resolve(&element)?;
            let p: UIInvokePattern = el.get_pattern().map_err(|_| PlatformError::Unsupported)?;
            p.invoke().map_err(|e| uia_error(&e))
        })
    }

    fn set_value(&self, element: &ElementRef, value: &str) -> PlatformResult<()> {
        let element = element.clone();
        let value = value.to_owned();
        self.call(move |w| {
            let el = w.resolve(&element)?;
            // A hard limit (SECURITY §1.1): never type into a password field.
            if el.is_password().unwrap_or(true) {
                return Err(PlatformError::AccessDenied);
            }
            let p: UIValuePattern = el.get_pattern().map_err(|_| PlatformError::Unsupported)?;
            p.set_value(&value).map_err(|e| uia_error(&e))
        })
    }

    fn toggle(&self, element: &ElementRef) -> PlatformResult<bool> {
        let element = element.clone();
        self.call(move |w| {
            let el = w.resolve(&element)?;
            let p: UITogglePattern = el.get_pattern().map_err(|_| PlatformError::Unsupported)?;
            let read = || -> PlatformResult<bool> {
                Ok(p.get_toggle_state().map_err(|e| uia_error(&e))? as i32 == 1)
            };
            let before = read()?;
            p.toggle().map_err(|e| uia_error(&e))?;
            // Win32 check boxes apply the click asynchronously: wait briefly for the new state.
            for _ in 0..20 {
                let now = read()?;
                if now != before {
                    return Ok(now);
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            read()
        })
    }

    fn select(&self, element: &ElementRef) -> PlatformResult<()> {
        let element = element.clone();
        self.call(move |w| {
            let el = w.resolve(&element)?;
            let p: UISelectionItemPattern =
                el.get_pattern().map_err(|_| PlatformError::Unsupported)?;
            p.select().map_err(|e| uia_error(&e))
        })
    }

    fn expand(&self, element: &ElementRef, expand: bool) -> PlatformResult<()> {
        let element = element.clone();
        self.call(move |w| {
            let el = w.resolve(&element)?;
            let p: UIExpandCollapsePattern =
                el.get_pattern().map_err(|_| PlatformError::Unsupported)?;
            if expand { p.expand() } else { p.collapse() }.map_err(|e| uia_error(&e))
        })
    }

    fn scroll_into_view(&self, element: &ElementRef) -> PlatformResult<()> {
        let element = element.clone();
        self.call(move |w| {
            let el = w.resolve(&element)?;
            let p: UIScrollItemPattern =
                el.get_pattern().map_err(|_| PlatformError::Unsupported)?;
            p.scroll_into_view().map_err(|e| uia_error(&e))
        })
    }

    fn bounds(&self, element: &ElementRef) -> PlatformResult<Rect> {
        let element = element.clone();
        self.call(move |w| {
            let el = w.resolve(&element)?;
            let r = el.get_bounding_rectangle().map_err(|e| uia_error(&e))?;
            rect(&r).ok_or_else(|| PlatformError::NotFound("that control on screen".into()))
        })
    }

    fn subscribe(&self, subscription: &UiSubscription, sink: UiEventSink) -> PlatformResult<u64> {
        let id = self.next_sub.fetch_add(1, Ordering::Relaxed);
        let subscription = subscription.clone();
        let sink: Arc<dyn Fn(UiEvent) + Send + Sync> = Arc::from(sink);
        self.call(move |w| w.subscribe(id, &subscription, sink))?;
        Ok(id)
    }

    fn unsubscribe(&self, id: u64) -> PlatformResult<()> {
        self.call(move |w| {
            w.unsubscribe(id);
            Ok(())
        })
    }
}

fn window_or_zero(element: &ElementRef) -> WindowId {
    element.window().unwrap_or(WindowId(0))
}

/// An active event subscription: what to remove on `unsubscribe`.
enum Sub {
    Focus(UIFocusChangedEventHandler),
    Automation(UIEventType, UIElement, UIEventHandler),
    Structure(UIElement, UIStructureChangeEventHandler),
    Property(UIElement, UIPropertyChangedEventHandler),
}

struct Worker {
    auto: UIAutomation,
    /// Batch-fetches the properties a node needs in one round trip.
    request: UICacheRequest,
    control_view: UICondition,
    elements: HashMap<String, UIElement>,
    order: VecDeque<String>,
    subs: HashMap<u64, Sub>,
    self_elevated: bool,
}

/// The properties fetched for every node.
const NODE_PROPERTIES: [UIProperty; 15] = [
    UIProperty::Name,
    UIProperty::AutomationId,
    UIProperty::ControlType,
    UIProperty::IsPassword,
    UIProperty::IsEnabled,
    UIProperty::BoundingRectangle,
    UIProperty::IsInvokePatternAvailable,
    UIProperty::IsValuePatternAvailable,
    UIProperty::IsTogglePatternAvailable,
    UIProperty::IsSelectionItemPatternAvailable,
    UIProperty::IsExpandCollapsePatternAvailable,
    UIProperty::IsScrollItemPatternAvailable,
    UIProperty::ValueValue,
    UIProperty::ToggleToggleState,
    UIProperty::SelectionItemIsSelected,
];

impl Worker {
    fn new() -> PlatformResult<Self> {
        let auto = UIAutomation::new_direct().map_err(|e| uia_error(&e))?;
        set_timeouts(&auto);
        let request = auto.create_cache_request().map_err(|e| uia_error(&e))?;
        for p in NODE_PROPERTIES {
            request.add_property(p).map_err(|e| uia_error(&e))?;
        }
        request
            .add_property(UIProperty::ExpandCollapseExpandCollapseState)
            .map_err(|e| uia_error(&e))?;
        let control_view = auto
            .get_control_view_condition()
            .map_err(|e| uia_error(&e))?;
        Ok(Self {
            auto,
            request,
            control_view,
            elements: HashMap::new(),
            order: VecDeque::new(),
            subs: HashMap::new(),
            self_elevated: process_elevated(None).unwrap_or(false),
        })
    }

    fn remember(&mut self, key: &ElementRef, el: &UIElement) {
        if self.elements.insert(key.0.clone(), el.clone()).is_none() {
            self.order.push_back(key.0.clone());
            while self.order.len() > CACHE {
                if let Some(old) = self.order.pop_front() {
                    self.elements.remove(&old);
                }
            }
        }
    }

    /// The top-level window, checked: it exists and isn't elevated above KIVO.
    fn window(&self, window: WindowId) -> PlatformResult<UIElement> {
        let hwnd = hwnd(window);
        // SAFETY: IsWindow accepts any value.
        if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            return Err(PlatformError::NotFound("that window".into()));
        }
        if !self.self_elevated && window_elevated(hwnd) {
            return Err(PlatformError::Elevated(window_title(hwnd)));
        }
        self.auto
            .element_from_handle(Handle::from(hwnd))
            .map_err(|e| uia_error(&e))
    }

    fn resolve(&mut self, element: &ElementRef) -> PlatformResult<UIElement> {
        let window = element
            .window()
            .ok_or_else(|| PlatformError::NotFound("that control".into()))?;
        // Re-check the window each time: it may have closed or been replaced.
        let root = self.window(window)?;
        if let Some(el) = self.elements.get(&element.0) {
            return Ok(el.clone());
        }
        let wanted = element.0.split_once(':').map(|(_, id)| id.to_owned());
        let all = root
            .find_all(TreeScope::Subtree, &self.control_view)
            .map_err(|e| uia_error(&e))?;
        for el in all {
            if let Ok(id) = el.get_runtime_id()
                && Some(runtime_key(&id)) == wanted
            {
                self.remember(element, &el);
                return Ok(el);
            }
        }
        Err(PlatformError::NotFound("that control".into()))
    }

    /// A node from an element fetched with `self.request` (cached properties).
    fn node_cached(&mut self, el: &UIElement, window: WindowId) -> UiNode {
        let prop = |p: UIProperty| el.get_cached_property_value(p).ok();
        let key = ElementRef::new(window, &el.get_runtime_id().unwrap_or_default());
        self.remember(&key, el);
        let bounds = el.get_cached_bounding_rectangle().ok();
        build_node(key, bounds.as_ref().and_then(rect), prop)
    }

    /// A node read live, property by property.
    fn node_live(&mut self, el: &UIElement, window: WindowId) -> PlatformResult<UiNode> {
        let cached = el
            .build_updated_cache(&self.request)
            .map_err(|e| uia_error(&e))?;
        Ok(self.node_cached(&cached, window))
    }

    fn children(&mut self, el: &UIElement, window: WindowId) -> Vec<(UIElement, UiNode)> {
        let Ok(kids) =
            el.find_all_build_cache(TreeScope::Children, &self.control_view, &self.request)
        else {
            return Vec::new();
        };
        kids.into_iter()
            .map(|k| {
                let node = self.node_cached(&k, window);
                (k, node)
            })
            .collect()
    }

    fn tree(
        &mut self,
        window: WindowId,
        max_depth: u8,
        max_nodes: usize,
    ) -> PlatformResult<UiNode> {
        let root_el = self.window(window)?;
        let mut root = self.node_live(&root_el, window)?;
        let mut budget = max_nodes.saturating_sub(1);
        root.children = self.subtree(&root_el, window, max_depth, &mut budget);
        Ok(root)
    }

    fn subtree(
        &mut self,
        el: &UIElement,
        window: WindowId,
        depth: u8,
        budget: &mut usize,
    ) -> Vec<UiNode> {
        if depth == 0 || *budget == 0 {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (kid, mut node) in self.children(el, window) {
            if *budget == 0 {
                break;
            }
            *budget -= 1;
            node.children = self.subtree(&kid, window, depth - 1, budget);
            if keep(&node) {
                out.push(node);
            } else {
                // An unnamed container: lift its children instead.
                *budget += 1;
                out.append(&mut node.children);
            }
        }
        out
    }

    fn find(&mut self, query: &ElementQuery, limit: usize) -> PlatformResult<Vec<UiNode>> {
        let window = match query.window {
            Some(w) => w,
            None => foreground().ok_or_else(|| PlatformError::NotFound("a window".into()))?,
        };
        let root = self.window(window)?;
        let condition = self.condition(query)?;
        let found = root
            .find_all_build_cache(TreeScope::Descendants, &condition, &self.request)
            .map_err(|e| uia_error(&e))?;
        let mut scored: Vec<(u32, UiNode)> = found
            .iter()
            .filter_map(|el| {
                let node = self.node_cached(el, window);
                let score = match &query.name {
                    Some(name) => name_score(name, &node.name),
                    None => 1,
                };
                (score > 0).then_some((score, node))
            })
            .collect();
        scored.sort_by_key(|s| std::cmp::Reverse(s.0));
        Ok(scored
            .into_iter()
            .take(limit.max(1))
            .map(|(_, n)| n)
            .collect())
    }

    /// The UIA condition for the exact parts of a query (id and role); names are fuzzy and
    /// matched afterwards.
    fn condition(&self, query: &ElementQuery) -> PlatformResult<UICondition> {
        let mut condition = self.control_view.clone();
        if let Some(id) = query.automation_id.as_deref().filter(|s| !s.is_empty()) {
            let c = self
                .auto
                .create_property_condition(UIProperty::AutomationId, Variant::from(id), None)
                .map_err(|e| uia_error(&e))?;
            condition = self
                .auto
                .create_and_condition(condition, c)
                .map_err(|e| uia_error(&e))?;
        }
        if let Some(role) = query.role.as_deref().filter(|s| !s.is_empty()) {
            let id = control_type_id(role)
                .ok_or_else(|| PlatformError::NotFound(format!("a control type called {role}")))?;
            let c = self
                .auto
                .create_property_condition(UIProperty::ControlType, Variant::from(id), None)
                .map_err(|e| uia_error(&e))?;
            condition = self
                .auto
                .create_and_condition(condition, c)
                .map_err(|e| uia_error(&e))?;
        }
        Ok(condition)
    }

    fn focused(&mut self) -> PlatformResult<Option<UiNode>> {
        let Ok(el) = self.auto.get_focused_element() else {
            return Ok(None);
        };
        let window = top_window(&self.auto, &el);
        if let Some(w) = window
            && !self.self_elevated
            && window_elevated(hwnd(w))
        {
            return Err(PlatformError::Elevated(window_title(hwnd(w))));
        }
        self.node_live(&el, window.unwrap_or(WindowId(0))).map(Some)
    }

    fn subscribe(
        &mut self,
        id: u64,
        sub: &UiSubscription,
        sink: Arc<dyn Fn(UiEvent) + Send + Sync>,
    ) -> PlatformResult<()> {
        let scope_el = match sub.window {
            Some(w) => Some(self.window(w)?),
            None => None,
        };
        let entry = match sub.kind {
            UiEventKind::FocusChanged => {
                let auto = self.auto.clone();
                let handler: Box<uiautomation::events::CustomFocusChangedEventHandlerFn> =
                    Box::new(move |sender: &UIElement| {
                        let window = top_window(&auto, sender).unwrap_or(WindowId(0));
                        // Focus inside an elevated window reveals nothing about it.
                        if window_elevated(hwnd(window)) {
                            return Ok(());
                        }
                        sink(UiEvent::FocusChanged {
                            element: ElementRef::new(
                                window,
                                &sender.get_runtime_id().unwrap_or_default(),
                            ),
                            role: sender
                                .get_control_type()
                                .map(|t| format!("{t:?}"))
                                .unwrap_or_default(),
                            name: sender.get_name().unwrap_or_default(),
                            is_password: sender.is_password().unwrap_or(false),
                        });
                        Ok(())
                    });
                let handler = UIFocusChangedEventHandler::from(handler);
                self.auto
                    .add_focus_changed_event_handler(None, &handler)
                    .map_err(|e| uia_error(&e))?;
                Sub::Focus(handler)
            }
            UiEventKind::WindowOpened | UiEventKind::WindowClosed => {
                let opened = sub.kind == UiEventKind::WindowOpened;
                let event = if opened {
                    UIEventType::Window_WindowOpened
                } else {
                    UIEventType::Window_WindowClosed
                };
                let target = match scope_el {
                    Some(el) => el,
                    None => self.auto.get_root_element().map_err(|e| uia_error(&e))?,
                };
                let handler: Box<uiautomation::events::CustomEventHandlerFn> =
                    Box::new(move |sender: &UIElement, _| {
                        let handle: isize = sender
                            .get_native_window_handle()
                            .map(Into::into)
                            .unwrap_or(0);
                        let window = WindowId(u64::try_from(handle).unwrap_or(0));
                        let element = sender
                            .get_runtime_id()
                            .ok()
                            .map(|id| ElementRef::new(window, &id));
                        sink(if opened {
                            UiEvent::WindowOpened {
                                element: element.unwrap_or_default(),
                                name: sender.get_name().unwrap_or_default(),
                            }
                        } else {
                            UiEvent::WindowClosed { element }
                        });
                        Ok(())
                    });
                let handler = UIEventHandler::from(handler);
                self.auto
                    .add_automation_event_handler(
                        event,
                        &target,
                        TreeScope::Subtree,
                        None,
                        &handler,
                    )
                    .map_err(|e| uia_error(&e))?;
                Sub::Automation(event, target, handler)
            }
            UiEventKind::StructureChanged => {
                let (Some(el), Some(window)) = (scope_el, sub.window) else {
                    return Err(PlatformError::NotFound("a window to watch".into()));
                };
                let handler: Box<uiautomation::events::CustomStructureChangedEventHandlerFn> =
                    Box::new(move |_: &UIElement, _, _| {
                        sink(UiEvent::StructureChanged { window });
                        Ok(())
                    });
                let handler = UIStructureChangeEventHandler::from(handler);
                self.auto
                    .add_structure_changed_event_handler(&el, TreeScope::Subtree, None, &handler)
                    .map_err(|e| uia_error(&e))?;
                Sub::Structure(el, handler)
            }
            UiEventKind::PropertyChanged => {
                let (Some(el), Some(window)) = (scope_el, sub.window) else {
                    return Err(PlatformError::NotFound("a window to watch".into()));
                };
                let handler: Box<uiautomation::events::CustomPropertyChangedEventHandlerFn> =
                    Box::new(
                        move |sender: &UIElement, property: UIProperty, value: Variant| {
                            // Never report what a password field holds.
                            if sender.is_password().unwrap_or(true) {
                                return Ok(());
                            }
                            let mut value: String = value.try_into().unwrap_or_default();
                            // Win32 proxies often send the event without the new value: read it.
                            if value.is_empty() {
                                value = match property {
                                    UIProperty::Name => sender.get_name().unwrap_or_default(),
                                    UIProperty::ValueValue => sender
                                        .get_pattern::<UIValuePattern>()
                                        .and_then(|p| p.get_value())
                                        .unwrap_or_default(),
                                    _ => value,
                                };
                            }
                            sink(UiEvent::PropertyChanged {
                                element: ElementRef::new(
                                    window,
                                    &sender.get_runtime_id().unwrap_or_default(),
                                ),
                                property: format!("{property:?}"),
                                value: clip(&value),
                            });
                            Ok(())
                        },
                    );
                let handler = UIPropertyChangedEventHandler::from(handler);
                self.auto
                    .add_property_changed_event_handler(
                        &el,
                        TreeScope::Subtree,
                        None,
                        &handler,
                        &[
                            UIProperty::Name,
                            UIProperty::ValueValue,
                            UIProperty::ToggleToggleState,
                        ],
                    )
                    .map_err(|e| uia_error(&e))?;
                Sub::Property(el, handler)
            }
        };
        self.subs.insert(id, entry);
        Ok(())
    }

    fn unsubscribe(&mut self, id: u64) {
        let Some(sub) = self.subs.remove(&id) else {
            return;
        };
        let result = match &sub {
            Sub::Focus(h) => self.auto.remove_focus_changed_event_handler(h),
            Sub::Automation(event, el, h) => {
                self.auto.remove_automation_event_handler(*event, el, h)
            }
            Sub::Structure(el, h) => self.auto.remove_structure_changed_event_handler(el, h),
            Sub::Property(el, h) => self.auto.remove_property_changed_event_handler(el, h),
        };
        if let Err(e) = result {
            tracing::debug!(error = %e, "removing a UIA event handler");
        }
    }

    fn remove_all(&mut self) {
        if !self.subs.is_empty() {
            let _ = self.auto.remove_all_event_handlers();
            self.subs.clear();
        }
    }
}

/// Sets UIA's provider timeouts so one hung app fails fast instead of blocking the thread.
fn set_timeouts(auto: &UIAutomation) {
    use windows::Win32::UI::Accessibility::{IUIAutomation, IUIAutomation2};
    use windows::core::Interface;
    let raw: &IUIAutomation = auto.as_ref();
    if let Ok(two) = raw.cast::<IUIAutomation2>() {
        // SAFETY: plain setters on a live COM object owned by this thread.
        unsafe {
            let _ = two.SetConnectionTimeout(CONNECTION_TIMEOUT_MS);
            let _ = two.SetTransactionTimeout(TRANSACTION_TIMEOUT_MS);
        }
    }
}

fn build_node(
    key: ElementRef,
    bounds: Option<Rect>,
    prop: impl Fn(UIProperty) -> Option<Variant>,
) -> UiNode {
    let text = |p| -> String { prop(p).and_then(|v| v.try_into().ok()).unwrap_or_default() };
    let flag = |p| -> bool { prop(p).and_then(|v| v.try_into().ok()).unwrap_or(false) };
    let int = |p| -> Option<i32> { prop(p).and_then(|v| v.try_into().ok()) };
    let is_password = flag(UIProperty::IsPassword);
    let mut actions = Vec::new();
    for (p, a) in [
        (UIProperty::IsInvokePatternAvailable, UiAction::Invoke),
        (UIProperty::IsValuePatternAvailable, UiAction::SetValue),
        (UIProperty::IsTogglePatternAvailable, UiAction::Toggle),
        (
            UIProperty::IsSelectionItemPatternAvailable,
            UiAction::Select,
        ),
        (
            UIProperty::IsExpandCollapsePatternAvailable,
            UiAction::Expand,
        ),
        (
            UIProperty::IsScrollItemPatternAvailable,
            UiAction::ScrollIntoView,
        ),
    ] {
        if flag(p) {
            actions.push(a);
        }
    }
    let has = |a| actions.contains(&a);
    let value =
        (has(UiAction::SetValue) && !is_password).then(|| clip(&text(UIProperty::ValueValue)));
    let toggled = has(UiAction::Toggle)
        .then(|| int(UIProperty::ToggleToggleState))
        .flatten()
        .map(|s| s == 1);
    let selected = has(UiAction::Select).then(|| flag(UIProperty::SelectionItemIsSelected));
    let expanded = has(UiAction::Expand)
        .then(|| int(UIProperty::ExpandCollapseExpandCollapseState))
        .flatten()
        .and_then(|s| match s {
            s if s == ExpandCollapseState::Collapsed as i32 => Some(false),
            s if s == ExpandCollapseState::LeafNode as i32 => None,
            _ => Some(true),
        });
    let role = int(UIProperty::ControlType)
        .and_then(control_type_name)
        .unwrap_or("Custom")
        .to_owned();
    UiNode {
        element: key,
        role,
        name: clip(&text(UIProperty::Name)),
        automation_id: text(UIProperty::AutomationId),
        value,
        toggled,
        selected,
        expanded,
        enabled: flag(UIProperty::IsEnabled),
        bounds,
        is_password,
        actions,
        children: Vec::new(),
    }
}

/// Whether a node is worth showing: it has a name, an id, an action or children.
fn keep(node: &UiNode) -> bool {
    !node.name.trim().is_empty()
        || !node.automation_id.is_empty()
        || !node.actions.is_empty()
        || node.children.is_empty() && !is_container(&node.role)
}

fn is_container(role: &str) -> bool {
    matches!(role, "Pane" | "Group" | "Custom" | "Document" | "Tree")
}

fn clip(s: &str) -> String {
    if s.chars().count() <= MAX_VALUE {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(MAX_VALUE).collect();
    out.push('…');
    out
}

fn rect(r: &uiautomation::types::Rect) -> Option<Rect> {
    let w = r.get_right() - r.get_left();
    let h = r.get_bottom() - r.get_top();
    (w > 0 && h > 0).then(|| Rect {
        x: r.get_left(),
        y: r.get_top(),
        width: u32::try_from(w).unwrap_or(0),
        height: u32::try_from(h).unwrap_or(0),
    })
}

fn runtime_key(id: &[i32]) -> String {
    id.iter().map(i32::to_string).collect::<Vec<_>>().join(".")
}

/// The UIA control types by name (UIA_ButtonControlTypeId = 50000 …).
const CONTROL_TYPES: [&str; 41] = [
    "Button",
    "Calendar",
    "CheckBox",
    "ComboBox",
    "Edit",
    "Hyperlink",
    "Image",
    "ListItem",
    "List",
    "Menu",
    "MenuBar",
    "MenuItem",
    "ProgressBar",
    "RadioButton",
    "ScrollBar",
    "Slider",
    "Spinner",
    "StatusBar",
    "Tab",
    "TabItem",
    "Text",
    "ToolBar",
    "ToolTip",
    "Tree",
    "TreeItem",
    "Custom",
    "Group",
    "Thumb",
    "DataGrid",
    "DataItem",
    "Document",
    "SplitButton",
    "Window",
    "Pane",
    "Header",
    "HeaderItem",
    "Table",
    "TitleBar",
    "Separator",
    "SemanticZoom",
    "AppBar",
];

fn control_type_name(id: i32) -> Option<&'static str> {
    usize::try_from(id - 50_000)
        .ok()
        .and_then(|i| CONTROL_TYPES.get(i).copied())
}

fn control_type_id(name: &str) -> Option<i32> {
    let wanted = name.replace([' ', '_', '-'], "").to_lowercase();
    CONTROL_TYPES
        .iter()
        .position(|t| t.to_lowercase() == wanted)
        .and_then(|i| i32::try_from(i).ok())
        .map(|i| 50_000 + i)
}

/// How well `name` matches what the user said: 0 = no match, higher is better. Case, access-key
/// ampersands and trailing ellipses don't matter; all words must appear.
pub(crate) fn name_score(wanted: &str, name: &str) -> u32 {
    let norm = |s: &str| {
        s.replace(['&', '…'], "")
            .replace("...", "")
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let (w, n) = (norm(wanted), norm(name));
    if w.is_empty() || n.is_empty() {
        return 0;
    }
    if w == n {
        4
    } else if n.starts_with(&w) {
        3
    } else if n.contains(&w) {
        2
    } else if w
        .split(' ')
        .all(|word| n.split(' ').any(|x| x.starts_with(word)))
    {
        1
    } else {
        0
    }
}

fn hwnd(window: WindowId) -> HWND {
    HWND(isize::try_from(window.0).unwrap_or(0) as *mut _)
}

fn foreground() -> Option<WindowId> {
    // SAFETY: no arguments.
    let h = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
    (!h.is_invalid()).then_some(WindowId(h.0 as u64))
}

/// The top-level window an element lives in.
fn top_window(auto: &UIAutomation, el: &UIElement) -> Option<WindowId> {
    let mut current = el.clone();
    let walker = auto.get_control_view_walker().ok()?;
    for _ in 0..64 {
        if let Ok(h) = current.get_native_window_handle() {
            let raw: isize = h.into();
            if raw != 0 {
                // SAFETY: GetAncestor accepts any handle.
                let root = unsafe { GetAncestor(HWND(raw as *mut _), GA_ROOT) };
                if !root.is_invalid() {
                    return Some(WindowId(root.0 as u64));
                }
            }
        }
        current = walker.get_parent(&current).ok()?;
    }
    None
}

fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    // SAFETY: the buffer outlives the call.
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    let title = String::from_utf16_lossy(&buf[..usize::try_from(n).unwrap_or(0)]);
    if title.is_empty() {
        "That app".into()
    } else {
        title
    }
}

/// Whether the window's process runs elevated. A process KIVO can't even query counts as
/// elevated (that's what UIPI would do to it too).
fn window_elevated(hwnd: HWND) -> bool {
    let mut pid = 0u32;
    // SAFETY: pid outlives the call.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut pid)) };
    if pid == 0 {
        return false;
    }
    process_elevated(Some(pid)).unwrap_or(true)
}

fn process_elevated(pid: Option<u32>) -> Option<bool> {
    // SAFETY: handles opened here are closed before returning; the struct outlives the call.
    unsafe {
        let (process, owned) = match pid {
            Some(pid) => (
                OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?,
                true,
            ),
            None => (GetCurrentProcess(), false),
        };
        let mut token = HANDLE::default();
        let opened = OpenProcessToken(process, TOKEN_QUERY, &raw mut token).is_ok();
        if owned {
            let _ = CloseHandle(process);
        }
        if !opened {
            return None;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some((&raw mut elevation).cast()),
            u32::try_from(size_of::<TOKEN_ELEVATION>()).unwrap_or(0),
            &raw mut len,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok.then_some(elevation.TokenIsElevated != 0)
    }
}

fn uia_error(e: &uiautomation::Error) -> PlatformError {
    // UIA_E_ELEMENTNOTAVAILABLE, UIA_E_TIMEOUT, E_ACCESSDENIED, UIA_E_ELEMENTNOTENABLED.
    #[allow(clippy::cast_possible_wrap, reason = "HRESULT bit patterns")]
    match e.code() {
        c if c == 0x8004_0201_u32 as i32 => PlatformError::NotFound("that control".into()),
        c if c == 0x8013_1505_u32 as i32 => PlatformError::Timeout,
        c if c == 0x8007_0005_u32 as i32 => PlatformError::AccessDenied,
        c if c == 0x8004_0200_u32 as i32 => PlatformError::Conflict("that control".into()),
        code => PlatformError::Os {
            code: i64::from(code),
            message: e.message().to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_loosely_but_need_every_word() {
        assert_eq!(name_score("export", "E&xport…"), 4);
        assert_eq!(name_score("save", "Save as"), 3);
        assert_eq!(name_score("me", "Remember me"), 2);
        assert_eq!(name_score("rem me", "Remember me"), 1);
        assert_eq!(name_score("cancel", "Save"), 0);
        assert_eq!(name_score("", "Save"), 0);
    }

    #[test]
    fn control_types_round_trip() {
        assert_eq!(control_type_id("Button"), Some(50_000));
        assert_eq!(control_type_id("list item"), Some(50_007));
        assert_eq!(control_type_name(50_032), Some("Window"));
        assert_eq!(control_type_name(50_033), Some("Pane"));
        assert_eq!(control_type_id("AppBar"), Some(50_040));
        assert_eq!(control_type_id("Bogus"), None);
    }

    #[test]
    fn pruning_keeps_named_or_actionable_nodes() {
        let pane = UiNode {
            role: "Pane".into(),
            ..UiNode::default()
        };
        assert!(!keep(&pane));
        assert!(keep(&UiNode {
            name: "Toolbar".into(),
            ..pane.clone()
        }));
        assert!(keep(&UiNode {
            actions: vec![UiAction::Invoke],
            ..pane.clone()
        }));
        assert!(keep(&UiNode {
            role: "Text".into(),
            ..UiNode::default()
        }));
    }

    #[test]
    fn this_process_is_not_elevated_in_tests() {
        // The test runner runs as the user; KIVO never elevates.
        assert_eq!(process_elevated(None), Some(false));
    }
}
