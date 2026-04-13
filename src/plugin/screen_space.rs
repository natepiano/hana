//! Systems for managing screen-space overlay cameras and render layer
//! propagation for [`ScreenSpace`] panels.

use bevy::camera::Camera3d;
use bevy::camera::Camera3dDepthTextureUsage;
use bevy::camera::ClearColorConfig;
use bevy::camera::ScalingMode;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::render_resource::TextureUsages;

use super::components::DiegeticPanel;
use super::components::ScreenDimension;
use super::components::ScreenPosition;
use super::components::ScreenSpace;

/// Positions and sizes [`ScreenSpace`] panels relative to the window.
///
/// Runs before `compute_panel_layouts` so that any dimension changes
/// trigger layout recomputation via Bevy change detection.
pub(super) fn position_screen_space_panels(
    windows: Query<&Window>,
    mut panels: Query<(&mut Transform, &mut DiegeticPanel, &ScreenSpace)>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let win_w = window.width();
    let win_h = window.height();
    if win_w <= 0.0 || win_h <= 0.0 {
        return;
    }
    let half_w = win_w / 2.0;
    let half_h = win_h / 2.0;

    for (mut transform, mut panel, screen_space) in &mut panels {
        // ── Sizing ──────────────────────────────────────────────
        if let Some(dim) = screen_space.width {
            let new_w = match dim {
                ScreenDimension::Fixed(px) => px,
                ScreenDimension::Percent(frac) => win_w * frac,
            };
            if (panel.width - new_w).abs() > 0.01 {
                panel.width = new_w;
            }
        }
        if let Some(dim) = screen_space.height {
            let new_h = match dim {
                ScreenDimension::Fixed(px) => px,
                ScreenDimension::Percent(frac) => win_h * frac,
            };
            if (panel.height - new_h).abs() > 0.01 {
                panel.height = new_h;
            }
        }

        // ── Positioning ─────────────────────────────────────────
        let (screen_x, screen_y) = match screen_space.position {
            ScreenPosition::Screen => {
                let (fx, fy) = panel.anchor.offset_fraction();
                (fx * win_w, fy * win_h)
            },
            ScreenPosition::At(pos) => (pos.x, pos.y),
        };

        // Convert screen coords (top-left origin, y-down) to camera
        // coords (center origin, y-up).
        transform.translation.x = screen_x - half_w;
        transform.translation.y = half_h - screen_y;
    }
}

/// Marker on overlay cameras spawned by the screen-space system.
#[derive(Component)]
pub(super) struct ScreenSpaceCamera {
    pub render_layers: RenderLayers,
    pub camera_order:  isize,
}

/// Marker on directional lights spawned alongside overlay cameras.
#[derive(Component)]
pub(super) struct ScreenSpaceLight {
    pub render_layers: RenderLayers,
}

