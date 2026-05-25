//! `cascade` — visualizes the text cascade rule: *my own override, else my
//! parent's, else the global default at the root.*
//!
//! Two attributes cascade, and the scene resolves each at every tier:
//!
//! - **Text alpha.** A standalone with no override paints the global default (`Blend`). A panel
//!   sets `Add` for the text under it, and a label inside inherits it. A second label authors its
//!   own `Multiply`, and a standalone authors its own `Add` — each beats what it would otherwise
//!   inherit.
//! - **Font unit.** A standalone with a bare size resolves to the global `font_unit` (`Meters`).
//!   The panel seeds `Points` for the text under it, which its bare-sized labels inherit.
//!
//! Each line of text names its tier, so the rendered alpha and size can be read
//! against the rule it demonstrates. Press `H` to home the camera.

use bevy::prelude::*;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::Mm;
use bevy_diegetic::Padding;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::TitleBar;

// camera home — translation centers the framed region, scale sets its extents.
const HOME_CENTER: Vec3 = Vec3::new(0.0, 0.6, 0.0);
const HOME_FRAME_SIZE: f32 = 5.5;
const HOME_PITCH: f32 = 0.15;
const HOME_YAW: f32 = 0.0;

// colors
const PANEL_BACKGROUND: Color = Color::srgba(0.02, 0.03, 0.07, 0.85);
const PANEL_BORDER: Color = Color::srgba(0.15, 0.7, 0.9, 0.5);
const TIER1_COLOR: Color = Color::srgb(0.55, 0.75, 1.0);
const TIER2_COLOR: Color = Color::srgb(0.35, 1.0, 0.7);
const TIER3_COLOR: Color = Color::srgb(1.0, 0.8, 0.3);

// text sizes
const PANEL_TEXT_SIZE: f32 = 13.0;
const PANEL_TITLE_SIZE: f32 = 16.0;
const STANDALONE_SIZE_METERS: f32 = 0.14;

fn main() {
    // `bevy_diegetic::DiegeticUiPlugin` is registered automatically by
    // `fairy_dust::sprinkle_example`.
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .with_orbit_cam(
            |_| {},
            OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike),
        )
        .with_camera_home(
            Transform::from_translation(HOME_CENTER).with_scale(Vec3::splat(HOME_FRAME_SIZE)),
        )
        .pitch(HOME_PITCH)
        .yaw(HOME_YAW)
        .with_title_bar(TitleBar::new().with_title("CASCADE"))
        .with_camera_control_panel()
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    // Tier 1 — a standalone with no overrides resolves both attributes to the
    // global defaults: alpha `Blend`, font unit `Meters`.
    commands.spawn((
        WorldText::new("tier 1 - global default: alpha Blend, font Meters"),
        WorldTextStyle::new(STANDALONE_SIZE_METERS).with_color(TIER1_COLOR),
        Transform::from_xyz(-1.7, 1.0, 0.0),
    ));

    // Tier 3 — a standalone authoring its own alpha (`Add`) beats the global
    // `Blend`. The same `WorldTextStyle::with_alpha_mode` powers the panel
    // labels below.
    commands.spawn((
        WorldText::new("tier 3 - standalone's own alpha = Add"),
        WorldTextStyle::new(STANDALONE_SIZE_METERS)
            .with_color(TIER3_COLOR)
            .with_alpha_mode(AlphaMode::Add),
        Transform::from_xyz(-1.7, 0.55, 0.0),
    ));

    // Tier 2 + tier 3 live inside this panel. The panel sets `Add` for the text
    // under it and seeds `Points` as the font unit its labels inherit.
    if let Ok(panel) = DiegeticPanel::world()
        .size(Mm(180.0), Mm(70.0))
        .anchor(Anchor::Center)
        .text_alpha_mode(AlphaMode::Add)
        .layout(build_panel)
        .build()
    {
        commands.spawn((panel, Transform::from_xyz(1.4, 0.6, 0.0)));
    } else {
        error!("failed to build cascade panel");
    }
}

/// Builds the panel layout. Bare-sized labels inherit the panel's seeded
/// `Points` font unit (tier 2 for font unit); the first label also inherits the
/// panel's `Add` alpha, while the second authors its own `Multiply` (tier 3).
fn build_panel(b: &mut LayoutBuilder) {
    let title = LayoutTextStyle::new(PANEL_TITLE_SIZE).with_color(TIER2_COLOR);
    let inherited = LayoutTextStyle::new(PANEL_TEXT_SIZE).with_color(TIER2_COLOR);
    let own = LayoutTextStyle::new(PANEL_TEXT_SIZE)
        .with_color(TIER3_COLOR)
        .with_alpha_mode(AlphaMode::Multiply);

    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::TopToBottom)
            .padding(Padding::all(Px(12.0)))
            .child_gap(Px(8.0))
            .background(PANEL_BACKGROUND)
            .border(Border::all(Px(2.0), PANEL_BORDER)),
        |b| {
            b.text("PANEL - alpha Add, font Points", title);
            b.text("tier 2 - inherited: alpha Add, font Points", inherited);
            b.text("tier 3 - label's own alpha = Multiply", own);
        },
    );
}
