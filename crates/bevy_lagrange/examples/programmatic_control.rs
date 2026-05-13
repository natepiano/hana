//! Demonstrates app-authored camera control through direct `OrbitCam` target fields.

use bevy::prelude::*;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::Anchor;
use fairy_dust::DescriptionPanel;
use fairy_dust::TitleBar;
use fairy_dust::TitleBarControlState;

const CUBE_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
const CUBE_SIZE: f32 = 1.0;
const CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, CUBE_SIZE * 0.5 + 0.2, 0.0);

const GROUND_SIZE: f32 = 8.0;

const HOME_FOCUS: Vec3 = Vec3::new(0.0, CUBE_SIZE * 0.5, 0.0);
const HOME_PITCH: f32 = 0.42;
const HOME_RADIUS: f32 = 6.0;
const HOME_YAW: f32 = -0.85;
const HOME_CONTROL: &str = "H Home";
const HOME_SMOOTHNESS: f32 = 0.35;
const HOME_FOCUS_EPSILON: f32 = 0.01;
const HOME_ORBIT_EPSILON: f32 = 0.01;

#[derive(Component)]
struct ProgrammaticCamera;

#[derive(Resource, Default)]
struct HomeReset {
    previous_smoothness: Option<CameraSmoothness>,
}

impl HomeReset {
    fn start(&mut self, camera: &mut OrbitCam) {
        self.previous_smoothness
            .get_or_insert_with(|| CameraSmoothness::from_camera(camera));
        camera.orbit_smoothness = HOME_SMOOTHNESS;
        camera.pan_smoothness = HOME_SMOOTHNESS;
        camera.zoom_smoothness = HOME_SMOOTHNESS;
    }

    const fn finish(&mut self, camera: &mut OrbitCam) {
        if let Some(previous) = self.previous_smoothness.take() {
            previous.apply(camera);
        }
    }

    const fn is_active(&self) -> bool { self.previous_smoothness.is_some() }
}

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

fn main() {
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_orbit_cam_bundle(
            configure_camera,
            (ProgrammaticCamera, OrbitCamPreset::BlenderLike),
        )
        .with_ground_plane()
        .size(GROUND_SIZE)
        .with_studio_lighting()
        .with_cube()
        .size(CUBE_SIZE)
        .color(CUBE_COLOR)
        .transform(Transform::from_translation(CUBE_TRANSLATION))
        .with_title_bar(
            TitleBar::new("Controls")
                .with_anchor(Anchor::TopLeft)
                .control(HOME_CONTROL),
        )
        .with_description_panel(description_panel())
        .with_camera_control_panel()
        .init_resource::<HomeReset>()
        .add_systems(Update, (home_camera, update_home_reset).chain())
        .run();
}

const fn configure_camera(camera: &mut OrbitCam) {
    camera.focus = HOME_FOCUS;
    camera.yaw = Some(HOME_YAW);
    camera.pitch = Some(HOME_PITCH);
    camera.radius = Some(HOME_RADIUS);
}

fn description_panel() -> DescriptionPanel {
    DescriptionPanel::new("Programmatic OrbitCam Control")
        .with_anchor(Anchor::BottomLeft)
        .line("Press H to home the camera")
        .line("The `home_camera` system, reads the keypress and mutates `target_focus`, `target_yaw`, `target_pitch`, and `target_radius` directly on the `OrbitCam`.")
        .line("`OrbitCam` then lerps to the home destination.")
}

fn home_camera(
    keys: Res<ButtonInput<KeyCode>>,
    mut reset: ResMut<HomeReset>,
    mut camera: Single<&mut OrbitCam, With<ProgrammaticCamera>>,
    mut title_bars: Query<&mut TitleBarControlState>,
) {
    if !keys.just_pressed(KeyCode::KeyH) {
        return;
    }

    reset.start(&mut camera);
    camera.target_focus = HOME_FOCUS;
    camera.target_yaw = HOME_YAW;
    camera.target_pitch = HOME_PITCH;
    camera.target_radius = HOME_RADIUS;
    set_home_control_active(&mut title_bars, true);
}

fn update_home_reset(
    mut reset: ResMut<HomeReset>,
    mut cameras: Query<&mut OrbitCam, With<ProgrammaticCamera>>,
    mut title_bars: Query<&mut TitleBarControlState>,
) {
    if !reset.is_active() {
        return;
    }

    let Ok(mut camera) = cameras.single_mut() else {
        return;
    };

    if !camera_targets_home(&camera) || camera_at_home(&camera) {
        reset.finish(&mut camera);
        set_home_control_active(&mut title_bars, false);
    }
}

fn set_home_control_active(title_bars: &mut Query<&mut TitleBarControlState>, active: bool) {
    for mut title_bar in title_bars {
        title_bar.set_active(HOME_CONTROL, active);
    }
}

fn camera_targets_home(camera: &OrbitCam) -> bool {
    camera.target_focus.distance(HOME_FOCUS) <= HOME_FOCUS_EPSILON
        && (camera.target_yaw - HOME_YAW).abs() <= HOME_ORBIT_EPSILON
        && (camera.target_pitch - HOME_PITCH).abs() <= HOME_ORBIT_EPSILON
        && (camera.target_radius - HOME_RADIUS).abs() <= HOME_FOCUS_EPSILON
}

fn camera_at_home(camera: &OrbitCam) -> bool {
    let (Some(yaw), Some(pitch), Some(radius)) = (camera.yaw, camera.pitch, camera.radius) else {
        return false;
    };

    camera.focus.distance(HOME_FOCUS) <= HOME_FOCUS_EPSILON
        && (yaw - HOME_YAW).abs() <= HOME_ORBIT_EPSILON
        && (pitch - HOME_PITCH).abs() <= HOME_ORBIT_EPSILON
        && (radius - HOME_RADIUS).abs() <= HOME_FOCUS_EPSILON
}
