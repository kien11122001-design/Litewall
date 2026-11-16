//! Decide what to create, move, reload or remove so the running wallpapers match the
//! config and the current monitor layout. Pure: the platform layer only executes the
//! returned actions (PRD §9).
use super::config::Config;
use super::geometry::RectI32;
use super::monitor::{MonitorId, MonitorInfo};
use super::zoom::ZoomMode;
use std::collections::HashMap;
use std::path::PathBuf;

/// What a monitor should be showing according to the config.
#[derive(Clone, Debug, PartialEq)]
pub struct Wanted {
    pub file: PathBuf,
    pub zoom: ZoomMode,
}

/// What a monitor is showing right now.
#[derive(Clone, Debug, PartialEq)]
pub struct Running {
    pub file: PathBuf,
    pub zoom: ZoomMode,
    pub rect: RectI32,
}

#[derive(Debug, PartialEq)]
pub enum Action {
    Create {
        id: MonitorId,
        rect: RectI32,
        wanted: Wanted,
    },
    Remove {
        id: MonitorId,
    },
    Move {
        id: MonitorId,
        rect: RectI32,
    },
    Load {
        id: MonitorId,
        file: PathBuf,
    },
    Zoom {
        id: MonitorId,
        zoom: ZoomMode,
    },
}

/// Actions that turn `running` into the desired state. Removals come first (sorted by id),
/// then one group per monitor in `monitors` order. A monitor is wanted when it is enabled
/// and has a file (its own override, else the default profile). `fallback_zoom` applies to
/// monitors without a profile. `reposition_all` forces a `Move` for every running monitor
/// (display changes can move the desktop host even when no monitor rect changed).
pub fn plan(
    monitors: &[MonitorInfo],
    cfg: &Config,
    fallback_zoom: ZoomMode,
    running: &HashMap<MonitorId, Running>,
    reposition_all: bool,
) -> Vec<Action> {
    let mut wanted: HashMap<MonitorId, Wanted> = HashMap::new();
    for m in monitors {
        if !cfg.is_enabled(&m.id) {
            continue;
        }
        let Some(file) = cfg.resolved_file(&m.id) else {
            continue;
        };
        let zoom = cfg.profile(&m.id).map_or(fallback_zoom, |p| p.zoom);
        wanted.insert(
            m.id.clone(),
            Wanted {
                file: file.to_path_buf(),
                zoom,
            },
        );
    }

    let mut actions = Vec::new();
    let mut stale: Vec<&MonitorId> = running
        .keys()
        .filter(|id| !wanted.contains_key(*id))
        .collect();
    stale.sort();
    for id in stale {
        actions.push(Action::Remove { id: id.clone() });
    }

    for m in monitors {
        let Some(w) = wanted.get(&m.id) else {
            continue;
        };
        match running.get(&m.id) {
            None => actions.push(Action::Create {
                id: m.id.clone(),
                rect: m.rect_px,
                wanted: w.clone(),
            }),
            Some(r) => {
                if reposition_all || r.rect != m.rect_px {
                    actions.push(Action::Move {
                        id: m.id.clone(),
                        rect: m.rect_px,
                    });
                }
                if r.file != w.file {
                    actions.push(Action::Load {
                        id: m.id.clone(),
                        file: w.file.clone(),
                    });
                }
                if r.zoom != w.zoom {
                    actions.push(Action::Zoom {
                        id: m.id.clone(),
                        zoom: w.zoom,
                    });
                }
            }
        }
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::{MonitorProfile, WallpaperSpec};

    const R_A: RectI32 = RectI32::new(-1920, 0, 0, 1080); // negative origin
    const R_B: RectI32 = RectI32::new(0, 0, 2560, 1600);
    const R_C: RectI32 = RectI32::new(0, -1080, 1920, 0);

    fn mon(id: &str, rect: RectI32) -> MonitorInfo {
        MonitorInfo {
            id: MonitorId::new(id),
            rect_px: rect,
            dpi: 96,
            refresh_hz: 60.0,
            primary: false,
        }
    }
    fn cfg(default: Option<&str>, profiles: Vec<MonitorProfile>) -> Config {
        Config {
            default_profile: default.map(|f| WallpaperSpec {
                file: PathBuf::from(f),
            }),
            monitors: profiles,
            ..Config::default()
        }
    }
    fn profile(id: &str, enabled: bool, file: Option<&str>, zoom: ZoomMode) -> MonitorProfile {
        MonitorProfile {
            id: MonitorId::new(id),
            enabled,
            file: file.map(PathBuf::from),
            zoom,
        }
    }
    fn run(file: &str, zoom: ZoomMode, rect: RectI32) -> Running {
        Running {
            file: PathBuf::from(file),
            zoom,
            rect,
        }
    }
    fn create(id: &str, rect: RectI32, file: &str, zoom: ZoomMode) -> Action {
        Action::Create {
            id: MonitorId::new(id),
            rect,
            wanted: Wanted {
                file: PathBuf::from(file),
                zoom,
            },
        }
    }
    fn state(items: Vec<(&str, Running)>) -> HashMap<MonitorId, Running> {
        items
            .into_iter()
            .map(|(id, r)| (MonitorId::new(id), r))
            .collect()
    }

    #[test]
    fn first_run_creates_every_monitor_with_the_fallback_zoom() {
        let m = [mon("A", R_A), mon("B", R_B)];
        let a = plan(
            &m,
            &cfg(Some("d.mp4"), vec![]),
            ZoomMode::Fit,
            &HashMap::new(),
            false,
        );
        assert_eq!(
            a,
            vec![
                create("A", R_A, "d.mp4", ZoomMode::Fit),
                create("B", R_B, "d.mp4", ZoomMode::Fit)
            ]
        );
    }

    #[test]
    fn nothing_to_do_when_in_sync() {
        let m = [mon("A", R_A), mon("B", R_B)];
        let c = cfg(Some("d.mp4"), vec![]);
        let r = state(vec![
            ("A", run("d.mp4", ZoomMode::Fill, R_A)),
            ("B", run("d.mp4", ZoomMode::Fill, R_B)),
        ]);
        assert!(plan(&m, &c, ZoomMode::Fill, &r, false).is_empty());
    }

    #[test]
    fn disabled_and_fileless_monitors_are_skipped() {
        let m = [mon("A", R_A), mon("B", R_B), mon("C", R_C)];
        let c = cfg(
            None,
            vec![
                profile("A", true, Some("a.mp4"), ZoomMode::Fill),
                profile("B", false, Some("b.mp4"), ZoomMode::Fill), // disabled
                profile("C", true, None, ZoomMode::Fill),           // no file anywhere
            ],
        );
        let a = plan(&m, &c, ZoomMode::Fill, &HashMap::new(), false);
        assert_eq!(a, vec![create("A", R_A, "a.mp4", ZoomMode::Fill)]);
    }

    #[test]
    fn per_monitor_file_and_zoom_override_the_default() {
        let m = [mon("A", R_A), mon("B", R_B)];
        let c = cfg(
            Some("d.mp4"),
            vec![profile("B", true, Some("b.mp4"), ZoomMode::Stretch)],
        );
        let a = plan(&m, &c, ZoomMode::Fit, &HashMap::new(), false);
        assert_eq!(
            a,
            vec![
                create("A", R_A, "d.mp4", ZoomMode::Fit),
                create("B", R_B, "b.mp4", ZoomMode::Stretch)
            ]
        );
    }

    #[test]
    fn unplugged_monitor_is_removed_and_a_new_one_created() {
        let m = [mon("B", R_B), mon("C", R_C)]; // A is gone, C is new
        let c = cfg(Some("d.mp4"), vec![]);
        let r = state(vec![
            ("A", run("d.mp4", ZoomMode::Fill, R_A)),
            ("B", run("d.mp4", ZoomMode::Fill, R_B)),
        ]);
        let a = plan(&m, &c, ZoomMode::Fill, &r, false);
        assert_eq!(
            a,
            vec![
                Action::Remove {
                    id: MonitorId::new("A")
                },
                create("C", R_C, "d.mp4", ZoomMode::Fill)
            ]
        );
    }

    #[test]
    fn rect_change_moves_and_reposition_all_forces_it() {
        let c = cfg(Some("d.mp4"), vec![]);
        let r = state(vec![("A", run("d.mp4", ZoomMode::Fill, R_A))]);
        // A moved from left-of-primary to above-primary (origin stays negative).
        let a = plan(&[mon("A", R_C)], &c, ZoomMode::Fill, &r, false);
        assert_eq!(
            a,
            vec![Action::Move {
                id: MonitorId::new("A"),
                rect: R_C
            }]
        );
        // Same rect, but the host moved: still repositioned.
        let a = plan(&[mon("A", R_A)], &c, ZoomMode::Fill, &r, true);
        assert_eq!(
            a,
            vec![Action::Move {
                id: MonitorId::new("A"),
                rect: R_A
            }]
        );
    }

    #[test]
    fn file_and_zoom_changes_reload_in_that_order() {
        let m = [mon("A", R_A)];
        let c = cfg(
            None,
            vec![profile("A", true, Some("new.mp4"), ZoomMode::Original)],
        );
        let r = state(vec![("A", run("old.mp4", ZoomMode::Fill, R_A))]);
        let a = plan(&m, &c, ZoomMode::Fill, &r, false);
        assert_eq!(
            a,
            vec![
                Action::Load {
                    id: MonitorId::new("A"),
                    file: PathBuf::from("new.mp4")
                },
                Action::Zoom {
                    id: MonitorId::new("A"),
                    zoom: ZoomMode::Original
                }
            ]
        );
    }

    #[test]
    fn disabling_or_clearing_the_file_removes_the_wallpaper() {
        let m = [mon("A", R_A)];
        let r = state(vec![("A", run("d.mp4", ZoomMode::Fill, R_A))]);
        let disabled = cfg(
            Some("d.mp4"),
            vec![profile("A", false, None, ZoomMode::Fill)],
        );
        let expect = vec![Action::Remove {
            id: MonitorId::new("A"),
        }];
        assert_eq!(plan(&m, &disabled, ZoomMode::Fill, &r, false), expect);
        let no_file = cfg(None, vec![]);
        assert_eq!(plan(&m, &no_file, ZoomMode::Fill, &r, false), expect);
    }

    #[test]
    fn removals_come_first_and_are_sorted() {
        let m = [mon("B", R_B)];
        let c = cfg(Some("d.mp4"), vec![]);
        let r = state(vec![
            ("Z", run("d.mp4", ZoomMode::Fill, R_A)),
            ("A", run("d.mp4", ZoomMode::Fill, R_C)),
        ]);
        let a = plan(&m, &c, ZoomMode::Fill, &r, false);
        assert_eq!(
            a,
            vec![
                Action::Remove {
                    id: MonitorId::new("A")
                },
                Action::Remove {
                    id: MonitorId::new("Z")
                },
                create("B", R_B, "d.mp4", ZoomMode::Fill)
            ]
        );
    }
}
