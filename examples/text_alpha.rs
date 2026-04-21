#![allow(
    clippy::expect_used,
    reason = "demo code; panic on invalid setup is acceptable"
)]

//! `text_alpha` — interactive walk-through of all `AlphaMode` variants for
//! text, paired with the three mutually-exclusive camera states
//! (MSAA / `StableTransparency` / Off).
//!
//! Scene launches in the library default: `AlphaMode::Blend` + `Msaa::Sample4`
//! on the camera. You may see color/ordering flicker on the coplanar
//! "GROUND" text as the camera moves — press `C` to cycle to
//! `StableTransparency` and watch it stabilize.
//!
//! Hotkeys:
//! - `H` — home the camera.
//! - `C` — cycle camera state: MSAA → `StableTransparency` → Off → MSAA.
//! - `1..7` — select the active `AlphaMode`: 1 Blend (default) · 2 Premultiplied · 3 Coverage · 4
//!   Add · 5 Multiply · 6 Mask · 7 Opaque.

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
const HUD_GAP: Px = Px(14.0);
const HUD_WIDTH: Px = Px(600.0);

const INFO_HEIGHT: Px = Px(720.0);
const INFO_HEADER_SIZE: Pt = Pt(14.0);
const INFO_BODY_SIZE: Pt = Pt(12.0);
const INFO_TITLE_SIZE: Pt = Pt(16.0);

const CAM_HELP_WIDTH: Px = Px(380.0);
const CAM_HELP_HEIGHT: Px = Px(200.0);
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

#[derive(Component)]
struct InfoPanel;

/// The three mutually-exclusive camera-pipeline states the example cycles
/// through. `StableTransparency` and MSAA cannot coexist, so they live on
/// the same cycle as a single choice.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum CameraState {
    Msaa,
    Stable,
    None,
}

impl CameraState {
    const fn next(self) -> Self {
        match self {
            Self::Msaa => Self::Stable,
            Self::Stable => Self::None,
            Self::None => Self::Msaa,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Msaa => "MSAA",
            Self::Stable => "Stable",
            Self::None => "None",
        }
    }
}

#[derive(Resource)]
struct ControlsState {
    alpha_mode:   AlphaMode,
    camera_state: CameraState,
}

impl Default for ControlsState {
    fn default() -> Self {
        Self {
            // Matches the library's `TextAlphaModeDefault::default()`.
            alpha_mode:   AlphaMode::Blend,
            // Launch with MSAA on; user presses C to see StableTransparency
            // fix the coplanar-text flicker on "GROUND".
            camera_state: CameraState::Msaa,
        }
    }
}

const ALPHA_MODES: [(AlphaMode, &str); 7] = [
    (AlphaMode::Blend, "Blend"),
    (AlphaMode::Premultiplied, "Premultiplied"),
    (AlphaMode::AlphaToCoverage, "Coverage"),
    (AlphaMode::Add, "Add"),
    (AlphaMode::Multiply, "Multiply"),
    (AlphaMode::Mask(0.5), "Mask"),
    (AlphaMode::Opaque, "Opaque"),
];

fn alpha_mode_label(mode: AlphaMode) -> &'static str {
    ALPHA_MODES
        .iter()
        .find(|(m, _)| std::mem::discriminant(m) == std::mem::discriminant(&mode))
        .map_or("?", |(_, l)| *l)
}

