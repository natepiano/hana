use super::*;

/// Tracks the mesh entity currently under the cursor for `LookAt` / `LookAtAndZoomToFit`.
#[derive(Resource, Default)]
pub(crate) struct HoveredEntity(pub(crate) Option<Entity>);

pub(crate) fn on_mesh_clicked(
    click: On<Pointer<Click>>,
    mut commands: Commands,
    selected: Query<Entity, With<selection_gizmo::Selected>>,
    active_easing: Res<ActiveEasing>,
    time: Res<Time<Virtual>>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    if time.is_paused() {
        return;
    }
    for entity in &selected {
        commands
            .entity(entity)
            .remove::<selection_gizmo::Selected>();
    }

    let clicked = click.entity;
    let camera = click.hit.camera;
    commands.entity(clicked).insert(selection_gizmo::Selected);
    commands.trigger(
        ZoomToFit::new(camera, clicked)
            .margin(ZOOM_MARGIN_MESH)
            .duration(Duration::from_millis(ZOOM_DURATION_MILLIS))
            .easing(active_easing.0),
    );
}

pub(crate) fn on_ground_clicked(
    click: On<Pointer<Click>>,
    mut commands: Commands,
    scene: Res<SceneEntities>,
    selected: Query<Entity, With<selection_gizmo::Selected>>,
    active_easing: Res<ActiveEasing>,
    time: Res<Time<Virtual>>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    if time.is_paused() {
        return;
    }
    for entity in &selected {
        commands
            .entity(entity)
            .remove::<selection_gizmo::Selected>();
    }

    let camera = click.hit.camera;
    commands.trigger(
        ZoomToFit::new(camera, scene.scene_bounds)
            .margin(ZOOM_MARGIN_SCENE)
            .duration(Duration::from_millis(ZOOM_DURATION_MILLIS))
            .easing(active_easing.0),
    );
}

pub(crate) fn on_below_clicked(
    click: On<Pointer<Click>>,
    mut commands: Commands,
    scene: Res<SceneEntities>,
    selected: Query<Entity, With<selection_gizmo::Selected>>,
    active_easing: Res<ActiveEasing>,
    time: Res<Time<Virtual>>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    if time.is_paused() {
        return;
    }
    for entity in &selected {
        commands
            .entity(entity)
            .remove::<selection_gizmo::Selected>();
    }

    let camera = click.hit.camera;
    commands.trigger(
        AnimateToFit::new(camera, scene.scene_bounds)
            .yaw(CAMERA_START_YAW)
            .pitch(CAMERA_START_PITCH)
            .margin(ZOOM_MARGIN_SCENE)
            .duration(Duration::from_millis(ANIMATE_FIT_DURATION_MILLIS))
            .easing(active_easing.0),
    );
}

pub(crate) fn on_mesh_dragged(
    drag: On<Pointer<Drag>>,
    mut transforms: Query<&mut Transform>,
    time: Res<Time<Virtual>>,
) {
    if time.is_paused() {
        return;
    }
    if let Ok(mut transform) = transforms.get_mut(drag.entity) {
        transform.rotate_y(drag.delta.x * DRAG_SENSITIVITY);
        transform.rotate_x(drag.delta.y * DRAG_SENSITIVITY);
    }
}

pub(crate) fn on_mesh_hover(hover: On<Pointer<Over>>, mut hovered: ResMut<HoveredEntity>) {
    hovered.0 = Some(hover.entity);
}

pub(crate) fn on_mesh_unhover(hover: On<Pointer<Out>>, mut hovered: ResMut<HoveredEntity>) {
    if hovered.0 == Some(hover.entity) {
        hovered.0 = None;
    }
}

pub(crate) fn look_at_hovered(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    hovered: Res<HoveredEntity>,
    scene: Res<SceneEntities>,
    second: Option<Res<second_window::SecondWindowEntities>>,
    active_easing: Res<ActiveEasing>,
    windows: Query<&Window>,
) {
    if !keyboard.just_pressed(KeyCode::KeyF) {
        return;
    }
    let Some(target) = hovered.0 else {
        return;
    };
    let camera = second_window::focused_camera(&scene, second.as_deref(), &windows);
    commands.trigger(
        LookAt::new(camera, target)
            .duration(Duration::from_millis(LOOK_AT_DURATION_MILLIS))
            .easing(active_easing.0),
    );
}

pub(crate) fn look_at_and_zoom_to_fit_hovered(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    hovered: Res<HoveredEntity>,
    scene: Res<SceneEntities>,
    second: Option<Res<second_window::SecondWindowEntities>>,
    active_easing: Res<ActiveEasing>,
    windows: Query<&Window>,
) {
    if !keyboard.just_pressed(KeyCode::KeyG) {
        return;
    }
    let Some(target) = hovered.0 else {
        return;
    };
    let camera = second_window::focused_camera(&scene, second.as_deref(), &windows);
    commands.trigger(
        LookAtAndZoomToFit::new(camera, target)
            .margin(ZOOM_MARGIN_MESH)
            .duration(Duration::from_millis(LOOK_AT_DURATION_MILLIS))
            .easing(active_easing.0),
    );
}
