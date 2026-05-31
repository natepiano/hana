//! Display state: mirrors the per-kind interaction sources, speed, and live
//! zoom direction the panel highlights. lagrange debounces the reported sources
//! (see `bevy_lagrange::OrbitCamReportingDebounce`), so the panel holds nothing
//! itself — it stores whatever lagrange reports and rebuilds when that changes.

use bevy::prelude::*;
use bevy_lagrange::CameraInteractionSources;
use bevy_lagrange::ControlSpeed;
use bevy_lagrange::OrbitCamInteractionKind;
use bevy_lagrange::OrbitCamInteractionState;
use bevy_lagrange::ZoomDirection;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum RenderState {
    #[default]
    Idle,
    Pending,
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
    pub(super) render_state: RenderState,
}

impl Default for CameraGuidanceDisplayState {
    fn default() -> Self { Self::from_display(CameraGuidanceDisplay::default()) }
}

impl CameraGuidanceDisplayState {
    pub(super) const fn from_display(display: CameraGuidanceDisplay) -> Self {
        Self {
            orbit:          display.orbit,
            pan:            display.pan,
            zoom:           display.zoom,
            orbit_speed:    display.orbit_speed,
            pan_speed:      display.pan_speed,
            zoom_speed:     display.zoom_speed,
            zoom_direction: display.zoom_direction,
            render_state:   RenderState::Idle,
        }
    }

    pub(super) const fn display(self) -> CameraGuidanceDisplay {
        CameraGuidanceDisplay {
            orbit:          self.orbit,
            pan:            self.pan,
            zoom:           self.zoom,
            orbit_speed:    self.orbit_speed,
            pan_speed:      self.pan_speed,
            zoom_speed:     self.zoom_speed,
            zoom_direction: self.zoom_direction,
        }
    }

    /// Mirrors a kind's reported interaction sources. lagrange has already
    /// debounced them, so the panel stores them verbatim and rebuilds on change.
    pub(super) fn set_sources(
        &mut self,
        kind: OrbitCamInteractionKind,
        sources: CameraInteractionSources,
    ) {
        let slot = match kind {
            OrbitCamInteractionKind::Orbit => &mut self.orbit,
            OrbitCamInteractionKind::Pan => &mut self.pan,
            OrbitCamInteractionKind::Zoom => &mut self.zoom,
            _ => return,
        };
        if *slot != sources {
            *slot = sources;
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
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CameraGuidanceDisplay {
    pub(super) orbit:          CameraInteractionSources,
    pub(super) pan:            CameraInteractionSources,
    pub(super) zoom:           CameraInteractionSources,
    pub(super) orbit_speed:    Option<ControlSpeed>,
    pub(super) pan_speed:      Option<ControlSpeed>,
    pub(super) zoom_speed:     Option<ControlSpeed>,
    pub(super) zoom_direction: Option<ZoomDirection>,
}

impl CameraGuidanceDisplay {
    pub(super) const fn from_interaction_state(state: OrbitCamInteractionState) -> Self {
        Self {
            orbit:          state.orbit_sources(),
            pan:            state.pan_sources(),
            zoom:           state.zoom_sources(),
            orbit_speed:    state.speed(OrbitCamInteractionKind::Orbit),
            pan_speed:      state.speed(OrbitCamInteractionKind::Pan),
            zoom_speed:     state.speed(OrbitCamInteractionKind::Zoom),
            zoom_direction: state.zoom_direction(),
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

        // Clearing the sources (lagrange has ended the interaction) rebuilds.
        display.set_sources(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::NONE,
        );
        assert!(display.display().orbit.is_empty());
        assert_eq!(display.render_state, RenderState::Pending);
    }
}
