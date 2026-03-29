//! Panel geometry rendering test — backgrounds, borders, and text.
//!
//! Displays several diegetic panels with different background colors,
//! border configurations, and text content to verify that panel geometry
//! (rectangles and borders) renders correctly alongside MSDF text.

use std::time::Duration;

use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
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
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Mm;
use bevy_diegetic::Padding;
use bevy_diegetic::Pt;
use bevy_diegetic::Sizing;
use bevy_diegetic::Unit;
use bevy_diegetic::default_panel_material;
use bevy_lagrange::AnimateToFit;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::SetFitTarget;
use bevy_lagrange::TrackpadBehavior;
use bevy_lagrange::ZoomToFit;

// ── Colors ──────────────────────────────────────────────────────────
const DARK_BG: Color = Color::srgba(0.3, 0.3, 0.35, 1.0);
const BLUE_BG: Color = Color::srgba(0.12, 0.18, 0.28, 0.95);
const GREEN_BG: Color = Color::srgba(0.08, 0.20, 0.12, 0.95);
const RED_ACCENT: Color = Color::srgb(1.0, 0.0, 0.0);
const BLUE_ACCENT: Color = Color::srgb(0.0, 0.0, 1.0);
const GREEN_ACCENT: Color = Color::srgb(0.0, 1.0, 0.0);
const DIVIDER_COLOR: Color = Color::srgba(0.4, 0.4, 0.5, 0.6);
const TEXT_COLOR: Color = Color::WHITE;
const SUBTLE_TEXT: Color = Color::srgba(0.6, 0.6, 0.65, 0.9);
const BORDER_COLOR: Color = Color::srgba(0.5, 0.5, 0.6, 0.7);

// ── Layout ──────────────────────────────────────────────────────────
const CARD_WIDTH: f32 = 80.0; // mm — each card
const CARD_HEIGHT: f32 = 60.0; // mm
const CARD_PAD: f32 = 4.0; // mm
const CHILD_GAP: f32 = 2.0; // mm
const CARD_GAP: f32 = 6.0; // mm — gap between the three cards
const PANEL_WIDTH: f32 = CARD_WIDTH * 3.0 + CARD_GAP * 2.0; // total
const PANEL_HEIGHT: f32 = CARD_HEIGHT; // same height
const ZOOM_MARGIN: f32 = 0.02;
const ZOOM_DURATION_MS: u64 = 600;
/// Illuminance calibrated so a Lambertian surface facing the light
/// produces output ≈ albedo at the default Bevy exposure (EV100 = 9.7).
/// Formula: PI / (2^(-9.7) / 1.2) ≈ 3137.
const SCENE_ILLUMINANCE: f32 = 3137.0;

/// How much illuminance changes per frame while +/- is held.
const ILLUMINANCE_STEP: f32 = 50.0;

// ── HUD ────────────────────────────────────────────────────────────
const HUD_WIDTH: f32 = 1100.0;
const HUD_HEIGHT: f32 = 58.0;
const HUD_PADDING: f32 = 14.0;
const HUD_GAP: f32 = 16.0;
const HUD_TITLE_SIZE: f32 = 20.0;
const HUD_BODY_SIZE: f32 = 18.0;
const HUD_HINT_SIZE: f32 = 15.0;

const HUD_BACKGROUND: Color = Color::srgba(0.02, 0.03, 0.07, 0.92);
const HUD_FRAME_BACKGROUND: Color = Color::srgba(0.01, 0.01, 0.03, 0.95);
const HUD_BORDER_ACCENT: Color = Color::srgba(0.15, 0.7, 0.9, 0.5);
const HUD_BORDER_DIM: Color = Color::srgba(0.1, 0.4, 0.6, 0.3);
const HUD_TITLE_COLOR: Color = Color::srgb(0.9, 0.95, 1.0);
const HUD_LABEL_COLOR: Color = Color::srgba(0.5, 0.55, 0.7, 0.8);
const HUD_VALUE_COLOR: Color = Color::srgb(0.3, 0.9, 1.0);
const HUD_ACTIVE_COLOR: Color = Color::srgb(0.3, 1.0, 0.8);
const HUD_INACTIVE_COLOR: Color = Color::srgba(0.6, 0.65, 0.8, 0.85);
const HUD_DIVIDER_COLOR: Color = Color::srgba(0.15, 0.4, 0.6, 0.25);
const HUD_HINT_COLOR: Color = Color::srgba(0.55, 0.6, 0.75, 0.8);

