#![allow(
    clippy::expect_used,
    reason = "demo code; panic on invalid setup is acceptable"
)]

//! TAA shimmer demonstration.
//!
//! Shows how Temporal Anti-Aliasing interacts with SDF border-only
//! panels. Two identical panels are displayed side by side:
//!
//! - **Left**: full-opacity white border — shimmers with TAA because Bevy excludes
//!   `AlphaMode::Blend` from the depth/motion prepass, so TAA has no stable data to accumulate
//!   against.
//!
//! - **Right**: reduced-alpha border (70%) — the lower contrast between the jittered alpha values
//!   and the background makes the shimmer much less noticeable.
//!
//! Press **T** to toggle TAA on/off and observe the difference.

use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_brp_extras::PortDisplay;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Mm;
use bevy_diegetic::Padding;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;
use bevy_lagrange::InputControl;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TrackpadBehavior;
use bevy_lagrange::TrackpadInput;
use bevy_window_manager::WindowManagerPlugin;

// ── Layout ──────────────────────────────────────────────────────────
const PANEL_WIDTH: f32 = 80.0; // mm
const PANEL_HEIGHT: f32 = 65.0; // mm
const PANEL_GAP: f32 = 10.0; // mm between panels
const PANEL_PAD: f32 = 4.0; // mm
const BORDER_WIDTH: f32 = 0.2; // mm — thin enough to trigger shimmer
const TITLE_SIZE: f32 = 10.0; // pt
const BODY_SIZE: f32 = 7.0; // pt
const HINT_SIZE: f32 = 6.0; // pt

// ── Colors ──────────────────────────────────────────────────────────
const FULL_ALPHA_BORDER: Color = Color::WHITE;
const REDUCED_ALPHA_BORDER: Color = Color::srgba(0.7, 0.7, 0.8, 0.6);
const TITLE_COLOR: Color = Color::WHITE;
const BODY_COLOR: Color = Color::srgba(0.7, 0.7, 0.75, 0.9);
const HINT_COLOR: Color = Color::srgba(0.5, 0.5, 0.55, 0.7);

// ── HUD ─────────────────────────────────────────────────────────────
const HUD_HEIGHT: f32 = 48.0;
const HUD_PADDING: f32 = 12.0;
const HUD_GAP: f32 = 14.0;
const HUD_TITLE_SIZE: f32 = 16.0;
const HUD_BODY_SIZE: f32 = 14.0;
const HUD_HINT_SIZE: f32 = 12.0;
const HUD_BACKGROUND: Color = Color::srgba(0.02, 0.03, 0.07, 0.92);
const HUD_FRAME_BACKGROUND: Color = Color::srgba(0.01, 0.01, 0.03, 0.95);
const HUD_BORDER_ACCENT: Color = Color::srgba(0.15, 0.7, 0.9, 0.5);
const HUD_BORDER_DIM: Color = Color::srgba(0.1, 0.4, 0.6, 0.3);
const HUD_TITLE_COLOR: Color = Color::srgb(0.9, 0.95, 1.0);
const HUD_LABEL_COLOR: Color = Color::srgba(0.5, 0.55, 0.7, 0.8);
const HUD_ACTIVE_COLOR: Color = Color::srgb(0.3, 1.0, 0.8);
const HUD_INACTIVE_COLOR: Color = Color::srgba(0.6, 0.65, 0.8, 0.85);
const HUD_DIVIDER_COLOR: Color = Color::srgba(0.15, 0.4, 0.6, 0.25);

#[derive(Component)]
struct HudPanel;

#[derive(Resource)]
struct TaaEnabled(bool);

impl Default for TaaEnabled {
    fn default() -> Self { Self(true) }
}

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            LagrangePlugin,
            BrpExtrasPlugin::default().port_in_title(PortDisplay::NonDefault),
            WindowManagerPlugin,
            DiegeticUiPlugin,
        ))
        .init_resource::<TaaEnabled>()
        .add_systems(Startup, setup)
        .add_systems(Update, (toggle_taa, update_hud))
        .run();
}

