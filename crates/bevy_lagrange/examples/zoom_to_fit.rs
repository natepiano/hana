//! Demonstrates `ZoomToFit`, `LookAt`, and `LookAtAndZoomToFit`.
//!
//! Controls:
//!   Z - `ZoomToFit` the cube (frames without changing look direction)
//!   L - `LookAt` the cube (rotates camera in place)
//!   K - `LookAtAndZoomToFit` the cube (rotates + frames)
//!   H - Return to the camera home pose
//!
//! Compare K vs Z: both frame the target, but `LookAtAndZoomToFit` also
//! changes the orbit focus to the target, while `ZoomToFit` keeps the
//! current focus and only adjusts radius.

use std::time::Duration;

use bevy::prelude::*;
use bevy_lagrange::AnimationBegin;
use bevy_lagrange::AnimationEnd;
use bevy_lagrange::AnimationSource;
use bevy_lagrange::LookAt;
use bevy_lagrange::LookAtAndZoomToFit;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::ZoomBegin;
use bevy_lagrange::ZoomEnd;
use bevy_lagrange::ZoomToFit;
use fairy_dust::Anchor;
use fairy_dust::Face;
use fairy_dust::TitleBar;
use fairy_dust::TitleBarControlState;
use fairy_dust::cube_face_text;

const FIT_DURATION: Duration = Duration::from_millis(800);
const FIT_MARGIN: f32 = 0.15;
const LOOK_AT_DURATION: Duration = Duration::from_millis(600);

const HOME_PITCH: f32 = 0.46;

const CUBE_SIZE: f32 = 1.0;
const CUBE_Y: f32 = CUBE_SIZE / 2.0 + 0.05;
const CUBE_X_OFFSET: f32 = 8.0 / 6.0;

const HOME_FRAME_SIZE: f32 = CUBE_SIZE * 3.5;

const REFERENCE_CUBE_COLOR: Color = Color::srgb(0.5, 0.5, 0.5);
const REFERENCE_CUBE_TRANSLATION: Vec3 = Vec3::new(-CUBE_X_OFFSET, CUBE_Y, 0.0);

const TARGET_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
const TARGET_TRANSLATION: Vec3 = Vec3::new(CUBE_X_OFFSET, CUBE_Y, 0.0);

const ZOOM_CONTROL: &str = "Z ZoomToFit";
const LOOK_CONTROL: &str = "L LookAt";
const LOOK_AND_ZOOM_CONTROL: &str = "K LookAtAndZoomToFit";

const REFERENCE_LABEL: &str = "Home";
const TARGET_LABEL: &str = "Look At Me";
const LABEL_SIZE: f32 = 0.15;
const LABEL_COLOR: Color = Color::srgb(0.05, 0.05, 0.1);

#[derive(Component)]
struct Target;

#[derive(Resource)]
struct TargetEntity(Entity);

fn main() {
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .with_cube()
        .size(CUBE_SIZE)
        .color(REFERENCE_CUBE_COLOR)
        .transform(Transform::from_translation(REFERENCE_CUBE_TRANSLATION))
        .face_text(Face::Front, REFERENCE_LABEL, LABEL_SIZE, LABEL_COLOR)
        .with_orbit_cam_bundle(|_| {}, OrbitCamPreset::BlenderLike)
        .with_camera_home(
            Transform::from_translation(REFERENCE_CUBE_TRANSLATION)
                .with_scale(Vec3::splat(HOME_FRAME_SIZE)),
        )
        .pitch(HOME_PITCH)
        .with_title_bar(
            TitleBar::new("Controls")
                .with_anchor(Anchor::TopLeft)
                .control(ZOOM_CONTROL)
                .control(LOOK_CONTROL)
                .control(LOOK_AND_ZOOM_CONTROL),
        )
        .with_camera_control_panel()
        .add_systems(Startup, spawn_target)
        .add_systems(Update, keyboard_input)
        .add_observer(on_zoom_begin)
        .add_observer(on_zoom_end)
        .add_observer(on_animation_begin)
        .add_observer(on_animation_end)
        .run();
}

fn spawn_target(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let entity = commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::from_size(Vec3::splat(CUBE_SIZE)))),
            MeshMaterial3d(materials.add(StandardMaterial::from(TARGET_COLOR))),
            Transform::from_translation(TARGET_TRANSLATION),
            Target,
        ))
        .with_children(|parent| {
            parent.spawn(cube_face_text(
                Face::Front,
                TARGET_LABEL,
                CUBE_SIZE,
                LABEL_SIZE,
                LABEL_COLOR,
            ));
        })
        .id();
    commands.insert_resource(TargetEntity(entity));
}

fn keyboard_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    camera_query: Query<Entity, With<OrbitCam>>,
    target: Res<TargetEntity>,
) {
    let Ok(camera) = camera_query.single() else {
        return;
    };

    if keys.just_pressed(KeyCode::KeyZ) {
        commands.trigger(
            ZoomToFit::new(camera, target.0)
                .margin(FIT_MARGIN)
                .duration(FIT_DURATION),
        );
    }

    if keys.just_pressed(KeyCode::KeyL) {
        commands.trigger(LookAt::new(camera, target.0).duration(LOOK_AT_DURATION));
    }

    if keys.just_pressed(KeyCode::KeyK) {
        commands.trigger(
            LookAtAndZoomToFit::new(camera, target.0)
                .margin(FIT_MARGIN)
                .duration(FIT_DURATION),
        );
    }
}

fn on_zoom_begin(
    trigger: On<ZoomBegin>,
    target: Option<Res<TargetEntity>>,
    mut bars: Query<&mut TitleBarControlState>,
) {
    let Some(target) = target else {
        return;
    };
    if trigger.target != target.0 {
        return;
    }
    for mut bar in &mut bars {
        bar.set_active(ZOOM_CONTROL, true);
    }
}

fn on_zoom_end(
    trigger: On<ZoomEnd>,
    target: Option<Res<TargetEntity>>,
    mut bars: Query<&mut TitleBarControlState>,
) {
    let Some(target) = target else {
        return;
    };
    if trigger.target != target.0 {
        return;
    }
    for mut bar in &mut bars {
        bar.set_active(ZOOM_CONTROL, false);
    }
}

fn on_animation_begin(trigger: On<AnimationBegin>, mut bars: Query<&mut TitleBarControlState>) {
    let control = match trigger.source {
        AnimationSource::LookAt => LOOK_CONTROL,
        AnimationSource::LookAtAndZoomToFit => LOOK_AND_ZOOM_CONTROL,
        _ => return,
    };
    for mut bar in &mut bars {
        bar.set_active(control, true);
    }
}

fn on_animation_end(trigger: On<AnimationEnd>, mut bars: Query<&mut TitleBarControlState>) {
    let control = match trigger.source {
        AnimationSource::LookAt => LOOK_CONTROL,
        AnimationSource::LookAtAndZoomToFit => LOOK_AND_ZOOM_CONTROL,
        _ => return,
    };
    for mut bar in &mut bars {
        bar.set_active(control, false);
    }
}
