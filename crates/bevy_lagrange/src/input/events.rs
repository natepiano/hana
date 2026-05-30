use bevy::prelude::*;

use super::CameraInputMetricKind;
use super::CameraInteractionSources;
use super::ControlSpeed;

/// Semantic kind of an `OrbitCam` interaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamInteractionKind {
    /// Orbit interaction.
    Orbit,
    /// Pan interaction.
    Pan,
    /// Zoom interaction.
    Zoom,
}

/// Emitted when an `OrbitCam` interaction starts.
#[derive(Clone, Copy, Debug, EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct OrbitCamInteractionStarted {
    /// Camera entity whose interaction started.
    #[event_target]
    pub camera:  Entity,
    /// Kind of interaction that started.
    pub kind:    OrbitCamInteractionKind,
    /// Sources that contributed to the interaction.
    pub sources: CameraInteractionSources,
}

/// Emitted when an `OrbitCam` interaction ends.
#[derive(Clone, Copy, Debug, EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct OrbitCamInteractionEnded {
    /// Camera entity whose interaction ended.
    #[event_target]
    pub camera:  Entity,
    /// Kind of interaction that ended.
    pub kind:    OrbitCamInteractionKind,
    /// Sources that contributed before the interaction ended.
    pub sources: CameraInteractionSources,
}

/// Emitted when an active `OrbitCam` interaction's source set changes.
#[derive(Clone, Copy, Debug, EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct OrbitCamInteractionSourcesChanged {
    /// Camera entity whose source attribution changed.
    #[event_target]
    pub camera:   Entity,
    /// Kind of interaction whose sources changed.
    pub kind:     OrbitCamInteractionKind,
    /// Sources active before the change.
    pub previous: CameraInteractionSources,
    /// Sources active after the change.
    pub current:  CameraInteractionSources,
}

impl OrbitCamInteractionSourcesChanged {
    /// Returns sources that joined this interaction.
    #[must_use]
    pub const fn added_sources(&self) -> CameraInteractionSources {
        self.current.difference(self.previous)
    }

    /// Returns sources that left this interaction.
    #[must_use]
    pub const fn removed_sources(&self) -> CameraInteractionSources {
        self.previous.difference(self.current)
    }
}

/// Emitted when an active `OrbitCam` interaction switches speed variant — for
/// the gamepad preset, when the `rb`/`lb` modifier is pressed or released
/// mid-interaction without changing the source set.
#[derive(Clone, Copy, Debug, EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct OrbitCamInteractionSpeedChanged {
    /// Camera entity whose interaction speed changed.
    #[event_target]
    pub camera: Entity,
    /// Kind of interaction whose speed changed.
    pub kind:   OrbitCamInteractionKind,
    /// Speed variant active after the change.
    pub speed:  ControlSpeed,
}

/// Emitted when logical input metrics are required but unavailable for a camera.
#[derive(Clone, Copy, Debug, EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct CameraInputMetricsMissing {
    /// Camera entity missing metrics.
    #[event_target]
    pub camera: Entity,
    /// Metric that could not be resolved.
    pub metric: CameraInputMetricKind,
}
