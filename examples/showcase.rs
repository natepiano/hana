//! Demonstrates clicking on meshes to zoom-to-fit.
//!
//! - Click a mesh to select it and zoom the camera to frame it
//! - Click the ground to deselect and zoom out to the full scene
//! - Drag a mesh to rotate it
//! - Selected meshes show a gizmo outline
//! - Press 'D' to toggle debug visualization of zoom-to-fit bounds

use std::f32::consts::PI;
use std::time::Duration;

use bevy::camera::RenderTarget;
use bevy::camera::ScalingMode;
use bevy::camera::visibility::RenderLayers;
use bevy::color::palettes::css::DEEP_SKY_BLUE;
use bevy::color::palettes::css::ORANGE;
use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;
use bevy::time::Virtual;
use bevy::ui::UiTargetCamera;
use bevy::window::WindowRef;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_lagrange::AnimateToFit;
use bevy_lagrange::AnimationBegin;
use bevy_lagrange::AnimationCancelled;
use bevy_lagrange::AnimationConflictPolicy;
use bevy_lagrange::AnimationEnd;
use bevy_lagrange::AnimationRejected;
use bevy_lagrange::AnimationSource;
use bevy_lagrange::CameraInputInterruptBehavior;
use bevy_lagrange::CameraMove;
use bevy_lagrange::CameraMoveBegin;
use bevy_lagrange::CameraMoveEnd;
use bevy_lagrange::FitVisualization;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::LookAt;
use bevy_lagrange::LookAtAndZoomToFit;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::PlayAnimation;
use bevy_lagrange::TrackpadBehavior;
use bevy_lagrange::ZoomBegin;
use bevy_lagrange::ZoomCancelled;
use bevy_lagrange::ZoomEnd;
use bevy_lagrange::ZoomToFit;
use bevy_window_manager::ManagedWindow;
use bevy_window_manager::WindowManagerPlugin;

// durations
const ANIMATE_FIT_DURATION_MS: u64 = 1200;
const ORBIT_MOVE_DURATION_MS: u64 = 800;
const LOOK_AT_DURATION_MS: u64 = 800;
const ZOOM_DURATION_MS: u64 = 1000;

// camera home
const CAMERA_START_PITCH: f32 = 0.4;
const CAMERA_START_YAW: f32 = -0.2;

// sensitivity
const DRAG_SENSITIVITY: f32 = 0.02;

// event log
const EVENT_LOG_COLOR: Color = Color::srgba(0.0, 1.0, 0.0, 0.9);
const EVENT_LOG_COLOR_RED: Color = Color::srgba(1.0, 0.3, 0.3, 0.9);
const EVENT_LOG_FONT_SIZE: f32 = 14.0;
const EVENT_LOG_SCROLL_SPEED: f32 = 120.0;
const EVENT_LOG_SEPARATOR: &str = "- - - - - - - - - - - -";
const EVENT_LOG_WIDTH: f32 = 300.0;
const UI_FONT_SIZE: f32 = 13.0;

// mesh settings
const GIZMO_DEPTH_BIAS: f32 = -0.005;
const GIZMO_LINE_WIDTH: f32 = 2.0;
const GIZMO_SCALE: f32 = 1.001;
const MESH_CENTER_Y: f32 = 1.0;
const SELECTION_GIZMO_LAYER: usize = 1;
const ZOOM_MARGIN_MESH: f32 = 0.15;
const ZOOM_MARGIN_SCENE: f32 = 0.08;

// window label
const WINDOW_LABEL_DURATION_SECS: f32 = 2.0;

// the easings!
const ALL_EASINGS: &[EaseFunction] = &[
    EaseFunction::Linear,
    EaseFunction::QuadraticIn,
    EaseFunction::QuadraticOut,
    EaseFunction::QuadraticInOut,
    EaseFunction::CubicIn,
    EaseFunction::CubicOut,
    EaseFunction::CubicInOut,
    EaseFunction::QuarticIn,
    EaseFunction::QuarticOut,
    EaseFunction::QuarticInOut,
    EaseFunction::QuinticIn,
    EaseFunction::QuinticOut,
    EaseFunction::QuinticInOut,
    EaseFunction::SmoothStepIn,
    EaseFunction::SmoothStepOut,
    EaseFunction::SmoothStep,
    EaseFunction::SmootherStepIn,
    EaseFunction::SmootherStepOut,
    EaseFunction::SmootherStep,
    EaseFunction::SineIn,
    EaseFunction::SineOut,
    EaseFunction::SineInOut,
    EaseFunction::CircularIn,
    EaseFunction::CircularOut,
    EaseFunction::CircularInOut,
    EaseFunction::ExponentialIn,
    EaseFunction::ExponentialOut,
    EaseFunction::ExponentialInOut,
    EaseFunction::ElasticIn,
    EaseFunction::ElasticOut,
    EaseFunction::ElasticInOut,
    EaseFunction::BackIn,
    EaseFunction::BackOut,
    EaseFunction::BackInOut,
    EaseFunction::BounceIn,
    EaseFunction::BounceOut,
    EaseFunction::BounceInOut,
];

// ============================================================================
// Types
// ============================================================================

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
enum AppState {
    #[default]
    Loading,
    Running,
}

#[derive(Component)]
struct Selected;

#[derive(Default, Reflect, GizmoConfigGroup)]
struct SelectionGizmo;

#[derive(Component)]
enum MeshShape {
    Cuboid(Vec3),
    Sphere(f32),
    Torus {
        minor_radius: f32,
        major_radius: f32,
    },
}

#[derive(Resource)]
struct SceneEntities {
    camera:       Entity,
    scene_bounds: Entity,
}

#[derive(Resource)]
struct ActiveEasing(EaseFunction);

