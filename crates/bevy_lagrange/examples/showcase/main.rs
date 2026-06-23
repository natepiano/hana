//! Demonstrates clicking on meshes to zoom-to-fit.
//!
//! - Click a mesh to select it and zoom the camera to frame it
//! - Click the ground to deselect and zoom out to the full scene
//! - Drag a mesh to rotate it
//! - Selected meshes show a gizmo outline
//! - Press 'Y' to toggle the fit overlay of zoom-to-fit bounds

mod animation_controls;
mod constants;
mod event_log;
mod input;
mod pointer;
mod policy_panel;
mod scene;
mod selection_gizmo;
mod ui;

use std::time::Duration;

use bevy::camera::ScalingMode;
use bevy::color::palettes::css::DEEP_SKY_BLUE;
use bevy::color::palettes::css::ORANGE;
use bevy::math::curve::easing::EaseFunction;
use bevy::prelude::*;
use bevy::time::Virtual;
use bevy::window::PrimaryWindow;
use bevy_kana::ToUsize;
use bevy_lagrange::AnimateToFit;
use bevy_lagrange::AnimationBegin;
use bevy_lagrange::AnimationConflictPolicy;
use bevy_lagrange::AnimationEnd;
use bevy_lagrange::AnimationReason;
use bevy_lagrange::AnimationRejected;
use bevy_lagrange::AnimationSource;
use bevy_lagrange::CameraInputDisabled;
use bevy_lagrange::CameraInputInterruptBehavior;
use bevy_lagrange::CameraMove;
use bevy_lagrange::CameraMoveBegin;
use bevy_lagrange::CameraMoveEnd;
use bevy_lagrange::FitOverlay;
use bevy_lagrange::LookAt;
use bevy_lagrange::LookAtAndZoomToFit;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::PlayAnimation;
use bevy_lagrange::ZoomBegin;
use bevy_lagrange::ZoomEnd;
use bevy_lagrange::ZoomReason;
use bevy_lagrange::ZoomToFit;
use constants::*;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::ControlActivation;
use fairy_dust::FairyDustOrbitCam;
use fairy_dust::TitleBar;
use fairy_dust::TitleBarOrientation;

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

// ============================================================================
// App entry point
// ============================================================================