// ── Home camera position ────────────────────────────────────────────
const HOME_FOCUS: Vec3 = Vec3::new(0.0, -0.02, 0.0);
const HOME_RADIUS: f32 = 0.35;
const HOME_YAW: f32 = 0.0;
const HOME_PITCH: f32 = 0.0;

/// Marker for the scene's directional light.
#[derive(Component)]
struct SceneLight;

/// Marker for the screen-space HUD panel.
#[derive(Component)]
struct HudPanel;

/// Current lighting/material preset and saved illuminance.
#[derive(Resource, Clone, Copy)]
struct LightingPreset {
    index:             u8,
    /// Illuminance to restore when switching back to lights-on.
    saved_illuminance: f32,
}

impl Default for LightingPreset {
    fn default() -> Self {
        Self {
            index:             0,
            saved_illuminance: SCENE_ILLUMINANCE,
        }
    }
}

impl LightingPreset {
    const fn is_unlit(self) -> bool { self.index == 2 || self.index == 3 }
    const fn lights_on(self) -> bool { self.index == 0 || self.index == 2 }

    const fn description(self) -> &'static str {
        match self.index {
            0 => "Lit + On",
            1 => "Lit + Off",
            2 => "Unlit + On",
            _ => "Unlit + Off",
        }
    }
}

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            LagrangePlugin,
            BrpExtrasPlugin::default().port_in_title(PortDisplay::NonDefault),
            MeshPickingPlugin,
            DiegeticUiPlugin,
        ))
        .init_resource::<LightingPreset>()
        .insert_resource(bevy::light::GlobalAmbientLight {
            color:                      Color::BLACK,
            brightness:                 0.0,
            affects_lightmapped_meshes: false,
        })
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                zoom_to_panel,
                cycle_lighting_preset,
                adjust_illuminance,
                home_camera,
                update_hud,
            ),
        )
        .run();
}

fn zoom_to_panel(
    children: Query<(Entity, &ChildOf), With<Mesh3d>>,
    panels: Query<Entity, (With<DiegeticPanel>, Without<HudPanel>)>,
    cameras: Query<Entity, With<OrbitCam>>,
    mut done: Local<bool>,
    mut commands: Commands,
) {
    if *done {
        return;
    }
    let Ok(panel) = panels.single() else { return };
    let Ok(camera) = cameras.single() else { return };

    // Wait for the display quad (a Mesh3d child of the panel) to exist.
    let has_mesh_child = children.iter().any(|(_, c)| c.parent() == panel);
    if !has_mesh_child {
        return;
    }

    *done = true;
    commands.trigger(SetFitTarget::new(camera, panel));
    commands.trigger(
        ZoomToFit::new(camera, panel)
            .margin(ZOOM_MARGIN)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );
}

/// Cycles through lighting presets with keys 1-4.
fn cycle_lighting_preset(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut preset: ResMut<LightingPreset>,
    mut panels: Query<&mut DiegeticPanel, Without<HudPanel>>,
    mut lights: Query<&mut DirectionalLight, With<SceneLight>>,
) {
    let new = if keyboard.just_pressed(KeyCode::Digit1) {
        Some(0)
    } else if keyboard.just_pressed(KeyCode::Digit2) {
        Some(1)
    } else if keyboard.just_pressed(KeyCode::Digit3) {
        Some(2)
    } else if keyboard.just_pressed(KeyCode::Digit4) {
        Some(3)
    } else {
        None
    };

    let Some(idx) = new else { return };

    // Save current illuminance before switching away from lights-on.
    if preset.lights_on()
        && let Some(light) = lights.iter().next()
    {
        preset.saved_illuminance = light.illuminance;
    }

    preset.index = idx;

    let unlit = preset.is_unlit();
    let lights_visible = preset.lights_on();

    for mut panel in &mut panels {
        let mat = panel.material.get_or_insert_with(default_panel_material);
        mat.unlit = unlit;
        let text_mat = panel
            .text_material
            .get_or_insert_with(default_panel_material);
        text_mat.unlit = unlit;
    }

    // Restore saved illuminance for lights-on, zero for lights-off.
    for mut light in &mut lights {
        light.illuminance = if lights_visible {
            preset.saved_illuminance
        } else {
            0.0
        };
    }
}

