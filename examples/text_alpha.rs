#![allow(
    clippy::expect_used,
    reason = "demo code; panic on invalid setup is acceptable"
)]

//! `text_alpha` — interactive demo of the two text transparency paths.
//!
//! Default: `AlphaToCoverage` (order-independent, needs MSAA). Press `T` to
//! add `StableTransparency` to the camera (inserts OIT + `Msaa::Off`); pair
//! with `AlphaMode::Blend` (key `2`) to see the Blend-compositing path.
//!
//! Hotkeys:
//! - `H` — home the camera.
//! - `M` — toggle MSAA (Sample4 ↔ Off). With A2C, MSAA off shows hard alpha.
//! - `T` — toggle `StableTransparency` on the camera.
//! - `1`..`5` — cycle `TextAlphaModeDefault`:
//!   Coverage / Blend / Mask(0.5) / Add / Multiply.

use std::time::Duration;

use bevy::camera::Camera3d;
use bevy::prelude::*;
use bevy::render::view::Msaa;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_brp_extras::PortDisplay;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::CornerRadius;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::GlyphSidedness;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::Padding;
use bevy_diegetic::Pt;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;
use bevy_diegetic::StableTransparency;
use bevy_diegetic::TextAlphaModeDefault;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_lagrange::CameraMove;
use bevy_lagrange::InputControl;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::PlayAnimation;
use bevy_lagrange::TrackpadBehavior;
use bevy_lagrange::TrackpadInput;
use bevy_window_manager::WindowManagerPlugin;

const HOME_FOCUS: Vec3 = Vec3::new(0.0, 0.3, 0.6);
const HOME_RADIUS: f32 = 3.8;
const HOME_YAW: f32 = 0.3;
const HOME_PITCH: f32 = 0.80;
const HOME_MS: u64 = 600;

const HUD_HEIGHT: Px = Px(44.0);
const HUD_PADDING: Px = Px(10.0);
const HUD_GAP: Px = Px(12.0);
const HUD_WIDTH: Px = Px(880.0);

const INFO_HEIGHT: Px = Px(440.0);
const INFO_HEADER_SIZE: Pt = Pt(13.0);
const INFO_BODY_SIZE: Pt = Pt(12.0);
const INFO_TITLE_SIZE: Pt = Pt(16.0);

const CAM_HELP_WIDTH: Px = Px(280.0);
const CAM_HELP_HEIGHT: Px = Px(160.0);
const CAM_HELP_LABEL_SIZE: Pt = Pt(12.0);
const CAM_HELP_HEADER_SIZE: Pt = Pt(13.0);
const CAM_HELP_TITLE_SIZE: Pt = Pt(16.0);
const CAM_HELP_RADIUS: Px = Px(15.0);
const CAM_HELP_FRAME_PAD: Px = Px(2.0);
const CAM_HELP_BORDER: Px = Px(2.0);
const CAM_HELP_INSET: Px = Px(CAM_HELP_FRAME_PAD.0 + CAM_HELP_BORDER.0);
const CAM_HELP_INNER_RADIUS: Px = Px(CAM_HELP_RADIUS.0 - CAM_HELP_INSET.0);

const HUD_TITLE_SIZE: Pt = Pt(14.0);
const HUD_HINT_SIZE: Pt = Pt(12.0);
const HUD_FRAME_BACKGROUND: Color = Color::srgba(0.01, 0.01, 0.03, 0.95);
const HUD_BACKGROUND: Color = Color::srgba(0.02, 0.03, 0.07, 0.80);
const HUD_BORDER_ACCENT: Color = Color::srgba(0.15, 0.7, 0.9, 0.5);
const HUD_BORDER_DIM: Color = Color::srgba(0.1, 0.4, 0.6, 0.3);
const HUD_TITLE_COLOR: Color = Color::srgb(0.9, 0.95, 1.0);
const HUD_DIVIDER_COLOR: Color = Color::srgba(0.15, 0.4, 0.6, 0.25);
const HUD_INACTIVE_COLOR: Color = Color::srgba(0.55, 0.60, 0.75, 0.85);
const HUD_ACTIVE_COLOR: Color = Color::srgb(0.3, 1.0, 0.8);

#[derive(Component)]
struct SceneCamera;

#[derive(Component)]
struct HudPanel;

#[derive(Resource)]
struct ControlsState {
    msaa_on:                bool,
    stable_transparency_on: bool,
    alpha_mode:             AlphaMode,
}

