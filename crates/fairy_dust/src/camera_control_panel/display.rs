//! Display state machine: tracks active and recently-held interaction sources
//! per orbit/pan/zoom slot so the panel can keep a source label visible for a
//! short hold window after the interaction ends.

use bevy::prelude::*;
use bevy_lagrange::CameraInteractionSources;
use bevy_lagrange::OrbitCamInteractionKind;
use bevy_lagrange::OrbitCamInteractionState;

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
    pub(super) render_state: RenderState,
}

impl Default for CameraGuidanceDisplayState {
    fn default() -> Self { Self::from_display(CameraGuidanceDisplay::default()) }
}

impl CameraGuidanceDisplayState {
    pub(super) const fn from_display(display: CameraGuidanceDisplay) -> Self {
        Self {
            orbit:        CameraGuidanceDisplaySlot::active(display.orbit),
            pan:          CameraGuidanceDisplaySlot::active(display.pan),
            zoom:         CameraGuidanceDisplaySlot::active(display.zoom),
            render_state: RenderState::Idle,
        }
    }

    pub(super) const fn display(self) -> CameraGuidanceDisplay {
        CameraGuidanceDisplay {
            orbit: self.orbit.sources(),
            pan:   self.pan.sources(),
            zoom:  self.zoom.sources(),
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
    pub(super) orbit: CameraInteractionSources,
    pub(super) pan:   CameraInteractionSources,
    pub(super) zoom:  CameraInteractionSources,
}

impl CameraGuidanceDisplay {
    pub(super) const fn from_interaction_state(state: OrbitCamInteractionState) -> Self {
        Self {
            orbit: state.orbit_sources(),
            pan:   state.pan_sources(),
            zoom:  state.zoom_sources(),
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

    pub(super) const fn all_sources(self) -> CameraInteractionSources {
        self.orbit.union(self.pan).union(self.zoom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
