//! Text-transparency control surface: app-wide default `AlphaMode` for text,
//! and the `StableTransparency` camera marker that opts into OIT + `Msaa::Off`
//! for correct `AlphaMode::Blend` ordering.
//!
//! # Two paths
//!
//! - **Default (order-independent):** `AlphaMode::AlphaToCoverage` on text,
//!   MSAA enabled on the camera. No `StableTransparency` needed. Correct for
//!   almost all scenes, including dense coplanar glyphs and scenes without
//!   overlapping transparent primitives.
//! - **Opt-in blended path:** add [`StableTransparency`] to a `Camera3d`. The
//!   observer inserts `OrderIndependentTransparencySettings`, sets the
//!   camera's depth texture to `TEXTURE_BINDING`, and forces `Msaa::Off`. It
//!   also propagates `Msaa::Off` to every `ScreenSpaceCamera` in the app so
//!   pipelines match. Pair with `AlphaMode::Blend` on text (via the per-style
//!   override or [`TextAlphaModeDefault`]) when you need animated fades or
//!   correct depth compositing with other translucent primitives.
//!
//! Removing the marker reverses all changes.

use bevy::camera::Camera3d;
use bevy::core_pipeline::oit::OrderIndependentTransparencySettings;
use bevy::prelude::*;
use bevy::render::render_resource::TextureUsages;
use bevy::render::view::Msaa;

use crate::screen_space::ScreenSpaceCamera;

/// App-wide default [`AlphaMode`] for diegetic text.
///
/// Resolution order (highest wins): per-`WorldTextStyle`/`LayoutTextStyle`
/// override → per-panel override → this resource → `AlphaToCoverage`.
#[derive(Resource, Debug, Clone, Copy)]
pub struct TextAlphaModeDefault(pub AlphaMode);

impl Default for TextAlphaModeDefault {
    fn default() -> Self { Self(AlphaMode::AlphaToCoverage) }
}

/// Camera marker opting into stable (order-independent) `AlphaMode::Blend`
/// compositing.
///
/// When added to a `Camera3d`, an observer configures that camera for OIT +
/// `Msaa::Off` and propagates `Msaa::Off` to every `ScreenSpaceCamera` in the
/// app (pipeline compatibility — MSAA sample counts must match across
/// cameras sharing attachments).
///
/// When removed, all changes are reversed.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct StableTransparency;

pub(super) fn on_stable_transparency_added(
    trigger: On<Add, StableTransparency>,
    mut cameras: Query<&mut Camera3d>,
    mut msaa_overlays: Query<Entity, With<ScreenSpaceCamera>>,
    mut commands: Commands,
) {
    let cam = trigger.event_target();
    if let Ok(mut camera_3d) = cameras.get_mut(cam) {
        camera_3d.depth_texture_usages.0 |= TextureUsages::TEXTURE_BINDING.bits();
    }
    commands.entity(cam).insert((
        OrderIndependentTransparencySettings::default(),
        Msaa::Off,
    ));
    for overlay in &mut msaa_overlays {
        commands.entity(overlay).insert(Msaa::Off);
    }
}

pub(super) fn on_stable_transparency_removed(
    trigger: On<Remove, StableTransparency>,
    mut overlays: Query<Entity, With<ScreenSpaceCamera>>,
    mut commands: Commands,
) {
    let cam = trigger.event_target();
    commands
        .entity(cam)
        .remove::<OrderIndependentTransparencySettings>()
        .insert(Msaa::default());
    for overlay in &mut overlays {
        commands.entity(overlay).insert(Msaa::default());
    }
}