impl Default for ControlsState {
    fn default() -> Self {
        Self {
            msaa_on:                true,
            stable_transparency_on: false,
            alpha_mode:             AlphaMode::AlphaToCoverage,
        }
    }
}

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            DiegeticUiPlugin,
            LagrangePlugin,
            BrpExtrasPlugin::default().port_in_title(PortDisplay::NonDefault),
            WindowManagerPlugin,
        ))
        .init_resource::<ControlsState>()
        .add_systems(Startup, setup)
        .add_systems(Update, (handle_hotkeys, apply_state_and_rebuild_hud))
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Ground: translucent plane.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(3.5, 3.5))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.10, 0.10, 0.12, 0.55),
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
    ));

    // Cube with WorldText on its front face.
    commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::default())),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.8, 0.7, 0.6),
                ..default()
            })),
            Transform::from_xyz(0.0, 0.51, 0.0),
        ))
        .with_children(|parent| {
            let style = WorldTextStyle::new(0.22)
                .with_color(Color::srgb(0.9, 0.3, 0.1))
                .with_sidedness(GlyphSidedness::OneSided);
            parent.spawn((
                WorldText::new("HELLO"),
                style,
                Transform::from_xyz(0.0, 0.0, 0.501),
            ));
        });

    // WorldText floating on the ground (coplanar reproducer).
    commands.spawn((
        WorldText::new("GROUND"),
        WorldTextStyle::new(0.45).with_color(Color::srgb(1.0, 0.85, 0.1)),
        Transform::from_xyz(0.0, 0.001, 1.125).with_rotation(Quat::from_rotation_x(
            -core::f32::consts::FRAC_PI_2,
        )),
    ));

    // Lighting.
    commands.insert_resource(GlobalAmbientLight {
        color:                      Color::WHITE,
        brightness:                 400.0,
        affects_lightmapped_meshes: true,
    });
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Scene camera.
    commands.spawn((orbit_cam_home(), Msaa::Sample4, SceneCamera));

    // Bottom-right camera-help legend (static).
    commands.spawn((
        DiegeticPanel::screen()
            .size(CAM_HELP_WIDTH, CAM_HELP_HEIGHT)
            .anchor(Anchor::BottomRight)
            .layout(build_camera_help)
            .build()
            .expect("valid camera help"),
        Transform::default(),
    ));

    // Top-right info panel: explains each alpha mode (25% screen width).
    commands.spawn((
        DiegeticPanel::screen()
            .size(Px(0.0), INFO_HEIGHT)
            .anchor(Anchor::TopRight)
            .width_percent(0.25)
            .layout(build_info_panel)
            .build()
            .expect("valid info panel"),
        Transform::default(),
    ));
}

fn orbit_cam_home() -> OrbitCam {
    OrbitCam {
        focus: HOME_FOCUS,
        radius: Some(HOME_RADIUS),
        yaw: Some(HOME_YAW),
        pitch: Some(HOME_PITCH),
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
        ..default()
    }
}

fn handle_hotkeys(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<ControlsState>,
    mut commands: Commands,
    cam: Query<Entity, With<SceneCamera>>,
) {
    if keyboard.just_pressed(KeyCode::KeyH)
        && let Ok(cam) = cam.single()
    {
        commands.trigger(PlayAnimation::new(
            cam,
            [CameraMove::ToOrbit {
                focus:    HOME_FOCUS,
                yaw:      HOME_YAW,
                pitch:    HOME_PITCH,
                radius:   HOME_RADIUS,
                duration: Duration::from_millis(HOME_MS),
                easing:   bevy::math::curve::easing::EaseFunction::CubicOut,
            }],
        ));
    }

    if keyboard.just_pressed(KeyCode::KeyM) {
        state.msaa_on = !state.msaa_on;
    }
    if keyboard.just_pressed(KeyCode::KeyT) {
        state.stable_transparency_on = !state.stable_transparency_on;
    }
    if keyboard.just_pressed(KeyCode::Digit1) {
        state.alpha_mode = AlphaMode::AlphaToCoverage;
    } else if keyboard.just_pressed(KeyCode::Digit2) {
        state.alpha_mode = AlphaMode::Blend;
    } else if keyboard.just_pressed(KeyCode::Digit3) {
        state.alpha_mode = AlphaMode::Mask(0.5);
    } else if keyboard.just_pressed(KeyCode::Digit4) {
        state.alpha_mode = AlphaMode::Add;
    } else if keyboard.just_pressed(KeyCode::Digit5) {
        state.alpha_mode = AlphaMode::Multiply;
    }
}

