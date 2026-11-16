//! Audio policy (PRD F-40 to F-42), pure and unit-tested: exactly one screen may play sound
//! (`audio.source`), every other screen is silent and decodes no audio at all. The source is
//! never moved automatically: if its screen disappears, everything stays silent.
use super::config::AudioConfig;
use super::monitor::{MonitorId, MonitorInfo};
use serde::{Deserialize, Serialize};

/// Volume presets offered in the tray (percent).
pub const VOLUME_PRESETS: [u8; 5] = [0, 25, 50, 75, 100];

/// What one screen's player should do with audio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioState {
    /// Audio track selected and unmuted (`aid=auto`, `mute=no`); otherwise `aid=no`, `mute=yes`.
    pub on: bool,
    /// 0..=100; always 0 when `on` is false, so a silent screen has exactly one state.
    pub volume: u8,
}

impl AudioState {
    pub const OFF: Self = Self {
        on: false,
        volume: 0,
    };
}

/// The audio state for screen `id`: on only for the configured source.
pub fn state_for(audio: &AudioConfig, id: &MonitorId) -> AudioState {
    if audio.source.as_ref() == Some(id) {
        AudioState {
            on: true,
            volume: audio.volume.min(100),
        }
    } else {
        AudioState::OFF
    }
}

/// What LiteWall does while another app plays audio (PRD F-44).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OtherAudio {
    /// Silence LiteWall (volume 0, the audio track stays selected).
    #[default]
    Mute,
    /// Lower LiteWall to `duck_percent` (never above the chosen volume).
    Duck,
    /// Do nothing.
    Off,
}

/// The state a player should really get: `base` (from `state_for`) reduced while another app
/// plays audio. A silent screen stays silent.
pub fn effective_state(
    base: AudioState,
    other_active: bool,
    mode: OtherAudio,
    duck_percent: u8,
) -> AudioState {
    if !base.on || !other_active {
        return base;
    }
    match mode {
        OtherAudio::Off => base,
        OtherAudio::Mute => AudioState {
            on: true,
            volume: 0,
        },
        OtherAudio::Duck => AudioState {
            on: true,
            volume: base.volume.min(duck_percent.min(100)),
        },
    }
}

/// One audio session on the default output device, as read from Core Audio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SessionInfo {
    pub pid: u32,
    /// `AudioSessionStateActive`: a stream is running (not paused, not idle).
    pub active: bool,
    /// The "system sounds" session (notification dings).
    pub system: bool,
    /// Peak meter value 0.0..=1.0 for this session; `None` when the meter is not available.
    pub peak: Option<f32>,
}

/// Peak value above which a session counts as audible.
pub const PEAK_THRESHOLD: f32 = 0.0001;

/// Another application is really playing: active, not ours, not system sounds and, when the
/// meter can be read, showing a signal (a muted browser tab or a paused player is not).
pub fn counts_as_other_audio(s: &SessionInfo, own_pid: u32) -> bool {
    s.active
        && !s.system
        && s.pid != 0
        && s.pid != own_pid
        && s.peak.map_or(true, |p| p > PEAK_THRESHOLD)
}

/// Process id of the first session that counts as other audio.
pub fn first_other_audio(sessions: &[SessionInfo], own_pid: u32) -> Option<u32> {
    sessions
        .iter()
        .find(|s| counts_as_other_audio(s, own_pid))
        .map(|s| s.pid)
}

