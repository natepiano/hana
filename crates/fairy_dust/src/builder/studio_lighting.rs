//! `StudioLightingBuilder` impls.

use bevy::prelude::*;

use super::CameraHomeBuilder;
use super::PrimitiveBuilder;
use super::SprinkleBuilder;
use super::StudioLightingBuilder;
use crate::lighting;
use crate::primitive::PrimitiveConfig;

impl<S> StudioLightingBuilder<S> {
    /// Sets the world-space target the key and fill lights aim at.
    ///
    /// Defaults to `(0.0, 0.45, 0.0)`. Set this to the position of the
    /// scene's primary subject so the cascade shadow map stays tight around
    /// what is being shadowed.
    #[must_use]
    pub const fn aim_at(mut self, target: Vec3) -> Self {
        self.config.aim_at = target;
        self
    }

    /// Sets the key (shadow-casting) directional light position in world
    /// space.
    ///
    /// Defaults to `(-3.5, 7.0, 4.8)`. The shadow direction is
    /// `(aim_at - key_light_pos).normalize()`; place the light in front of
    /// and above the subject (relative to the camera) to cast shadows that
    /// trail back behind it.
    #[must_use]
    pub const fn key_light_pos(mut self, pos: Vec3) -> Self {
        self.config.key_light_pos = pos;
        self
    }

    /// Sets the key light illuminance in lux.
    ///
    /// Defaults to the studio rig's calibrated key light value. Use this when
    /// an example wants the studio direction, fill, point light, and shadow
    /// settings, but needs a specific key-light brightness for comparison.
    #[must_use]
    pub const fn key_light_illuminance(mut self, illuminance: f32) -> Self {
        self.config.key_light_illuminance = illuminance;
        self
    }

    /// Finalizes the studio lighting configuration and starts configuring a
    /// ground plane.
    #[must_use]
    pub fn with_ground_plane(self) -> PrimitiveBuilder<S> {
        PrimitiveBuilder {
            parent:  self.finish(),
            config:  PrimitiveConfig::ground_plane(),
            inserts: Vec::new(),
        }
    }

    /// Finalizes the studio lighting configuration and starts configuring a
    /// cube.
    #[must_use]
    pub fn with_cube(self) -> PrimitiveBuilder<S> {
        PrimitiveBuilder {
            parent:  self.finish(),
            config:  PrimitiveConfig::cube(),
            inserts: Vec::new(),
        }
    }

    /// Finalizes the studio lighting configuration and starts configuring a
    /// camera home pose.
    #[must_use]
    pub fn with_camera_home(self) -> CameraHomeBuilder<S> { self.finish().with_camera_home() }

    fn finish(mut self) -> SprinkleBuilder<S> {
        lighting::install(&mut self.parent.app, self.config);
        self.parent
    }
}
