//! Display state machine: tracks active and recently-held interaction sources
//! per orbit/pan/zoom slot so the panel can keep a source label visible for a
//! short hold window after the interaction ends.

use bevy::prelude::*;
use bevy_lagrange::CameraInteractionSources;
use bevy_lagrange::ControlSpeed;
use bevy_lagrange::OrbitCamInteractionKind;
use bevy_lagrange::OrbitCamInteractionState;
use bevy_lagrange::ZoomDirection;

use super::constants::SOURCE_HOLD_SECONDS;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum RenderState {
    #[default]
    Idle,
    Pending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DisplayChange {
    Unchanged,
    Changed,
}

impl DisplayChange {
    const fn is_changed(self) -> bool { matches!(self, Self::Changed) }
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(super) struct CameraGuidanceDisplayState {
    orbit:                   CameraGuidanceDisplaySlot,
    pan:                     CameraGuidanceDisplaySlot,
    zoom:                    CameraGuidanceDisplaySlot,
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
            orbit:          CameraGuidanceDisplaySlot::active(display.orbit),
            pan:            CameraGuidanceDisplaySlot::active(display.pan),
            zoom:           CameraGuidanceDisplaySlot::active(display.zoom),
            orbit_speed:    display.orbit_speed,
            pan_speed:      display.pan_speed,
            zoom_speed:     display.zoom_speed,
            zoom_direction: display.zoom_direction,
            render_state:   RenderState::Idle,
        }
    }

    pub(super) const fn display(self) -> CameraGuidanceDisplay {
        CameraGuidanceDisplay {
            orbit:          self.orbit.sources(),
            pan:            self.pan.sources(),
            zoom:           self.zoom.sources(),
            orbit_speed:    self.orbit_speed,
            pan_speed:      self.pan_speed,
            zoom_speed:     self.zoom_speed,
            zoom_direction: self.zoom_direction,
        }
    }

    /// Records the live zoom direction read from the camera input when a zoom
    /// interaction begins or changes. A `None` argument (zero-delta frame) keeps
    /// the last direction so the held row stays correct through the hold window.
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

    pub(super) fn activate(
        &mut self,
        kind: OrbitCamInteractionKind,
        sources: CameraInteractionSources,
        now: f32,
    ) {
        let Some(slot) = self.slot_mut(kind) else {
            return;
        };
        if slot.activate(sources, now) == DisplayChange::Changed {
            self.render_state = RenderState::Pending;
        }
    }

    pub(super) fn hold(
        &mut self,
        kind: OrbitCamInteractionKind,
        sources: CameraInteractionSources,
        now: f32,
    ) {
        let Some(slot) = self.slot_mut(kind) else {
            return;
        };
        if slot.hold(sources, now) == DisplayChange::Changed {
            self.render_state = RenderState::Pending;
        }
    }

    pub(super) fn expire_held_sources(&mut self, now: f32) {
        let orbit = self.orbit.expire(now);
        let pan = self.pan.expire(now);
        let zoom = self.zoom.expire(now);
        if orbit.is_changed() || pan.is_changed() || zoom.is_changed() {
            self.render_state = RenderState::Pending;
        }
    }

    const fn slot_mut(
        &mut self,
        kind: OrbitCamInteractionKind,
    ) -> Option<&mut CameraGuidanceDisplaySlot> {
        match kind {
            OrbitCamInteractionKind::Orbit => Some(&mut self.orbit),
            OrbitCamInteractionKind::Pan => Some(&mut self.pan),
            OrbitCamInteractionKind::Zoom => Some(&mut self.zoom),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct CameraGuidanceDisplaySlot {
    active_sources: CameraInteractionSources,
    held_sources:   CameraInteractionSources,
    held_until:     Option<f32>,
}

impl CameraGuidanceDisplaySlot {
    const fn active(sources: CameraInteractionSources) -> Self {
        Self {
            active_sources: sources,
            held_sources:   CameraInteractionSources::NONE,
            held_until:     None,
        }
    }

    const fn sources(self) -> CameraInteractionSources {
        self.active_sources.union(self.held_sources)
    }

    fn activate(&mut self, sources: CameraInteractionSources, now: f32) -> DisplayChange {
        let before = self.sources();
        let inactive_sources = self.active_sources.difference(sources);

        self.active_sources = sources;
        self.held_sources = self
            .held_sources
            .union(inactive_sources)
            .difference(sources);
        if !inactive_sources.is_empty() {
            self.held_until = Some(now + SOURCE_HOLD_SECONDS);
        }
        if self.held_sources.is_empty() {
            self.held_until = None;
        }

        if before == self.sources() {
            DisplayChange::Unchanged
        } else {
            DisplayChange::Changed
        }
    }

    fn hold(&mut self, sources: CameraInteractionSources, now: f32) -> DisplayChange {
        let before = self.sources();

        self.active_sources = self.active_sources.difference(sources);
        self.held_sources = self.held_sources.union(sources);
        if !sources.is_empty() {
            self.held_until = Some(now + SOURCE_HOLD_SECONDS);
        }

        if before == self.sources() {
            DisplayChange::Unchanged
        } else {
            DisplayChange::Changed
        }
    }

    fn expire(&mut self, now: f32) -> DisplayChange {
        if self.held_until.is_none_or(|held_until| now < held_until) {
            return DisplayChange::Unchanged;
        }

        self.held_until = None;
        if self.held_sources.is_empty() {
            return DisplayChange::Unchanged;
        }

        self.held_sources = CameraInteractionSources::NONE;
        DisplayChange::Changed
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
            zoom_direction: None,
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
        // so the held row stays correct through the hold window, with no rebuild.
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
    fn ended_source_is_held_until_expiry() {
        let mut display = CameraGuidanceDisplayState::default();

        display.activate(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::SMOOTH_SCROLL,
            1.0,
        );
        assert_eq!(display.render_state, RenderState::Pending);
        assert_eq!(
            display.display().orbit,
            CameraInteractionSources::SMOOTH_SCROLL
        );

        display.render_state = RenderState::Idle;
        display.hold(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::SMOOTH_SCROLL,
            1.0,
        );
        assert_eq!(display.render_state, RenderState::Idle);

        display.expire_held_sources(1.14);
        assert_eq!(display.render_state, RenderState::Idle);
        assert_eq!(
            display.display().orbit,
            CameraInteractionSources::SMOOTH_SCROLL
        );

        display.expire_held_sources(1.15);
        assert_eq!(display.render_state, RenderState::Pending);
        assert!(display.display().orbit.is_empty());
    }

    #[test]
    fn repeated_scroll_edges_do_not_request_rebuilds_before_expiry() {
        let mut display = CameraGuidanceDisplayState::default();

        display.activate(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::SMOOTH_SCROLL,
            1.0,
        );
        display.render_state = RenderState::Idle;
        display.hold(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::SMOOTH_SCROLL,
            1.0,
        );
        display.activate(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::SMOOTH_SCROLL,
            1.05,
        );
        display.hold(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::SMOOTH_SCROLL,
            1.1,
        );

        assert_eq!(display.render_state, RenderState::Idle);
        assert_eq!(
            display.display().orbit,
            CameraInteractionSources::SMOOTH_SCROLL
        );
    }

    #[test]
    fn alternating_sources_hold_union_until_expiry() {
        let mut display = CameraGuidanceDisplayState::default();

        display.activate(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::MOUSE,
            1.0,
        );
        display.render_state = RenderState::Idle;

        display.activate(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::SMOOTH_SCROLL,
            1.05,
        );
        assert_eq!(display.render_state, RenderState::Pending);
        assert_eq!(
            display.display().orbit,
            CameraInteractionSources::MOUSE.union(CameraInteractionSources::SMOOTH_SCROLL)
        );

        display.render_state = RenderState::Idle;
        display.activate(
            OrbitCamInteractionKind::Orbit,
            CameraInteractionSources::MOUSE,
            1.1,
        );
        assert_eq!(display.render_state, RenderState::Idle);
        assert_eq!(
            display.display().orbit,
            CameraInteractionSources::MOUSE.union(CameraInteractionSources::SMOOTH_SCROLL)
        );

        display.expire_held_sources(1.24);
        assert_eq!(display.render_state, RenderState::Idle);
        assert_eq!(
            display.display().orbit,
            CameraInteractionSources::MOUSE.union(CameraInteractionSources::SMOOTH_SCROLL)
        );

        display.expire_held_sources(1.25);
        assert_eq!(display.render_state, RenderState::Pending);
        assert_eq!(display.display().orbit, CameraInteractionSources::MOUSE);
    }
}
