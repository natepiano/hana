//! `DragState`, `SlackAdjustment`, `handle_drag`, `handle_keyboard`, and the
//! `on_drag_start`, `on_despawnable_clicked`, and `on_cable_mesh_child_added`
//! pointer observers.

use std::time::Duration;

use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;
use bevy_catenary::Cable;
use bevy_catenary::CableMeshChild;
use bevy_catenary::CurveKind;
use bevy_catenary::DebugGizmos;
use bevy_catenary::Solver;
use bevy_lagrange::ZoomToFit;

use super::constants::MIN_TAUT_CABLE_SLACK;
use super::constants::NAVIGATION_DURATION_MS;
use super::constants::RAY_EPSILON;
use super::constants::SLACK_ADJUSTMENT_STEP;
use super::constants::ZOOM_DURATION_MS;
use super::constants::ZOOM_MARGIN_GROUND;
use super::constants::ZOOM_MARGIN_MESH;
use super::constants::ZOOM_MARGIN_NAVIGATION;
use super::detach_demo;
use super::detach_demo::DetachDemoEntity;
use super::entities;
use super::entities::Despawnable;
use super::entities::Draggable;
use super::entities::NodeCube;
use super::entities::Selected;
use super::entities::SlackLocked;
use super::scene::SceneEntities;
use super::scene::SharedCableMaterial;
use super::sections::CurrentSection;
use super::sections::SectionBounds;

#[derive(Resource, Default)]
pub(crate) struct DragState {
    entity:      Option<Entity>,
    y_height:    f32,
    /// Offset from the cursor hit point to the entity center (XZ only).
    grab_offset: Vec2,
}

#[derive(Clone, Copy)]
enum SlackAdjustment {
    Increase,
    Decrease,
    None,
}

impl SlackAdjustment {
    const fn delta(self) -> f32 {
        match self {
            Self::Increase => SLACK_ADJUSTMENT_STEP,
            Self::Decrease => -SLACK_ADJUSTMENT_STEP,
            Self::None => 0.0,
        }
    }
}

impl From<&ButtonInput<KeyCode>> for SlackAdjustment {
    fn from(keyboard: &ButtonInput<KeyCode>) -> Self {
        match (
            keyboard.pressed(KeyCode::Equal),
            keyboard.pressed(KeyCode::Minus),
        ) {
            (true, false) => Self::Increase,
            (false, true) => Self::Decrease,
            _ => Self::None,
        }
    }
}

fn cursor_ray_y_plane(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    window: &Window,
    y_height: f32,
) -> Option<Vec3> {
    let cursor = window.cursor_position()?;
    let ray = camera.viewport_to_world(camera_transform, cursor).ok()?;
    let denom = ray.direction.y;
    if denom.abs() < RAY_EPSILON {
        return None;
    }
    let t = (y_height - ray.origin.y) / denom;
    if t < 0.0 {
        return None;
    }
    Some(ray.origin + ray.direction * t)
}

pub(crate) fn handle_drag(
    mut drag_state: ResMut<DragState>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window>,
    mut draggables: Query<&mut Transform, With<Draggable>>,
) {
    if mouse_buttons.just_released(MouseButton::Left) {
        drag_state.entity = None;
        return;
    }

    let Some(dragged) = drag_state.entity else {
        return;
    };
    if !mouse_buttons.pressed(MouseButton::Left) {
        drag_state.entity = None;
        return;
    }

    let Ok((camera, camera_transform)) = cameras.single() else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(hit) = cursor_ray_y_plane(camera, camera_transform, window, drag_state.y_height)
    else {
        return;
    };

    if let Ok(mut transform) = draggables.get_mut(dragged) {
        transform.translation.x = hit.x + drag_state.grab_offset.x;
        transform.translation.z = hit.z + drag_state.grab_offset.y;
    }
}

pub(crate) fn on_drag_start(
    click: On<Pointer<Press>>,
    mut drag_state: ResMut<DragState>,
    transforms: Query<&Transform, With<Draggable>>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    windows: Query<&Window>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    let entity = click.entity;
    let Ok(transform) = transforms.get(entity) else {
        return;
    };
    drag_state.entity = Some(entity);
    drag_state.y_height = transform.translation.y;
    drag_state.grab_offset = Vec2::ZERO;

    let Ok((camera, camera_transform)) = cameras.single() else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(hit) = cursor_ray_y_plane(camera, camera_transform, window, transform.translation.y)
    else {
        return;
    };
    drag_state.grab_offset = Vec2::new(
        transform.translation.x - hit.x,
        transform.translation.z - hit.z,
    );
}

