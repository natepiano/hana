use bevy::prelude::*;

use super::CameraInteractionSources;
use super::ControlSpeed;
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

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    struct CameraMotionBits: u8 {
        const ORBITING = 1 << 0;
        const PANNING  = 1 << 1;
        const ZOOMING  = 1 << 2;
    }
}

/// Per-frame motion flags indicating which camera actions are active.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub struct CameraMotion {
    bits: u8,
}

impl CameraMotion {
    /// Empty motion set.
    pub const NONE: Self = Self::from_motion_bits(CameraMotionBits::empty());
    /// Orbit motion is active.
    pub const ORBITING: Self = Self::from_motion_bits(CameraMotionBits::ORBITING);
    /// Pan motion is active.
    pub const PANNING: Self = Self::from_motion_bits(CameraMotionBits::PANNING);
    /// Zoom motion is active.
    pub const ZOOMING: Self = Self::from_motion_bits(CameraMotionBits::ZOOMING);

    const fn from_motion_bits(motion_bits: CameraMotionBits) -> Self {
        Self {
            bits: motion_bits.bits(),
        }
    }

    /// Returns `true` when no motion is active.
    #[must_use]
    pub const fn is_empty(self) -> bool { self.bits == Self::NONE.bits }

    /// Returns `true` when `other` is fully contained in this set.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool { self.bits & other.bits == other.bits }

    /// Returns `true` when this set shares at least one motion with `other`.
    #[must_use]
    pub const fn intersects(self, other: Self) -> bool { self.bits & other.bits != Self::NONE.bits }

    /// Returns a set containing motions from both sets.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self {
            bits: self.bits | other.bits,
        }
    }

    /// Returns this set without any motions from `other`.
    #[must_use]
    pub const fn difference(self, other: Self) -> Self {
        Self {
            bits: self.bits & !other.bits,
        }
    }
}

/// Semantic per-frame camera input consumed by the orbit controller.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Component, Default)]
pub struct OrbitCamInput {
    orbit:                OrbitDelta,
    pan:                  PanDelta,
    zoom_coarse:          CoarseZoomDelta,
    zoom_smooth:          SmoothZoomDelta,
    orbit_sources:        CameraInteractionSources,
    pan_sources:          CameraInteractionSources,
    zoom_sources:         CameraInteractionSources,
    zoom_impulse_sources: CameraInteractionSources,
    orbit_speed:          ControlSpeed,
    pan_speed:            ControlSpeed,
    zoom_speed:           ControlSpeed,
    motion:               CameraMotion,
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
    pub const fn sources(&self) -> CameraInteractionSources {
        self.orbit_sources
            .union(self.pan_sources)
            .union(self.zoom_sources)
    }

    /// Returns active sources for orbit input this frame.
    #[must_use]
    pub const fn orbit_sources(&self) -> CameraInteractionSources { self.orbit_sources }

    /// Returns active sources for pan input this frame.
    #[must_use]
    pub const fn pan_sources(&self) -> CameraInteractionSources { self.pan_sources }

    /// Returns active sources for zoom input this frame.
    #[must_use]
    pub const fn zoom_sources(&self) -> CameraInteractionSources { self.zoom_sources }

    /// Returns the speed variant of the orbit input this frame.
    #[must_use]
    pub const fn orbit_speed(&self) -> ControlSpeed { self.orbit_speed }

    /// Returns the speed variant of the pan input this frame.
    #[must_use]
    pub const fn pan_speed(&self) -> ControlSpeed { self.pan_speed }

    /// Returns the speed variant of the zoom input this frame.
    #[must_use]
    pub const fn zoom_speed(&self) -> ControlSpeed { self.zoom_speed }

    pub(crate) const fn set_orbit_speed(&mut self, speed: ControlSpeed) {
        self.orbit_speed = speed;
    }

    pub(crate) const fn set_pan_speed(&mut self, speed: ControlSpeed) { self.pan_speed = speed; }

    pub(crate) const fn set_zoom_speed(&mut self, speed: ControlSpeed) { self.zoom_speed = speed; }

    /// Returns the active motion flags for this frame.
    #[must_use]
    pub const fn motion(&self) -> CameraMotion { self.motion }

    /// Returns `true` when the frame carries any camera intent.
    #[must_use]
    pub const fn has_input(&self) -> bool { !self.motion.is_empty() }

    /// Returns `true` when the frame carries orbit intent.
    #[must_use]
    pub const fn has_orbit(&self) -> bool { self.motion.contains(CameraMotion::ORBITING) }

