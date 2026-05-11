use bevy::prelude::*;

use super::CameraInteractionSources;
use super::ManualInputSource;

/// Orbit motion expressed in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct OrbitDelta(Vec2);

impl From<Vec2> for OrbitDelta {
    fn from(value: Vec2) -> Self { Self(value) }
}

impl OrbitDelta {
    /// Returns the logical-pixel delta.
    #[must_use]
    pub const fn pixels(self) -> Vec2 { self.0 }
}

/// Pan motion expressed in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct PanDelta(Vec2);

impl From<Vec2> for PanDelta {
    fn from(value: Vec2) -> Self { Self(value) }
}

impl PanDelta {
    /// Returns the logical-pixel delta.
    #[must_use]
    pub const fn pixels(self) -> Vec2 { self.0 }
}

/// Coarse zoom amount from stepped sources such as mouse wheels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct CoarseZoomDelta(f32);

impl From<f32> for CoarseZoomDelta {
    fn from(value: f32) -> Self { Self(value) }
}

impl CoarseZoomDelta {
    /// Returns the zoom amount.
    #[must_use]
    pub const fn amount(self) -> f32 { self.0 }
}

/// Smooth zoom amount from continuous sources such as pixel scroll or pinch.
#[derive(Clone, Copy, Debug, Default, PartialEq, Reflect)]
pub struct SmoothZoomDelta(f32);

impl From<f32> for SmoothZoomDelta {
    fn from(value: f32) -> Self { Self(value) }
}

impl SmoothZoomDelta {
    /// Returns the zoom amount.
    #[must_use]
    pub const fn amount(self) -> f32 { self.0 }
}

/// Semantic per-frame camera input consumed by the orbit controller.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Component, Default)]
pub struct OrbitCamInput {
    orbit:        OrbitDelta,
    pan:          PanDelta,
    zoom_coarse:  CoarseZoomDelta,
    zoom_smooth:  SmoothZoomDelta,
    sources:      CameraInteractionSources,
    orbit_active: bool,
    pan_active:   bool,
    zoom_active:  bool,
}

impl OrbitCamInput {
    /// Returns the orbit delta.
    #[must_use]
    pub const fn orbit(&self) -> OrbitDelta { self.orbit }

    /// Returns the pan delta.
    #[must_use]
    pub const fn pan(&self) -> PanDelta { self.pan }

    /// Returns the coarse zoom delta.
    #[must_use]
    pub const fn zoom_coarse(&self) -> CoarseZoomDelta { self.zoom_coarse }

    /// Returns the smooth zoom delta.
    #[must_use]
    pub const fn zoom_smooth(&self) -> SmoothZoomDelta { self.zoom_smooth }

    /// Returns all active sources for this frame.
    #[must_use]
    pub const fn sources(&self) -> CameraInteractionSources { self.sources }

    /// Returns `true` when the frame carries any camera intent.
    #[must_use]
    pub const fn has_input(&self) -> bool {
        self.orbit_active || self.pan_active || self.zoom_active
    }

    /// Returns `true` when the frame carries orbit intent.
    #[must_use]
    pub const fn has_orbit(&self) -> bool { self.orbit_active }

    /// Returns `true` when the frame carries pan intent.
    #[must_use]
    pub const fn has_pan(&self) -> bool { self.pan_active }

    /// Returns `true` when the frame carries zoom intent.
    #[must_use]
    pub const fn has_zoom(&self) -> bool { self.zoom_active }

    pub(super) fn clear(&mut self) -> &mut Self {
        *self = Self::default();
        self
    }

    pub(super) fn orbit_pixels_from(
        &mut self,
        delta: impl Into<OrbitDelta>,
        source: ManualInputSource,
    ) -> &mut Self {
        let delta = delta.into();
        self.orbit.0 += delta.0;
        self.orbit_active = true;
        self.sources = self.sources.union(source.sources());
        self
    }

    pub(super) fn pan_pixels_from(
        &mut self,
        delta: impl Into<PanDelta>,
        source: ManualInputSource,
    ) -> &mut Self {
        let delta = delta.into();
        self.pan.0 += delta.0;
        self.pan_active = true;
        self.sources = self.sources.union(source.sources());
        self
    }

    pub(super) fn zoom_coarse_from(
        &mut self,
        delta: impl Into<CoarseZoomDelta>,
        source: ManualInputSource,
    ) -> &mut Self {
        let delta = delta.into();
        self.zoom_coarse.0 += delta.0;
        self.zoom_active = true;
        self.sources = self.sources.union(source.sources());
        self
    }

    pub(super) fn zoom_smooth_from(
        &mut self,
        delta: impl Into<SmoothZoomDelta>,
        source: ManualInputSource,
    ) -> &mut Self {
        let delta = delta.into();
        self.zoom_smooth.0 += delta.0;
        self.zoom_active = true;
        self.sources = self.sources.union(source.sources());
        self
    }
}