fn setup(mut commands: Commands, windows: Query<&Window>) {
    // ── Left panel: full-alpha border (shimmers) ────────────────────
    commands.spawn((
        DiegeticPanel::world()
            .size(Mm(PANEL_WIDTH), Mm(PANEL_HEIGHT))
            .anchor(Anchor::TopCenter)
            .with_tree(build_panel(
                "Full Alpha Border",
                "border: Color::WHITE (alpha 1.0)",
                "High contrast alpha variation\ncauses visible shimmer\nunder TAA jitter.\n\nNote: TAA also softens text.\nToggle T to compare.",
                FULL_ALPHA_BORDER,
            ))
            .build()
            .expect("valid panel dimensions"),
        Transform::from_xyz(-(PANEL_WIDTH + PANEL_GAP) * 0.5 * 0.001, 0.0, 0.0),
    ));

    // ── Right panel: reduced-alpha border (stable) ──────────────────
    commands.spawn((
        DiegeticPanel::world()
            .size(Mm(PANEL_WIDTH), Mm(PANEL_HEIGHT))
            .anchor(Anchor::TopCenter)
            .with_tree(build_panel(
                "Reduced Alpha Border",
                "border: srgba(0.7, 0.7, 0.8, 0.6)",
                "Lower alpha reduces the\nframe-to-frame contrast,\nso TAA can converge.\n\nNote: TAA also softens text.\nToggle T to compare.",
                REDUCED_ALPHA_BORDER,
            ))
            .build()
            .expect("valid panel dimensions"),
        Transform::from_xyz((PANEL_WIDTH + PANEL_GAP) * 0.5 * 0.001, 0.0, 0.0),
    ));

    // ── Lighting ────────────────────────────────────────────────────
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            illuminance: 3137.0,
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // ── HUD ─────────────────────────────────────────────────────────
    let unlit_material = bevy_diegetic::default_panel_material();
    let unlit = StandardMaterial {
        unlit: true,
        ..unlit_material
    };
    let hud_width = windows.iter().next().map_or(800.0, Window::width);
    let mut hud_panel = DiegeticPanel::screen()
        .size(Sizing::fixed(Px(hud_width)), Sizing::fixed(Px(HUD_HEIGHT)))
        .anchor(Anchor::TopLeft)
        .material(unlit.clone())
        .text_material(unlit)
        .layout(|b| {
            build_hud_content(b, true);
        })
        .build()
        .expect("valid HUD dimensions");
    hud_panel.set_tree(build_hud_tree(true, hud_width));
    commands.spawn((HudPanel, hud_panel, Transform::default()));

    // ── Camera ──────────────────────────────────────────────────────
    commands.spawn((
        OrbitCam {
            radius: Some(0.25),
            yaw: Some(0.0),
            pitch: Some(0.0),
            button_orbit: MouseButton::Middle,
            button_pan: MouseButton::Middle,
            modifier_pan: Some(KeyCode::ShiftLeft),
            input_control: Some(InputControl {
                trackpad: Some(TrackpadInput {
                    behavior:    TrackpadBehavior::BlenderLike {
                        modifier_pan:  Some(KeyCode::ShiftLeft),
                        modifier_zoom: Some(KeyCode::ControlLeft),
                    },
                    sensitivity: 0.5,
                }),
                ..default()
            }),
            zoom_lower_limit: 0.000_000_1,
            ..default()
        },
        Msaa::Off,
        bevy::anti_alias::taa::TemporalAntiAliasing::default(),
    ));
}

// ── Panel builders ──────────────────────────────────────────────────

fn build_panel(title: &str, subtitle: &str, description: &str, border_color: Color) -> LayoutTree {
    let title_style = LayoutTextStyle::new(bevy_diegetic::Pt(TITLE_SIZE)).with_color(TITLE_COLOR);
    let sub_style = LayoutTextStyle::new(bevy_diegetic::Pt(BODY_SIZE)).with_color(BODY_COLOR);
    let body_style = LayoutTextStyle::new(bevy_diegetic::Pt(HINT_SIZE)).with_color(HINT_COLOR);

    let mut builder = LayoutBuilder::new(PANEL_WIDTH, PANEL_HEIGHT);
    builder.with(
        El::new()
            .direction(Direction::TopToBottom)
            .padding(Padding::all(PANEL_PAD))
            .child_gap(2.0)
            .border(Border::all(Mm(BORDER_WIDTH), border_color))
            .width(Sizing::grow_min(0.0))
            .height(Sizing::grow_min(0.0)),
        |b| {
            b.text(title, title_style);

            // Divider.
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::fixed(Mm(0.3)))
                    .background(border_color),
                |_| {},
            );

            b.text(subtitle, sub_style);
            b.text(description, body_style);
        },
    );
    builder.build()
}

