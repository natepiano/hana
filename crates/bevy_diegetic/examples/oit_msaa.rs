//! `oit_msaa` — toggle order-independent transparency (OIT) on and off to see
//! the trade it makes against MSAA.
//!
//! Three translucent `AlphaMode::Blend` words lie coplanar on the ground,
//! overlapping each other. Under the default path (MSAA on, no OIT) their
//! composite order is view-dependent: orbit the camera and the overlap region
//! swings color as the per-fragment draw order flips at grazing angles. Press
//! `Space` to add `bevy_diegetic::StableTransparency` to the camera — OIT
//! composites those fragments by depth regardless of draw order, so the color
//! holds steady. The cost shows on the cube: OIT forces `Msaa::Off`, so its
//! silhouette edges alias while OIT is on.
//!
//! Hotkeys:
//! - `Space` — toggle OIT (`StableTransparency`) on the camera.
//! - `H` — home the camera.

use std::f32::consts::FRAC_PI_2;
use std::f32::consts::FRAC_PI_4;

use bevy::prelude::*;
use bevy_diegetic::Anchor;
use bevy_diegetic::StableTransparency;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::ControlActivation;
use fairy_dust::DescriptionPanel;
use fairy_dust::TitleBar;

const GROUND_SIZE: f32 = 6.0;
const GROUND_COLOR: Color = Color::srgb(0.08, 0.08, 0.09);

const CUBE_SIZE: f32 = 1.0;
const CUBE_POS: Vec3 = Vec3::new(1.9, 0.5, 0.0);
const CUBE_COLOR: Color = Color::srgb(0.85, 0.7, 0.5);

/// The coplanar word repeated once per translucent layer.
const STACK_WORD: &str = "OIT";
/// World-meter height of each coplanar layer.
const STACK_SIZE: f32 = 0.9;
/// Per-layer alpha — low enough that three overlapping layers visibly differ
/// by draw order, high enough that each color reads.
const STACK_ALPHA: f32 = 0.5;
/// Height above the ground plane; all layers share it so they stay coplanar.
const STACK_LIFT: f32 = 0.002;

/// One translucent `Blend` layer: a color and its lateral offset from center.
/// The small spread keeps the words overlapping while leaving each color
/// identifiable at the edges.
const STACK_LAYERS: [(Color, f32); 3] = [
    (Color::srgba(1.0, 0.2, 0.2, STACK_ALPHA), -0.09),
    (Color::srgba(0.2, 1.0, 0.3, STACK_ALPHA), 0.0),
    (Color::srgba(0.3, 0.5, 1.0, STACK_ALPHA), 0.09),
];

const LIGHT_AIM: Vec3 = Vec3::new(0.4, 0.3, 0.0);

const HOME_CENTER: Vec3 = Vec3::new(0.4, 0.25, 0.0);
const HOME_FRAME: f32 = 4.2;
const HOME_YAW: f32 = 0.5;
/// Low pitch so the home pose looks across the ground at a grazing angle,
/// where the coplanar color shift is strongest.
const HOME_PITCH: f32 = 0.18;

const OIT_CONTROL: &str = "Space OIT";

/// Source of truth for the OIT toggle. Drives both the camera marker and the
/// title-bar chip highlight.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
enum OitState {
    /// No OIT: MSAA on, coplanar text shifts with view angle.
    #[default]
    Off,
    /// OIT on: `Msaa::Off`, coplanar text stable, mesh edges alias.
    On,
}

fn main() {
    // `bevy_diegetic::DiegeticUiPlugin` is registered automatically by
    // `fairy_dust::sprinkle_example`.
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .aim_at(LIGHT_AIM)
        .with_ground_plane()
        .size(GROUND_SIZE)
        .color(GROUND_COLOR)
        .with_cube()
        .size(CUBE_SIZE)
        .color(CUBE_COLOR)
        .transform(
            Transform::from_translation(CUBE_POS).with_rotation(Quat::from_rotation_y(FRAC_PI_4)),
        )
        .with_orbit_cam(
            |_| {},
            OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike),
        )
        .with_camera_home(
            Transform::from_translation(HOME_CENTER).with_scale(Vec3::splat(HOME_FRAME)),
        )
        .yaw(HOME_YAW)
        .pitch(HOME_PITCH)
        .with_title_bar(TitleBar::new().control(OIT_CONTROL))
        .wire_chip_to_state::<OitState, _>(OIT_CONTROL, |state| match state {
            OitState::On => ControlActivation::Active,
            OitState::Off => ControlActivation::Inactive,
        })
        .with_camera_control_panel()
        .with_description_panel(description_panel())
        .init_resource::<OitState>()
        .add_systems(Startup, setup)
        .add_systems(Update, toggle_oit)
        .run();
}

fn description_panel() -> DescriptionPanel {
    DescriptionPanel::new("OIT vs MSAA")
        .with_anchor(Anchor::TopRight)
        .line("Space toggles StableTransparency.")
        .line("OIT off (MSAA): cube edges are")
        .line("anti-aliased, but the coplanar OIT")
        .line("text swings color as you orbit.")
        .line("OIT on: the text holds steady;")
        .line("cube edges alias (MSAA is off).")
        .line("Orbit (MMB) at a low angle to see")
        .line("the shift, then toggle.")
}

/// Spawns the three coplanar translucent layers that exhibit the view-angle
/// color shift. All layers share `STACK_LIFT` so they are coplanar; the
/// per-layer lateral offset keeps them overlapping.
fn setup(mut commands: Commands) {
    let flat = Quat::from_rotation_x(-FRAC_PI_2);
    for (color, offset) in STACK_LAYERS {
        commands.spawn((
            WorldText::new(STACK_WORD),
            WorldTextStyle::new(STACK_SIZE)
                .with_color(color)
                .with_anchor(Anchor::Center),
            Transform::from_xyz(offset, STACK_LIFT, 0.0).with_rotation(flat),
        ));
    }
}

/// On `Space`, flip [`OitState`] and apply it to the orbit camera by
/// inserting or removing [`StableTransparency`]. The `bevy_diegetic` observers
/// handle the OIT settings and the `Msaa` swap across every camera on the
/// window (scene plus the screen-space overlay cameras).
fn toggle_oit(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<OitState>,
    cameras: Query<Entity, With<OrbitCam>>,
    mut commands: Commands,
) {
    if !keyboard.just_pressed(KeyCode::Space) {
        return;
    }
    *state = match *state {
        OitState::Off => OitState::On,
        OitState::On => OitState::Off,
    };
    for camera in &cameras {
        match *state {
            OitState::On => {
                commands.entity(camera).insert(StableTransparency);
            },
            OitState::Off => {
                commands.entity(camera).remove::<StableTransparency>();
            },
        }
    }
}
