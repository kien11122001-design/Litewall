//! Monitor identity and diffing (PRD §9). Pure; the Win32 enumeration that
//! produces `MonitorInfo` lives in `platform/monitors.rs` (M4).
use super::geometry::RectI32;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Stable id: prefer the display device-interface path, fall back to `\\.\DISPLAYn`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MonitorId(pub String);

impl MonitorId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Pick the stable id: the display device-interface path (contains the output's UID, so it
/// survives reboots and is unique per connection) when Windows gave one, else the GDI name
/// (`\\.\DISPLAYn`, which can change when monitors are re-plugged).
pub fn choose_id(interface_path: &str, device_name: &str) -> MonitorId {
    let path = interface_path.trim();
    if path.starts_with("\\\\?\\") {
        return MonitorId::new(path);
    }
    let name = device_name.trim();
    MonitorId::new(if name.is_empty() { "UNKNOWN" } else { name })
}

/// Make ids unique in list order: a repeated id gets `#2`, `#3`, ... appended.
pub fn dedupe_ids(monitors: &mut [MonitorInfo]) {
    let mut seen: HashMap<String, u32> = HashMap::new();
    for m in monitors.iter_mut() {
        let n = seen.entry(m.id.0.clone()).or_insert(0);
        *n += 1;
        if *n > 1 {
            m.id = MonitorId(format!("{}#{}", m.id.0, n));
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MonitorInfo {
    pub id: MonitorId,
    /// Virtual-desktop physical pixels (may be negative).
    pub rect_px: RectI32,
    pub dpi: u32,
    pub refresh_hz: f32,
    pub primary: bool,
}

#[derive(Debug, Default, PartialEq)]
pub struct MonitorDiff {
    pub added: Vec<MonitorId>,
    pub removed: Vec<MonitorId>,
    /// Same id, different rect or DPI.
    pub changed: Vec<MonitorId>,
}

impl MonitorDiff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
}

/// Diff two monitor lists by id. Output vectors are sorted for determinism.
pub fn diff(old: &[MonitorInfo], new: &[MonitorInfo]) -> MonitorDiff {
    let old_map: HashMap<&MonitorId, &MonitorInfo> = old.iter().map(|m| (&m.id, m)).collect();
    let new_map: HashMap<&MonitorId, &MonitorInfo> = new.iter().map(|m| (&m.id, m)).collect();
    let mut d = MonitorDiff::default();
    for (id, n) in &new_map {
        match old_map.get(id) {
            None => d.added.push((*id).clone()),
            Some(o) if o.rect_px != n.rect_px || o.dpi != n.dpi => d.changed.push((*id).clone()),
            Some(_) => {}
        }
    }
    for id in old_map.keys() {
        if !new_map.contains_key(*id) {
            d.removed.push((*id).clone());
        }
    }
    d.added.sort();
    d.removed.sort();
    d.changed.sort();
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(id: &str, r: RectI32, dpi: u32) -> MonitorInfo {
        MonitorInfo {
            id: MonitorId::new(id),
            rect_px: r,
            dpi,
            refresh_hz: 60.0,
            primary: false,
        }
    }
    const R1: RectI32 = RectI32::new(0, 0, 1920, 1080);
    const R2: RectI32 = RectI32::new(-1920, 0, 0, 1080);

    #[test]
    fn identical_lists_are_empty() {
        let a = vec![m("A", R1, 96), m("B", R2, 96)];
        assert!(diff(&a, &a.clone()).is_empty());
    }

    #[test]
    fn added_removed() {
        let old = vec![m("A", R1, 96)];
        let new = vec![m("A", R1, 96), m("B", R2, 96)];
        assert_eq!(
            diff(&old, &new),
            MonitorDiff {
                added: vec![MonitorId::new("B")],
                ..Default::default()
            }
        );
        assert_eq!(
            diff(&new, &old),
            MonitorDiff {
                removed: vec![MonitorId::new("B")],
                ..Default::default()
            }
        );
    }

    #[test]
    fn rect_or_dpi_change_including_negative_origin() {
        let old = vec![m("A", R1, 96), m("B", R2, 96)];
        // B moves from left of A to above A (origin stays negative), A changes DPI.
        let new = vec![m("A", R1, 144), m("B", RectI32::new(0, -1080, 1920, 0), 96)];
        let d = diff(&old, &new);
        assert_eq!(d.changed, vec![MonitorId::new("A"), MonitorId::new("B")]);
        assert!(d.added.is_empty() && d.removed.is_empty());
    }

    #[test]
    fn refresh_rate_alone_is_not_a_change_and_order_is_irrelevant() {
        let mut b = m("B", R2, 96);
        let old = vec![m("A", R1, 96), b.clone()];
        b.refresh_hz = 144.0;
        let new = vec![b, m("A", R1, 96)];
        assert!(diff(&old, &new).is_empty());
    }
}

#[cfg(test)]
mod id_tests {
    use super::*;

    fn info(id: &str) -> MonitorInfo {
        MonitorInfo {
            id: MonitorId::new(id),
            rect_px: RectI32::new(0, 0, 1920, 1080),
            dpi: 96,
            refresh_hz: 60.0,
            primary: false,
        }
    }

    #[test]
    fn interface_path_wins_and_gdi_name_is_the_fallback() {
        let path = "\\\\?\\DISPLAY#GSM5B09#5&1a2b&0&UID4357#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}";
        assert_eq!(choose_id(path, "\\\\.\\DISPLAY1").as_str(), path);
        assert_eq!(choose_id(&format!("  {path} "), "x").as_str(), path);
        assert_eq!(choose_id("", "\\\\.\\DISPLAY2").as_str(), "\\\\.\\DISPLAY2");
        assert_eq!(
            choose_id("garbage", " \\\\.\\DISPLAY3 ").as_str(),
            "\\\\.\\DISPLAY3"
        );
        assert_eq!(choose_id("", "").as_str(), "UNKNOWN");
    }

    #[test]
    fn duplicate_ids_get_numbered_in_order() {
        let mut v = vec![info("A"), info("B"), info("A"), info("A")];
        dedupe_ids(&mut v);
        let ids: Vec<_> = v.iter().map(|m| m.id.as_str().to_string()).collect();
        assert_eq!(ids, ["A", "B", "A#2", "A#3"]);
        let mut unique = vec![info("A"), info("B")];
        dedupe_ids(&mut unique);
        assert_eq!(unique[0].id.as_str(), "A");
        assert_eq!(unique[1].id.as_str(), "B");
    }
}
