//! Demonstrates clicking on meshes to zoom-to-fit.
//!
//! - Click a mesh to select it and zoom the camera to frame it
//! - Click the ground to deselect and zoom out to the full scene
//! - Drag a mesh to rotate it
//! - Selected meshes show a gizmo outline
//! - Press 'D' to toggle debug overlay of zoom-to-fit bounds

mod animation_controls;
mod constants;
mod event_log;
mod pointer;
mod scene;
mod second_window;
mod selection_gizmo;
mod ui;

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
use bevy_kana::ToUsize;
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
use bevy_lagrange::FitOverlay;
use bevy_lagrange::ForceUpdate;
use bevy_lagrange::InputControl;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::LookAt;
use bevy_lagrange::LookAtAndZoomToFit;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::PlayAnimation;
use bevy_lagrange::TrackpadInput;
use bevy_lagrange::ZoomBegin;
use bevy_lagrange::ZoomCancelled;
use bevy_lagrange::ZoomEnd;
use bevy_lagrange::ZoomToFit;
use bevy_window_manager::ManagedWindow;
use bevy_window_manager::WindowManagerPlugin;
use constants::*;

// ============================================================================
// Types
// ============================================================================

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
enum AppState {
    #[default]
    Loading,
    Running,
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

/// Marker resource: when present, the next `AnimationEnd` enables the event log.
#[derive(Resource)]
struct EnableLogOnAnimationEnd;

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
        .init_gizmo_group::<selection_gizmo::SelectionGizmo>()
        .init_state::<AppState>()
        .init_resource::<ActiveEasing>()
        .init_resource::<event_log::EventLog>()
        .init_resource::<pointer::HoveredEntity>()
        .add_systems(Startup, (setup, selection_gizmo::init_selection_gizmo))
        .add_systems(
            Update,
            initial_fit_to_scene.run_if(in_state(AppState::Loading)),
        )
        .add_systems(
            Update,
            (
                second_window::log_window_focus,
                second_window::despawn_window_labels,
                ui::toggle_pause,
                event_log::toggle_event_log,
                selection_gizmo::draw_selection_gizmo,
                selection_gizmo::draw_hover_gizmo,
                selection_gizmo::sync_selection_gizmo_layers,
                event_log::update_event_log_text,
                event_log::scroll_event_log,
                (
                    second_window::toggle_second_window,
                    animation_controls::toggle_debug_overlay,
                    animation_controls::toggle_projection,
                    animation_controls::randomize_easing,
                    animation_controls::animate_camera,
                    animation_controls::animate_fit_to_scene,
                    animation_controls::toggle_interrupt_behavior,
                    animation_controls::toggle_animation_conflict_policy,
                    pointer::look_at_hovered,
                    pointer::look_at_and_zoom_to_fit_hovered,
                )
                    .run_if(not_paused),
            ),
        )
        .add_observer(event_log::enable_log_on_initial_fit)
        .add_observer(event_log::log_animation_begin)
        .add_observer(event_log::log_animation_end)
        .add_observer(event_log::log_animation_cancelled)
        .add_observer(event_log::log_camera_move_start)
        .add_observer(event_log::log_camera_move_end)
        .add_observer(event_log::log_zoom_begin)
        .add_observer(event_log::log_zoom_end)
        .add_observer(event_log::log_zoom_cancelled)
        .add_observer(event_log::log_animation_rejected)
        .add_observer(second_window::on_second_window_removed)
        .run();
}

// ============================================================================
// Scene setup
// ============================================================================

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let ground = scene::spawn_scene_objects(&mut commands, &mut meshes, &mut materials);

    // Camera (middle-click orbit, shift+middle pan, trackpad support)
    let camera = commands
        .spawn(OrbitCam {
            button_orbit: MouseButton::Middle,
            button_pan: MouseButton::Middle,
            modifier_pan: Some(KeyCode::ShiftLeft),
            input_control: Some(InputControl {
                trackpad: Some(TrackpadInput::blender_default()),
                ..default()
            }),
            yaw: Some(CAMERA_START_YAW),
            pitch: Some(CAMERA_START_PITCH),
            ..default()
        })
        .id();

    ui::spawn_ui(&mut commands, camera);

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

fn not_paused(time: Res<Time<Virtual>>) -> bool { !time.is_paused() }
