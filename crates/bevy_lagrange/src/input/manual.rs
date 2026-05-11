use bevy::ecs::query::QueryEntityError;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::CoarseZoomDelta;
use super::ManualInputSource;
use super::OrbitCamInput;
use super::OrbitCamManual;
use super::OrbitDelta;
use super::PanDelta;
use super::SmoothZoomDelta;

/// System parameter for writing app-authored manual `OrbitCamInput`.
#[derive(SystemParam)]
pub struct OrbitCamManualInputWriter<'w, 's> {
    inputs: Query<'w, 's, &'static mut OrbitCamInput, With<OrbitCamManual>>,
}

impl OrbitCamManualInputWriter<'_, '_> {
    /// Returns a manual input writer for `camera`.
    ///
    /// # Errors
    ///
    /// Returns [`QueryEntityError`] when `camera` is not in manual input mode, does
    /// not have an `OrbitCamInput` component, or cannot be queried.
    pub fn get_mut(
        &mut self,
        camera: Entity,
        source: ManualInputSource,
    ) -> Result<OrbitCamManualInput<'_>, QueryEntityError> {
        self.inputs
            .get_mut(camera)
            .map(|input| OrbitCamManualInput { input, source })
    }
}

/// Mutable manual input handle for a single camera.
pub struct OrbitCamManualInput<'input> {
    input:  Mut<'input, OrbitCamInput>,
    source: ManualInputSource,
}

impl OrbitCamManualInput<'_> {
    /// Clears all camera intent for the frame.
    pub fn clear(&mut self) -> &mut Self {
        self.input.clear();
        self
    }

    /// Adds orbit intent in logical pixels.
    pub fn orbit_pixels(&mut self, delta: impl Into<OrbitDelta>) -> &mut Self {
        self.input.orbit_pixels_from(delta, self.source);
        self
    }

    /// Marks orbit input active for the frame without adding motion.
    pub fn orbit_active(&mut self) -> &mut Self {
        self.input.orbit_active_with_sources(self.source.sources());
        self
    }

    /// Adds pan intent in logical pixels.
    pub fn pan_pixels(&mut self, delta: impl Into<PanDelta>) -> &mut Self {
        self.input.pan_pixels_from(delta, self.source);
        self
    }

    /// Marks pan input active for the frame without adding motion.
    pub fn pan_active(&mut self) -> &mut Self {
        self.input.pan_active_with_sources(self.source.sources());
        self
    }

    /// Adds coarse zoom intent.
    pub fn zoom_coarse_amount(&mut self, delta: impl Into<CoarseZoomDelta>) -> &mut Self {
        self.input.zoom_coarse_from(delta, self.source);
        self
    }

    /// Adds smooth zoom intent.
    pub fn zoom_smooth_amount(&mut self, delta: impl Into<SmoothZoomDelta>) -> &mut Self {
        self.input.zoom_smooth_from(delta, self.source);
        self
    }

    /// Marks zoom input active for the frame without adding zoom.
    pub fn zoom_active(&mut self) -> &mut Self {
        self.input.zoom_active_with_sources(self.source.sources());
        self
    }
}