/// Animates the camera to the home viewing angle, framing the panel.
/// Adjusts scene illuminance with +/- keys (continuous while held).
/// [R] resets to the calibrated default.
fn adjust_illuminance(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut lights: Query<&mut DirectionalLight, With<SceneLight>>,
) {
    let up = keyboard.pressed(KeyCode::Equal) || keyboard.pressed(KeyCode::NumpadAdd);
    let down = keyboard.pressed(KeyCode::Minus) || keyboard.pressed(KeyCode::NumpadSubtract);
    let reset = keyboard.just_pressed(KeyCode::KeyR);

    if !up && !down && !reset {
        return;
    }

    for mut light in &mut lights {
        if reset {
            light.illuminance = SCENE_ILLUMINANCE;
        } else if up {
            light.illuminance += ILLUMINANCE_STEP;
        } else if down {
            light.illuminance = (light.illuminance - ILLUMINANCE_STEP).max(0.0);
        }
    }
}

fn home_camera(
    keyboard: Res<ButtonInput<KeyCode>>,
    panels: Query<Entity, (With<DiegeticPanel>, Without<HudPanel>)>,
    cameras: Query<Entity, With<OrbitCam>>,
    mut commands: Commands,
) {
    if !keyboard.just_pressed(KeyCode::KeyH) {
        return;
    }
    let Ok(panel) = panels.single() else { return };
    let Ok(camera) = cameras.single() else { return };
    commands.trigger(
        AnimateToFit::new(camera, panel)
            .yaw(HOME_YAW)
            .pitch(HOME_PITCH)
            .margin(ZOOM_MARGIN)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );
}

fn on_panel_clicked(mut click: On<Pointer<Click>>, mut commands: Commands) {
    if click.button != PointerButton::Primary {
        return;
    }
    click.propagate(false);
    let camera = click.hit.camera;
    commands.trigger(
        ZoomToFit::new(camera, click.entity)
            .margin(ZOOM_MARGIN)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );
}

