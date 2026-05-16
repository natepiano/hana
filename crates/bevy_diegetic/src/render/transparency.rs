//! `StableTransparency` — camera marker that opts into OIT + `Msaa::Off`
//! for correct `AlphaMode::Blend` ordering. App-wide alpha-mode defaults
//! live on [`CascadeDefaults::text_alpha`](crate::CascadeDefaults).
//!
//! # Two paths
//!
//! - **Default (order-independent):** `AlphaMode::AlphaToCoverage` on text, MSAA enabled on the
//!   camera. No `StableTransparency` needed. Correct for almost all scenes, including dense
//!   coplanar glyphs and scenes without overlapping transparent primitives.
//! - **Opt-in blended path:** add [`StableTransparency`] to a `Camera3d`. Three observers in this
//!   module then aggressively manage MSAA across every camera that shares the window:
//!   - On add: insert `OrderIndependentTransparencySettings` + `Msaa::Off` on the OIT camera, set
//!     its depth texture to `TEXTURE_BINDING`, and propagate `Msaa::Off` to every existing
//!     `ScreenSpaceCamera`.
//!   - On any new `ScreenSpaceCamera` spawned after that (e.g. when a `DiegeticPanel::screen()`
//!     appears mid-app): force `Msaa::Off` on it too, so the late-spawn case stays consistent.
//!   - On remove: strip OIT and restore `Msaa::default()` everywhere it was forced off.
//!
//!   Pair with `AlphaMode::Blend` on text (via the per-style override, per-panel override, or
//!   [`CascadeDefaults::text_alpha`](crate::CascadeDefaults)) when you need animated fades or
//!   correct depth compositing with other translucent primitives.
//!
//! # Why so aggressive
//!
//! OIT and MSAA cannot coexist on cameras that share a framebuffer. Bevy's OIT plugin panics on
//! a single camera that has both; even a sibling camera with default MSAA stalls the swap chain
//! on macOS Metal, producing a window that only repaints on OS-level events (move it to another
//! monitor and the latest frame appears). The three observers exist to keep that mismatch from
//! happening in either spawn order.

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
/// # MSAA is aggressively managed while this marker is present
///
/// OIT and MSAA cannot coexist on cameras that share a framebuffer: Bevy's
/// OIT plugin panics on a single camera that has both, and even when only a
/// sibling camera has MSAA the attachment formats mismatch — on macOS Metal
/// the swap chain stalls and the window appears frozen, only repainting on
/// OS-level events (window move, focus change). To prevent that, this crate
/// owns the `Msaa` component on every camera that could share a framebuffer
/// with an OIT camera, for the entire time `StableTransparency` is present:
///
/// 1. **OIT camera turns on**: [`on_stable_transparency_added`] inserts
///    `OrderIndependentTransparencySettings` + `Msaa::Off` on the OIT camera,
///    and propagates `Msaa::Off` to every existing `ScreenSpaceCamera`.
/// 2. **Screen-space camera spawns later** (e.g. when a `DiegeticPanel::screen()`
///    triggers `setup_screen_space_view`): [`on_screen_space_camera_added`]
///    detects the active OIT camera and forces `Msaa::Off` on the new
///    overlay camera before it can render with a default-MSAA pipeline.
/// 3. **OIT camera turns off**: [`on_stable_transparency_removed`] strips
///    OIT and restores `Msaa::default()` on both the OIT camera and every
///    `ScreenSpaceCamera`.
///
/// Net effect: do not set `Msaa` manually on a camera that lives alongside
/// a `StableTransparency` camera on the same window — this module will
/// overwrite it. If you need MSAA in a scene, use
/// `AlphaMode::AlphaToCoverage` on text and skip `StableTransparency`.
///
/// Also inserted on the OIT camera: depth texture `TEXTURE_BINDING` usage
/// (required by the OIT resolve pass).
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

/// Fires when a new `ScreenSpaceCamera` is added. If any camera already has
/// `OrderIndependentTransparencySettings`, the framebuffer is in OIT mode and
/// every camera sharing it must run with `Msaa::Off` — otherwise mismatched
/// attachment formats stall the swap chain and the window only repaints on
/// OS events. Pairs with [`on_stable_transparency_added`], which handles the
/// reverse spawn order (screen-space camera already exists when OIT turns on).
pub(super) fn on_screen_space_camera_added(
    trigger: On<Add, ScreenSpaceCamera>,
    oit_cameras: Query<Entity, With<OrderIndependentTransparencySettings>>,
    mut commands: Commands,
) {
    if oit_cameras.iter().next().is_none() {
        return;
    }
    commands.entity(trigger.entity).insert(Msaa::Off);
}
