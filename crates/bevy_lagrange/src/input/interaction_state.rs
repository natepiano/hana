use bevy::prelude::*;

use super::CameraInteractionSources;
use super::ControlSpeed;
use super::OrbitCamInteractionKind;
use super::ZoomDirection;

/// Read-only state describing the active interaction for an `OrbitCam`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
pub struct OrbitCamInteractionState {
    orbit:          CameraInteractionSources,
    pan:            CameraInteractionSources,
    zoom:           CameraInteractionSources,
    orbit_speed:    Option<ControlSpeed>,
    pan_speed:      Option<ControlSpeed>,
    zoom_speed:     Option<ControlSpeed>,
    zoom_direction: Option<ZoomDirection>,
}

impl OrbitCamInteractionState {
    /// Returns `true` when any interaction is active.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        !self.orbit.is_empty() || !self.pan.is_empty() || !self.zoom.is_empty()
    }

    /// Returns `true` when `kind` is active.
    #[must_use]
    pub const fn is_kind_active(&self, kind: OrbitCamInteractionKind) -> bool {
        !self.sources(kind).is_empty()
    }

    /// Returns the sources currently contributing to `kind`.
    #[must_use]
    pub const fn sources(&self, kind: OrbitCamInteractionKind) -> CameraInteractionSources {
        match kind {
            OrbitCamInteractionKind::Orbit => self.orbit,
            OrbitCamInteractionKind::Pan => self.pan,
            OrbitCamInteractionKind::Zoom => self.zoom,
        }
    }

    /// Returns the sources currently contributing to orbit input.
    #[must_use]
    pub const fn orbit_sources(&self) -> CameraInteractionSources { self.orbit }

    /// Returns the sources currently contributing to pan input.
    #[must_use]
    pub const fn pan_sources(&self) -> CameraInteractionSources { self.pan }

    /// Returns the sources currently contributing to zoom input.
    #[must_use]
    pub const fn zoom_sources(&self) -> CameraInteractionSources { self.zoom }

    /// Returns the reported speed variant for `kind`, or `None` while a fresh
    /// gamepad interaction has not yet settled under the reporting-speed
    /// debounce. `Slow` is reported immediately; only the return to `Normal`
    /// waits out the settle window.
    #[must_use]
    pub const fn speed(&self, kind: OrbitCamInteractionKind) -> Option<ControlSpeed> {
        match kind {
            OrbitCamInteractionKind::Orbit => self.orbit_speed,
            OrbitCamInteractionKind::Pan => self.pan_speed,
            OrbitCamInteractionKind::Zoom => self.zoom_speed,
        }
    }

    /// Returns the direction of the active zoom interaction, or `None` when no
    /// zoom is active. Held through the reporting-debounce window so a control
    /// panel can light the engaged direction's row, and reversing direction
    /// updates it immediately without waiting on the debounce.
    #[must_use]
    pub const fn zoom_direction(&self) -> Option<ZoomDirection> { self.zoom_direction }

    pub(crate) const fn set_sources(
        &mut self,
        kind: OrbitCamInteractionKind,
        sources: CameraInteractionSources,
    ) {
        match kind {
            OrbitCamInteractionKind::Orbit => self.orbit = sources,
            OrbitCamInteractionKind::Pan => self.pan = sources,
            OrbitCamInteractionKind::Zoom => self.zoom = sources,
        }
    }

    pub(crate) const fn set_speed(
        &mut self,
        kind: OrbitCamInteractionKind,
        speed: Option<ControlSpeed>,
    ) {
        match kind {
            OrbitCamInteractionKind::Orbit => self.orbit_speed = speed,
            OrbitCamInteractionKind::Pan => self.pan_speed = speed,
            OrbitCamInteractionKind::Zoom => self.zoom_speed = speed,
        }
    }

    pub(crate) const fn set_zoom_direction(&mut self, zoom_direction: Option<ZoomDirection>) {
        self.zoom_direction = zoom_direction;
    }
}