fn apply_state_and_rebuild_hud(
    state: Res<ControlsState>,
    mut commands: Commands,
    mut alpha_default: ResMut<TextAlphaModeDefault>,
    cam: Query<(Entity, Option<&StableTransparency>), With<SceneCamera>>,
    all_cameras: Query<Entity, With<Camera3d>>,
    panels: Query<Entity, With<HudPanel>>,
) {
    if !state.is_changed() {
        return;
    }

    // Insert Msaa on ALL Camera3d entities so scene + HUD overlays stay in
    // sync; mismatched sample counts on the same window stall rendering.
    let msaa = if state.msaa_on {
        Msaa::Sample4
    } else {
        Msaa::Off
    };
    for e in &all_cameras {
        commands.entity(e).insert(msaa);
    }
    if let Ok((entity, marker)) = cam.single() {
        match (state.stable_transparency_on, marker.is_some()) {
            (true, false) => {
                commands.entity(entity).insert(StableTransparency);
            },
            (false, true) => {
                commands.entity(entity).remove::<StableTransparency>();
            },
            _ => {},
        }
    }

    alpha_default.0 = state.alpha_mode;

    for e in &panels {
        commands.entity(e).despawn();
    }
    spawn_hud_panel(&mut commands, &state);
}

fn spawn_hud_panel(commands: &mut Commands, state: &ControlsState) {
    let msaa_on = state.msaa_on;
    let stable_on = state.stable_transparency_on;
    let mode = state.alpha_mode;

    commands.spawn((
        HudPanel,
        DiegeticPanel::screen()
            .size(HUD_WIDTH, HUD_HEIGHT)
            .anchor(Anchor::TopLeft)
            .layout(move |b| build_controls(b, msaa_on, stable_on, mode))
            .build()
            .expect("valid HUD"),
        Transform::default(),
    ));
}

fn hud_text_style(active: bool) -> LayoutTextStyle {
    LayoutTextStyle::new(HUD_HINT_SIZE).with_color(if active {
        HUD_ACTIVE_COLOR
    } else {
        HUD_INACTIVE_COLOR
    })
}

fn build_controls(b: &mut LayoutBuilder, msaa_on: bool, stable_on: bool, mode: AlphaMode) {
    let title = LayoutTextStyle::new(HUD_TITLE_SIZE).with_color(HUD_TITLE_COLOR);

    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(Px(2.0)))
            .background(HUD_FRAME_BACKGROUND)
            .border(Border::all(Px(2.0), HUD_BORDER_ACCENT)),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .direction(Direction::LeftToRight)
                    .padding(Padding::new(Px(8.0), HUD_PADDING, Px(8.0), HUD_PADDING))
                    .child_gap(HUD_GAP)
                    .child_align_y(AlignY::Center)
                    .clip()
                    .background(HUD_BACKGROUND)
                    .border(Border::all(Px(1.0), HUD_BORDER_DIM)),
                |b| {
                    b.text("CONTROLS", title);
                    hud_top_divider(b);
                    b.text("H home", hud_text_style(false));
                    b.text("M MSAA", hud_text_style(msaa_on));
                    b.text("T StableTransparency", hud_text_style(stable_on));
                    hud_top_divider(b);
                    b.text(
                        "1 Coverage",
                        hud_text_style(matches!(mode, AlphaMode::AlphaToCoverage)),
                    );
                    b.text(
                        "2 Blend",
                        hud_text_style(matches!(mode, AlphaMode::Blend)),
                    );
                    b.text(
                        "3 Mask",
                        hud_text_style(matches!(mode, AlphaMode::Mask(_))),
                    );
                    b.text(
                        "4 Add",
                        hud_text_style(matches!(mode, AlphaMode::Add)),
                    );
                    b.text(
                        "5 Multiply",
                        hud_text_style(matches!(mode, AlphaMode::Multiply)),
                    );
                },
            );
        },
    );
}

fn hud_top_divider(b: &mut LayoutBuilder) {
    b.with(
        El::new()
            .width(Sizing::fixed(Px(1.0)))
            .height(Sizing::fixed(Px(20.0)))
            .background(HUD_DIVIDER_COLOR),
        |_| {},
    );
}

fn build_info_panel(b: &mut LayoutBuilder) {
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(Px(2.0)))
            .background(HUD_FRAME_BACKGROUND)
            .border(Border::all(Px(2.0), HUD_BORDER_ACCENT)),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .direction(Direction::TopToBottom)
                    .padding(Padding::all(Px(10.0)))
                    .child_gap(Px(4.0))
                    .background(HUD_BACKGROUND)
                    .border(Border::all(Px(1.0), HUD_BORDER_DIM)),
                |b| {
                    info_title(b);
                    info_section(
                        b,
                        "Coverage \u{2014} default",
                        "MSAA maps alpha to a coverage mask. Smooth edges, \
                         order-independent. Needs MSAA on the camera (M).",
                    );
                    info_section(
                        b,
                        "Blend",
                        "Alpha compositing. Flickers on camera angle. Add \
                         StableTransparency (T) to route through OIT and \
                         stabilize the order.",
                    );
                    info_section(
                        b,
                        "Mask",
                        "Hard alpha test. No MSAA / ordering needs, but no \
                         anti-aliasing \u{2014} diagonals look jagged.",
                    );
                    info_section(
                        b,
                        "Add",
                        "Additive. Order-independent. Good for neon / glow / \
                         holographic text over dark backgrounds.",
                    );
                    info_section(
                        b,
                        "Multiply",
                        "Multiplicative. Order-independent. Good for ink / tint \
                         over light backgrounds; disappears on dark ones.",
                    );
                },
            );
        },
    );
}

