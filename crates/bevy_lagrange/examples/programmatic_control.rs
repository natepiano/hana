//! Drives an `OrbitCam` from app code by mutating `target_focus`, `target_yaw`,
//! `target_pitch`, and `target_radius` directly. Pressing **H** kicks off a home
//! animation that temporarily raises the camera's `orbit_smoothness`,
//! `pan_smoothness`, and `zoom_smoothness` so the lerp reads as a slow fly-to,
//! then restores the previous smoothness once the camera arrives. The
//! `HomeAnimationBegin` / `HomeAnimationEnd` events expose the animation window
//! for other systems (here, the title-bar control chip) to react to.
//!
//! Controls:
//!   H — home the camera

use bevy::prelude::*;
use bevy_kana::event;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::Anchor;
use fairy_dust::DescriptionPanel;
use fairy_dust::TitleBar;

fn main() {
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_orbit_cam_preset_bundle(
            configure_camera,
            OrbitCamPreset::BlenderLike,
            ProgrammaticCamera,
        )
        .with_ground_plane()
        .with_studio_lighting()
        .with_cube()
        .size(CUBE_SIZE)
        .color(CUBE_COLOR)
        .transform(Transform::from_translation(CUBE_TRANSLATION))
        .with_title_bar(
            TitleBar::new()
                .with_title("Programmatic Control")
                .with_anchor(Anchor::TopLeft)
                .control(HOME_CONTROL),
        )
        .wire_chip_to_events::<HomeAnimationBegin, HomeAnimationEnd>(HOME_CONTROL)
        .with_description_panel(description_panel())
        .with_camera_control_panel()
        .init_resource::<HomeReset>()
        // `H` runs `home_camera` through Fairy Dust's shortcut binding, which
        // fires it only when no modifier is held. `home_camera` writes the
        // targets and starts the smoothness override; `update_home_reset` then
        // polls each frame for arrival or user takeover.
        .with_shortcut(KeyCode::KeyH, home_camera)
        .add_systems(Update, update_home_reset)
        .run();
}

// ═════════════════════════════════════════════════════════════════════════════
// CAMERA HOME — programmatic OrbitCam control via target_focus/yaw/pitch/radius.
//
// How it works:
//   1. `configure_camera` seeds the initial `focus`/`yaw`/`pitch`/`radius` when the camera spawns.
//   2. On **H**, `home_camera` records the camera's current smoothness into `HomeReset`, raises
//      smoothness so the lerp is slow, writes the home `target_*` fields, and emits
//      `HomeAnimationBegin`.
//   3. `OrbitCam` itself lerps `focus`/`yaw`/`pitch`/`radius` toward those targets each frame.
//   4. `update_home_reset` checks each frame whether the targets still point at home (user takeover
//      changes them) and whether the camera has arrived; either condition restores the saved
//      smoothness and emits `HomeAnimationEnd`.
// ═════════════════════════════════════════════════════════════════════════════

const HOME_CONTROL: &str = "H Home";
const HOME_FOCUS: Vec3 = Vec3::new(0.0, CUBE_SIZE * 0.5, 0.0);
const HOME_PITCH: f32 = 0.42;
const HOME_RADIUS: f32 = 6.0;
const HOME_YAW: f32 = -0.85;
const HOME_SMOOTHNESS: f32 = 0.35;
const HOME_FOCUS_EPSILON: f32 = 0.01;
const HOME_ORBIT_EPSILON: f32 = 0.01;

event!(
    /// Fires when the example begins driving the camera toward home.
    HomeAnimationBegin
);
event!(
    /// Fires when the camera reaches home or the user takes manual control.
    HomeAnimationEnd
);

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

const fn configure_camera(camera: &mut OrbitCam) {
    camera.focus = HOME_FOCUS;
    camera.yaw = Some(HOME_YAW);
    camera.pitch = Some(HOME_PITCH);
    camera.radius = Some(HOME_RADIUS);
}

fn home_camera(
    mut commands: Commands,
    mut reset: ResMut<HomeReset>,
    mut camera: Single<&mut OrbitCam, With<ProgrammaticCamera>>,
) {
    reset.start(&mut camera);
    camera.target_focus = HOME_FOCUS;
    camera.target_yaw = HOME_YAW;
    camera.target_pitch = HOME_PITCH;
    camera.target_radius = HOME_RADIUS;
    commands.trigger(HomeAnimationBegin);
}

fn update_home_reset(
    mut commands: Commands,
    mut reset: ResMut<HomeReset>,
    mut cameras: Query<&mut OrbitCam, With<ProgrammaticCamera>>,
) {
    if !reset.is_active() {
        return;
    }

    let Ok(mut camera) = cameras.single_mut() else {
        return;
    };

    // Finish when the user has taken control (targets no longer point at home)
    // or when the camera has actually arrived.
    if !camera_targets_home(&camera) || camera_at_home(&camera) {
        reset.finish(&mut camera);
        commands.trigger(HomeAnimationEnd);
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

// ═════════════════════════════════════════════════════════════════════════════
// SCENE SCAFFOLDING — cube the camera homes onto, ground sized to match.
// ═════════════════════════════════════════════════════════════════════════════

const CUBE_COLOR: Color = fairy_dust::EXAMPLE_CUBE_COLOR;
const CUBE_SIZE: f32 = fairy_dust::EXAMPLE_CUBE_SIZE;
const CUBE_TRANSLATION: Vec3 = fairy_dust::example_cube_on_ground(0.2);

// ═════════════════════════════════════════════════════════════════════════════
// UI — description panel explaining the home flow on screen.
// ═════════════════════════════════════════════════════════════════════════════

const DESCRIPTION_BODY_SIZE: f32 = 10.0;
const DESCRIPTION_HEADING: &str = "How it works";
const DESCRIPTION_LINES: [&str; 5] = [
    "1. When you press H to home the camera",
    "2. The code saves the current smoothness in HomeReset resource",
    "3. Raises smoothness so OrbitCam lerps more slowly",
    "4. Writes the home target focus, yaw, pitch, and radius",
    "5. Restores the saved smoothness when home is reached",
];

fn description_panel() -> DescriptionPanel {
    DescriptionPanel::new(DESCRIPTION_HEADING)
        .with_fit_width()
        .with_body_size(DESCRIPTION_BODY_SIZE)
        .lines(DESCRIPTION_LINES)
}
