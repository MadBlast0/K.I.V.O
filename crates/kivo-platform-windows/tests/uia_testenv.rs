//! The UIA journey against the dummy app (TOOLS_AND_CONTROL §10, TOOL-19–22, TOOL-39): find,
//! tree, set a value, invoke, toggle, select, expand, events and the fake Export. Only the test
//! app is touched; UIA patterns act on it directly, so no keystrokes or clicks reach the desktop.

#![cfg(windows)]

use kivo_platform::{
    ElementQuery, PlatformError, UiAction, UiAutomation, UiEvent, UiEventKind, UiSubscription,
    WindowId, Windows,
};
use kivo_platform_windows::{WindowsUiAutomation, WindowsWindows};
use kivo_testkit::testenv::{TestApp, wait_for};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn window_titled(title: &str) -> Option<WindowId> {
    WindowsWindows
        .list()
        .ok()?
        .into_iter()
        .find(|w| w.title == title)
        .map(|w| w.id)
}

fn by_id(uia: &WindowsUiAutomation, window: WindowId, id: &str) -> kivo_platform::UiNode {
    let found = uia
        .find(
            &ElementQuery {
                window: Some(window),
                automation_id: Some(id.into()),
                ..ElementQuery::default()
            },
            5,
        )
        .expect("find");
    found
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("no element {id}"))
}

fn status(uia: &WindowsUiAutomation, window: WindowId) -> String {
    let node = by_id(uia, window, "108");
    uia.describe(&node.element).expect("describe").name
}

#[test]
fn uia_journey_on_the_dummy_app() {
    let app = TestApp::launch("KIVO Test App · uia journey");
    let window = wait_for(Duration::from_secs(10), || window_titled(&app.title))
        .expect("the dummy app's window appears");
    let uia = WindowsUiAutomation::new().expect("UIA thread");

    // The tree is compact and names the controls by their AutomationIds.
    let tree = uia.tree(window, 4, 200).expect("tree");
    assert_eq!(tree.role, "Window");
    let mut ids = Vec::new();
    fn walk(n: &kivo_platform::UiNode, ids: &mut Vec<String>) {
        ids.push(n.automation_id.clone());
        n.children.iter().for_each(|c| walk(c, ids));
    }
    walk(&tree, &mut ids);
    for id in ["101", "102", "103", "104", "105", "106", "107"] {
        assert!(ids.contains(&id.to_owned()), "tree has {id}: {ids:?}");
    }
    assert!(tree.count() <= 200);

    // Fuzzy name search, best match first.
    let greet = uia
        .find(
            &ElementQuery {
                window: Some(window),
                name: Some("greet".into()),
                ..ElementQuery::default()
            },
            3,
        )
        .expect("find by name");
    assert_eq!(greet[0].automation_id, "103");
    assert_eq!(greet[0].role, "Button");

    // PLAN-23 / UX-39: "where is the Export button" finds it by name with real bounds inside the
    // window, which is what the Island is placed beside.
    let export = uia
        .find(
            &ElementQuery {
                window: Some(window),
                name: Some("export".into()),
                ..ElementQuery::default()
            },
            3,
        )
        .expect("find Export");
    let bounds = export[0].bounds.expect("Export has bounds");
    let frame = WindowsWindows
        .list()
        .unwrap()
        .into_iter()
        .find(|w| w.id == window)
        .unwrap()
        .bounds;
    assert!(bounds.width > 0 && bounds.height > 0, "{bounds:?}");
    assert!(
        bounds.x >= frame.x
            && bounds.y >= frame.y
            && bounds.x + bounds.width as i32 <= frame.x + frame.width as i32
            && bounds.y + bounds.height as i32 <= frame.y + frame.height as i32,
        "{bounds:?} inside {frame:?}"
    );
    assert!(greet[0].actions.contains(&UiAction::Invoke));
    let buttons = uia
        .find(
            &ElementQuery {
                window: Some(window),
                role: Some("button".into()),
                ..ElementQuery::default()
            },
            20,
        )
        .expect("find by role");
    assert!(buttons.iter().all(|b| b.role == "Button"));
    assert!(buttons.len() >= 3);

    // Set a value and invoke: the app greets.
    let name = by_id(&uia, window, "101");
    uia.set_value(&name.element, "Ada").expect("set value");
    assert_eq!(
        uia.describe(&name.element).unwrap().value.as_deref(),
        Some("Ada")
    );
    uia.invoke(&greet[0].element).expect("invoke");
    assert_eq!(
        wait_for(Duration::from_secs(3), || {
            let s = status(&uia, window);
            (s == "Hello, Ada").then_some(s)
        })
        .as_deref(),
        Some("Hello, Ada")
    );

    // A password field: flagged, value never read, never typed into.
    let password = by_id(&uia, window, "102");
    assert!(password.is_password);
    assert_eq!(password.value, None);
    assert_eq!(
        uia.set_value(&password.element, "hunter2"),
        Err(PlatformError::AccessDenied)
    );

    // Toggle the check box on and back off.
    let remember = by_id(&uia, window, "104");
    assert_eq!(remember.toggled, Some(false));
    assert_eq!(uia.toggle(&remember.element), Ok(true));
    assert_eq!(uia.describe(&remember.element).unwrap().toggled, Some(true));
    assert_eq!(uia.toggle(&remember.element), Ok(false));

    // Select a list item.
    let beta = uia
        .find(
            &ElementQuery {
                window: Some(window),
                name: Some("Beta".into()),
                role: Some("ListItem".into()),
                ..ElementQuery::default()
            },
            1,
        )
        .expect("find list item");
    assert_eq!(beta.len(), 1);
    uia.select(&beta[0].element).expect("select");
    assert_eq!(uia.describe(&beta[0].element).unwrap().selected, Some(true));
    uia.scroll_into_view(&beta[0].element)
        .expect("scroll into view");

    // Expand and collapse the combo box.
    let colour = by_id(&uia, window, "106");
    assert!(colour.actions.contains(&UiAction::Expand));
    uia.expand(&colour.element, true).expect("expand");
    assert_eq!(uia.describe(&colour.element).unwrap().expanded, Some(true));
    uia.expand(&colour.element, false).expect("collapse");
    assert_eq!(uia.describe(&colour.element).unwrap().expanded, Some(false));

    // Bounds are on screen, inside the window.
    let b = uia.bounds(&greet[0].element).expect("bounds");
    let w = tree.bounds.expect("window bounds");
    assert!(w.contains(b.center()));

    // Events on demand: the Export window opening is reported without polling.
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let sub = uia
        .subscribe(
            &UiSubscription {
                kind: UiEventKind::WindowOpened,
                window: None,
            },
            Box::new(move |e| sink.lock().unwrap().push(e)),
        )
        .expect("subscribe");
    let export = by_id(&uia, window, "107");
    uia.invoke(&export.element).expect("open Export");
    let export_window = wait_for(Duration::from_secs(5), || window_titled("Export"))
        .expect("the Export window opens");
    let opened = wait_for(Duration::from_secs(5), || {
        events.lock().unwrap().iter().find_map(|e| match e {
            UiEvent::WindowOpened { name, .. } if name == "Export" => Some(()),
            _ => None,
        })
    });
    assert!(
        opened.is_some(),
        "WindowOpened arrived: {:?}",
        events.lock().unwrap()
    );
    uia.unsubscribe(sub).expect("unsubscribe");

    // The fake Export writes the name into the chosen file inside the out folder only.
    let file = by_id(&uia, export_window, "201");
    uia.set_value(&file.element, "greeting.txt")
        .expect("file name");
    let save = by_id(&uia, export_window, "202");
    uia.invoke(&save.element).expect("save");
    let path = app.out.path().join("greeting.txt");
    let written = wait_for(Duration::from_secs(5), || {
        std::fs::read_to_string(&path).ok()
    });
    assert_eq!(written.as_deref(), Some("Ada"));

    // A window that's gone is reported as not found, not as an OS error.
    drop(app);
    let gone = wait_for(Duration::from_secs(5), || {
        uia.describe(&name.element)
            .err()
            .filter(|e| matches!(e, PlatformError::NotFound(_)))
    });
    assert!(gone.is_some());
}

