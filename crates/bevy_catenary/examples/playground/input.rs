//! `DragState`, `handle_drag`, shortcut systems, slack title-bar pulse systems,
//! and pointer observers.

use std::time::Duration;

use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;
use bevy_catenary::Cable;
use bevy_catenary::CableMeshChild;
use bevy_catenary::CurveKind;
use bevy_catenary::Solver;
use bevy_lagrange::ZoomToFit;
use fairy_dust::FairyDustOrbitCam;

use super::constants::DETACH_R_RESET_FLASH_SECONDS;
use super::constants::MIN_TAUT_CABLE_SLACK;
use super::constants::NAVIGATION_DURATION_MS;
use super::constants::RAY_EPSILON;
use super::constants::SLACK_ADJUSTMENT_STEP;
use super::constants::SLACK_PULSE_SECONDS;
use super::constants::ZOOM_DURATION_MS;
use super::constants::ZOOM_MARGIN_GROUND;
use super::constants::ZOOM_MARGIN_MESH;
use super::detach_demo;
use super::detach_demo::DetachDemoEntity;
use super::entities;
use super::entities::Despawnable;
use super::entities::Draggable;
use super::entities::FullSceneTarget;
use super::entities::NodeCube;
use super::entities::Selected;
use super::entities::SlackLocked;
use super::labels::RResetFlash;
use super::scene::SharedCableMaterial;

#[derive(Resource, Default)]
pub(crate) struct DragState {
    entity:      Option<Entity>,
    y_height:    f32,
    /// Offset from the cursor hit point to the entity center (XZ only).
    grab_offset: Vec2,
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
    cameras: Query<(&Camera, &GlobalTransform), With<FairyDustOrbitCam>>,
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
    cameras: Query<(&Camera, &GlobalTransform), With<FairyDustOrbitCam>>,
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

/// `F` — frame the whole scene by fitting the ground plane.
pub(crate) fn frame_full_scene(
    mut commands: Commands,
    camera: Single<Entity, With<FairyDustOrbitCam>>,
    ground: Single<Entity, With<FullSceneTarget>>,
) {
    commands.trigger(
        ZoomToFit::new(*camera, *ground)
            .margin(ZOOM_MARGIN_GROUND)
            .duration(Duration::from_millis(NAVIGATION_DURATION_MS))
            .easing(EaseFunction::CubicOut),
    );
}

/// `R` — despawn and respawn the Detach Policy section, and flash the
/// "R - Reset" ground line yellow.
pub(crate) fn reset_detach_demo(
    mut commands: Commands,
    time: Res<Time>,
    mut r_reset_flash: ResMut<RResetFlash>,
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
    r_reset_flash.flash_until_secs = Some(time.elapsed_secs() + DETACH_R_RESET_FLASH_SECONDS);

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

fn adjust_cable_slack(delta: f32, cables: &mut Query<&mut Cable, Without<SlackLocked>>) {
    for mut cable in cables.iter_mut() {
        match &mut cable.solver {
            Solver::Catenary(catenary)
            | Solver::Routed {
                curve_kind: CurveKind::Catenary(catenary),
                ..
            } => {
                catenary.slack = (catenary.slack + delta).max(MIN_TAUT_CABLE_SLACK);
            },
            _ => {},
        }
    }
}

/// `=` held — increase catenary slack each frame.
pub(crate) fn increase_slack(mut cables: Query<&mut Cable, Without<SlackLocked>>) {
    adjust_cable_slack(SLACK_ADJUSTMENT_STEP, &mut cables);
}

/// `-` held — decrease catenary slack each frame.
pub(crate) fn decrease_slack(mut cables: Query<&mut Cable, Without<SlackLocked>>) {
    adjust_cable_slack(-SLACK_ADJUSTMENT_STEP, &mut cables);
}

/// Lights the `+` slack segment in the title bar.
#[derive(Event)]
pub(crate) struct SlackPlusPulseBegin;

/// Clears the `+` slack segment highlight.
#[derive(Event)]
pub(crate) struct SlackPlusPulseEnd;

/// Lights the `-` slack segment in the title bar.
#[derive(Event)]
pub(crate) struct SlackMinusPulseBegin;

/// Clears the `-` slack segment highlight.
#[derive(Event)]
pub(crate) struct SlackMinusPulseEnd;

/// Deadlines (in elapsed seconds) at which each slack segment highlight clears.
#[derive(Resource, Default)]
pub(crate) struct SlackPulse {
    plus_end_secs:  Option<f32>,
    minus_end_secs: Option<f32>,
}

/// Flashes the `+` slack segment for [`SLACK_PULSE_SECONDS`].
pub(crate) fn begin_plus_slack_pulse(
    time: Res<Time>,
    mut pulse: ResMut<SlackPulse>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs();
    commands.trigger(SlackPlusPulseBegin);
    pulse.plus_end_secs = Some(now + SLACK_PULSE_SECONDS);
}

/// Flashes the `-` slack segment for [`SLACK_PULSE_SECONDS`].
pub(crate) fn begin_minus_slack_pulse(
    time: Res<Time>,
    mut pulse: ResMut<SlackPulse>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs();
    commands.trigger(SlackMinusPulseBegin);
    pulse.minus_end_secs = Some(now + SLACK_PULSE_SECONDS);
}

/// Clears slack title-bar highlights once their deadlines pass.
pub(crate) fn clear_slack_pulses(
    time: Res<Time>,
    mut pulse: ResMut<SlackPulse>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs();
    if pulse.plus_end_secs.is_some_and(|end| now >= end) {
        commands.trigger(SlackPlusPulseEnd);
        pulse.plus_end_secs = None;
    }

    if pulse.minus_end_secs.is_some_and(|end| now >= end) {
        commands.trigger(SlackMinusPulseEnd);
        pulse.minus_end_secs = None;
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