impl Default for ActiveEasing {
    fn default() -> Self { Self(EaseFunction::CubicOut) }
}

#[derive(Component)]
struct EventLogNode;

#[derive(Component)]
struct CameraInputInterruptBehaviorLabel;

#[derive(Component)]
struct AnimationConflictPolicyLabel;

#[derive(Component)]
struct PausedOverlay;

#[derive(Component)]
struct EventLogHint;

#[derive(Component)]
struct EventLogToggleHint;

#[derive(Resource)]
struct SecondWindowEntities {
    window: Entity,
    camera: Entity,
}

#[derive(Component)]
struct SecondWindowCamera;

#[derive(Component)]
struct WindowLabel(Timer);

/// Tracks the mesh entity currently under the cursor for `LookAt` / `LookAtAndZoomToFit`.
#[derive(Resource, Default)]
struct HoveredEntity(Option<Entity>);

/// Marker resource: when present, the next `AnimationEnd` enables the event log.
#[derive(Resource)]
struct EnableLogOnAnimationEnd;

struct PendingLogEntry {
    text:  String,
    color: Color,
}

#[derive(Resource, Default)]
struct EventLog {
    enabled: bool,
    pending: Vec<PendingLogEntry>,
}

// ============================================================================
// App entry point
// ============================================================================

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "extras - window 1".into(),
                    ..default()
                }),
                ..default()
            }),
            LagrangePlugin,
            MeshPickingPlugin,
            BrpExtrasPlugin::default(),
            WindowManagerPlugin,
        ))
        .init_gizmo_group::<SelectionGizmo>()
        .init_state::<AppState>()
        .init_resource::<ActiveEasing>()
        .init_resource::<EventLog>()
        .init_resource::<HoveredEntity>()
        .add_systems(Startup, (setup, init_selection_gizmo))
        .add_systems(
            Update,
            initial_fit_to_scene.run_if(in_state(AppState::Loading)),
        )
        .add_systems(
            Update,
            (
                cleanup_second_window,
                log_window_focus,
                despawn_window_labels,
                toggle_pause,
                toggle_event_log,
                draw_selection_gizmo,
                draw_hover_gizmo,
                sync_selection_gizmo_layers,
                update_event_log_text,
                scroll_event_log,
                (
                    toggle_second_window,
                    toggle_debug_visualization,
                    toggle_projection,
                    randomize_easing,
                    animate_camera,
                    animate_fit_to_scene,
                    toggle_interrupt_behavior,
                    toggle_animation_conflict_policy,
                    look_at_hovered,
                    look_at_and_zoom_to_fit_hovered,
                )
                    .run_if(not_paused),
            ),
        )
        .add_observer(enable_log_on_initial_fit)
        .add_observer(log_animation_begin)
        .add_observer(log_animation_end)
        .add_observer(log_animation_cancelled)
        .add_observer(log_camera_move_start)
        .add_observer(log_camera_move_end)
        .add_observer(log_zoom_begin)
        .add_observer(log_zoom_end)
        .add_observer(log_zoom_cancelled)
        .add_observer(log_animation_rejected)
        .run();
}

// ============================================================================
// Scene setup
// ============================================================================

fn init_selection_gizmo(mut config_store: ResMut<GizmoConfigStore>) {
    let (config, _) = config_store.config_mut::<SelectionGizmo>();
    config.depth_bias = GIZMO_DEPTH_BIAS;
    config.line.width = GIZMO_LINE_WIDTH;
    config.render_layers = RenderLayers::layer(SELECTION_GIZMO_LAYER);
}