// ── HUD ─────────────────────────────────────────────────────────────

fn build_hud_tree(taa: bool, width: f32) -> LayoutTree {
    let mut builder = LayoutBuilder::new(width, HUD_HEIGHT);
    build_hud_content(&mut builder, taa);
    builder.build()
}

fn build_hud_content(b: &mut LayoutBuilder, taa: bool) {
    let title = LayoutTextStyle::new(HUD_TITLE_SIZE).with_color(HUD_TITLE_COLOR);
    let hint = LayoutTextStyle::new(HUD_HINT_SIZE).with_color(HUD_LABEL_COLOR);

    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(2.0))
            .background(HUD_FRAME_BACKGROUND)
            .border(Border::all(2.0, HUD_BORDER_ACCENT)),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .direction(Direction::LeftToRight)
                    .padding(Padding::new(8.0, HUD_PADDING, 8.0, HUD_PADDING))
                    .child_gap(HUD_GAP)
                    .child_align_y(bevy_diegetic::AlignY::Center)
                    .clip()
                    .background(HUD_BACKGROUND)
                    .border(Border::all(1.0, HUD_BORDER_DIM)),
                |b| {
                    b.text("TAA SHIMMER", title);
                    hud_separator(b);

                    let taa_label = if taa { "TAA On" } else { "TAA Off" };
                    let taa_color = if taa {
                        HUD_ACTIVE_COLOR
                    } else {
                        HUD_INACTIVE_COLOR
                    };
                    b.text(
                        taa_label,
                        LayoutTextStyle::new(HUD_BODY_SIZE).with_color(taa_color),
                    );
                    hud_separator(b);

                    b.text("T Toggle TAA", hint);
                },
            );
        },
    );
}

fn hud_separator(b: &mut LayoutBuilder) {
    b.with(
        El::new()
            .width(Sizing::fixed(1.0))
            .height(Sizing::GROW)
            .background(HUD_DIVIDER_COLOR),
        |_| {},
    );
}

// ── Systems ─────────────────────────────────────────────────────────

fn toggle_taa(
    keyboard: Res<ButtonInput<KeyCode>>,
    cameras: Query<(Entity, Has<bevy::anti_alias::taa::TemporalAntiAliasing>), With<OrbitCam>>,
    mut taa_enabled: ResMut<TaaEnabled>,
    mut commands: Commands,
) {
    if !keyboard.just_pressed(KeyCode::KeyT) {
        return;
    }
    for (entity, has_taa) in &cameras {
        if has_taa {
            commands
                .entity(entity)
                .remove::<bevy::anti_alias::taa::TemporalAntiAliasing>();
            taa_enabled.0 = false;
        } else {
            commands
                .entity(entity)
                .insert(bevy::anti_alias::taa::TemporalAntiAliasing::default());
            taa_enabled.0 = true;
        }
    }
}

fn update_hud(
    taa: Res<TaaEnabled>,
    windows: Query<&Window>,
    mut huds: Query<(&mut Transform, &mut DiegeticPanel), With<HudPanel>>,
    mut previous_state: Local<(bool, u32)>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let win_width = window.width();
    let half_width = win_width / 2.0;
    let half_height = window.height() / 2.0;
    let state = (taa.0, win_width.to_bits());

    for (mut transform, mut panel) in &mut huds {
        transform.translation.x = -half_width;
        transform.translation.y = half_height;

        let width_changed = (panel.width() - win_width).abs() > 1.0;
        if width_changed {
            panel.set_width(win_width);
        }
        if *previous_state != state || width_changed {
            let panel_width = panel.width();
            panel.set_tree(build_hud_tree(taa.0, panel_width));
        }
    }
    *previous_state = state;
}
