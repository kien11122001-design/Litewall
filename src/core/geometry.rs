//! Rectangle math in virtual-desktop physical pixels. Origins may be negative.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RectI32 {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl RectI32 {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
    pub fn width(&self) -> i64 {
        self.right as i64 - self.left as i64
    }
    pub fn height(&self) -> i64 {
        self.bottom as i64 - self.top as i64
    }
    pub fn is_empty(&self) -> bool {
        self.width() <= 0 || self.height() <= 0
    }
    pub fn area(&self) -> i64 {
        if self.is_empty() {
            0
        } else {
            self.width() * self.height()
        }
    }
    /// Half-open containment: `left <= x < right`, `top <= y < bottom`.
    pub fn contains_point(&self, x: i64, y: i64) -> bool {
        x >= self.left as i64
            && x < self.right as i64
            && y >= self.top as i64
            && y < self.bottom as i64
    }
    pub fn intersect(&self, o: &Self) -> Option<Self> {
        let r = Self {
            left: self.left.max(o.left),
            top: self.top.max(o.top),
            right: self.right.min(o.right),
            bottom: self.bottom.min(o.bottom),
        };
        if r.is_empty() {
            None
        } else {
            Some(r)
        }
    }
    pub fn translate(&self, dx: i32, dy: i32) -> Self {
        Self {
            left: self.left.saturating_add(dx),
            top: self.top.saturating_add(dy),
            right: self.right.saturating_add(dx),
            bottom: self.bottom.saturating_add(dy),
        }
    }
}

/// Bounding rectangle of all non-empty rects (used by span mode, P1).
pub fn union_rect(rects: &[RectI32]) -> Option<RectI32> {
    rects
        .iter()
        .filter(|r| !r.is_empty())
        .fold(None, |acc, r| match acc {
            None => Some(*r),
            Some(a) => Some(RectI32::new(
                a.left.min(r.left),
                a.top.min(r.top),
                a.right.max(r.right),
                a.bottom.max(r.bottom),
            )),
        })
}

/// Convert a virtual-desktop rect into the coordinate space of a parent window.
/// `parent_origin` is the parent's client origin in virtual-desktop coordinates
/// (what `MapWindowPoints(NULL, parent, ..)` is derived from in `platform/`).
pub fn to_parent_relative(rect: RectI32, parent_origin: (i32, i32)) -> RectI32 {
    rect.translate(
        parent_origin.0.saturating_neg(),
        parent_origin.1.saturating_neg(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_and_empty() {
        let r = RectI32::new(-1920, 0, 0, 1080);
        assert_eq!((r.width(), r.height(), r.area()), (1920, 1080, 1920 * 1080));
        assert!(RectI32::new(5, 5, 5, 10).is_empty());
        assert_eq!(RectI32::new(10, 10, 0, 0).area(), 0);
    }

    #[test]
    fn intersect_cases() {
        let a = RectI32::new(0, 0, 100, 100);
        assert_eq!(
            a.intersect(&RectI32::new(50, 50, 150, 150)),
            Some(RectI32::new(50, 50, 100, 100))
        );
        assert_eq!(a.intersect(&RectI32::new(100, 0, 200, 100)), None); // touching edge
        assert_eq!(
            a.intersect(&RectI32::new(-50, -50, 10, 10)),
            Some(RectI32::new(0, 0, 10, 10))
        );
    }

    #[test]
    fn union_with_negative_origin() {
        let left = RectI32::new(-1920, 0, 0, 1080);
        let main = RectI32::new(0, 0, 2560, 1600);
        let above = RectI32::new(0, -1080, 1920, 0);
        assert_eq!(
            union_rect(&[left, main, above]),
            Some(RectI32::new(-1920, -1080, 2560, 1600))
        );
        assert_eq!(union_rect(&[]), None);
        assert_eq!(union_rect(&[RectI32::new(1, 1, 1, 1)]), None);
    }

    #[test]
    fn parent_relative_negative_and_positive() {
        let mon = RectI32::new(-1920, 0, 0, 1080);
        // Parent (WorkerW) spans the whole virtual desktop starting at (-1920, 0).
        assert_eq!(
            to_parent_relative(mon, (-1920, 0)),
            RectI32::new(0, 0, 1920, 1080)
        );
        // Parent origin at (0,0): coordinates unchanged (and stay negative).
        assert_eq!(to_parent_relative(mon, (0, 0)), mon);
        // Parent above-left of everything.
        let m2 = RectI32::new(0, 0, 2560, 1600);
        assert_eq!(
            to_parent_relative(m2, (-1920, -1080)),
            RectI32::new(1920, 1080, 4480, 2680)
        );
    }

    #[test]
    fn contains_is_half_open() {
        let r = RectI32::new(0, 0, 10, 10);
        assert!(r.contains_point(0, 0) && r.contains_point(9, 9));
        assert!(!r.contains_point(10, 5) && !r.contains_point(-1, 5));
    }
}
