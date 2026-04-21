//! Text-transparency control surface: app-wide default `AlphaMode` for text,
//! and the `StableTransparency` camera marker that opts into OIT + `Msaa::Off`
//! for correct `AlphaMode::Blend` ordering.
//!
//! # Two paths
//!
//! - **Default (order-independent):** `AlphaMode::AlphaToCoverage` on text, MSAA enabled on the
//!   camera. No `StableTransparency` needed. Correct for almost all scenes, including dense
//!   coplanar glyphs and scenes without overlapping transparent primitives.
//! - **Opt-in blended path:** add [`StableTransparency`] to a `Camera3d`. The observer inserts
//!   `OrderIndependentTransparencySettings`, sets the camera's depth texture to `TEXTURE_BINDING`,
//!   and forces `Msaa::Off`. It also propagates `Msaa::Off` to every `ScreenSpaceCamera` in the app
//!   so pipelines match. Pair with `AlphaMode::Blend` on text (via the per-style override or
//!   [`TextAlphaModeDefault`]) when you need animated fades or correct depth compositing with other
//!   translucent primitives.
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
/// Defaults to [`AlphaMode::Blend`] — recommended if the best-looking
/// anti-aliased text is the goal. Blend is classic alpha compositing and
/// depends on submission ordering to look correct; on coplanar text you may
/// see color/ordering flicker as the camera moves.
///
/// When that flicker appears, add [`StableTransparency`] to the camera. It
/// enables Bevy's **Order Independent Transparency** — per
/// `bevy_core_pipeline::oit`: *"This can correctly render some scenes that
/// would otherwise have artifacts due to alpha blending, but uses more
/// memory."* OIT and MSAA cannot coexist on the same camera — Bevy's OIT
/// plugin explicitly panics if both are present, so
/// [`StableTransparency`]'s observer forces `Msaa::Off`.
///
/// [`AlphaMode::Premultiplied`] is worth trying as an alternative to
/// [`AlphaMode::Blend`] — per Bevy's docs, it *"behaves more like
/// `AlphaMode::Blend` for alpha values closer to 1.0, and more like
/// `AlphaMode::Add` for alpha values closer to 0.0"* and is *"used to avoid
/// 'border' or 'outline' artifacts."* Paired with [`StableTransparency`] it
/// can settle on the darker, arguably more physically-correct lit color
/// instead of flipping to a brighter one at certain camera angles. Whether
/// Blend or Premultiplied looks better is scene-dependent — worth comparing
/// both with [`StableTransparency`] enabled.
///
/// If you need MSAA in the scene (or want to skip OIT's extra memory cost),
/// use [`AlphaMode::AlphaToCoverage`] instead. Per Bevy's docs: *"Spreads
/// the fragment out over a hardware-dependent number of sample locations
/// proportional to the alpha value. This requires multisample antialiasing;
/// if MSAA isn't on, this is identical to `AlphaMode::Mask` with a value of
/// 0.5."* MSAA is effectively a requirement for `AlphaToCoverage` to look
/// good — strongly encouraged to keep it on whenever you pick this path.
///
/// **Mixing alpha modes** across the app is fine and encouraged for creative
/// uses — [`AlphaMode::Add`] for glow, [`AlphaMode::Multiply`] for tint — so
/// long as you keep in mind that [`StableTransparency`] and MSAA cannot
/// coexist on the same camera.
///
/// Resolution order (highest wins): per-`WorldTextStyle`/`LayoutTextStyle`
/// override → per-panel override → this resource → [`AlphaMode::Blend`].
#[derive(Resource, Debug, Clone, Copy)]
pub struct TextAlphaModeDefault(pub AlphaMode);

impl Default for TextAlphaModeDefault {
    fn default() -> Self { Self(AlphaMode::Blend) }
}

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
/// This is the companion to the [`AlphaMode::Blend`] default — use it when
/// you see coplanar-text flicker. If you need MSAA in your scene, use
/// [`AlphaMode::AlphaToCoverage`] on text instead and skip this marker. See
/// [`TextAlphaModeDefault`] for the full two-path overview.
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
