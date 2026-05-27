//! `aa_text` — compare every anti-aliasing path the text renderer can use, over
//! plain unlit text with nothing else in the scene.
//!
//! slug renders glyph edges as analytic alpha coverage, sampled once per pixel.
//! At grazing angles that single sample can't represent the foreshortened pixel
//! footprint, so edges stair-step. There are two fundamentally different places
//! to fix it.
//!
//! **In the coverage shader (`A` + `E`).** slug anti-aliases glyph edges inside
//! the fragment shader — no extra pass, and it survives OIT (which forces
//! `Msaa::Off`). Two orthogonal controls ([`TextAntiAlias`]), both on by default:
//! - **Supersample** (`A`) — evaluates coverage at four sub-pixel sample points and averages,
//!   fixing the stepping along a shallow edge that a single sample can't resolve.
//! - **Anisotropic band** (`E`) — sizes the edge ramp from the distance gradient so it holds ~1px
//!   per screen axis, fixing the convex-corner balloon at grazing angles that the scalar band
//!   over-widens into a wing.
//!
//! They fix different artifacts, so the best result is both on. They are toggles
//! for performance: the band is nearly free, but supersampling evaluates coverage
//! four (or five, combined) times per fragment, so a text-dense frame can reclaim
//! fill-rate by dropping to the band alone or turning both off.
//!
//! **As a post-process pass over the resolved frame.** Three modes, mutually
//! exclusive (selecting one drops the others):
//! - **SMAA** (`S`) — luma-edge detection in image space; keeps MSAA on.
//! - **FXAA** (`F`) — cheaper, blurrier luma-edge pass; keeps MSAA on.
//! - **TAA** (`T`) — temporal blend across frames; requires `Msaa::Off` plus the depth/motion
//!   prepasses. Included for completeness — note it ghosts on alpha-blended glyphs (the
//!   transparency the renderer exists to draw), so it is the weakest fit here even though it AA's
//!   the most.
//!
//! The text is unlit, so its color never varies as you orbit. Orbit to a grazing
//! angle (MMB) to see the artifacts, then toggle each path.
//!
//! Hotkeys:
//! - `A` — toggle supersampling (4 samples vs 1).
//! - `E` — toggle the anisotropic edge band (vs the scalar band).
//! - `S` / `F` / `T` — select SMAA / FXAA / TAA on the camera (press again for none).
//! - `H` — home the camera.

use bevy::anti_alias::fxaa::Fxaa;
use bevy::anti_alias::smaa::Smaa;
use bevy::anti_alias::taa::TemporalAntiAliasing;
use bevy::prelude::*;
use bevy::render::camera::MipBias;
use bevy::render::camera::TemporalJitter;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::TextAntiAlias;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::ControlActivation;
use fairy_dust::TitleBar;

const HEADLINE_TEXT: &str = "Anti-aliasing";
const HEADLINE_SIZE: f32 = 0.40;
const HEADLINE_Y: f32 = 0.18;
const SMALL_TEXT: &str = "the quick brown fox jumps over the lazy dog";
const SMALL_SIZE: f32 = 0.05;
const SMALL_Y: f32 = -0.06;
const DISPLAY_Z: f32 = 0.0;
const TEXT_COLOR: Color = Color::srgb(0.92, 0.92, 0.94);

const HOME_FOCUS: Vec3 = Vec3::new(0.0, 0.08, DISPLAY_Z);
const HOME_FRAME: f32 = 4.0;
const HOME_PITCH: f32 = 0.0;
const HOME_YAW: f32 = 0.0;

/// Title-bar control labels.
const SMAA_CONTROL: &str = "S SMAA";
const FXAA_CONTROL: &str = "F FXAA";
const TAA_CONTROL: &str = "T TAA";
const SUPERSAMPLE_CONTROL: &str = "A SSAA";
const EDGE_BAND_CONTROL: &str = "E BAND";

/// Which post-process anti-aliasing pass is active on the camera. The three
/// passes are mutually exclusive — selecting one removes the others. Orthogonal
/// to [`TextAntiAlias`], which runs in the coverage shader regardless.
#[derive(Resource, Clone, Copy, Default, PartialEq, Eq)]
enum PostAa {
    /// No post-process pass; rely on MSAA + supersampling alone.
    #[default]
    None,
    /// SMAA: image-space luma-edge pass, MSAA stays on.
    Smaa,
    /// FXAA: cheaper image-space luma-edge pass, MSAA stays on.
    Fxaa,
    /// TAA: temporal blend; forces `Msaa::Off` and adds the prepasses.
    Taa,
}