const fn alpha_mode_description(mode: AlphaMode) -> &'static str {
    match mode {
        AlphaMode::Blend => {
            "Classic alpha compositing. The MSDF shader writes fractional \
             alpha at glyph edges for smooth anti-aliasing; Blend uses that \
             value directly to composite each fragment with the background. \
             Interior glyphs render fully opaque when the color's alpha is \
             1.0; edge fragments blend smoothly.\n\n\
             Caveat: routes through the transparent queue. Coplanar text can \
             flicker as the camera angle changes — per-mesh back-to-front \
             sort flips at certain angles.\n\n\
             Recommended: camera state = Stable (press C). Routes \
             compositing through OIT and eliminates flicker. Tradeoff: \
             StableTransparency forces MSAA off."
        },
        AlphaMode::Premultiplied => {
            "Sibling of Blend; assumes RGB channels are pre-multiplied by \
             alpha. Per Bevy's docs, behaves like Blend at alphas near 1.0 \
             and like Add at alphas near 0.0. Avoids border/outline \
             artifacts that Blend can show on some textures.\n\n\
             On lit coplanar text, Premultiplied + StableTransparency can \
             settle on a darker, arguably more physically-correct color \
             where Blend flips between brighter and dimmer shades.\n\n\
             Same transparent-queue caveat as Blend. Recommended: camera \
             state = Stable. Worth comparing with Blend on your scene."
        },
        AlphaMode::AlphaToCoverage => {
            "Order-independent anti-aliasing via sub-pixel coverage. The \
             shader's fractional alpha becomes a sample-mask pattern; MSAA \
             smooths it into a perceived gradient. Bypasses the transparent \
             queue entirely — no depth-sort issues.\n\n\
             The only anti-aliased path that works with MSAA. Without MSAA, \
             degrades to Mask(0.5) and looks jagged.\n\n\
             Recommended: camera state = MSAA. Use this when other geometry \
             in the scene benefits from MSAA and you want to avoid OIT."
        },
        AlphaMode::Add => {
            "Additive blending — glyph color is added to whatever is behind \
             it. Great for neon, glow, and holographic effects over dark \
             backgrounds.\n\n\
             Caveat: even though addition is mathematically commutative, \
             Bevy still routes this through the transparent queue, and \
             depth-test interactions on coplanar fragments can flicker.\n\n\
             Recommended: camera state = Stable for stable compositing on \
             coplanar text."
        },
        AlphaMode::Multiply => {
            "Multiplicative blending — glyph color multiplies into the \
             background. Good for ink or tint effects over light \
             backgrounds; disappears on dark ones.\n\n\
             Same transparent-queue caveats as Add. Recommended: camera \
             state = Stable when multiple multiplied layers overlap."
        },
        AlphaMode::Mask(_) => {
            "Hard alpha test at the configured threshold (0.5 here). \
             Fragments above threshold render fully opaque; below threshold \
             are discarded. Bypasses the transparent queue — no sorting \
             issues, no MSAA dependency.\n\n\
             Caveat: the MSDF shader's smooth fractional-alpha edge is \
             thrown away — edges look jagged.\n\n\
             Recommended: use only for retro / pixel-art looks where \
             crisp thresholded edges are desired."
        },
        AlphaMode::Opaque => {
            "Disables alpha handling entirely. Each glyph renders as a \
             colored rectangle with no shape; the MSDF silhouette is lost.\n\n\
             Not useful for text. Included here only for completeness \
             — try it and you'll see colored squares instead of letters."
        },
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
        Transform::from_xyz(0.0, 0.001, 1.125)
            .with_rotation(Quat::from_rotation_x(-core::f32::consts::FRAC_PI_2)),
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

    // Bottom-right camera-help legend (static). Pinned to Blend so the
    // panel text stays legible regardless of the current alpha-mode default.
    commands.spawn((
        DiegeticPanel::screen()
            .size(CAM_HELP_WIDTH, CAM_HELP_HEIGHT)
            .anchor(Anchor::BottomRight)
            .text_alpha_mode(AlphaMode::Blend)
            .layout(build_camera_help)
            .build()
            .expect("valid camera help"),
        Transform::default(),
    ));

    // HUD and info panels are spawned by `apply_state_and_rebuild_hud` on
    // the first frame (via `Res::is_changed()` on initial resource insert).
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

    if keyboard.just_pressed(KeyCode::KeyC) {
        state.camera_state = state.camera_state.next();
    }

    // Digit1..Digit7 → ALPHA_MODES[0..7].
    let digits = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
    ];
    for (i, key) in digits.iter().enumerate() {
        if keyboard.just_pressed(*key) {
            state.alpha_mode = ALPHA_MODES[i].0;
            break;
        }
    }
}

fn apply_state_and_rebuild_hud(
    state: Res<ControlsState>,
    mut commands: Commands,
    mut alpha_default: ResMut<TextAlphaModeDefault>,
    cam: Query<(Entity, Option<&StableTransparency>), With<SceneCamera>>,
    all_cameras: Query<Entity, With<Camera3d>>,
    hud_panels: Query<Entity, With<HudPanel>>,
    info_panels: Query<Entity, With<InfoPanel>>,
) {
    if !state.is_changed() {
        return;
    }

    // Apply camera state. The three cases are mutually exclusive — OIT and
    // MSAA cannot coexist on the same camera.
    match state.camera_state {
        CameraState::Msaa => {
            for e in &all_cameras {
                commands.entity(e).insert(Msaa::Sample4);
            }
            if let Ok((entity, marker)) = cam.single()
                && marker.is_some()
            {
                commands.entity(entity).remove::<StableTransparency>();
            }
        },
        CameraState::Stable => {
            if let Ok((entity, marker)) = cam.single()
                && marker.is_none()
            {
                commands.entity(entity).insert(StableTransparency);
            }
            // StableTransparency's observer propagates Msaa::Off.
        },
        CameraState::None => {
            for e in &all_cameras {
                commands.entity(e).insert(Msaa::Off);
            }
            if let Ok((entity, marker)) = cam.single()
                && marker.is_some()
            {
                commands.entity(entity).remove::<StableTransparency>();
            }
        },
    }

    alpha_default.0 = state.alpha_mode;

    for e in &hud_panels {
        commands.entity(e).despawn();
    }
    for e in &info_panels {
        commands.entity(e).despawn();
    }
    spawn_hud_panel(&mut commands, &state);
    spawn_info_panel(&mut commands, &state);
}

