//! Demonstrates multiple viewports in a single window and multiple windows,
//! each with an independent `OrbitCam`.
//!
//! The primary window has a full-size view and a minimap overlay in the
//! top-right corner. A second OS window shows a separate camera angle.

use std::collections::HashMap;

use bevy::camera::RenderTarget;
use bevy::camera::Viewport;
use bevy::prelude::*;
use bevy::window::ClosingWindow;
use bevy::window::WindowRef;
use bevy::window::WindowResized;
use bevy_kana::event;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::ResolvedOrbitCamInputRoute;
use bevy_window_manager::ManagedWindow;
use fairy_dust::Anchor;
use fairy_dust::Face;
use fairy_dust::TitleBar;

// camera
const MINIMAP_CAMERA_ORDER: isize = 1;
const MINIMAP_CAMERA_TRANSLATION: Vec3 = Vec3::new(1.0, 1.5, 4.0);
const PRIMARY_CAMERA_TRANSLATION: Vec3 = Vec3::new(0.0, 0.5, 5.0);
const SECOND_WINDOW_CAMERA_TRANSLATION: Vec3 = Vec3::new(5.0, 1.5, 7.0);

// cube
const BACK_FACE_LABEL: &str = "BACK";
const BOTTOM_FACE_LABEL: &str = "BOTTOM";
const CUBE_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
const CUBE_SIZE: f32 = 1.0;
const CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, 0.8, 0.0);
const FACE_LABEL_COLOR: Color = Color::srgb(0.9, 0.3, 0.1);
const FACE_LABEL_SIZE: f32 = 0.15;
const FRONT_FACE_LABEL: &str = "FRONT";
const LEFT_FACE_LABEL: &str = "LEFT";
const RIGHT_FACE_LABEL: &str = "RIGHT";
const TOP_FACE_LABEL: &str = "TOP";

// home pose (per camera)
const PRIMARY_HOME_YAW: f32 = 0.0;
const PRIMARY_HOME_PITCH: f32 = 0.46;
const PRIMARY_HOME_RADIUS: f32 = 5.0;
const MINIMAP_HOME_YAW: f32 = 0.0;
const MINIMAP_HOME_PITCH: f32 = 1.4;
const MINIMAP_HOME_RADIUS: f32 = 6.0;
const SECOND_HOME_YAW: f32 = 0.8;
const SECOND_HOME_PITCH: f32 = 0.4;
const SECOND_HOME_RADIUS: f32 = 7.0;

// home animation
const HOME_CONTROL: &str = "H Home (active cam)";
const HOME_FOCUS_EPSILON: f32 = 0.01;
const HOME_ORBIT_EPSILON: f32 = 0.01;
const HOME_SMOOTHNESS: f32 = 0.35;

// viewport
const MINIMAP_VIEWPORT_DIVISOR: u32 = 5;

// window
const MINIMAP_CAMERA_NAME: &str = "Minimap";
const PRIMARY_CAMERA_NAME: &str = "Main";
const SECOND_WINDOW_CAMERA_NAME: &str = "Second window";
const SECOND_WINDOW_NAME: &str = "second_window";
const SECOND_WINDOW_TITLE: &str = "Second window";

fn main() {
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .with_cube()
        .size(CUBE_SIZE)
        .color(CUBE_COLOR)
        .transform(Transform::from_translation(CUBE_TRANSLATION))
        .face_text(
            Face::Front,
            FRONT_FACE_LABEL,
            FACE_LABEL_SIZE,
            FACE_LABEL_COLOR,
        )
        .face_text(
            Face::Back,
            BACK_FACE_LABEL,
            FACE_LABEL_SIZE,
            FACE_LABEL_COLOR,
        )
        .face_text(Face::Top, TOP_FACE_LABEL, FACE_LABEL_SIZE, FACE_LABEL_COLOR)
        .face_text(
            Face::Bottom,
            BOTTOM_FACE_LABEL,
            FACE_LABEL_SIZE,
            FACE_LABEL_COLOR,
        )
        .face_text(
            Face::Left,
            LEFT_FACE_LABEL,
            FACE_LABEL_SIZE,
            FACE_LABEL_COLOR,
        )
        .face_text(
            Face::Right,
            RIGHT_FACE_LABEL,
            FACE_LABEL_SIZE,
            FACE_LABEL_COLOR,
        )
        .with_title_bar(
            TitleBar::new()
                .with_anchor(Anchor::TopLeft)
                .control(HOME_CONTROL),
        )
        .wire_chip_to_events::<HomeAnimationBegin, HomeAnimationEnd>(HOME_CONTROL)
        .with_camera_control_panel()
        .init_resource::<HomeReset>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                cleanup_cameras_on_window_close,
                set_camera_viewports,
                (home_on_keypress, update_home_reset).chain(),
            ),
        )
        .run();
}