#[allow(clippy::too_many_lines)]
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Ground plane (clickable from above — deselects and zooms to scene bounds)
    let ground = commands
        .spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(12.0, 12.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.3, 0.5, 0.3).with_alpha(0.85),
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
        ))
        .observe(on_ground_clicked)
        .id();

    // Underside plane (clickable from below — deselects and animates back to scene)
    commands
        .spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(12.0, 12.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.0, 0.0, 0.0, 0.0),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_rotation(Quat::from_rotation_x(PI)),
        ))
        .observe(on_below_clicked);

    // Directional light
    commands.spawn((
        DirectionalLight {
            illuminance: 3000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::ZYX, 0.0, PI / 4.0, -PI / 4.0)),
    ));

    // Cuboid
    let cuboid_size = Vec3::new(1.0, 1.0, 1.0);
    commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(cuboid_size.x, cuboid_size.y, cuboid_size.z))),
            MeshMaterial3d(materials.add(Color::srgb(0.8, 0.7, 0.6))),
            Transform::from_xyz(-2.5, MESH_CENTER_Y, 0.0),
            MeshShape::Cuboid(cuboid_size),
        ))
        .observe(on_mesh_clicked)
        .observe(on_mesh_dragged)
        .observe(on_mesh_hover)
        .observe(on_mesh_unhover);

    // Sphere
    let sphere_radius = 0.5;
    commands
        .spawn((
            Mesh3d(meshes.add(Sphere::new(sphere_radius).mesh().uv(128, 64))),
            MeshMaterial3d(materials.add(Color::srgb(0.9, 0.3, 0.2))),
            Transform::from_xyz(0.0, MESH_CENTER_Y, 0.0),
            MeshShape::Sphere(sphere_radius),
        ))
        .observe(on_mesh_clicked)
        .observe(on_mesh_dragged)
        .observe(on_mesh_hover)
        .observe(on_mesh_unhover);

    // Torus
    let torus_minor = 0.25;
    let torus_major = 0.75;
    commands
        .spawn((
            Mesh3d(
                meshes.add(
                    Torus::new(torus_minor, torus_major)
                        .mesh()
                        .minor_resolution(64)
                        .major_resolution(64),
                ),
            ),
            MeshMaterial3d(materials.add(Color::srgb(0.9, 0.5, 0.1))),
            Transform::from_xyz(2.5, MESH_CENTER_Y, 0.0),
            MeshShape::Torus {
                minor_radius: torus_minor,
                major_radius: torus_major,
            },
        ))
        .observe(on_mesh_clicked)
        .observe(on_mesh_dragged)
        .observe(on_mesh_hover)
        .observe(on_mesh_unhover);

    // Camera (middle-click orbit, shift+middle pan, trackpad support)
    let camera = commands
        .spawn(OrbitCam {
            button_orbit: MouseButton::Middle,
            button_pan: MouseButton::Middle,
            modifier_pan: Some(KeyCode::ShiftLeft),
            trackpad_behavior: TrackpadBehavior::BlenderLike {
                modifier_pan:  Some(KeyCode::ShiftLeft),
                modifier_zoom: Some(KeyCode::ControlLeft),
            },
            trackpad_pinch_to_zoom_enabled: true,
            yaw: Some(CAMERA_START_YAW),
            pitch: Some(CAMERA_START_PITCH),
            ..default()
        })
        .id();

    // Instructions
    commands.spawn((
        Text::new("Click a mesh to zoom-to-fit\nClick the ground to zoom back out\n\nPress:\n'Esc' pause / unpause\n'P' toggle projection\n'D' debug visualization\n'H' Home w/animate fit to scene\n'A' animate camera\n'F' look at hovered mesh\n'G' look at + zoom-to-fit hovered mesh\n'R' randomize easing\n'E' reset to 'CubicOut' easing\n'I' toggle interrupt behavior\n'Q' cycle conflict policy\n'W' toggle second window"),
        TextFont {
            font_size: UI_FONT_SIZE,
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        },
        UiTargetCamera(camera),
    ));

    // Interrupt behavior hint (bottom-left)
    commands.spawn((
        Text::new(interrupt_behavior_hint_text(
            CameraInputInterruptBehavior::Ignore,
        )),
        TextFont {
            font_size: UI_FONT_SIZE,
            ..default()
        },
        TextColor(Color::srgba(0.7, 0.7, 0.7, 0.7)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        },
        CameraInputInterruptBehaviorLabel,
        UiTargetCamera(camera),
    ));

    // Conflict policy hint (bottom-left, above interrupt behavior)
    commands.spawn((
        Text::new(conflict_policy_hint_text(AnimationConflictPolicy::LastWins)),
        TextFont {
            font_size: UI_FONT_SIZE,
            ..default()
        },
        TextColor(Color::srgba(0.7, 0.7, 0.7, 0.7)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(32.0),
            left: Val::Px(12.0),
            ..default()
        },
        AnimationConflictPolicyLabel,
        UiTargetCamera(camera),
    ));

    // Event log scroll container (right edge, scrollable, hidden until enabled)
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            right: Val::Px(12.0),
            width: Val::Px(EVENT_LOG_WIDTH),
            bottom: Val::Px(72.0),
            flex_direction: FlexDirection::Column,
            overflow: Overflow::scroll_y(),
            ..default()
        },
        Visibility::Hidden,
        Pickable::IGNORE,
        EventLogNode,
        UiTargetCamera(camera),
    ));

    // Log toggle hint (bottom-right, always visible once initial animation completes)
    commands.spawn((
        Text::new("'L' toggle log off and on"),
        TextFont {
            font_size: UI_FONT_SIZE,
            ..default()
        },
        TextColor(Color::srgba(0.7, 0.7, 0.7, 0.7)),
        TextLayout::new_with_justify(Justify::Left),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(12.0),
            right: Val::Px(12.0),
            width: Val::Px(EVENT_LOG_WIDTH),
            ..default()
        },
        Visibility::Hidden,
        EventLogToggleHint,
        UiTargetCamera(camera),
    ));

    // Log scroll/clear hints (bottom-right, hidden until log enabled)
    commands.spawn((
        Text::new("Up/Down scroll log\n'C' clear log"),
        TextFont {
            font_size: UI_FONT_SIZE,
            ..default()
        },
        TextColor(Color::srgba(0.7, 0.7, 0.7, 0.7)),
        TextLayout::new_with_justify(Justify::Left),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(28.0),
            right: Val::Px(12.0),
            width: Val::Px(EVENT_LOG_WIDTH),
            ..default()
        },
        Visibility::Hidden,
        EventLogHint,
        UiTargetCamera(camera),
    ));

    // Paused overlay (centered, hidden until Esc)
    commands.spawn((
        Text::new("PAUSED"),
        TextFont {
            font_size: 48.0,
            ..default()
        },
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.4)),
        TextLayout::new_with_justify(Justify::Center),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(46.0),
            width: Val::Percent(100.0),
            ..default()
        },
        Visibility::Hidden,
        PausedOverlay,
        UiTargetCamera(camera),
    ));

    commands.insert_resource(SceneEntities {
        camera,
        scene_bounds: ground,
    });
}

fn initial_fit_to_scene(
    mut commands: Commands,
    scene: Res<SceneEntities>,
    mesh_query: Query<&Mesh3d>,
    meshes: Res<Assets<Mesh>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let Ok(mesh3d) = mesh_query.get(scene.scene_bounds) else {
        return;
    };
    if meshes.get(&mesh3d.0).is_none() {
        return;
    }
    commands.insert_resource(EnableLogOnAnimationEnd);
    commands.trigger(
        AnimateToFit::new(scene.camera, scene.scene_bounds)
            .yaw(CAMERA_START_YAW)
            .pitch(CAMERA_START_PITCH)
            .margin(ZOOM_MARGIN_SCENE)
            .easing(EaseFunction::QuadraticInOut),
    );
    next_state.set(AppState::Running);
}

// ============================================================================
// Second window
// ============================================================================