    /// Returns `true` when the frame carries pan intent.
    #[must_use]
    pub const fn has_pan(&self) -> bool { self.motion.contains(CameraMotion::PANNING) }

    /// Returns `true` when the frame carries zoom intent.
    #[must_use]
    pub const fn has_zoom(&self) -> bool { self.motion.contains(CameraMotion::ZOOMING) }

    pub(crate) fn clear(&mut self) -> &mut Self {
        *self = Self::default();
        self
    }

    pub(super) fn orbit_pixels_from(
        &mut self,
        delta: impl Into<OrbitDelta>,
        source: ManualInputSource,
    ) -> &mut Self {
        self.orbit_pixels_with_sources(delta, source.sources())
    }

    pub(crate) fn orbit_pixels_with_sources(
        &mut self,
        delta: impl Into<OrbitDelta>,
        sources: CameraInteractionSources,
    ) -> &mut Self {
        let delta = delta.into();
        self.orbit.0 += delta.0;
        self.motion = self.motion.union(CameraMotion::ORBITING);
        self.orbit_sources = self.orbit_sources.union(sources);
        self
    }

    pub(crate) const fn orbit_active_with_sources(
        &mut self,
        sources: CameraInteractionSources,
    ) -> &mut Self {
        self.motion = self.motion.union(CameraMotion::ORBITING);
        self.orbit_sources = self.orbit_sources.union(sources);
        self
    }

    pub(super) fn pan_pixels_from(
        &mut self,
        delta: impl Into<PanDelta>,
        source: ManualInputSource,
    ) -> &mut Self {
        self.pan_pixels_with_sources(delta, source.sources())
    }

    pub(crate) fn pan_pixels_with_sources(
        &mut self,
        delta: impl Into<PanDelta>,
        sources: CameraInteractionSources,
    ) -> &mut Self {
        let delta = delta.into();
        self.pan.0 += delta.0;
        self.motion = self.motion.union(CameraMotion::PANNING);
        self.pan_sources = self.pan_sources.union(sources);
        self
    }

    pub(crate) const fn pan_active_with_sources(
        &mut self,
        sources: CameraInteractionSources,
    ) -> &mut Self {
        self.motion = self.motion.union(CameraMotion::PANNING);
        self.pan_sources = self.pan_sources.union(sources);
        self
    }

    pub(super) fn zoom_coarse_from(
        &mut self,
        delta: impl Into<CoarseZoomDelta>,
        source: ManualInputSource,
    ) -> &mut Self {
        self.zoom_coarse_with_sources(delta, source.sources())
    }

    pub(crate) fn zoom_coarse_with_sources(
        &mut self,
        delta: impl Into<CoarseZoomDelta>,
        sources: CameraInteractionSources,
    ) -> &mut Self {
        let delta = delta.into();
        self.zoom_coarse.0 += delta.0;
        self.motion = self.motion.union(CameraMotion::ZOOMING);
        self.zoom_sources = self.zoom_sources.union(sources);
        self.zoom_impulse_sources = self.zoom_impulse_sources.union(sources);
        self
    }

    pub(super) fn zoom_smooth_from(
        &mut self,
        delta: impl Into<SmoothZoomDelta>,
        source: ManualInputSource,
    ) -> &mut Self {
        self.zoom_smooth_with_sources(delta, source.sources())
    }

    pub(crate) fn zoom_smooth_with_sources(
        &mut self,
        delta: impl Into<SmoothZoomDelta>,
        sources: CameraInteractionSources,
    ) -> &mut Self {
        let delta = delta.into();
        self.zoom_smooth.0 += delta.0;
        self.motion = self.motion.union(CameraMotion::ZOOMING);
        self.zoom_sources = self.zoom_sources.union(sources);
        self
    }

    pub(crate) const fn zoom_active_with_sources(
        &mut self,
        sources: CameraInteractionSources,
    ) -> &mut Self {
        self.motion = self.motion.union(CameraMotion::ZOOMING);
        self.zoom_sources = self.zoom_sources.union(sources);
        self
    }

    pub(crate) const fn zoom_impulse_sources(&self) -> CameraInteractionSources {
        self.zoom_impulse_sources
    }

    pub(crate) fn clear_orbit(&mut self) -> &mut Self {
        self.orbit = OrbitDelta::default();
        self.orbit_sources = CameraInteractionSources::NONE;
        self.motion = self.motion.difference(CameraMotion::ORBITING);
        self
    }

    pub(crate) fn clear_pan(&mut self) -> &mut Self {
        self.pan = PanDelta::default();
        self.pan_sources = CameraInteractionSources::NONE;
        self.motion = self.motion.difference(CameraMotion::PANNING);
        self
    }
}