fn spawn_hud_panel(commands: &mut Commands, state: &ControlsState) {
    let camera_state = state.camera_state;
    commands.spawn((
        HudPanel,
        DiegeticPanel::screen()
            .size(HUD_WIDTH, HUD_HEIGHT)
            .anchor(Anchor::TopLeft)
            .text_alpha_mode(AlphaMode::Blend)
            .layout(move |b| build_controls(b, camera_state))
            .build()
            .expect("valid HUD"),
        Transform::default(),
    ));
}

fn spawn_info_panel(commands: &mut Commands, state: &ControlsState) {
    let mode = state.alpha_mode;
    commands.spawn((
        InfoPanel,
        DiegeticPanel::screen()
            .size(Px(0.0), INFO_HEIGHT)
            .anchor(Anchor::TopRight)
            .width_percent(0.22)
            .text_alpha_mode(AlphaMode::Blend)
            .layout(move |b| build_info_panel(b, mode))
            .build()
            .expect("valid info panel"),
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

fn build_controls(b: &mut LayoutBuilder, camera_state: CameraState) {
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
                    hud_divider(b);
                    b.text("H home", hud_text_style(false));
                    hud_divider(b);
                    b.text("C camera:", hud_text_style(false));
                    for opt in [CameraState::Msaa, CameraState::Stable, CameraState::None] {
                        b.text(opt.label(), hud_text_style(opt == camera_state));
                    }
                },
            );
        },
    );
}

fn hud_divider(b: &mut LayoutBuilder) {
    b.with(
        El::new()
            .width(Sizing::fixed(Px(1.0)))
            .height(Sizing::fixed(Px(20.0)))
            .background(HUD_DIVIDER_COLOR),
        |_| {},
    );
}

fn build_info_panel(b: &mut LayoutBuilder, active: AlphaMode) {
    let title = LayoutTextStyle::new(INFO_TITLE_SIZE).with_color(HUD_TITLE_COLOR);
    let active_header = LayoutTextStyle::new(INFO_HEADER_SIZE).with_color(HUD_ACTIVE_COLOR);
    let body = LayoutTextStyle::new(INFO_BODY_SIZE).with_color(HUD_INACTIVE_COLOR);

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
                    .child_gap(Px(8.0))
                    .background(HUD_BACKGROUND)
                    .border(Border::all(Px(1.0), HUD_BORDER_DIM)),
                |b| {
                    // Title.
                    b.with(El::new().width(Sizing::GROW), |b| {
                        b.text("ALPHA MODES", title);
                    });
                    // Vertical list of modes 1..7; active is highlighted.
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .direction(Direction::TopToBottom)
                            .child_gap(Px(2.0)),
                        |b| {
                            for (idx, (mode, label)) in ALPHA_MODES.iter().enumerate() {
                                let is_active =
                                    std::mem::discriminant(mode) == std::mem::discriminant(&active);
                                let chip_style = hud_text_style(is_active);
                                b.with(El::new().width(Sizing::GROW), |b| {
                                    b.text(format!("{} {}", idx + 1, label), chip_style);
                                });
                            }
                        },
                    );
                    // Divider.
                    b.with(
                        El::new()
                            .width(Sizing::GROW)
                            .height(Sizing::fixed(Px(1.0)))
                            .background(HUD_DIVIDER_COLOR),
                        |_| {},
                    );
                    // Active mode name.
                    b.with(El::new().width(Sizing::GROW), |b| {
                        b.text(alpha_mode_label(active), active_header);
                    });
                    // Description paragraph.
                    b.with(El::new().width(Sizing::GROW), |b| {
                        b.text(alpha_mode_description(active), body);
                    });
                },
            );
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
