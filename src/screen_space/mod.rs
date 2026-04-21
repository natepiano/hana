//! Screen-space overlay support for diegetic panels.

use bevy::camera::Camera3d;
use bevy::camera::Camera3dDepthTextureUsage;
use bevy::camera::ClearColorConfig;
use bevy::camera::ScalingMode;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::render_resource::TextureUsages;

use crate::panel::DiegeticPanel;
use crate::panel::PanelMode;
use crate::panel::PanelSystems;
use crate::panel::ScreenDimension;
use crate::panel::ScreenPosition;

/// Marker on overlay cameras spawned by the screen-space system.
#[derive(Component)]
pub(crate) struct ScreenSpaceCamera {
    pub render_layers: RenderLayers,
    pub camera_order:  isize,
}

/// Marker on directional lights spawned alongside overlay cameras.
#[derive(Component)]
pub(crate) struct ScreenSpaceLight {
    pub render_layers: RenderLayers,
}

pub(crate) struct ScreenSpacePlugin;

impl Plugin for ScreenSpacePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                position_screen_space_panels.before(PanelSystems::ComputeLayout),
                setup_screen_space_cameras.after(PanelSystems::ComputeLayout),
            ),
        )
        .add_systems(
            PostUpdate,
            (
                propagate_screen_space_render_layers,
                cleanup_screen_space_cameras,
            ),
        );
    }
}

/// Positions and sizes screen-space panels relative to the window.
///
/// Runs before `compute_panel_layouts` so that any dimension changes
/// trigger layout recomputation via Bevy change detection.
fn position_screen_space_panels(
    windows: Query<&Window>,
    mut panels: Query<(&mut Transform, &mut DiegeticPanel)>,
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

    for (mut transform, mut panel) in &mut panels {
        let PanelMode::Screen {
            position,
            width,
            height,
            ..
        } = panel.mode()
        else {
            continue;
        };
        let position = *position;
        let width = *width;
        let height = *height;

        if let Some(dim) = width {
            let new_w = match dim {
                ScreenDimension::Fixed(px) => px,
                ScreenDimension::Percent(frac) => win_w * frac,
            };
            if (panel.width() - new_w).abs() > 0.01 {
                panel.set_width(new_w);
            }
        }
        if let Some(dim) = height {
            let new_h = match dim {
                ScreenDimension::Fixed(px) => px,
                ScreenDimension::Percent(frac) => win_h * frac,
            };
            if (panel.height() - new_h).abs() > 0.01 {
                panel.set_height(new_h);
            }
        }

        let (screen_x, screen_y) = match position {
            ScreenPosition::Screen => {
                let (fx, fy) = panel.anchor().offset_fraction();
                (fx * win_w, fy * win_h)
            },
            ScreenPosition::At(pos) => (pos.x, pos.y),
        };

        transform.translation.x = screen_x - half_w;
        transform.translation.y = half_h - screen_y;
    }
}

/// Spawns overlay cameras and lights for newly added screen-space panels.
///
/// For each unique `(camera_order, render_layers)` pair, a single shared
/// orthographic camera is created with `ScalingMode::WindowSize`
/// (1 world unit = 1 logical pixel). A directional light on the same
/// render layers provides stable illumination for PBR text materials.
fn setup_screen_space_cameras(
    added_panels: Query<(Entity, &DiegeticPanel), Added<DiegeticPanel>>,
    existing_cameras: Query<&ScreenSpaceCamera>,
    mut commands: Commands,
) {
    let mut spawned_this_frame: Vec<(isize, RenderLayers)> = Vec::new();

    for (panel_entity, panel) in &added_panels {
        let PanelMode::Screen {
            camera_order,
            ref render_layers,
            ..
        } = *panel.mode()
        else {
            continue;
        };

        let layers = render_layers;
        let order = camera_order;

        commands.entity(panel_entity).insert(layers.clone());

        let camera_exists = existing_cameras
            .iter()
            .any(|cam| cam.render_layers == *layers && cam.camera_order == order)
            || spawned_this_frame
                .iter()
                .any(|(o, l)| *o == order && *l == *layers);

        if camera_exists {
            continue;
        }

        spawned_this_frame.push((order, layers.clone()));

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
            bevy::render::view::Msaa::default(),
            layers.clone(),
        ));

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
fn propagate_screen_space_render_layers(
    panels_with_layers: Query<(Entity, &RenderLayers, &DiegeticPanel)>,
    children_query: Query<&Children>,
    existing_layers: Query<&RenderLayers>,
    mut commands: Commands,
) {
    for (panel_entity, panel_layers, panel) in &panels_with_layers {
        if !panel.mode().is_screen() {
            continue;
        }
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

/// Despawns overlay cameras and lights when no screen-space panels
/// reference their `(camera_order, render_layers)` pair.
fn cleanup_screen_space_cameras(
    panels: Query<&DiegeticPanel>,
    cameras: Query<(Entity, &ScreenSpaceCamera)>,
    lights: Query<(Entity, &ScreenSpaceLight)>,
    mut commands: Commands,
    mut prev_pairs: Local<Vec<(isize, RenderLayers)>>,
) {
    let current_pairs: Vec<(isize, RenderLayers)> = panels
        .iter()
        .filter_map(|panel| {
            if let PanelMode::Screen {
                camera_order,
                ref render_layers,
                ..
            } = *panel.mode()
            {
                Some((camera_order, render_layers.clone()))
            } else {
                None
            }
        })
        .collect();

    if current_pairs.len() == prev_pairs.len()
        && current_pairs
            .iter()
            .zip(prev_pairs.iter())
            .all(|(a, b)| a.0 == b.0 && a.1 == b.1)
    {
        *prev_pairs = current_pairs;
        return;
    }

    for (entity, cam) in &cameras {
        let still_active = current_pairs
            .iter()
            .any(|(order, layers)| *order == cam.camera_order && *layers == cam.render_layers);
        if !still_active {
            commands.entity(entity).despawn();
        }
    }

    for (entity, light) in &lights {
        let still_active = current_pairs
            .iter()
            .any(|(_, layers)| *layers == light.render_layers);
        if !still_active {
            commands.entity(entity).despawn();
        }
    }

    *prev_pairs = current_pairs;
}