fn main() {
    let mut app = fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_camera_home()
        .yaw(CAMERA_START_YAW)
        .pitch(CAMERA_START_PITCH)
        .margin(HOME_MARGIN)
        .with_title_bar(showcase_title_bar())
        .wire_chip_to_activation::<event_log::EventLog>(EVENT_LOG_CONTROL)
        .wire_chip_to_activation::<animation_controls::FitOverlayActive>(OVERLAY_CONTROL)
        .wire_chip_to_state::<Time<Virtual>, _>(PAUSE_CONTROL, |time| {
            chip_activation(time.is_paused())
        })
        .wire_chip_to_state::<animation_controls::ProjectionMode, _>(PERSPECTIVE_CONTROL, |mode| {
            chip_activation(matches!(
                mode,
                animation_controls::ProjectionMode::Perspective
            ))
        })
        .wire_chip_to_state::<animation_controls::ProjectionMode, _>(ORTHOGRAPHIC_CONTROL, |mode| {
            chip_activation(matches!(
                mode,
                animation_controls::ProjectionMode::Orthographic
            ))
        })
        .wire_chip_to_events_filtered::<AnimationBegin, AnimationEnd, _, _>(
            ANIMATE_CONTROL,
            |begin| begin.source == AnimationSource::PlayAnimation,
            |end| end.source == AnimationSource::PlayAnimation,
        )
        .wire_chip_to_events_filtered::<AnimationBegin, AnimationEnd, _, _>(
            LOOK_AT_CONTROL,
            |begin| begin.source == AnimationSource::LookAt,
            |end| end.source == AnimationSource::LookAt,
        )
        .wire_chip_to_events_filtered::<AnimationBegin, AnimationEnd, _, _>(
            LOOK_AND_FIT_CONTROL,
            |begin| begin.source == AnimationSource::LookAtAndZoomToFit,
            |end| end.source == AnimationSource::LookAtAndZoomToFit,
        )
        .wire_chip_to_state::<animation_controls::EasingFlash, _>(EASING_CONTROL, |flash| {
            chip_activation(flash.random_active())
        })
        .wire_chip_to_state::<animation_controls::EasingFlash, _>(EASING_RESET_CONTROL, |flash| {
            chip_activation(flash.reset_active())
        })
        .with_camera_control_panel();

    app.app_mut()
        .init_gizmo_group::<selection_gizmo::SelectionGizmo>();
    app.app_mut().init_state::<AppState>();

    app.add_plugins(input::ShowcaseInputPlugin)
        .init_resource::<ActiveEasing>()
        .init_resource::<event_log::EventLog>()
        .init_resource::<policy_panel::PolicyDisplay>()
        .init_resource::<policy_panel::KeyFlash>()
        .init_resource::<pointer::HoveredEntity>()
        .init_resource::<animation_controls::ProjectionRefit>()
        .init_resource::<animation_controls::ProjectionMode>()
        .init_resource::<animation_controls::FitOverlayActive>()
        .init_resource::<animation_controls::EasingFlash>()
        .add_systems(
            Startup,
            (
                set_primary_window_title,
                setup,
                selection_gizmo::init_selection_gizmo,
            ),
        )
        .add_systems(
            Update,
            initial_fit_to_scene.run_if(in_state(AppState::Loading)),
        )
        .add_systems(
            Update,
            (
                selection_gizmo::draw_selection_gizmo,
                selection_gizmo::draw_hover_gizmo,
                event_log::rebuild_log_panel,
                policy_panel::rebuild_policy_panel,
                policy_panel::tick_key_flash,
                animation_controls::apply_projection_refit,
                animation_controls::tick_easing_flash,
            ),
        )
        .add_observer(event_log::enable_log_on_initial_fit)
        .add_observer(event_log::log_animation_begin)
        .add_observer(event_log::log_animation_end)
        .add_observer(event_log::log_camera_move_start)
        .add_observer(event_log::log_camera_move_end)
        .add_observer(event_log::log_zoom_begin)
        .add_observer(event_log::log_zoom_end)
        .add_observer(event_log::log_animation_rejected)
        .run();
}

/// Maps a boolean activation condition onto a title-bar chip highlight.
const fn chip_activation(active: bool) -> ControlActivation {
    if active {
        ControlActivation::Active
    } else {
        ControlActivation::Inactive
    }
}

fn showcase_title_bar() -> TitleBar {
    TitleBar::new()
        .with_title(SHOWCASE_TITLE)
        .with_anchor(Anchor::TopLeft)
        .with_orientation(TitleBarOrientation::Vertical)
        .control(PAUSE_CONTROL)
        .control(PERSPECTIVE_CONTROL)
        .control(ORTHOGRAPHIC_CONTROL)
        .control(OVERLAY_CONTROL)
        .control(ANIMATE_CONTROL)
        .control(LOOK_AT_CONTROL)
        .control(LOOK_AND_FIT_CONTROL)
        .control(EASING_CONTROL)
        .control(EASING_RESET_CONTROL)
        .control(EVENT_LOG_CONTROL)
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

    // Camera using the BlenderLike input preset (middle-mouse orbit, shift+MMB pan,
    // numpad-driven view changes).
    let camera = commands
        .spawn((
            OrbitCam {
                yaw: Some(CAMERA_START_YAW),
                pitch: Some(CAMERA_START_PITCH),
                ..default()
            },
            OrbitCamInputMode::with_preset(OrbitCamPreset::blender_like()),
            FairyDustOrbitCam,
        ))
        .id();

    ui::spawn_ui(&mut commands, &mut materials);

    commands.insert_resource(SceneEntities {
        camera,
        scene_bounds: ground,
    });
}

fn set_primary_window_title(mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    window.title = PRIMARY_WINDOW_TITLE.into();
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
    commands.insert_resource(event_log::EnableLogOnAnimationEnd);
    commands.trigger(
        AnimateToFit::new(scene.camera, scene.scene_bounds)
            .yaw(CAMERA_START_YAW)
            .pitch(CAMERA_START_PITCH)
            .margin(ZOOM_MARGIN_SCENE)
            .easing(EaseFunction::QuadraticInOut),
    );
    next_state.set(AppState::Running);
}