pub(crate) fn on_despawnable_clicked(click: On<Pointer<Click>>, mut commands: Commands) {
    commands.entity(click.entity).despawn();
}

pub(crate) fn on_cable_mesh_child_added(
    trigger: On<Insert, CableMeshChild>,
    cables: Query<&CableMeshChild>,
    mut commands: Commands,
) {
    let Ok(cable_mesh_child) = cables.get(trigger.event_target()) else {
        return;
    };
    commands.entity(cable_mesh_child.0).observe(on_mesh_clicked);
}

pub(crate) fn handle_keyboard(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut debug_gizmos: ResMut<DebugGizmos>,
    scene_entities: Res<SceneEntities>,
    mut cables: Query<&mut Cable, Without<SlackLocked>>,
    shared_cable_material: Res<SharedCableMaterial>,
    detach_entities: Query<Entity, With<DetachDemoEntity>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    node_mesh_query: Query<&Mesh3d, (With<NodeCube>, Without<Draggable>, Without<Despawnable>)>,
    node_material_query: Query<
        &MeshMaterial3d<StandardMaterial>,
        (With<NodeCube>, Without<Draggable>, Without<Despawnable>),
    >,
) {
    if keyboard.just_pressed(KeyCode::KeyD) {
        *debug_gizmos = match *debug_gizmos {
            DebugGizmos::Enabled => DebugGizmos::Disabled,
            DebugGizmos::Disabled => DebugGizmos::Enabled,
        };
    }

    if keyboard.just_pressed(KeyCode::KeyF) {
        commands.trigger(
            ZoomToFit::new(scene_entities.camera, scene_entities.ground)
                .margin(ZOOM_MARGIN_GROUND)
                .duration(Duration::from_millis(NAVIGATION_DURATION_MS))
                .easing(EaseFunction::CubicOut),
        );
    }

    let slack_delta = SlackAdjustment::from(keyboard.as_ref()).delta();
    if slack_delta != 0.0 {
        for mut cable in &mut cables {
            match &mut cable.solver {
                Solver::Catenary(catenary)
                | Solver::Routed {
                    curve_kind: CurveKind::Catenary(catenary),
                    ..
                } => {
                    catenary.slack = (catenary.slack + slack_delta).max(MIN_TAUT_CABLE_SLACK);
                },
                _ => {},
            }
        }
    }

    if keyboard.just_pressed(KeyCode::KeyR) {
        let node_mesh = node_mesh_query.iter().next().map(|m| m.0.clone());
        let node_material = node_material_query.iter().next().map(|m| m.0.clone());

        for entity in &detach_entities {
            commands.entity(entity).despawn();
        }

        if let (Some(node_mesh), Some(node_material)) = (node_mesh, node_material) {
            detach_demo::spawn_detach_demo(
                &mut commands,
                &mut meshes,
                &mut materials,
                &node_mesh,
                &node_material,
                &shared_cable_material.0,
            );
        }
    }
}

pub(crate) fn on_mesh_clicked(
    click: On<Pointer<Click>>,
    mut commands: Commands,
    selected: Query<Entity, With<Selected>>,
    draggables: Query<(), With<Draggable>>,
) {
    if draggables.get(click.entity).is_ok() {
        return;
    }

    entities::deselect_all(&mut commands, &selected);

    let clicked = click.entity;
    let camera = click.hit.camera;
    commands.entity(clicked).insert(Selected);
    commands.trigger(
        ZoomToFit::new(camera, clicked)
            .margin(ZOOM_MARGIN_MESH)
            .duration(Duration::from_millis(ZOOM_DURATION_MS))
            .easing(EaseFunction::CubicOut),
    );
}

pub(crate) fn on_ground_clicked(
    _click: On<Pointer<Click>>,
    mut commands: Commands,
    selected: Query<Entity, With<Selected>>,
    scene_entities: Res<SceneEntities>,
    section_bounds: Res<SectionBounds>,
    current_section: Res<CurrentSection>,
) {
    entities::deselect_all(&mut commands, &selected);

    commands.trigger(
        ZoomToFit::new(scene_entities.camera, section_bounds.0[current_section.0])
            .margin(ZOOM_MARGIN_NAVIGATION)
            .duration(Duration::from_millis(ZOOM_DURATION_MS))
            .easing(EaseFunction::CubicOut),
    );
}