fn orbit_cam_default() -> OrbitCam { OrbitCam::default() }

#[derive(Clone, Copy)]
struct HomePose {
    focus:  Vec3,
    yaw:    f32,
    pitch:  f32,
    radius: f32,
}

#[derive(Resource, Default)]
struct CameraHomes(HashMap<Entity, HomePose>);

fn setup(mut commands: Commands) {
    // --- Primary window: main camera ---
    let primary = commands
        .spawn((
            Name::new(PRIMARY_CAMERA_NAME),
            Transform::from_translation(PRIMARY_CAMERA_TRANSLATION),
            orbit_cam_default(),
            OrbitCamPreset::BlenderLike,
        ))
        .id();

    // --- Primary window: minimap viewport overlay ---
    let minimap = commands
        .spawn((
            Name::new(MINIMAP_CAMERA_NAME),
            Transform::from_translation(MINIMAP_CAMERA_TRANSLATION),
            Camera {
                order: MINIMAP_CAMERA_ORDER,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            orbit_cam_default(),
            OrbitCamPreset::BlenderLike,
            MinimapCamera,
        ))
        .id();

    // --- Second OS window ---
    let second_window = commands
        .spawn((
            Window {
                title: SECOND_WINDOW_TITLE.to_owned(),
                ..default()
            },
            ManagedWindow {
                name: SECOND_WINDOW_NAME.into(),
            },
        ))
        .id();

    let second = commands
        .spawn((
            Name::new(SECOND_WINDOW_CAMERA_NAME),
            Transform::from_translation(SECOND_WINDOW_CAMERA_TRANSLATION),
            Camera::default(),
            RenderTarget::Window(WindowRef::Entity(second_window)),
            orbit_cam_default(),
            OrbitCamPreset::BlenderLike,
        ))
        .id();

    commands.insert_resource(CameraHomes(HashMap::from([
        (
            primary,
            HomePose {
                focus:  CUBE_TRANSLATION,
                yaw:    PRIMARY_HOME_YAW,
                pitch:  PRIMARY_HOME_PITCH,
                radius: PRIMARY_HOME_RADIUS,
            },
        ),
        (
            minimap,
            HomePose {
                focus:  CUBE_TRANSLATION,
                yaw:    MINIMAP_HOME_YAW,
                pitch:  MINIMAP_HOME_PITCH,
                radius: MINIMAP_HOME_RADIUS,
            },
        ),
        (
            second,
            HomePose {
                focus:  CUBE_TRANSLATION,
                yaw:    SECOND_HOME_YAW,
                pitch:  SECOND_HOME_PITCH,
                radius: SECOND_HOME_RADIUS,
            },
        ),
    ])));
}

/// Homes the camera the cursor is currently over to its stored pose. The
/// smoothness snapshot + `HomeAnimationBegin` trigger feed the chip-wiring
/// machinery at the bottom of the file — the multi-cam routing decision is
/// the only interesting line here.
fn home_on_keypress(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    route: Res<ResolvedOrbitCamInputRoute>,
    homes: Res<CameraHomes>,
    mut reset: ResMut<HomeReset>,
    mut cams: Query<&mut OrbitCam>,
) {
    if !keys.just_pressed(KeyCode::KeyH) {
        return;
    }
    let Some(cam) = route.routed_camera() else {
        return;
    };
    let Some(&pose) = homes.0.get(&cam) else {
        return;
    };
    let Ok(mut orbit) = cams.get_mut(cam) else {
        return;
    };
    let was_empty = reset.animating.is_empty();
    reset
        .animating
        .entry(cam)
        .or_insert_with(|| CameraSmoothness::from_camera(&orbit));
    orbit.orbit_smoothness = HOME_SMOOTHNESS;
    orbit.pan_smoothness = HOME_SMOOTHNESS;
    orbit.zoom_smoothness = HOME_SMOOTHNESS;
    orbit.target_focus = pose.focus;
    orbit.target_yaw = pose.yaw;
    orbit.target_pitch = pose.pitch;
    orbit.target_radius = pose.radius;
    if was_empty {
        commands.trigger(HomeAnimationBegin);
    }
}

#[derive(Component)]
struct MinimapCamera;

/// Despawns cameras whose render-target window is marked `ClosingWindow`.
/// Prevents `camera_system` from panicking on a stale `RenderTarget`.
fn cleanup_cameras_on_window_close(
    mut commands: Commands,
    closing: Query<Entity, With<ClosingWindow>>,
    cameras: Query<(Entity, &RenderTarget)>,
) {
    for (camera_entity, target) in &cameras {
        if let RenderTarget::Window(WindowRef::Entity(window)) = target
            && closing.get(*window).is_ok()
        {
            commands.entity(camera_entity).despawn();
        }
    }
}

fn set_camera_viewports(
    windows: Query<&Window>,
    mut resize_events: MessageReader<WindowResized>,
    mut right_camera: Single<&mut Camera, With<MinimapCamera>>,
) {
    for resize_event in resize_events.read() {
        let Ok(window) = windows.get(resize_event.window) else {
            continue;
        };
        let size = window.resolution.physical_width() / MINIMAP_VIEWPORT_DIVISOR;
        right_camera.viewport = Some(Viewport {
            physical_position: UVec2::new(window.resolution.physical_width() - size, 0),
            physical_size: UVec2::new(size, size),
            ..default()
        });
    }
}

// ---------------------------------------------------------------------------
// Chip-wiring machinery.
//
// Everything below exists so the `H Home (active cam)` chip in the title bar
// lights up while a home animation is in flight and goes dark when it
// finishes. The pattern mirrors `programmatic_control.rs`, adapted for
// multiple cameras — at most one chip is ever active, even if several
// cameras are animating, because the title bar has only one chip total.
// ---------------------------------------------------------------------------

event!(
    /// Fires when the first camera in an idle batch starts animating to home.
    HomeAnimationBegin
);
event!(
    /// Fires when the last camera in the active batch reaches home or is
    /// overridden by the user.
    HomeAnimationEnd
);

#[derive(Clone, Copy)]
struct CameraSmoothness {
    orbit: f32,
    pan:   f32,
    zoom:  f32,
}

impl CameraSmoothness {
    const fn from_camera(camera: &OrbitCam) -> Self {
        Self {
            orbit: camera.orbit_smoothness,
            pan:   camera.pan_smoothness,
            zoom:  camera.zoom_smoothness,
        }
    }

    const fn apply(self, camera: &mut OrbitCam) {
        camera.orbit_smoothness = self.orbit;
        camera.pan_smoothness = self.pan;
        camera.zoom_smoothness = self.zoom;
    }
}

#[derive(Resource, Default)]
struct HomeReset {
    animating: HashMap<Entity, CameraSmoothness>,
}

/// Per frame: for each camera animating home, finish it if the camera has
/// arrived or the user overrode the target. Fires `HomeAnimationEnd` when
/// the map empties.
fn update_home_reset(
    mut commands: Commands,
    mut reset: ResMut<HomeReset>,
    homes: Res<CameraHomes>,
    mut cams: Query<&mut OrbitCam>,
) {
    if reset.animating.is_empty() {
        return;
    }
    let finished: Vec<Entity> = reset
        .animating
        .keys()
        .copied()
        .filter(|cam| {
            let Some(pose) = homes.0.get(cam) else {
                return true;
            };
            let Ok(orbit) = cams.get(*cam) else {
                return true;
            };
            !camera_targets_home(orbit, pose) || camera_at_home(orbit, pose)
        })
        .collect();
    for cam in finished {
        if let Some(smoothness) = reset.animating.remove(&cam)
            && let Ok(mut orbit) = cams.get_mut(cam)
        {
            smoothness.apply(&mut orbit);
        }
    }
    if reset.animating.is_empty() {
        commands.trigger(HomeAnimationEnd);
    }
}

fn camera_targets_home(camera: &OrbitCam, pose: &HomePose) -> bool {
    camera.target_focus.distance(pose.focus) <= HOME_FOCUS_EPSILON
        && (camera.target_yaw - pose.yaw).abs() <= HOME_ORBIT_EPSILON
        && (camera.target_pitch - pose.pitch).abs() <= HOME_ORBIT_EPSILON
        && (camera.target_radius - pose.radius).abs() <= HOME_FOCUS_EPSILON
}

fn camera_at_home(camera: &OrbitCam, pose: &HomePose) -> bool {
    let (Some(yaw), Some(pitch), Some(radius)) = (camera.yaw, camera.pitch, camera.radius) else {
        return false;
    };
    camera.focus.distance(pose.focus) <= HOME_FOCUS_EPSILON
        && (yaw - pose.yaw).abs() <= HOME_ORBIT_EPSILON
        && (pitch - pose.pitch).abs() <= HOME_ORBIT_EPSILON
        && (radius - pose.radius).abs() <= HOME_FOCUS_EPSILON
}
