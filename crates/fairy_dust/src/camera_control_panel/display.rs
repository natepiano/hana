//! Display state: mirrors the per-kind interaction sources, speed, and live
//! zoom direction the panel highlights. lagrange debounces the reported sources
//! (see `bevy_lagrange::OrbitCamReportingDebounce`), and the panel holds
//! released sources briefly so highlights can fade after input ends.

use std::time::Duration;

use bevy::prelude::*;
use bevy_lagrange::CameraInteractionSources;
use bevy_lagrange::ControlSpeed;
use bevy_lagrange::OrbitCamInteractionKind;
use bevy_lagrange::OrbitCamInteractionState;
use bevy_lagrange::ZoomDirection;

use super::constants::HIGHLIGHT_RELEASE_HOLD;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum RenderState {
    #[default]
    Idle,
    Pending,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ReleaseCountdowns {
    orbit: Option<Duration>,
    pan:   Option<Duration>,
    zoom:  Option<Duration>,
}

impl ReleaseCountdowns {
    const fn empty() -> Self {
        Self {
            orbit: None,
            pan:   None,
            zoom:  None,
        }
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(super) struct CameraGuidanceDisplayState {
    orbit:                   CameraInteractionSources,
    pan:                     CameraInteractionSources,
    zoom:                    CameraInteractionSources,
    orbit_speed:             Option<ControlSpeed>,
    pan_speed:               Option<ControlSpeed>,
    zoom_speed:              Option<ControlSpeed>,
    zoom_direction:          Option<ZoomDirection>,
    slow_mode_active:        bool,
    releases:                ReleaseCountdowns,
    pub(super) render_state: RenderState,
}

impl Default for CameraGuidanceDisplayState {
    fn default() -> Self { Self::from_display(CameraGuidanceDisplay::default()) }
}

impl CameraGuidanceDisplayState {
    pub(super) const fn from_display(display: CameraGuidanceDisplay) -> Self {
        Self {
            orbit:            display.orbit,
            pan:              display.pan,
            zoom:             display.zoom,
            orbit_speed:      display.orbit_speed,
            pan_speed:        display.pan_speed,
            zoom_speed:       display.zoom_speed,
            zoom_direction:   display.zoom_direction,
            slow_mode_active: display.slow_mode_active,
            releases:         ReleaseCountdowns::empty(),
            render_state:     RenderState::Idle,
        }
    }

    pub(super) const fn display(self) -> CameraGuidanceDisplay {
        CameraGuidanceDisplay {
            orbit:            self.orbit,
            pan:              self.pan,
            zoom:             self.zoom,
            orbit_speed:      self.orbit_speed,
            pan_speed:        self.pan_speed,
            zoom_speed:       self.zoom_speed,
            zoom_direction:   self.zoom_direction,
            slow_mode_active: self.slow_mode_active,
        }
    }

    /// Mirrors a kind's reported interaction sources and holds the displayed
    /// sources through the panel release window.
    pub(super) fn set_sources(
        &mut self,
        kind: OrbitCamInteractionKind,
        sources: CameraInteractionSources,
    ) {
        let (slot, release) = match kind {
            OrbitCamInteractionKind::Orbit => (&mut self.orbit, &mut self.releases.orbit),
            OrbitCamInteractionKind::Pan => (&mut self.pan, &mut self.releases.pan),
            OrbitCamInteractionKind::Zoom => (&mut self.zoom, &mut self.releases.zoom),
            _ => return,
        };

        if sources.is_empty() {
            if !slot.is_empty() {
                *release = Some(HIGHLIGHT_RELEASE_HOLD);
            }
            return;
        }

        if *slot != sources {
            *slot = sources;
            self.render_state = RenderState::Pending;
        }
        *release = None;
    }

    pub(super) fn tick_highlight_release(&mut self, delta: Duration) {
        let orbit_expired =
            tick_release_countdown(&mut self.orbit, &mut self.releases.orbit, delta);
        let pan_expired = tick_release_countdown(&mut self.pan, &mut self.releases.pan, delta);
        let zoom_expired = tick_release_countdown(&mut self.zoom, &mut self.releases.zoom, delta);

        if orbit_expired || pan_expired || zoom_expired {
            self.render_state = RenderState::Pending;
        }
    }

    /// Records the live zoom direction read from the camera input when a zoom
    /// interaction begins or changes. A `None` argument (zero-delta frame) keeps
    /// the last direction so the highlighted row stays correct while the zoom is
    /// reported.
    pub(super) fn set_zoom_direction(&mut self, zoom_direction: Option<ZoomDirection>) {
        let Some(zoom_direction) = zoom_direction else {
            return;
        };
        if self.zoom_direction != Some(zoom_direction) {
            self.zoom_direction = Some(zoom_direction);
            self.render_state = RenderState::Pending;
        }
    }

    pub(super) fn set_speed(&mut self, kind: OrbitCamInteractionKind, speed: Option<ControlSpeed>) {
        let slot = match kind {
            OrbitCamInteractionKind::Orbit => &mut self.orbit_speed,
            OrbitCamInteractionKind::Pan => &mut self.pan_speed,
            OrbitCamInteractionKind::Zoom => &mut self.zoom_speed,
            _ => return,
        };
        if *slot != speed {
            *slot = speed;
            self.render_state = RenderState::Pending;
        }
    }

    pub(super) const fn set_slow_mode_active(&mut self, active: bool) {
        if self.slow_mode_active != active {
            self.slow_mode_active = active;
            self.render_state = RenderState::Pending;
        }
    }
}

fn tick_release_countdown(
    sources: &mut CameraInteractionSources,
    release: &mut Option<Duration>,
    delta: Duration,
) -> bool {
    let Some(remaining) = release.as_mut() else {
        return false;
    };
    let next = remaining.saturating_sub(delta);
    if next == Duration::ZERO {
        *sources = CameraInteractionSources::NONE;
        *release = None;
        return true;
    }

    *remaining = next;
    false
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CameraGuidanceDisplay {
    pub(super) orbit:            CameraInteractionSources,
    pub(super) pan:              CameraInteractionSources,
    pub(super) zoom:             CameraInteractionSources,
    pub(super) orbit_speed:      Option<ControlSpeed>,
    pub(super) pan_speed:        Option<ControlSpeed>,
    pub(super) zoom_speed:       Option<ControlSpeed>,
    pub(super) zoom_direction:   Option<ZoomDirection>,
    pub(super) slow_mode_active: bool,
}

impl CameraGuidanceDisplay {
    pub(super) const fn from_camera_state(
        state: OrbitCamInteractionState,
        slow_mode_active: bool,
    ) -> Self {
        Self {
            orbit: state.orbit_sources(),
            pan: state.pan_sources(),
            zoom: state.zoom_sources(),
            orbit_speed: state.speed(OrbitCamInteractionKind::Orbit),
            pan_speed: state.speed(OrbitCamInteractionKind::Pan),
            zoom_speed: state.speed(OrbitCamInteractionKind::Zoom),
            zoom_direction: state.zoom_direction(),
            slow_mode_active,
        }
    }

    pub(super) const fn sources(self, kind: OrbitCamInteractionKind) -> CameraInteractionSources {
        match kind {
            OrbitCamInteractionKind::Orbit => self.orbit,
            OrbitCamInteractionKind::Pan => self.pan,
            OrbitCamInteractionKind::Zoom => self.zoom,
            _ => CameraInteractionSources::NONE,
        }
    }

    pub(super) const fn zoom_direction(self) -> Option<ZoomDirection> { self.zoom_direction }

    pub(super) const fn slow_mode_active(self) -> bool { self.slow_mode_active }

    pub(super) const fn speed(self, kind: OrbitCamInteractionKind) -> Option<ControlSpeed> {
        match kind {
            OrbitCamInteractionKind::Orbit => self.orbit_speed,
            OrbitCamInteractionKind::Pan => self.pan_speed,
            OrbitCamInteractionKind::Zoom => self.zoom_speed,
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_zoom_direction_updates_and_survives_zero_delta_frames() {
        let mut display = CameraGuidanceDisplayState::default();

        display.set_zoom_direction(Some(ZoomDirection::In));
        assert_eq!(display.display().zoom_direction, Some(ZoomDirection::In));
        assert_eq!(display.render_state, RenderState::Pending);

        // A zero-delta frame reports `None` and must keep the captured direction
        // so the highlighted row stays correct while the zoom is reported.
        display.render_state = RenderState::Idle;
        display.set_zoom_direction(None);
        assert_eq!(display.display().zoom_direction, Some(ZoomDirection::In));
        assert_eq!(display.render_state, RenderState::Idle);

        // Reversing direction (release zoom-in, press zoom-out) flips the tag and
        // requests a rebuild even though the source bit is unchanged.
        display.set_zoom_direction(Some(ZoomDirection::Out));
        assert_eq!(display.display().zoom_direction, Some(ZoomDirection::Out));
        assert_eq!(display.render_state, RenderState::Pending);
    }

    #[test]
    fn set_sources_marks_pending_only_on_change() {
        let mut display = CameraGuidanceDisplayState::default();

        display.set_sources(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::NONE,
        );
        assert_eq!(display.releases.orbit, None);
        assert_eq!(display.render_state, RenderState::Idle);

        display.set_sources(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::MOUSE,
        );
        assert_eq!(display.display().orbit, CameraInteractionSources::MOUSE);
        assert_eq!(display.render_state, RenderState::Pending);

        // Re-reporting the same sources does not request another rebuild.
        display.render_state = RenderState::Idle;
        display.set_sources(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::MOUSE,
        );
        assert_eq!(display.render_state, RenderState::Idle);

        // The release hold keeps the displayed sources stable until its
        // countdown elapses.
        display.set_sources(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::NONE,
        );
        assert_eq!(display.display().orbit, CameraInteractionSources::MOUSE);
        assert_eq!(display.releases.orbit, Some(HIGHLIGHT_RELEASE_HOLD));
        assert_eq!(display.render_state, RenderState::Idle);

        display.set_sources(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::MOUSE,
        );
        assert_eq!(display.display().orbit, CameraInteractionSources::MOUSE);
        assert_eq!(display.releases.orbit, None);
        assert_eq!(display.render_state, RenderState::Idle);
    }

    #[test]
    fn release_countdown_clears_sources_and_marks_pending() {
        let mut display = CameraGuidanceDisplayState::default();

        display.set_sources(
            OrbitCamInteractionKind::Pan,
            CameraInteractionSources::KEYBOARD,
        );
        display.render_state = RenderState::Idle;
        display.set_sources(OrbitCamInteractionKind::Pan, CameraInteractionSources::NONE);

        let partial_delta = HIGHLIGHT_RELEASE_HOLD / 2;
        display.tick_highlight_release(partial_delta);
        assert_eq!(display.display().pan, CameraInteractionSources::KEYBOARD);
        assert_eq!(
            display.releases.pan,
            Some(HIGHLIGHT_RELEASE_HOLD.saturating_sub(partial_delta)),
        );
        assert_eq!(display.render_state, RenderState::Idle);

        display.tick_highlight_release(HIGHLIGHT_RELEASE_HOLD);
        assert!(display.display().pan.is_empty());
        assert_eq!(display.releases.pan, None);
        assert_eq!(display.render_state, RenderState::Pending);
    }

    #[test]
    fn slow_mode_active_marks_pending_only_on_change() {
        let mut display = CameraGuidanceDisplayState::default();

        display.set_slow_mode_active(false);
        assert!(!display.display().slow_mode_active);
        assert_eq!(display.render_state, RenderState::Idle);

        display.set_slow_mode_active(true);
        assert!(display.display().slow_mode_active);
        assert_eq!(display.render_state, RenderState::Pending);

        display.render_state = RenderState::Idle;
        display.set_slow_mode_active(true);
        assert_eq!(display.render_state, RenderState::Idle);
    }
}
