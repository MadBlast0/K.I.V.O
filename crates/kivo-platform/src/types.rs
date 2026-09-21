use serde::{Deserialize, Serialize};

/// An audio device, as the OS identifies it (stable across restarts where the OS allows).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeviceId(pub String);

/// A top-level window, as the OS identifies it for this session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WindowId(pub u64);

/// Screen coordinates in physical pixels (virtual desktop, all monitors).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn contains(&self, p: Point) -> bool {
        let right = i64::from(self.x) + i64::from(self.width);
        let bottom = i64::from(self.y) + i64::from(self.height);
        p.x >= self.x && p.y >= self.y && i64::from(p.x) < right && i64::from(p.y) < bottom
    }

    pub fn center(&self) -> Point {
        let half = |n: u32| i32::try_from(n / 2).unwrap_or(i32::MAX);
        Point {
            x: self.x.saturating_add(half(self.width)),
            y: self.y.saturating_add(half(self.height)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_contains_its_top_left_but_not_its_far_edge() {
        let r = Rect {
            x: -100,
            y: 0,
            width: 200,
            height: 50,
        };
        assert!(r.contains(Point { x: -100, y: 0 }));
        assert!(r.contains(Point { x: 99, y: 49 }));
        assert!(!r.contains(Point { x: 100, y: 10 }));
        assert_eq!(r.center(), Point { x: 0, y: 25 });
    }
}
