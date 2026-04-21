//! `StableTransparency` — camera marker that opts into OIT + `Msaa::Off`
//! for correct `AlphaMode::Blend` ordering. App-wide alpha-mode defaults
//! live on [`CascadeDefaults::text_alpha`](crate::CascadeDefaults).
//!
//! # Two paths
//!
//! - **Default (order-independent):** `AlphaMode::AlphaToCoverage` on text, MSAA enabled on the
//!   camera. No `StableTransparency` needed. Correct for almost all scenes, including dense
//!   coplanar glyphs and scenes without overlapping transparent primitives.
//! - **Opt-in blended path:** add [`StableTransparency`] to a `Camera3d`. The observer inserts
//!   `OrderIndependentTransparencySettings`, sets the camera's depth texture to `TEXTURE_BINDING`,
//!   and forces `Msaa::Off`. It also propagates `Msaa::Off` to every `ScreenSpaceCamera` in the app
//!   so pipelines match. Pair with `AlphaMode::Blend` on text (via the per-style override,
//!   per-panel override, or [`CascadeDefaults::text_alpha`](crate::CascadeDefaults)) when you need
//!   animated fades or correct depth compositing with other translucent primitives.
//!
//! Removing the marker reverses all changes.

use bevy::camera::Camera3d;
use bevy::core_pipeline::oit::OrderIndependentTransparencySettings;
use bevy::prelude::*;
use bevy::render::render_resource::TextureUsages;
use bevy::render::view::Msaa;

use crate::screen_space::ScreenSpaceCamera;

/// Camera marker that opts a `Camera3d` into **Order Independent
/// Transparency** for stable compositing of [`AlphaMode::Blend`] (and
/// [`AlphaMode::Premultiplied`]) text.
///
/// When added, an observer:
/// - Inserts `OrderIndependentTransparencySettings` on the camera.
/// - Sets the camera's depth texture to `TEXTURE_BINDING` (required by the OIT resolve pass).
/// - Forces `Msaa::Off` on the camera **and on every `ScreenSpaceCamera`** in the app. Bevy's OIT
///   plugin panics if a camera has OIT and MSAA (`MSAA is not supported when using
///   OrderIndependentTransparency`), so MSAA must be off everywhere that shares the framebuffer.
///
/// When removed, OIT is stripped and `Msaa::default()` is restored on the
/// camera and on all `ScreenSpaceCamera`s.
///
/// Use this marker when coplanar text flickers under the default
/// `AlphaMode::AlphaToCoverage` path. Pair with `AlphaMode::Blend` or
/// `AlphaMode::Premultiplied` on text, configured via
/// [`CascadeDefaults::text_alpha`](crate::CascadeDefaults), per-panel
/// override ([`DiegeticPanel`](crate::DiegeticPanel)), or per-style
/// override ([`WorldTextStyle`](crate::WorldTextStyle) /
/// [`LayoutTextStyle`](crate::LayoutTextStyle)).
///
/// If you need MSAA in the scene (or want to skip OIT's extra memory
/// cost), use `AlphaMode::AlphaToCoverage` on text instead and skip this
/// marker. `AlphaMode::AlphaToCoverage` per Bevy's docs: *"Spreads the
/// fragment out over a hardware-dependent number of sample locations
/// proportional to the alpha value. This requires multisample
/// antialiasing; if MSAA isn't on, this is identical to
/// `AlphaMode::Mask` with a value of 0.5."* MSAA is effectively a
/// requirement for `AlphaToCoverage` to look good.
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
    commands
        .entity(cam)
        .insert((OrderIndependentTransparencySettings::default(), Msaa::Off));
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
