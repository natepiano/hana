//! Small shared helpers: easing and the world-panel materials used by both the
//! anchor-demo and hinge-chain scenes.

use bevy::prelude::*;
use bevy_diegetic::default_panel_material;

/// Cubic smoothstep easing on `t ∈ [0, 1]`.
pub(crate) fn smoothstep(t: f32) -> f32 { t * t * 2.0f32.mul_add(-t, 3.0) }

pub(crate) fn panel_material() -> StandardMaterial { default_panel_material() }

pub(crate) fn text_material() -> StandardMaterial {
    StandardMaterial {
        unlit: true,
        ..default_panel_material()
    }
}
