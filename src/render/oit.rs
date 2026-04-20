//! [`ensure_oit_on_cameras`] — enables OIT on scene cameras when a
//! [`DiegeticPanel`] wants geometry-mode rendering.
//!
//! OIT is a rendering concern (order-independent alpha compositing), so
//! the system lives in `render/` even though it inspects panel state to
//! decide when to activate.

use bevy::camera::Camera3d;
use bevy::core_pipeline::oit::OrderIndependentTransparencySettings;
use bevy::prelude::*;
use bevy::render::render_resource::TextureUsages;
use bevy::render::view::Msaa;

use crate::panel::DiegeticPanel;
use crate::panel::RenderMode;
use crate::screen_space::ScreenSpaceCamera;

/// Ensures scene `Camera3d` entities have OIT enabled for correct
/// transparent panel rendering.
///
/// # Why OIT is needed
///
/// In Geometry mode, each panel element (background, border, text) is a
/// separate transparent mesh. When multiple transparent fragments overlap
/// at a pixel, standard alpha blending composites them in submission
/// order — which can be wrong when the camera moves (distance-based sort
/// flips). OIT stores ALL transparent fragments in a linked list and
/// resolves them by actual depth, producing correct compositing
/// regardless of camera angle.
///
/// # Constraints
///
/// - OIT requires `Msaa::Off` — this system disables MSAA if present.
/// - Only activates when at least one panel uses [`RenderMode::Geometry`], since OIT is unnecessary
///   for texture-only panels.
/// - Screen-space overlay cameras are excluded via [`Without<ScreenSpaceCamera>`] — they don't need
///   OIT and adding it corrupts the shared OIT buffer.
pub(super) fn ensure_oit_on_cameras(
    panels: Query<&DiegeticPanel>,
    mut cameras: Query<
        (Entity, &mut Camera3d, &mut Msaa),
        (
            Without<OrderIndependentTransparencySettings>,
            Without<ScreenSpaceCamera>,
        ),
    >,
    mut commands: Commands,
) {
    let has_geometry_panels = panels
        .iter()
        .any(|p| p.render_mode() == RenderMode::Geometry);
    if !has_geometry_panels {
        return;
    }

    for (entity, mut camera_3d, mut msaa) in &mut cameras {
        camera_3d.depth_texture_usages.0 |= TextureUsages::TEXTURE_BINDING.bits();
        if *msaa != Msaa::Off {
            *msaa = Msaa::Off;
        }
        commands
            .entity(entity)
            .insert(OrderIndependentTransparencySettings::default());
    }
}
