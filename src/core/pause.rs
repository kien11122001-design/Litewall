//! Pause reasons, reducer, per-monitor tracker and debounce (PRD §6, §8). Pure.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Independent pause causes. Each source sets/clears only its own bit, so
/// overlapping causes (locked *and* on battery) resume correctly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PauseReasons(u16);

impl PauseReasons {
    pub const NONE: Self = Self(0);
    /// Manual pause; never auto-cleared.
    pub const USER: Self = Self(1 << 0);
    /// A window covers this monitor.
    pub const COVERED: Self = Self(1 << 1);
    /// D3D exclusive fullscreen / presentation mode.
    pub const FULLSCREEN: Self = Self(1 << 2);
    pub const BATTERY: Self = Self(1 << 3);
    /// Lock, screensaver, fast-user-switch.
    pub const LOCKED: Self = Self(1 << 4);
    pub const REMOTE: Self = Self(1 << 5);
    pub const SLEEP: Self = Self(1 << 6);
    /// Nothing to play / load error.
    pub const NO_SOURCE: Self = Self(1 << 7);
    /// Screens switched off (idle timeout, lid, before sleep). Also covers Modern Standby,
    /// where no classic suspend notification is sent.
    pub const DISPLAY_OFF: Self = Self(1 << 8);

    pub fn bits(self) -> u16 {
        self.0
    }
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub fn contains(self, o: Self) -> bool {
        self.0 & o.0 == o.0
    }
    pub fn union(self, o: Self) -> Self {
        Self(self.0 | o.0)
    }
    pub fn and(self, o: Self) -> Self {
        Self(self.0 & o.0)
    }
    pub fn insert(&mut self, o: Self) {
        self.0 |= o.0;
    }
    pub fn remove(&mut self, o: Self) {
        self.0 &= !o.0;
    }
    pub fn set(&mut self, o: Self, on: bool) {
        if on {
            self.insert(o)
        } else {
            self.remove(o)
        }
    }
    /// True if non-empty and every set bit is within `mask`.
    pub fn only_within(self, mask: Self) -> bool {
        !self.is_empty() && self.0 & !mask.0 == 0
    }
    /// Clear everything except the sticky manual-pause bit.
    pub fn clear_auto(&mut self) {
        self.0 &= Self::USER.0;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayState {
    Playing,
    Paused,
    /// P1 (F-43): `vid=no` while covered; audio keeps running.
    AudioOnly,
}

/// Reasons for which the video can be dropped while audio continues.
const VISUAL_ONLY: PauseReasons =
    PauseReasons(PauseReasons::COVERED.0 | PauseReasons::FULLSCREEN.0);

/// `Playing` iff no reasons. With `allow_audio_only`, a pause caused only by
/// covering/fullscreen becomes `AudioOnly` instead of `Paused`.
pub fn reduce(reasons: PauseReasons, allow_audio_only: bool) -> PlayState {
    if reasons.is_empty() {
        PlayState::Playing
    } else if allow_audio_only && reasons.only_within(VISUAL_ONLY) {
        PlayState::AudioOnly
    } else {
        PlayState::Paused
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PauseScope {
    #[default]
    ThisMonitor,
    AllMonitors,
}

/// Holds global and per-monitor reasons and computes the effective set.
#[derive(Debug, Default)]
pub struct PauseTracker {
    scope: PauseScope,
    global: PauseReasons,
    per: HashMap<String, PauseReasons>,
}

impl PauseTracker {
    pub fn new(scope: PauseScope) -> Self {
        Self {
            scope,
            ..Self::default()
        }
    }
    pub fn set_scope(&mut self, scope: PauseScope) {
        self.scope = scope;
    }
    pub fn add_monitor(&mut self, id: &str) {
        self.per.entry(id.to_string()).or_default();
    }
    pub fn remove_monitor(&mut self, id: &str) {
        self.per.remove(id);
    }
    pub fn set_global(&mut self, flag: PauseReasons, on: bool) {
        self.global.set(flag, on);
    }
    pub fn set_monitor(&mut self, id: &str, flag: PauseReasons, on: bool) {
        self.per.entry(id.to_string()).or_default().set(flag, on);
    }
    pub fn global(&self) -> PauseReasons {
        self.global
    }
    /// Reasons that currently apply to `id`.
    pub fn effective(&self, id: &str) -> PauseReasons {
        let mut r = self.global;
        if let Some(m) = self.per.get(id) {
            r = r.union(*m);
        }
        if self.scope == PauseScope::AllMonitors {
            for m in self.per.values() {
                r = r.union(m.and(VISUAL_ONLY));
            }
        }
        r
    }
    pub fn state(&self, id: &str, allow_audio_only: bool) -> PlayState {
        reduce(self.effective(id), allow_audio_only)
    }
}

/// Debounce for the COVERED signal: pause only after `pause_after` of continuous
/// "covered", resume only after `resume_after` of continuous "uncovered".
/// Time is injected (monotonic `Duration`), so this is fully deterministic.
#[derive(Clone, Debug)]
pub struct Hysteresis {
    pause_after: Duration,
    resume_after: Duration,
    stable: bool,
    pending: Option<(bool, Duration)>, // (target, since)
}

impl Hysteresis {
    pub fn new(pause_after: Duration, resume_after: Duration) -> Self {
        Self {
            pause_after,
            resume_after,
            stable: false,
            pending: None,
        }
    }
    /// Spec defaults: 250 ms to pause, 100 ms to resume.
    pub fn with_defaults() -> Self {
        Self::new(Duration::from_millis(250), Duration::from_millis(100))
    }
    pub fn stable(&self) -> bool {
        self.stable
    }
    fn delay_for(&self, target: bool) -> Duration {
        if target {
            self.pause_after
        } else {
            self.resume_after
        }
    }
    /// Feed the raw (undebounced) value at time `now`; returns the debounced value.
    pub fn observe(&mut self, raw: bool, now: Duration) -> bool {
        if raw == self.stable {
            self.pending = None;
            return self.stable;
        }
        let since = match self.pending {
            Some((t, s)) if t == raw => s,
            _ => {
                self.pending = Some((raw, now));
                now
            }
        };
        if now.saturating_sub(since) >= self.delay_for(raw) {
            self.stable = raw;
            self.pending = None;
        }
        self.stable
    }
    /// When a one-shot timer should call `observe` again (None = no timer needed).
    pub fn next_deadline(&self) -> Option<Duration> {
        self.pending.map(|(t, since)| since + self.delay_for(t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn empty_is_playing_any_reason_is_paused() {
        assert_eq!(reduce(PauseReasons::NONE, false), PlayState::Playing);
        assert_eq!(reduce(PauseReasons::LOCKED, false), PlayState::Paused);
        assert_eq!(reduce(PauseReasons::NO_SOURCE, true), PlayState::Paused);
    }

    #[test]
    fn overlapping_reasons_resume_only_when_all_clear() {
        let mut r = PauseReasons::NONE;
        r.insert(PauseReasons::LOCKED);
        r.insert(PauseReasons::BATTERY);
        r.remove(PauseReasons::LOCKED);
        assert_eq!(reduce(r, false), PlayState::Paused);
        r.remove(PauseReasons::BATTERY);
        assert_eq!(reduce(r, false), PlayState::Playing);
    }

    #[test]
    fn user_pause_is_sticky() {
        let mut r = PauseReasons::USER
            .union(PauseReasons::COVERED)
            .union(PauseReasons::SLEEP);
        r.clear_auto();
        assert_eq!(r, PauseReasons::USER);
        assert_eq!(reduce(r, true), PlayState::Paused); // never AudioOnly
    }

    #[test]
    fn audio_only_only_for_visual_reasons() {
        assert_eq!(reduce(PauseReasons::COVERED, true), PlayState::AudioOnly);
        assert_eq!(
            reduce(PauseReasons::COVERED.union(PauseReasons::FULLSCREEN), true),
            PlayState::AudioOnly
        );
        assert_eq!(reduce(PauseReasons::COVERED, false), PlayState::Paused);
        assert_eq!(
            reduce(PauseReasons::COVERED.union(PauseReasons::LOCKED), true),
            PlayState::Paused
        );
    }

    #[test]
    fn tracker_this_monitor_scope() {
        let mut t = PauseTracker::new(PauseScope::ThisMonitor);
        t.add_monitor("A");
        t.add_monitor("B");
        t.set_monitor("A", PauseReasons::COVERED, true);
        assert_eq!(t.state("A", false), PlayState::Paused);
        assert_eq!(t.state("B", false), PlayState::Playing);
        t.set_global(PauseReasons::LOCKED, true);
        assert_eq!(t.state("B", false), PlayState::Paused);
        t.set_global(PauseReasons::LOCKED, false);
        assert_eq!(t.state("B", false), PlayState::Playing);
    }

    #[test]
    fn tracker_all_monitors_scope_copies_covered_only() {
        let mut t = PauseTracker::new(PauseScope::AllMonitors);
        t.add_monitor("A");
        t.add_monitor("B");
        t.set_monitor("A", PauseReasons::COVERED, true);
        assert_eq!(t.state("B", false), PlayState::Paused);
        // Non-visual per-monitor reasons (NO_SOURCE) must not leak to other monitors.
        t.set_monitor("A", PauseReasons::COVERED, false);
        t.set_monitor("A", PauseReasons::NO_SOURCE, true);
        assert_eq!(t.state("B", false), PlayState::Playing);
        // Removing the covered monitor releases the others.
        t.set_monitor("A", PauseReasons::COVERED, true);
        t.remove_monitor("A");
        assert_eq!(t.state("B", false), PlayState::Playing);
    }

    #[test]
    fn hysteresis_pauses_after_delay_only() {
        let mut h = Hysteresis::with_defaults();
        assert!(!h.observe(true, MS(0)));
        assert_eq!(h.next_deadline(), Some(MS(250)));
        assert!(!h.observe(true, MS(249)));
        assert!(h.observe(true, MS(250)));
        assert_eq!(h.next_deadline(), None);
    }

    #[test]
    fn hysteresis_flapping_never_pauses() {
        let mut h = Hysteresis::with_defaults();
        // Alt-Tab animation: covered for 200 ms, uncovered briefly, repeat.
        let mut t = 0;
        for _ in 0..10 {
            assert!(!h.observe(true, MS(t)));
            assert!(!h.observe(true, MS(t + 200)));
            assert!(!h.observe(false, MS(t + 210)));
            t += 220;
        }
    }

    #[test]
    fn hysteresis_resumes_after_100ms() {
        let mut h = Hysteresis::with_defaults();
        h.observe(true, MS(0));
        assert!(h.observe(true, MS(300)));
        assert!(h.observe(false, MS(400)));
        assert!(h.observe(false, MS(499)));
        assert!(!h.observe(false, MS(500)));
    }
}