fn all_cameras(scene: &SceneEntities, second: Option<&SecondWindowEntities>) -> Vec<Entity> {
    let mut cameras = vec![scene.camera];
    if let Some(sw) = second {
        cameras.push(sw.camera);
    }
    cameras
}

/// Returns the camera entity whose window is currently focused.
fn focused_camera(
    scene: &SceneEntities,
    second: Option<&SecondWindowEntities>,
    windows: &Query<&Window>,
) -> Entity {
    if let Some(sw) = second
        && let Ok(win) = windows.get(sw.window)
        && win.focused
    {
        return sw.camera;
    }
    // Primary window is the default fallback
    scene.camera
}

fn toggle_second_window(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    scene: Res<SceneEntities>,
    second: Option<Res<SecondWindowEntities>>,
    easing: Res<ActiveEasing>,
    camera_query: Query<&OrbitCam>,
    mut log: ResMut<EventLog>,
) {
    if !keyboard.just_pressed(KeyCode::KeyW) {
        return;
    }

    if let Some(sw) = second {
        commands.entity(sw.window).despawn();
        commands.entity(sw.camera).despawn();
        commands.remove_resource::<SecondWindowEntities>();
        log.push("Window 2: closed".into());
        return;
    }

    let window = commands
        .spawn((
            Window {
                title: "extras - window 2".into(),
                ..default()
            },
            ManagedWindow {
                window_name: "window_2".into(),
            },
        ))
        .id();

    // Clone settings from primary camera
    let Ok(primary) = camera_query.get(scene.camera) else {
        return;
    };
    let mut second_cam = *primary;
    second_cam.yaw = Some(CAMERA_START_YAW);
    second_cam.pitch = Some(CAMERA_START_PITCH);

    let camera = commands
        .spawn((
            second_cam,
            RenderTarget::Window(WindowRef::Entity(window)),
            SecondWindowCamera,
        ))
        .id();

    commands.trigger(
        AnimateToFit::new(camera, scene.scene_bounds)
            .yaw(CAMERA_START_YAW)
            .pitch(CAMERA_START_PITCH)
            .margin(ZOOM_MARGIN_SCENE)
            .easing(easing.0),
    );

    // "Window 2" label centered in the second window, auto-despawns after a couple seconds
    commands.spawn((
        Text::new("Window 2"),
        TextFont {
            font_size: 48.0,
            ..default()
        },
        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.4)),
        TextLayout::new_with_justify(Justify::Center),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(46.0),
            width: Val::Percent(100.0),
            ..default()
        },
        UiTargetCamera(camera),
        WindowLabel(Timer::from_seconds(
            WINDOW_LABEL_DURATION_SECS,
            TimerMode::Once,
        )),
    ));

    commands.insert_resource(SecondWindowEntities { window, camera });
    log.push("Window 2: opened".into());
}

fn log_window_focus(
    second: Option<Res<SecondWindowEntities>>,
    windows: Query<(Entity, &Window)>,
    mut log: ResMut<EventLog>,
    mut last_focused: Local<Option<Entity>>,
) {
    let Some(sw) = second else {
        *last_focused = None;
        return;
    };

    let current_focused = windows.iter().find(|(_, w)| w.focused).map(|(e, _)| e);

    if current_focused != *last_focused {
        if let Some(focused) = current_focused {
            let label = if focused == sw.window {
                "Window 2"
            } else {
                "Window 1"
            };
            log.push(format!("{label} focused"));
        }
        *last_focused = current_focused;
    }
}

fn cleanup_second_window(
    mut commands: Commands,
    second: Option<Res<SecondWindowEntities>>,
    windows: Query<(), With<Window>>,
    mut log: ResMut<EventLog>,
) {
    let Some(sw) = second else {
        return;
    };
    // If the window entity no longer has a `Window` component, it was closed by the user
    if windows.get(sw.window).is_err() {
        commands.entity(sw.camera).despawn();
        commands.remove_resource::<SecondWindowEntities>();
        log.push("Window 2: closed".into());
    }
}

fn despawn_window_labels(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut WindowLabel)>,
) {
    for (entity, mut label) in &mut query {
        label.0.tick(time.delta());
        if label.0.just_finished() {
            commands.entity(entity).despawn();
        }
    }
}

// ============================================================================
// Pause
// ============================================================================

fn not_paused(time: Res<Time<Virtual>>) -> bool { !time.is_paused() }

fn toggle_pause(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut time: ResMut<Time<Virtual>>,
    mut overlay: Query<&mut Visibility, With<PausedOverlay>>,
) {
    if !keyboard.just_pressed(KeyCode::Escape) {
        return;
    }
    if time.is_paused() {
        time.unpause();
        for mut vis in &mut overlay {
            *vis = Visibility::Hidden;
        }
    } else {
        time.pause();
        for mut vis in &mut overlay {
            *vis = Visibility::Inherited;
        }
    }
}

// ============================================================================
// Pointer interaction
// ============================================================================

fn on_mesh_clicked(
    click: On<Pointer<Click>>,
    mut commands: Commands,
    selected: Query<Entity, With<Selected>>,
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
        commands.entity(entity).remove::<Selected>();
    }

    let clicked = click.entity;
    let camera = click.hit.camera;
    commands.entity(clicked).insert(Selected);
    commands.trigger(
        ZoomToFit::new(camera, clicked)
            .margin(ZOOM_MARGIN_MESH)
            .duration(Duration::from_millis(ZOOM_DURATION_MS))
            .easing(active_easing.0),
    );
}

