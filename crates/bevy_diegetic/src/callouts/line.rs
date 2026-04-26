use bevy::color::Color;
use bevy::math::Vec3;
use bevy::prelude::Commands;
use bevy::prelude::Component;
use bevy::prelude::Entity;
use bevy::prelude::Transform;
use bevy::prelude::Visibility;

use super::caps::CalloutCap;
use super::constants::DEFAULT_CAP_SIZE;
use super::constants::DEFAULT_LINE_THICKNESS;
use crate::panel::SurfaceShadow;

/// World-space/local-space callout line with configurable end caps.
///
/// The line is expressed in the entity's local space. If the entity is
/// parented or transformed, the rendered callout follows naturally.
#[derive(Component, Clone, Debug)]
pub struct CalloutLine {
    pub(super) start:          Vec3,
    pub(super) end:            Vec3,
    pub(super) color:          Color,
    pub(super) thickness:      f32,
    pub(super) cap_size:       f32,
    pub(super) start_inset:    f32,
    pub(super) end_inset:      f32,
    pub(super) start_cap:      CalloutCap,
    pub(super) end_cap:        CalloutCap,
    pub(super) surface_shadow: SurfaceShadow,
}

impl CalloutLine {
    /// Creates a new line from `start` to `end`.
    #[must_use]
    pub const fn new(start: Vec3, end: Vec3) -> Self {
        Self {
            start,
            end,
            color: Color::WHITE,
            thickness: DEFAULT_LINE_THICKNESS,
            cap_size: DEFAULT_CAP_SIZE,
            start_inset: 0.0,
            end_inset: 0.0,
            start_cap: CalloutCap::None,
            end_cap: CalloutCap::None,
            surface_shadow: SurfaceShadow::Off,
        }
    }

    /// Sets the line color.
    #[must_use]
    pub const fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Sets the shaft thickness in world meters.
    #[must_use]
    pub const fn thickness(mut self, thickness: f32) -> Self {
        self.thickness = thickness;
        self
    }

    /// Sets the cap size in world meters.
    #[must_use]
    pub const fn cap_size(mut self, cap_size: f32) -> Self {
        self.cap_size = cap_size;
        self
    }

    /// Insets the start of the visible shaft inward from `start`.
    #[must_use]
    pub const fn start_inset(mut self, inset: f32) -> Self {
        self.start_inset = inset;
        self
    }

    /// Insets the end of the visible shaft inward from `end`.
    #[must_use]
    pub const fn end_inset(mut self, inset: f32) -> Self {
        self.end_inset = inset;
        self
    }

    /// Sets the cap at the start of the line.
    #[must_use]
    pub const fn start_cap(mut self, cap: CalloutCap) -> Self {
        self.start_cap = cap;
        self
    }

    /// Sets the cap at the end of the line.
    #[must_use]
    pub const fn end_cap(mut self, cap: CalloutCap) -> Self {
        self.end_cap = cap;
        self
    }

    /// Controls whether this callout contributes to shadows.
    #[must_use]
    pub const fn surface_shadow(mut self, surface_shadow: SurfaceShadow) -> Self {
        self.surface_shadow = surface_shadow;
        self
    }
}

/// Child marker for generated callout meshes.
#[derive(Component)]
pub(super) struct CalloutVisual;

/// Spawns a callout-line entity under `parent`.
///
/// This is the simplest public entry point. The actual SDF mesh segments
/// are built by the callout rendering system.
pub(super) fn spawn_callout_line(commands: &mut Commands, parent: Entity, line: &CalloutLine) {
    commands
        .entity(parent)
        .with_child((line.clone(), Transform::IDENTITY, Visibility::Inherited));
}
