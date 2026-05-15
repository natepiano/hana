//! Demonstrates multiple viewports in a single window and multiple windows,
//! each with an independent `OrbitCam`.
//!
//! The primary window has a full-size view and a minimap overlay in the
//! top-right corner. A second OS window shows a separate camera angle.

use bevy::camera::RenderTarget;
use bevy::camera::Viewport;
use bevy::prelude::*;
use bevy::window::ClosingWindow;
use bevy::window::WindowRef;
use bevy::window::WindowResized;
use std::collections::HashMap;

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
const CUBE_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
const CUBE_SIZE: f32 = 1.0;
const CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, 0.8, 0.0);
const FACE_LABEL_SIZE: f32 = 0.15;
const FACE_LABEL_COLOR: Color = Color::srgb(0.9, 0.3, 0.1);

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

// viewport
const MINIMAP_VIEWPORT_DIVISOR: u32 = 5;

// window
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
        .face_text(Face::Front, "FRONT", FACE_LABEL_SIZE, FACE_LABEL_COLOR)
        .face_text(Face::Back, "BACK", FACE_LABEL_SIZE, FACE_LABEL_COLOR)
        .face_text(Face::Top, "TOP", FACE_LABEL_SIZE, FACE_LABEL_COLOR)
        .face_text(Face::Bottom, "BOTTOM", FACE_LABEL_SIZE, FACE_LABEL_COLOR)
        .face_text(Face::Left, "LEFT", FACE_LABEL_SIZE, FACE_LABEL_COLOR)
        .face_text(Face::Right, "RIGHT", FACE_LABEL_SIZE, FACE_LABEL_COLOR)
        .with_title_bar(
            TitleBar::new()
                .with_anchor(Anchor::TopLeft)
                .control("H Home (active cam)"),
        )
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                cleanup_cameras_on_window_close,
                set_camera_viewports,
                home_on_keypress,
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
            Transform::from_translation(PRIMARY_CAMERA_TRANSLATION),
            orbit_cam_default(),
            OrbitCamPreset::BlenderLike,
        ))
        .id();

    // --- Primary window: minimap viewport overlay ---
    let minimap = commands
        .spawn((
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

/// Homes the camera the cursor is currently over to its stored pose.
fn home_on_keypress(
    keys: Res<ButtonInput<KeyCode>>,
    route: Res<ResolvedOrbitCamInputRoute>,
    homes: Res<CameraHomes>,
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
    orbit.target_focus = pose.focus;
    orbit.target_yaw = pose.yaw;
    orbit.target_pitch = pose.pitch;
    orbit.target_radius = pose.radius;
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