fn on_ground_clicked(
    click: On<Pointer<Click>>,
    mut commands: Commands,
    scene: Res<SceneEntities>,
    selected: Query<Entity, With<Selected>>,
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
        commands.entity(entity).remove::<Selected>();
    }

    let camera = click.hit.camera;
    commands.trigger(
        ZoomToFit::new(camera, scene.scene_bounds)
            .margin(ZOOM_MARGIN_SCENE)
            .duration(Duration::from_millis(ZOOM_DURATION_MS))
            .easing(active_easing.0),
    );
}

fn on_below_clicked(
    click: On<Pointer<Click>>,
    mut commands: Commands,
    scene: Res<SceneEntities>,
    selected: Query<Entity, With<Selected>>,
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
        commands.entity(entity).remove::<Selected>();
    }

    let camera = click.hit.camera;
    commands.trigger(
        AnimateToFit::new(camera, scene.scene_bounds)
            .yaw(CAMERA_START_YAW)
            .pitch(CAMERA_START_PITCH)
            .margin(ZOOM_MARGIN_SCENE)
            .duration(Duration::from_millis(ANIMATE_FIT_DURATION_MS))
            .easing(active_easing.0),
    );
}

fn on_mesh_dragged(
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

fn on_mesh_hover(hover: On<Pointer<Over>>, mut hovered: ResMut<HoveredEntity>) {
    hovered.0 = Some(hover.entity);
}

fn on_mesh_unhover(hover: On<Pointer<Out>>, mut hovered: ResMut<HoveredEntity>) {
    if hovered.0 == Some(hover.entity) {
        hovered.0 = None;
    }
}

fn look_at_hovered(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    hovered: Res<HoveredEntity>,
    scene: Res<SceneEntities>,
    second: Option<Res<SecondWindowEntities>>,
    active_easing: Res<ActiveEasing>,
    windows: Query<&Window>,
) {
    if !keyboard.just_pressed(KeyCode::KeyF) {
        return;
    }
    let Some(target) = hovered.0 else {
        return;
    };
    let cam = focused_camera(&scene, second.as_deref(), &windows);
    commands.trigger(
        LookAt::new(cam, target)
            .duration(Duration::from_millis(LOOK_AT_DURATION_MS))
            .easing(active_easing.0),
    );
}

fn look_at_and_zoom_to_fit_hovered(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    hovered: Res<HoveredEntity>,
    scene: Res<SceneEntities>,
    second: Option<Res<SecondWindowEntities>>,
    active_easing: Res<ActiveEasing>,
    windows: Query<&Window>,
) {
    if !keyboard.just_pressed(KeyCode::KeyG) {
        return;
    }
    let Some(target) = hovered.0 else {
        return;
    };
    let cam = focused_camera(&scene, second.as_deref(), &windows);
    commands.trigger(
        LookAtAndZoomToFit::new(cam, target)
            .margin(ZOOM_MARGIN_MESH)
            .duration(Duration::from_millis(LOOK_AT_DURATION_MS))
            .easing(active_easing.0),
    );
}

// ============================================================================
// Selection gizmo
// ============================================================================

fn draw_shape_gizmo(
    gizmos: &mut Gizmos<SelectionGizmo>,
    transform: &Transform,
    shape: &MeshShape,
    color: Color,
) {
    match shape {
        MeshShape::Cuboid(size) => {
            gizmos.cube(
                Transform::from_translation(transform.translation)
                    .with_rotation(transform.rotation)
                    .with_scale(*size * GIZMO_SCALE),
                color,
            );
        },
        MeshShape::Sphere(radius) => {
            gizmos.sphere(
                Isometry3d::new(transform.translation, transform.rotation),
                radius * GIZMO_SCALE,
                color,
            );
        },
        MeshShape::Torus {
            minor_radius,
            major_radius,
        } => {
            gizmos.primitive_3d(
                &Torus::new(*minor_radius * GIZMO_SCALE, *major_radius * GIZMO_SCALE),
                Isometry3d::new(transform.translation, transform.rotation),
                color,
            );
        },
    }
}

fn draw_selection_gizmo(
    mut gizmos: Gizmos<SelectionGizmo>,
    query: Query<(&Transform, &MeshShape), With<Selected>>,
) {
    let color = Color::from(DEEP_SKY_BLUE);
    for (transform, shape) in &query {
        draw_shape_gizmo(&mut gizmos, transform, shape, color);
    }
}

fn draw_hover_gizmo(
    mut gizmos: Gizmos<SelectionGizmo>,
    hovered: Res<HoveredEntity>,
    query: Query<(&Transform, &MeshShape), Without<Selected>>,
) {
    let Some(entity) = hovered.0 else {
        return;
    };
    let Ok((transform, shape)) = query.get(entity) else {
        return;
    };
    draw_shape_gizmo(&mut gizmos, transform, shape, Color::from(ORANGE));
}

type GizmoLayerQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static FitVisualization>,
        Option<&'static RenderLayers>,
    ),
    With<OrbitCam>,
>;

/// Cameras with `FitVisualization` lose the selection gizmo layer so the outline
/// doesn't compete with the debug overlay. Cameras without it get the layer back.
fn sync_selection_gizmo_layers(mut commands: Commands, camera_query: GizmoLayerQuery) {
    let with_selection = RenderLayers::from_layers(&[0, SELECTION_GIZMO_LAYER]);
    let without_selection = RenderLayers::layer(0);

    for (entity, has_viz, current_layers) in &camera_query {
        let desired = if has_viz.is_some() {
            &without_selection
        } else {
            &with_selection
        };
        if current_layers != Some(desired) {
            commands.entity(entity).insert(desired.clone());
        }
    }
}

// ============================================================================
// Keyboard actions
// ============================================================================

fn toggle_debug_visualization(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    scene: Res<SceneEntities>,
    second: Option<Res<SecondWindowEntities>>,
    viz_query: Query<(), With<FitVisualization>>,
    windows: Query<&Window>,
) {
    if !keyboard.just_pressed(KeyCode::KeyD) {
        return;
    }

    let cam = focused_camera(&scene, second.as_deref(), &windows);
    if viz_query.get(cam).is_ok() {
        commands.entity(cam).remove::<FitVisualization>();
    } else {
        commands.entity(cam).insert(FitVisualization);
    }
}

