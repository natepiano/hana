//! Systems for managing screen-space overlay cameras and render layer
//! propagation for [`ScreenSpace`] panels.

use bevy::camera::ClearColorConfig;
use bevy::camera::ScalingMode;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use super::components::ScreenSpace;

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
            Camera3d::default(),
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