fn main() {
    // `bevy_diegetic::DiegeticUiPlugin` is registered automatically by
    // `fairy_dust::sprinkle_example`. It registers `TextAntiAlias`, so this
    // example only toggles it.
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_orbit_cam(
            |_| {},
            OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike),
        )
        .with_camera_home(
            Transform::from_translation(HOME_FOCUS).with_scale(Vec3::splat(HOME_FRAME)),
        )
        .pitch(HOME_PITCH)
        .yaw(HOME_YAW)
        .with_title_bar(
            TitleBar::new()
                .control(SMAA_CONTROL)
                .control(FXAA_CONTROL)
                .control(TAA_CONTROL)
                .control(SUPERSAMPLE_CONTROL)
                .control(EDGE_BAND_CONTROL),
        )
        .wire_chip_to_state::<PostAa, _>(SMAA_CONTROL, |mode| activation(*mode == PostAa::Smaa))
        .wire_chip_to_state::<PostAa, _>(FXAA_CONTROL, |mode| activation(*mode == PostAa::Fxaa))
        .wire_chip_to_state::<PostAa, _>(TAA_CONTROL, |mode| activation(*mode == PostAa::Taa))
        .wire_chip_to_state::<TextAntiAlias, _>(SUPERSAMPLE_CONTROL, |aa| {
            activation(aa.supersamples())
        })
        .wire_chip_to_state::<TextAntiAlias, _>(EDGE_BAND_CONTROL, |aa| {
            activation(aa.anisotropic())
        })
        .init_resource::<PostAa>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (select_post_aa, toggle_supersample, toggle_edge_band),
        )
        .run();
}

/// Maps a bool to a title-bar chip activation.
const fn activation(active: bool) -> ControlActivation {
    if active {
        ControlActivation::Active
    } else {
        ControlActivation::Inactive
    }
}

fn setup(mut commands: Commands) {
    commands.spawn((
        Name::new("Headline"),
        WorldText::new(HEADLINE_TEXT),
        WorldTextStyle::new(HEADLINE_SIZE)
            .with_color(TEXT_COLOR)
            .with_unlit()
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(0.0, HEADLINE_Y, DISPLAY_Z),
    ));
    commands.spawn((
        Name::new("Small line"),
        WorldText::new(SMALL_TEXT),
        WorldTextStyle::new(SMALL_SIZE)
            .with_color(TEXT_COLOR)
            .with_unlit()
            .with_shadow_mode(GlyphShadowMode::None),
        Transform::from_xyz(0.0, SMALL_Y, DISPLAY_Z),
    ));
}

/// On `S`/`F`/`T`, select that post-process mode (or turn it off if it is
/// already active) and reconcile the camera's components.
fn select_post_aa(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<PostAa>,
    cameras: Query<Entity, With<OrbitCam>>,
    mut commands: Commands,
) {
    let pressed = if keyboard.just_pressed(KeyCode::KeyS) {
        PostAa::Smaa
    } else if keyboard.just_pressed(KeyCode::KeyF) {
        PostAa::Fxaa
    } else if keyboard.just_pressed(KeyCode::KeyT) {
        PostAa::Taa
    } else {
        return;
    };
    // Pressing the active mode's key turns it off.
    let next = if *mode == pressed {
        PostAa::None
    } else {
        pressed
    };
    *mode = next;
    for camera in &cameras {
        apply_post_aa(&mut commands, camera, next);
    }
}

/// Strips every post-process pass off `camera`, then installs the one `mode`
/// selects. TAA is the only mode that touches MSAA — it requires `Msaa::Off`;
/// all others keep `Msaa::default()`. The frozen [`TemporalJitter`]/[`MipBias`]
/// TAA leaves behind are removed so the off-state renders unshifted.
fn apply_post_aa(commands: &mut Commands, camera: Entity, mode: PostAa) {
    let mut entity = commands.entity(camera);
    entity
        .remove::<Smaa>()
        .remove::<Fxaa>()
        .remove::<TemporalAntiAliasing>()
        .remove::<TemporalJitter>()
        .remove::<MipBias>();
    match mode {
        PostAa::None => {
            entity.insert(Msaa::default());
        },
        PostAa::Smaa => {
            entity.insert((Msaa::default(), Smaa::default()));
        },
        PostAa::Fxaa => {
            entity.insert((Msaa::default(), Fxaa::default()));
        },
        PostAa::Taa => {
            entity.insert((Msaa::Off, TemporalAntiAliasing::default()));
        },
    }
}

/// On `A`, flip the supersampling axis of [`TextAntiAlias`] (the band axis is
/// left as-is, so this composes with `E`).
fn toggle_supersample(keyboard: Res<ButtonInput<KeyCode>>, mut aa: ResMut<TextAntiAlias>) {
    if !keyboard.just_pressed(KeyCode::KeyA) {
        return;
    }
    *aa = match *aa {
        TextAntiAlias::Off => TextAntiAlias::Supersample,
        TextAntiAlias::Supersample => TextAntiAlias::Off,
        TextAntiAlias::Anisotropic => TextAntiAlias::Both,
        TextAntiAlias::Both => TextAntiAlias::Anisotropic,
    };
}

/// On `E`, flip the anisotropic-band axis of [`TextAntiAlias`] (the supersample
/// axis is left as-is, so this composes with `A`).
fn toggle_edge_band(keyboard: Res<ButtonInput<KeyCode>>, mut aa: ResMut<TextAntiAlias>) {
    if !keyboard.just_pressed(KeyCode::KeyE) {
        return;
    }
    *aa = match *aa {
        TextAntiAlias::Off => TextAntiAlias::Anisotropic,
        TextAntiAlias::Anisotropic => TextAntiAlias::Off,
        TextAntiAlias::Supersample => TextAntiAlias::Both,
        TextAntiAlias::Both => TextAntiAlias::Supersample,
    };
}