fn animate_camera(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    scene: Res<SceneEntities>,
    second: Option<Res<SecondWindowEntities>>,
    easing: Res<ActiveEasing>,
    camera_query: Query<&OrbitCam>,
    windows: Query<&Window>,
) {
    if !keyboard.just_pressed(KeyCode::KeyA) {
        return;
    }

    let cam = focused_camera(&scene, second.as_deref(), &windows);
    let Ok(camera) = camera_query.get(cam) else {
        return;
    };

    let e = easing.0;
    let half_pi = PI / 2.0;
    let yaw = camera.target_yaw;
    let pitch = camera.target_pitch;
    let radius = camera.target_radius;
    let focus = camera.target_focus;

    let camera_moves = [
        CameraMove::ToOrbit {
            focus,
            yaw: yaw + half_pi,
            pitch,
            radius,
            duration: Duration::from_millis(ORBIT_MOVE_DURATION_MS),
            easing: e,
        },
        CameraMove::ToOrbit {
            focus,
            yaw: yaw + half_pi * 2.0,
            pitch,
            radius,
            duration: Duration::from_millis(ORBIT_MOVE_DURATION_MS),
            easing: e,
        },
        CameraMove::ToOrbit {
            focus,
            yaw: yaw + half_pi * 3.0,
            pitch,
            radius,
            duration: Duration::from_millis(ORBIT_MOVE_DURATION_MS),
            easing: e,
        },
        CameraMove::ToOrbit {
            focus,
            yaw: yaw + half_pi * 4.0,
            pitch,
            radius,
            duration: Duration::from_millis(ORBIT_MOVE_DURATION_MS),
            easing: e,
        },
    ];

    commands.trigger(PlayAnimation::new(cam, camera_moves));
}

fn randomize_easing(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut easing: ResMut<ActiveEasing>,
    time: Res<Time>,
    mut log: ResMut<EventLog>,
) {
    if keyboard.just_pressed(KeyCode::KeyR) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let index = (time.elapsed_secs() * 1000.0) as usize % ALL_EASINGS.len();
        easing.0 = ALL_EASINGS[index];
        log.push(format!("Easing: {:#?}", easing.0));
    }
    if keyboard.just_pressed(KeyCode::KeyE) {
        easing.0 = EaseFunction::CubicOut;
        log.push("Easing: reset to CubicOut".into());
    }
}

fn animate_fit_to_scene(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    scene: Res<SceneEntities>,
    second: Option<Res<SecondWindowEntities>>,
    easing: Res<ActiveEasing>,
    windows: Query<&Window>,
) {
    if !keyboard.just_pressed(KeyCode::KeyH) {
        return;
    }

    let cam = focused_camera(&scene, second.as_deref(), &windows);
    commands.trigger(
        AnimateToFit::new(cam, scene.scene_bounds)
            .yaw(CAMERA_START_YAW)
            .pitch(CAMERA_START_PITCH)
            .margin(ZOOM_MARGIN_SCENE)
            .duration(Duration::from_millis(ANIMATE_FIT_DURATION_MS))
            .easing(easing.0),
    );
}

/// Toggles between perspective and orthographic projection, then re-fits the scene.
///
/// The fit is deferred one frame via `pending_fit` because `OrbitCam` needs to
/// process the projection change (syncing radius ↔ orthographic scale) before the
/// fit calculation can produce correct results.
#[allow(clippy::too_many_arguments)]
fn toggle_projection(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    scene: Res<SceneEntities>,
    second: Option<Res<SecondWindowEntities>>,
    active_easing: Res<ActiveEasing>,
    mut camera_query: Query<(&mut Projection, &mut OrbitCam)>,
    mut log: ResMut<EventLog>,
    mut pending_fit: Local<bool>,
) {
    // Deferred fit: projection was changed last frame, `OrbitCam` has now synced.
    if *pending_fit {
        *pending_fit = false;
        for cam in all_cameras(&scene, second.as_deref()) {
            commands.trigger(
                AnimateToFit::new(cam, scene.scene_bounds)
                    .yaw(CAMERA_START_YAW)
                    .pitch(CAMERA_START_PITCH)
                    .margin(ZOOM_MARGIN_SCENE)
                    .duration(Duration::from_millis(ANIMATE_FIT_DURATION_MS))
                    .easing(active_easing.0),
            );
        }
        return;
    }

    if !keyboard.just_pressed(KeyCode::KeyP) {
        return;
    }
    let mut logged = false;
    for cam in all_cameras(&scene, second.as_deref()) {
        let Ok((mut projection, mut camera)) = camera_query.get_mut(cam) else {
            continue;
        };
        match *projection {
            Projection::Perspective(_) => {
                *projection = Projection::from(OrthographicProjection {
                    scaling_mode: ScalingMode::FixedVertical {
                        viewport_height: 1.0,
                    },
                    far: 40.0,
                    ..OrthographicProjection::default_3d()
                });
                if !logged {
                    log.push("Projection: Orthographic".into());
                    logged = true;
                }
            },
            Projection::Orthographic(_) => {
                *projection = Projection::Perspective(PerspectiveProjection::default());
                if !logged {
                    log.push("Projection: Perspective".into());
                    logged = true;
                }
            },
            Projection::Custom(_) => {},
        }
        camera.force_update = true;
    }
    if logged {
        *pending_fit = true;
    }
}

// ============================================================================
// Behavior configuration
// ============================================================================