fn setup(mut commands: Commands, windows: Query<&Window>) {
    // ── Single panel containing three cards ──────────────────────────
    let tree = build_unified_panel();
    commands
        .spawn((
            DiegeticPanel {
                tree,
                width: PANEL_WIDTH,
                height: PANEL_HEIGHT,
                layout_unit: Some(Unit::Millimeters),
                anchor: Anchor::TopCenter,
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, 0.0),
        ))
        .observe(on_panel_clicked);

    // ── Lighting ────────────────────────────────────────────────────
    // Illuminance calibrated so PBR output ≈ albedo for a Lambertian
    // surface facing the light (N·L=1). Higher values cause the
    // tonemapper to compress and desaturate colors.
    commands.spawn((
        SceneLight,
        DirectionalLight {
            shadows_enabled: true,
            illuminance: SCENE_ILLUMINANCE,
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // ── HUD ─────────────────────────────────────────────────────────
    let unlit_material = StandardMaterial {
        unlit: true,
        ..default_panel_material()
    };
    let hud_width = windows.iter().next().map_or(HUD_WIDTH, Window::width);
    let (mut hud_panel, screen_space) = DiegeticPanel::builder()
        .size_px(hud_width, HUD_HEIGHT)
        .anchor(Anchor::TopLeft)
        .material(unlit_material.clone())
        .text_material(unlit_material)
        .layout(|b| {
            build_hud_content(b, LightingPreset::default(), SCENE_ILLUMINANCE);
        })
        .build_screen_space();
    hud_panel.tree = build_hud_tree(LightingPreset::default(), SCENE_ILLUMINANCE, hud_width);
    commands.spawn((HudPanel, hud_panel, screen_space, Transform::default()));

    // ── Camera ──────────────────────────────────────────────────────
    commands.spawn((
        OrbitCam {
            focus: HOME_FOCUS,
            radius: Some(HOME_RADIUS),
            yaw: Some(HOME_YAW),
            pitch: Some(HOME_PITCH),
            button_orbit: MouseButton::Middle,
            button_pan: MouseButton::Middle,
            modifier_pan: Some(KeyCode::ShiftLeft),
            trackpad_behavior: TrackpadBehavior::BlenderLike {
                modifier_pan:  Some(KeyCode::ShiftLeft),
                modifier_zoom: Some(KeyCode::ControlLeft),
            },
            trackpad_sensitivity: 1.0,
            trackpad_pinch_to_zoom_enabled: true,
            zoom_sensitivity: 1.0,
            zoom_lower_limit: 0.000_000_1,
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            near: 0.001,
            near_clip_plane: Vec4::new(0.0, 0.0, -1.0, -0.001),
            ..default()
        }),
        bevy::camera::Exposure::default(),
    ));
}

// ── Panel builders ──────────────────────────────────────────────────

/// Single panel with three cards laid out side by side.
/// No panel-level background — the cards' own backgrounds make them
/// appear as three separate panels within one RTT texture.
#[allow(clippy::too_many_lines)]
fn build_unified_panel() -> bevy_diegetic::LayoutTree {
    let title_style = LayoutTextStyle::new(Pt(10.0)).with_color(TEXT_COLOR);
    let body_style = LayoutTextStyle::new(Pt(7.0)).with_color(SUBTLE_TEXT);

    let mut builder = LayoutBuilder::new(PANEL_WIDTH, PANEL_HEIGHT);
    builder.with(
        El::new()
            .direction(Direction::LeftToRight)
            .child_gap(CARD_GAP)
            .width(Sizing::grow_min(0.0))
            .height(Sizing::grow_min(0.0)),
        |b| {
            // ── Card 1: Backgrounds ─────────────────────────────
            b.with(
                El::new()
                    .direction(Direction::TopToBottom)
                    .padding(Padding::all(CARD_PAD))
                    .child_gap(CHILD_GAP)
                    .background(DARK_BG)
                    .corner_radius(CornerRadius::all(Mm(3.0)))
                    .width(Sizing::grow_min(0.0))
                    .height(Sizing::grow_min(0.0)),
                |b| {
                    b.text("Backgrounds", title_style.clone());
                    b.text("Nested elements with fills", body_style.clone());

                    b.with(
                        El::new()
                            .direction(Direction::LeftToRight)
                            .child_gap(CHILD_GAP)
                            .width(Sizing::grow_min(0.0))
                            .height(Sizing::grow_min(0.0)),
                        |b| {
                            b.with(
                                El::new()
                                    .background(RED_ACCENT)
                                    .corner_radius(CornerRadius::all(Mm(1.5)))
                                    .padding(Padding::all(3.0))
                                    .width(Sizing::grow_min(0.0))
                                    .height(Sizing::grow_min(0.0)),
                                |b| {
                                    b.text("Red", body_style.clone());
                                },
                            );
                            b.with(
                                El::new()
                                    .background(BLUE_ACCENT)
                                    .corner_radius(CornerRadius::all(Mm(1.5)))
                                    .padding(Padding::all(3.0))
                                    .width(Sizing::grow_min(0.0))
                                    .height(Sizing::grow_min(0.0)),
                                |b| {
                                    b.text("Blue", body_style.clone());
                                },
                            );
                            b.with(
                                El::new()
                                    .background(GREEN_ACCENT)
                                    .corner_radius(CornerRadius::all(Mm(1.5)))
                                    .padding(Padding::all(3.0))
                                    .width(Sizing::grow_min(0.0))
                                    .height(Sizing::grow_min(0.0)),
                                |b| {
                                    b.text("Green", body_style.clone());
                                },
                            );
                        },
                    );

                    b.with(
                        El::new()
                            .background(BLUE_BG)
                            .padding(Padding::all(3.0))
                            .width(Sizing::grow_min(0.0))
                            .height(Sizing::grow_min(0.0)),
                        |b| {
                            b.text("Nested background", body_style.clone());
                        },
                    );
                },
            );

            // ── Card 2: Borders ─────────────────────────────────
            b.with(
                El::new()
                    .direction(Direction::TopToBottom)
                    .padding(Padding::all(CARD_PAD))
                    .child_gap(CHILD_GAP)
                    .border(Border::all(Mm(0.5), BORDER_COLOR))
                    .width(Sizing::grow_min(0.0))
                    .height(Sizing::grow_min(0.0)),
                |b| {
                    b.text("Borders", title_style.clone());

                    b.with(
                        El::new()
                            .border(Border::all(Mm(0.3), BLUE_ACCENT))
                            .padding(Padding::all(2.0))
                            .width(Sizing::grow_min(0.0))
                            .height(Sizing::fit_min(0.0)),
                        |b| {
                            b.text("Thin blue border", body_style.clone());
                        },
                    );

                    b.with(
                        El::new()
                            .border(Border::all(Mm(1.0), RED_ACCENT))
                            .padding(Padding::all(2.0))
                            .width(Sizing::grow_min(0.0))
                            .height(Sizing::fit_min(0.0)),
                        |b| {
                            b.text("Thick red border", body_style.clone());
                        },
                    );

                    b.with(
                        El::new()
                            .direction(Direction::TopToBottom)
                            .child_gap(CHILD_GAP)
                            .border(Border::all(Mm(0.3), BORDER_COLOR).between_children(Mm(0.3)))
                            .padding(Padding::all(2.0))
                            .width(Sizing::grow_min(0.0))
                            .height(Sizing::grow_min(0.0)),
                        |b| {
                            b.text("Row A", body_style.clone());
                            b.text("Row B", body_style.clone());
                            b.text("Row C", body_style.clone());
                        },
                    );
                },
            );

            // ── Card 3: Combined ────────────────────────────────
            b.with(
                El::new()
                    .direction(Direction::TopToBottom)
                    .padding(Padding::all(CARD_PAD))
                    .child_gap(CHILD_GAP)
                    .background(DARK_BG)
                    .border(Border::all(Mm(0.5), BLUE_ACCENT))
                    .corner_radius(CornerRadius::all(Mm(3.0)))
                    .width(Sizing::grow_min(0.0))
                    .height(Sizing::grow_min(0.0)),
                |b| {
                    b.text("Combined", title_style.clone());

                    b.with(
                        El::new()
                            .background(GREEN_BG)
                            .border(Border::all(Mm(0.3), GREEN_ACCENT))
                            .padding(Padding::all(3.0))
                            .width(Sizing::grow_min(0.0))
                            .height(Sizing::fit_min(0.0)),
                        |b| {
                            b.text("Card with bg + border", body_style.clone());
                        },
                    );

                    b.with(
                        El::new()
                            .direction(Direction::LeftToRight)
                            .child_gap(CHILD_GAP)
                            .width(Sizing::grow_min(0.0))
                            .height(Sizing::fixed(12.0)),
                        |b| {
                            // Overflow visible — text spills out of the box.
                            b.with(
                                El::new()
                                    .background(BLUE_BG)
                                    .border(Border::all(Mm(0.3), BLUE_ACCENT))
                                    .padding(Padding::all(2.0))
                                    .width(Sizing::grow_min(0.0))
                                    .height(Sizing::grow_min(0.0)),
                                |b| {
                                    b.text("Overflow visible", body_style.clone());
                                    b.text("This text spills out", body_style.clone());
                                },
                            );
                            // Overflow clipped — text is hidden at the boundary.
                            b.with(
                                El::new()
                                    .clip()
                                    .background(GREEN_BG)
                                    .border(Border::all(Mm(0.3), GREEN_ACCENT))
                                    .padding(Padding::all(2.0))
                                    .width(Sizing::grow_min(0.0))
                                    .height(Sizing::grow_min(0.0)),
                                |b| {
                                    b.text("Overflow clipped", body_style.clone());
                                    b.text("This text is hidden", body_style.clone());
                                },
                            );
                        },
                    );

                    b.with(
                        El::new()
                            .direction(Direction::TopToBottom)
                            .child_gap(1.5)
                            .background(BLUE_BG)
                            .border(Border::all(Mm(0.3), DIVIDER_COLOR).between_children(Mm(0.2)))
                            .padding(Padding::all(2.0))
                            .width(Sizing::grow_min(0.0))
                            .height(Sizing::grow_min(0.0)),
                        |b| {
                            b.text("Item 1", body_style.clone());
                            b.text("Item 2", body_style.clone());
                            b.text("Item 3", body_style.clone());
                        },
                    );
                },
            );
        },
    );
    builder.build()
}

// ── HUD builders ───────────────────────────────────────────────────

fn build_hud_tree(preset: LightingPreset, lux: f32, width: f32) -> LayoutTree {
    let mut builder = LayoutBuilder::new(width, HUD_HEIGHT);
    build_hud_content(&mut builder, preset, lux);
    builder.build()
}

fn build_hud_content(b: &mut LayoutBuilder, preset: LightingPreset, lux: f32) {
    let title = LayoutTextStyle::new(HUD_TITLE_SIZE).with_color(HUD_TITLE_COLOR);
    let label = LayoutTextStyle::new(HUD_BODY_SIZE).with_color(HUD_LABEL_COLOR);
    let value = LayoutTextStyle::new(HUD_BODY_SIZE).with_color(HUD_VALUE_COLOR);
    let hint = LayoutTextStyle::new(HUD_HINT_SIZE).with_color(HUD_HINT_COLOR);

    // Outer frame — dark surround with bright accent border.
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(2.0))
            .background(HUD_FRAME_BACKGROUND)
            .border(Border::all(2.0, HUD_BORDER_ACCENT)),
        |b| {
            // Inner frame — single horizontal row.
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .direction(Direction::LeftToRight)
                    .padding(Padding::new(8.0, HUD_PADDING, 8.0, HUD_PADDING))
                    .child_gap(HUD_GAP)
                    .child_align_y(AlignY::Center)
                    .clip()
                    .background(HUD_BACKGROUND)
                    .border(Border::all(1.0, HUD_BORDER_DIM)),
                |b| {
                    b.text("PANEL CONTROL", title);
                    hud_separator(b);

                    b.text("PRESET", label.clone());
                    b.with(
                        El::new().width(Sizing::fixed(180.0)).height(Sizing::FIT),
                        |b| {
                            b.text(preset.description(), value.clone());
                        },
                    );
                    hud_separator(b);

                    b.text("LUX", label);
                    b.text(format!("{lux:.0}"), value);
                    hud_separator(b);

                    // Preset list.
                    for i in 0u8..4 {
                        let active = preset.index == i;
                        let marker = if active { "\u{25b8}" } else { " " };
                        let color = if active {
                            HUD_ACTIVE_COLOR
                        } else {
                            HUD_INACTIVE_COLOR
                        };
                        b.text(
                            format!("{marker}{}", i + 1),
                            LayoutTextStyle::new(HUD_BODY_SIZE).with_color(color),
                        );
                    }
                    hud_separator(b);

                    b.text("H Home   R Reset   \u{00b1} Light", hint);
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

fn update_hud(
    preset: Res<LightingPreset>,
    lights: Query<&DirectionalLight, With<SceneLight>>,
    windows: Query<&Window>,
    mut huds: Query<(&mut Transform, &mut DiegeticPanel), With<HudPanel>>,
    mut previous_state: Local<(u8, u32, u32)>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let lux = lights.iter().next().map_or(0.0, |l| l.illuminance);
    let win_width = window.width();
    let half_width = win_width / 2.0;
    let half_height = window.height() / 2.0;
    let state = (preset.index, lux.to_bits(), win_width.to_bits());

    for (mut transform, mut panel) in &mut huds {
        transform.translation.x = -half_width;
        transform.translation.y = half_height;

        let width_changed = (panel.width - win_width).abs() > 1.0;
        if width_changed {
            panel.width = win_width;
        }
        if *previous_state != state || width_changed {
            panel.tree = build_hud_tree(*preset, lux, panel.width);
        }
    }
    *previous_state = state;
}