#[test]
fn structure_and_property_events_are_scoped_to_one_window() {
    let app = TestApp::launch("KIVO Test App · uia events");
    let window = wait_for(Duration::from_secs(10), || window_titled(&app.title))
        .expect("the dummy app's window appears");
    let uia = WindowsUiAutomation::new().expect("UIA thread");
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let sub = uia
        .subscribe(
            &UiSubscription {
                kind: UiEventKind::PropertyChanged,
                window: Some(window),
            },
            Box::new(move |e| sink.lock().unwrap().push(e)),
        )
        .expect("subscribe");
    // Unscoped structure/property subscriptions are refused.
    assert!(
        uia.subscribe(
            &UiSubscription {
                kind: UiEventKind::StructureChanged,
                window: None,
            },
            Box::new(|_| {}),
        )
        .is_err()
    );
    let name = by_id(&uia, window, "101");
    uia.set_value(&name.element, "Grace").unwrap();
    let greet = by_id(&uia, window, "103");
    uia.invoke(&greet.element).unwrap();
    let seen = wait_for(Duration::from_secs(5), || {
        events.lock().unwrap().iter().find_map(|e| match e {
            UiEvent::PropertyChanged { value, .. } if value.contains("Grace") => Some(()),
            _ => None,
        })
    });
    assert!(seen.is_some(), "events: {:?}", events.lock().unwrap());
    // Typing into the password field never surfaces its value.
    let password = by_id(&uia, window, "102");
    assert!(password.is_password);
    assert!(events.lock().unwrap().iter().all(|e| match e {
        UiEvent::PropertyChanged { element, .. } => element != &password.element,
        _ => true,
    }));
    uia.unsubscribe(sub).unwrap();
}

#[test]
fn focus_reports_the_focused_element_without_moving_focus() {
    let uia = WindowsUiAutomation::new().expect("UIA thread");
    // Whatever has focus now (a terminal, the IDE): only read, never changed.
    let before = WindowsWindows.foreground().ok().flatten().map(|w| w.id);
    let _ = uia.focused();
    let after = WindowsWindows.foreground().ok().flatten().map(|w| w.id);
    assert_eq!(before, after);
}
