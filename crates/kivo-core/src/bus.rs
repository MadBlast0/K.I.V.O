//! The in-process event bus (ARCHITECTURE §4.1): one sender, any number of subscribers
//! (IPC forwarding, Activity and Audit writers, metrics). Events are shared, not copied.

use crate::event::Event;
use std::sync::Arc;
use tokio::sync::broadcast;

/// Enough headroom for a burst of partial transcripts and levels while a subscriber is busy.
const CAPACITY: usize = 1024;

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<Arc<Event>>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(CAPACITY);
        Self { tx }
    }

    /// Publishes an event to every current subscriber. Returns how many received it
    /// (0 is normal when nobody is listening yet).
    pub fn publish(&self, event: Event) -> usize {
        self.tx.send(Arc::new(event)).unwrap_or(0)
    }

    pub fn subscribe(&self) -> Subscription {
        Subscription {
            rx: self.tx.subscribe(),
        }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Subscription {
    rx: broadcast::Receiver<Arc<Event>>,
}

/// What a subscriber gets next.
#[derive(Debug)]
pub enum Received {
    Event(Arc<Event>),
    /// The subscriber fell behind and this many events were dropped for it. The caller decides
    /// what that means (the IPC forwarder, for example, resends a full state snapshot).
    Missed(u64),
    /// The bus was dropped; no more events will arrive.
    Closed,
}

impl Subscription {
    pub async fn recv(&mut self) -> Received {
        match self.rx.recv().await {
            Ok(event) => Received::Event(event),
            Err(broadcast::error::RecvError::Lagged(n)) => Received::Missed(n),
            Err(broadcast::error::RecvError::Closed) => Received::Closed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventKind, UiEvent};

    fn overlay_shown() -> Event {
        Event::new(EventKind::Ui(UiEvent::OverlayShown))
    }

    #[tokio::test]
    async fn every_subscriber_gets_every_event_in_order() {
        let bus = EventBus::new();
        let mut a = bus.subscribe();
        let mut b = bus.subscribe();
        let first = overlay_shown();
        let second = Event::new(EventKind::Ui(UiEvent::OverlayHidden));
        assert_eq!(bus.publish(first.clone()), 2);
        bus.publish(second.clone());
        for sub in [&mut a, &mut b] {
            assert!(matches!(sub.recv().await, Received::Event(e) if *e == first));
            assert!(matches!(sub.recv().await, Received::Event(e) if *e == second));
        }
    }

    #[tokio::test]
    async fn publishing_with_no_subscribers_is_fine() {
        assert_eq!(EventBus::new().publish(overlay_shown()), 0);
    }

    #[tokio::test]
    async fn a_slow_subscriber_is_told_how_much_it_missed() {
        let bus = EventBus::new();
        let mut slow = bus.subscribe();
        for _ in 0..CAPACITY + 5 {
            bus.publish(overlay_shown());
        }
        assert!(matches!(slow.recv().await, Received::Missed(5)));
    }

    #[tokio::test]
    async fn subscribers_see_closed_when_the_bus_is_dropped() {
        let bus = EventBus::new();
        let mut sub = bus.subscribe();
        drop(bus);
        assert!(matches!(sub.recv().await, Received::Closed));
    }
}