/// One-line description of all sessions for the log, e.g. `pid 4242 active signal; pid 7 idle silent`.
pub fn describe_sessions(sessions: &[SessionInfo], own_pid: u32) -> String {
    if sessions.is_empty() {
        return "no sessions".to_string();
    }
    sessions
        .iter()
        .map(|s| {
            let mut text = format!("pid {}", s.pid);
            if s.pid == own_pid {
                text.push_str(" (LiteWall)");
            }
            if s.system {
                text.push_str(" system-sounds");
            }
            text.push_str(if s.active { " active" } else { " idle" });
            // Only the category, not the value: the log line is written when it changes, and
            // the peak changes on every check.
            text.push_str(match s.peak {
                Some(p) if p > PEAK_THRESHOLD => " signal",
                Some(_) => " silent",
                None => " meter n/a",
            });
            text
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Number of consecutive quiet checks (2 s apart) before "another app is playing" is cleared.
/// Music has silent moments and apps pause for a second: muting is immediate, unmuting waits.
pub const QUIET_CHECKS_TO_RESUME: u32 = 3;

/// Turns the raw readings ("some other app is audible right now") into a stable flag.
#[derive(Clone, Debug, Default)]
pub struct OtherAudioTracker {
    active: bool,
    quiet: u32,
}

impl OtherAudioTracker {
    pub fn active(&self) -> bool {
        self.active
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    /// Feed one reading; returns the stable flag.
    pub fn observe(&mut self, audible: bool) -> bool {
        if audible {
            self.active = true;
            self.quiet = 0;
        } else if self.active {
            self.quiet += 1;
            if self.quiet >= QUIET_CHECKS_TO_RESUME {
                self.active = false;
                self.quiet = 0;
            }
        }
        self.active
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceStatus {
    /// No audio source is configured: silent everywhere (the default).
    NotSet,
    /// The source screen is connected.
    Present,
    /// A source is configured but its screen is not connected right now.
    Missing,
}

pub fn source_status(audio: &AudioConfig, monitors: &[MonitorInfo]) -> SourceStatus {
    match &audio.source {
        None => SourceStatus::NotSet,
        Some(id) if monitors.iter().any(|m| &m.id == id) => SourceStatus::Present,
        Some(_) => SourceStatus::Missing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::geometry::RectI32;

    fn id(s: &str) -> MonitorId {
        MonitorId::new(s)
    }
    fn mon(s: &str) -> MonitorInfo {
        MonitorInfo {
            id: id(s),
            rect_px: RectI32::new(0, 0, 1920, 1080),
            dpi: 96,
            refresh_hz: 60.0,
            primary: false,
        }
    }
    fn cfg(source: Option<&str>, volume: u8) -> AudioConfig {
        AudioConfig {
            source: source.map(id),
            volume,
            ..AudioConfig::default()
        }
    }

    #[test]
    fn default_config_is_silent_everywhere() {
        let c = AudioConfig::default();
        assert_eq!(state_for(&c, &id("A")), AudioState::OFF);
        assert_eq!(source_status(&c, &[mon("A")]), SourceStatus::NotSet);
    }

    #[test]
    fn only_the_source_screen_plays() {
        let c = cfg(Some("B"), 40);
        assert_eq!(state_for(&c, &id("A")), AudioState::OFF);
        assert_eq!(
            state_for(&c, &id("B")),
            AudioState {
                on: true,
                volume: 40
            }
        );
        assert_eq!(state_for(&c, &id("C")), AudioState::OFF);
    }

    #[test]
    fn volume_is_clamped_and_zero_volume_is_still_on() {
        assert_eq!(state_for(&cfg(Some("A"), 250), &id("A")).volume, 100);
        let zero = state_for(&cfg(Some("A"), 0), &id("A"));
        assert!(zero.on && zero.volume == 0);
    }

    #[test]
    fn a_missing_source_is_reported_and_never_moves() {
        let c = cfg(Some("B"), 40);
        assert_eq!(
            source_status(&c, &[mon("A"), mon("B")]),
            SourceStatus::Present
        );
        assert_eq!(source_status(&c, &[mon("A")]), SourceStatus::Missing);
        // Nobody else takes over while B is away.
        assert_eq!(state_for(&c, &id("A")), AudioState::OFF);
    }

    #[test]
    fn presets_are_ascending_and_end_at_100() {
        assert!(VOLUME_PRESETS.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(*VOLUME_PRESETS.last().unwrap(), 100);
    }

    #[test]
    fn effective_state_mutes_ducks_or_ignores_only_a_playing_source() {
        let on = AudioState {
            on: true,
            volume: 60,
        };
        // Nothing else playing: unchanged in every mode.
        for m in [OtherAudio::Mute, OtherAudio::Duck, OtherAudio::Off] {
            assert_eq!(effective_state(on, false, m, 20), on);
        }
        let v = |s: AudioState| (s.on, s.volume);
        assert_eq!(
            v(effective_state(on, true, OtherAudio::Mute, 20)),
            (true, 0)
        );
        assert_eq!(
            v(effective_state(on, true, OtherAudio::Duck, 20)),
            (true, 20)
        );
        assert_eq!(effective_state(on, true, OtherAudio::Off, 20), on);
        // Ducking never raises the volume, and the percentage is clamped.
        assert_eq!(effective_state(on, true, OtherAudio::Duck, 90).volume, 60);
        assert_eq!(effective_state(on, true, OtherAudio::Duck, 250).volume, 60);
        // A silent screen stays exactly OFF.
        assert_eq!(
            effective_state(AudioState::OFF, true, OtherAudio::Mute, 20),
            AudioState::OFF
        );
    }

    #[test]
    fn tracker_mutes_at_once_and_unmutes_after_three_quiet_checks() {
        let mut t = OtherAudioTracker::default();
        assert!(!t.observe(false));
        assert!(t.observe(true));
        assert!(t.observe(false));
        assert!(t.observe(false));
        // A sound in between restarts the count.
        assert!(t.observe(true));
        assert!(t.observe(false));
        assert!(t.observe(false));
        assert!(!t.observe(false));
        assert!(!t.active());
    }

    #[test]
    fn tracker_reset_clears_everything() {
        let mut t = OtherAudioTracker::default();
        t.observe(true);
        t.reset();
        assert!(!t.active());
        assert!(!t.observe(false));
    }

    fn session(pid: u32, active: bool, system: bool, peak: Option<f32>) -> SessionInfo {
        SessionInfo {
            pid,
            active,
            system,
            peak,
        }
    }

    #[test]
    fn only_active_audible_foreign_sessions_count() {
        let own = 100;
        assert!(counts_as_other_audio(
            &session(5, true, false, Some(0.3)),
            own
        ));
        assert!(!counts_as_other_audio(
            &session(5, false, false, Some(0.3)),
            own
        )); // idle
        assert!(!counts_as_other_audio(
            &session(own, true, false, Some(0.3)),
            own
        )); // ours
        assert!(!counts_as_other_audio(
            &session(0, true, false, Some(0.3)),
            own
        ));
        assert!(!counts_as_other_audio(
            &session(5, true, true, Some(0.3)),
            own
        )); // system
        assert!(!counts_as_other_audio(
            &session(5, true, false, Some(0.0)),
            own
        )); // silent
            // Meter unavailable: an active session is enough.
        assert!(counts_as_other_audio(&session(5, true, false, None), own));
    }

    #[test]
    fn first_other_audio_skips_ours_and_describe_lists_everything() {
        let own = 100;
        let list = [
            session(own, true, false, Some(0.5)),
            session(7, false, false, Some(0.0)),
            session(42, true, false, Some(0.31)),
            session(43, true, false, None),
        ];
        assert_eq!(first_other_audio(&list, own), Some(42));
        assert_eq!(first_other_audio(&list[..2], own), None);
        assert_eq!(first_other_audio(&[], own), None);
        let text = describe_sessions(&list, own);
        assert!(text.contains("pid 100 (LiteWall) active signal"), "{text}");
        assert!(text.contains("pid 7 idle silent"), "{text}");
        assert!(text.contains("pid 43 active meter n/a"), "{text}");
        assert_eq!(describe_sessions(&[], own), "no sessions");
        // The peak value itself must not change the text (it would defeat "log on change").
        let a = describe_sessions(&[session(42, true, false, Some(0.31))], own);
        let b = describe_sessions(&[session(42, true, false, Some(0.77))], own);
        assert_eq!(a, b);
    }
}