fn interrupt_behavior_hint_text(behavior: CameraInputInterruptBehavior) -> String {
    match behavior {
        CameraInputInterruptBehavior::Ignore => {
            "CameraInputInterruptBehavior::Ignore - camera input during animation is ignored".into()
        },
        CameraInputInterruptBehavior::Cancel => {
            "CameraInputInterruptBehavior::Cancel - camera input during animation will cancel it".into()
        },
        CameraInputInterruptBehavior::Complete => {
            "CameraInputInterruptBehavior::Complete - camera input during animation will jump to final position"
                .into()
        },
    }
}

fn toggle_interrupt_behavior(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    scene: Res<SceneEntities>,
    second: Option<Res<SecondWindowEntities>>,
    mut behavior_query: Query<&mut CameraInputInterruptBehavior>,
    mut hint_query: Query<&mut Text, With<CameraInputInterruptBehaviorLabel>>,
    mut log: ResMut<EventLog>,
) {
    if !keyboard.just_pressed(KeyCode::KeyI) {
        return;
    }

    // Determine what the new behavior should be based on the primary camera
    let new_behavior =
        behavior_query
            .get(scene.camera)
            .map_or(
                CameraInputInterruptBehavior::Ignore,
                |behavior| match *behavior {
                    CameraInputInterruptBehavior::Ignore => CameraInputInterruptBehavior::Cancel,
                    CameraInputInterruptBehavior::Cancel => CameraInputInterruptBehavior::Complete,
                    CameraInputInterruptBehavior::Complete => CameraInputInterruptBehavior::Ignore,
                },
            );

    for cam in all_cameras(&scene, second.as_deref()) {
        if let Ok(mut behavior) = behavior_query.get_mut(cam) {
            *behavior = new_behavior;
        } else {
            commands.entity(cam).insert(new_behavior);
        }
    }

    for mut text in &mut hint_query {
        **text = interrupt_behavior_hint_text(new_behavior);
    }
    log.push(format!("CameraInputInterruptBehavior: {new_behavior:?}"));
}

fn conflict_policy_hint_text(policy: AnimationConflictPolicy) -> String {
    match policy {
        AnimationConflictPolicy::LastWins => {
            "AnimationConflictPolicy::LastWins - new animation cancels current one".into()
        },
        AnimationConflictPolicy::FirstWins => {
            "AnimationConflictPolicy::FirstWins - new animation is rejected while one is playing"
                .into()
        },
    }
}

fn toggle_animation_conflict_policy(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    scene: Res<SceneEntities>,
    second: Option<Res<SecondWindowEntities>>,
    mut policy_query: Query<&mut AnimationConflictPolicy>,
    mut hint_query: Query<&mut Text, With<AnimationConflictPolicyLabel>>,
    mut log: ResMut<EventLog>,
) {
    if !keyboard.just_pressed(KeyCode::KeyQ) {
        return;
    }

    // Determine what the new policy should be based on the primary camera
    let new_policy =
        policy_query
            .get(scene.camera)
            .map_or(AnimationConflictPolicy::FirstWins, |policy| match *policy {
                AnimationConflictPolicy::LastWins => AnimationConflictPolicy::FirstWins,
                AnimationConflictPolicy::FirstWins => AnimationConflictPolicy::LastWins,
            });

    for cam in all_cameras(&scene, second.as_deref()) {
        if let Ok(mut policy) = policy_query.get_mut(cam) {
            *policy = new_policy;
        } else {
            commands.entity(cam).insert(new_policy);
        }
    }

    for mut text in &mut hint_query {
        **text = conflict_policy_hint_text(new_policy);
    }
    log.push(format!("AnimationConflictPolicy: {new_policy:?}"));
}

// ============================================================================
// Event log
// ============================================================================

/// Enables the event log when the initial `AnimateToFit` animation completes.
#[allow(clippy::type_complexity)]
fn enable_log_on_initial_fit(
    _trigger: On<AnimationEnd>,
    mut commands: Commands,
    marker: Option<Res<EnableLogOnAnimationEnd>>,
    mut log: ResMut<EventLog>,
    mut container_query: Query<
        &mut Visibility,
        (
            With<EventLogNode>,
            Without<EventLogHint>,
            Without<EventLogToggleHint>,
        ),
    >,
    mut hint_query: Query<
        &mut Visibility,
        (
            With<EventLogHint>,
            Without<EventLogNode>,
            Without<EventLogToggleHint>,
        ),
    >,
    mut toggle_hint_query: Query<
        &mut Visibility,
        (
            With<EventLogToggleHint>,
            Without<EventLogNode>,
            Without<EventLogHint>,
        ),
    >,
) {
    if marker.is_none() {
        return;
    }
    commands.remove_resource::<EnableLogOnAnimationEnd>();
    log.enabled = true;
    for mut vis in &mut container_query {
        *vis = Visibility::Inherited;
    }
    for mut vis in &mut hint_query {
        *vis = Visibility::Inherited;
    }
    for mut vis in &mut toggle_hint_query {
        *vis = Visibility::Inherited;
    }
}

#[allow(clippy::type_complexity)]
fn toggle_event_log(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut log: ResMut<EventLog>,
    mut container_query: Query<
        (Entity, &mut Visibility, &mut ScrollPosition),
        (With<EventLogNode>, Without<EventLogHint>),
    >,
    children_query: Query<&Children>,
    mut hint_query: Query<&mut Visibility, (With<EventLogHint>, Without<EventLogNode>)>,
) {
    if !keyboard.just_pressed(KeyCode::KeyL) {
        return;
    }

    log.enabled = !log.enabled;

    if log.enabled {
        for (_, mut vis, _) in &mut container_query {
            *vis = Visibility::Inherited;
        }
        for mut vis in &mut hint_query {
            *vis = Visibility::Inherited;
        }
    } else {
        // Clear log entries and hide
        for (entity, mut vis, mut scroll) in &mut container_query {
            if let Ok(children) = children_query.get(entity) {
                for child in children.iter() {
                    commands.entity(child).despawn();
                }
            }
            scroll.y = 0.0;
            *vis = Visibility::Hidden;
        }
        for mut vis in &mut hint_query {
            *vis = Visibility::Hidden;
        }
        log.pending.clear();
    }
}