/// Spawns overlay cameras and lights for newly added [`ScreenSpace`] panels.
///
/// For each unique `(camera_order, render_layers)` pair, a single shared
/// orthographic camera is created with `ScalingMode::WindowSize`
/// (1 world unit = 1 logical pixel). A directional light on the same
/// render layers provides stable illumination for PBR text materials.
pub(super) fn setup_screen_space_cameras(
    added_panels: Query<(Entity, &ScreenSpace), Added<ScreenSpace>>,
    existing_cameras: Query<&ScreenSpaceCamera>,
    mut commands: Commands,
) {
    for (panel_entity, screen_space) in &added_panels {
        let layers = &screen_space.render_layers;
        let order = screen_space.camera_order;

        // Add render layers to the panel entity.
        commands.entity(panel_entity).insert(layers.clone());

        // Check if a camera for this (order, layers) pair already exists.
        let camera_exists = existing_cameras
            .iter()
            .any(|cam| cam.render_layers == *layers && cam.camera_order == order);

        if camera_exists {
            continue;
        }

        // Spawn overlay camera — orthographic, 1 unit = 1 pixel.
        commands.spawn((
            ScreenSpaceCamera {
                render_layers: layers.clone(),
                camera_order:  order,
            },
            Camera3d {
                depth_texture_usages: Camera3dDepthTextureUsage(
                    (TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING).bits(),
                ),
                ..default()
            },
            Camera {
                order,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::WindowSize,
                far: 2000.0,
                ..OrthographicProjection::default_3d()
            }),
            Transform::from_xyz(0.0, 0.0, 1000.0).looking_at(Vec3::ZERO, Vec3::Y),
            bevy::render::view::Msaa::Off,
            layers.clone(),
        ));

        // Spawn a directional light on the same layers for PBR illumination.
        commands.spawn((
            ScreenSpaceLight {
                render_layers: layers.clone(),
            },
            DirectionalLight {
                illuminance: 5000.0,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_rotation(Quat::from_euler(
                EulerRot::XYZ,
                -std::f32::consts::FRAC_PI_4,
                std::f32::consts::FRAC_PI_4,
                0.0,
            )),
            layers.clone(),
        ));
    }
}

/// Propagates [`RenderLayers`] from screen-space panel parents to their
/// children that are missing the component.
///
/// Runs after text/image/gizmo reconciliation so that newly spawned
/// children are picked up.
pub(super) fn propagate_screen_space_render_layers(
    panels_with_layers: Query<(Entity, &RenderLayers), With<ScreenSpace>>,
    children_query: Query<&Children>,
    existing_layers: Query<&RenderLayers>,
    mut commands: Commands,
) {
    for (panel_entity, panel_layers) in &panels_with_layers {
        let Ok(children) = children_query.get(panel_entity) else {
            continue;
        };
        propagate_layers_recursive(
            &children_query,
            &existing_layers,
            &mut commands,
            children,
            panel_layers,
        );
    }
}

/// Recursively propagates `RenderLayers` to all descendants.
fn propagate_layers_recursive(
    children_query: &Query<&Children>,
    existing_layers: &Query<&RenderLayers>,
    commands: &mut Commands,
    children: &Children,
    layers: &RenderLayers,
) {
    for child in children.iter() {
        if existing_layers.get(child).is_err() {
            commands.entity(child).insert(layers.clone());
        }
        if let Ok(grandchildren) = children_query.get(child) {
            propagate_layers_recursive(
                children_query,
                existing_layers,
                commands,
                grandchildren,
                layers,
            );
        }
    }
}

/// Despawns overlay cameras and lights when no [`ScreenSpace`] panels
/// reference their `(camera_order, render_layers)` pair.
pub(super) fn cleanup_screen_space_cameras(
    mut removed: RemovedComponents<ScreenSpace>,
    remaining_panels: Query<&ScreenSpace>,
    cameras: Query<(Entity, &ScreenSpaceCamera)>,
    lights: Query<(Entity, &ScreenSpaceLight)>,
    mut commands: Commands,
) {
    // Only run if at least one ScreenSpace component was removed this frame.
    if removed.read().next().is_none() {
        return;
    }

    // Collect which (order, layers) pairs are still in use.
    let active_pairs: Vec<(isize, &RenderLayers)> = remaining_panels
        .iter()
        .map(|ss| (ss.camera_order, &ss.render_layers))
        .collect();

    // Despawn cameras whose pair is no longer active.
    for (entity, cam) in &cameras {
        let still_active = active_pairs
            .iter()
            .any(|(order, layers)| *order == cam.camera_order && **layers == cam.render_layers);
        if !still_active {
            commands.entity(entity).despawn();
        }
    }

    // Despawn lights whose layers are no longer active.
    for (entity, light) in &lights {
        let still_active = active_pairs
            .iter()
            .any(|(_, layers)| **layers == light.render_layers);
        if !still_active {
            commands.entity(entity).despawn();
        }
    }
}