fn info_title(b: &mut LayoutBuilder) {
    let title = LayoutTextStyle::new(INFO_TITLE_SIZE).with_color(HUD_TITLE_COLOR);
    b.with(El::new().width(Sizing::GROW), |b| {
        b.text("ALPHA MODES", title);
    });
}

fn info_section(b: &mut LayoutBuilder, header_text: &str, body_text: &str) {
    let header = LayoutTextStyle::new(INFO_HEADER_SIZE).with_color(HUD_ACTIVE_COLOR);
    let body = LayoutTextStyle::new(INFO_BODY_SIZE).with_color(HUD_INACTIVE_COLOR);

    // Header row.
    b.with(El::new().width(Sizing::GROW), |b| {
        b.text(header_text, header);
    });
    // Body: single GROW-width El with left padding for indent. Text is a
    // direct child — matches the working `debug_text` pattern in units.rs.
    b.with(
        El::new()
            .width(Sizing::GROW)
            .padding(Padding::new(Px(12.0), Px(0.0), Px(2.0), Px(4.0))),
        |b| {
            b.text(body_text, body);
        },
    );
}

fn build_camera_help(b: &mut LayoutBuilder) {
    let title = LayoutTextStyle::new(CAM_HELP_TITLE_SIZE).with_color(HUD_TITLE_COLOR);
    let header = LayoutTextStyle::new(CAM_HELP_HEADER_SIZE).with_color(HUD_ACTIVE_COLOR);
    let label = LayoutTextStyle::new(CAM_HELP_LABEL_SIZE).with_color(HUD_INACTIVE_COLOR);

    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(CAM_HELP_FRAME_PAD))
            .corner_radius(CornerRadius::new(
                CAM_HELP_RADIUS,
                Px(0.0),
                CAM_HELP_RADIUS,
                Px(0.0),
            ))
            .background(HUD_FRAME_BACKGROUND)
            .border(Border::all(CAM_HELP_BORDER, HUD_BORDER_ACCENT)),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .direction(Direction::TopToBottom)
                    .padding(Padding::all(Px(10.0)))
                    .child_gap(Px(6.0))
                    .corner_radius(CornerRadius::new(
                        CAM_HELP_INNER_RADIUS,
                        Px(0.0),
                        CAM_HELP_INNER_RADIUS,
                        Px(0.0),
                    ))
                    .background(HUD_BACKGROUND)
                    .border(Border::all(Px(1.0), HUD_BORDER_DIM)),
                |b| {
                    b.text("CAMERA", title);
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::GROW)
                            .direction(Direction::LeftToRight)
                            .child_gap(Px(12.0)),
                        |b| {
                            b.with(
                                El::new()
                                    .width(Sizing::GROW)
                                    .direction(Direction::TopToBottom)
                                    .child_gap(Px(4.0)),
                                |b| {
                                    b.text("Mouse", header.clone());
                                    b.text("MMB drag \u{2192} Orbit", label.clone());
                                    b.text("Shift+MMB \u{2192} Pan", label.clone());
                                    b.text("Scroll \u{2192} Zoom", label.clone());
                                },
                            );
                            b.with(
                                El::new()
                                    .width(Sizing::fixed(Px(1.0)))
                                    .height(Sizing::GROW)
                                    .background(HUD_DIVIDER_COLOR),
                                |_| {},
                            );
                            b.with(
                                El::new()
                                    .width(Sizing::GROW)
                                    .direction(Direction::TopToBottom)
                                    .child_gap(Px(4.0)),
                                |b| {
                                    b.text("Trackpad", header.clone());
                                    b.text("Scroll \u{2192} Orbit", label.clone());
                                    b.text("Shift+Scroll \u{2192} Pan", label.clone());
                                    b.text("Ctrl+Scroll \u{2192} Zoom", label.clone());
                                    b.text("Pinch \u{2192} Zoom", label);
                                },
                            );
                        },
                    );
                },
            );
        },
    );
}