impl EventLog {
    fn push(&mut self, text: String) {
        if !self.enabled {
            return;
        }
        self.pending.push(PendingLogEntry {
            text,
            color: EVENT_LOG_COLOR,
        });
    }

    fn push_red(&mut self, text: String) {
        if !self.enabled {
            return;
        }
        self.pending.push(PendingLogEntry {
            text,
            color: EVENT_LOG_COLOR_RED,
        });
    }

    fn separator(&mut self) {
        if !self.enabled {
            return;
        }
        self.pending.push(PendingLogEntry {
            text:  EVENT_LOG_SEPARATOR.into(),
            color: EVENT_LOG_COLOR,
        });
    }
}

fn fmt_vec3(v: Vec3) -> String { format!("({:.1}, {:.1}, {:.1})", v.x, v.y, v.z) }

fn log_animation_begin(event: On<AnimationBegin>, mut log: ResMut<EventLog>) {
    log.push(format!("AnimationBegin\n  source={:?}", event.source));
}

fn log_animation_end(event: On<AnimationEnd>, mut log: ResMut<EventLog>) {
    log.push(format!("AnimationEnd\n  source={:?}", event.source));
    if event.source != AnimationSource::ZoomToFit {
        log.separator();
    }
}

fn log_camera_move_start(event: On<CameraMoveBegin>, mut log: ResMut<EventLog>) {
    log.push(format!(
        "CameraMoveBegin\n  translation={}\n  focus={}\n  duration={:.0}ms\n  easing={:?}",
        fmt_vec3(event.camera_move.translation()),
        fmt_vec3(event.camera_move.focus()),
        event.camera_move.duration_ms(),
        event.camera_move.easing(),
    ));
}

fn log_camera_move_end(_event: On<CameraMoveEnd>, mut log: ResMut<EventLog>) {
    log.push("CameraMoveEnd".into());
}

fn log_zoom_begin(event: On<ZoomBegin>, mut log: ResMut<EventLog>) {
    log.push(format!(
        "ZoomBegin\n  margin={:.2}\n  duration={:.0}ms\n  easing={:?}",
        event.margin,
        event.duration.as_secs_f32() * 1000.0,
        event.easing,
    ));
}

fn log_zoom_end(_event: On<ZoomEnd>, mut log: ResMut<EventLog>) {
    log.push("ZoomEnd".into());
    log.separator();
}

fn log_animation_cancelled(event: On<AnimationCancelled>, mut log: ResMut<EventLog>) {
    log.push_red(format!(
        "AnimationCancelled\n  source={:?}\n  move_translation={}\n  move_focus={}",
        event.source,
        fmt_vec3(event.camera_move.translation()),
        fmt_vec3(event.camera_move.focus()),
    ));
}

fn log_zoom_cancelled(_event: On<ZoomCancelled>, mut log: ResMut<EventLog>) {
    log.push_red("ZoomCancelled".into());
}

fn log_animation_rejected(event: On<AnimationRejected>, mut log: ResMut<EventLog>) {
    log.push_red(format!("AnimationRejected\n  source={:?}", event.source));
}

/// Spawns pending log entries as child `Text` nodes inside the scroll container
/// and auto-scrolls to the bottom.
fn update_event_log_text(
    mut commands: Commands,
    mut log: ResMut<EventLog>,
    container_query: Query<(Entity, &Node, &ComputedNode), With<EventLogNode>>,
    mut scroll_query: Query<&mut ScrollPosition, With<EventLogNode>>,
) {
    if log.pending.is_empty() {
        return;
    }

    let Ok((container, _node, computed)) = container_query.single() else {
        return;
    };

    for entry in log.pending.drain(..) {
        commands.entity(container).with_child((
            Text::new(entry.text),
            TextFont {
                font_size: EVENT_LOG_FONT_SIZE,
                ..default()
            },
            TextColor(entry.color),
        ));
    }

    // Auto-scroll to bottom
    if let Ok(mut scroll) = scroll_query.single_mut() {
        let content_height = computed.content_size().y;
        let container_height = computed.size().y;
        let max_scroll =
            (content_height - container_height).max(0.0) * computed.inverse_scale_factor();
        scroll.y = EVENT_LOG_SCROLL_SPEED.mul_add(4.0, max_scroll);
    }
}

/// Scrolls the event log with Up/Down arrow keys, clears with 'C'.
fn scroll_event_log(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut scroll_query: Query<(Entity, &mut ScrollPosition, &ComputedNode), With<EventLogNode>>,
    children_query: Query<&Children>,
) {
    let Ok((container, mut scroll, computed)) = scroll_query.single_mut() else {
        return;
    };

    if keyboard.just_pressed(KeyCode::KeyC) {
        if let Ok(children) = children_query.get(container) {
            for child in children.iter() {
                commands.entity(child).despawn();
            }
        }
        scroll.y = 0.0;
        return;
    }

    let dy = if keyboard.pressed(KeyCode::ArrowDown) {
        EVENT_LOG_SCROLL_SPEED
    } else if keyboard.pressed(KeyCode::ArrowUp) {
        -EVENT_LOG_SCROLL_SPEED
    } else {
        return;
    };

    let max_scroll =
        (computed.content_size().y - computed.size().y).max(0.0) * computed.inverse_scale_factor();
    scroll.y = (scroll.y + dy).clamp(0.0, max_scroll);
}
