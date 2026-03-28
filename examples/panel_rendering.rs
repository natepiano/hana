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
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::CornerRadius;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::Mm;
use bevy_diegetic::Padding;
use bevy_diegetic::Pt;
use bevy_diegetic::RenderMode;
use bevy_diegetic::Sizing;
use bevy_diegetic::Unit;
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_panorbit_camera::PanOrbitCameraPlugin;
use bevy_panorbit_camera::TrackpadBehavior;
use bevy_panorbit_camera_ext::FitVisualization;
use bevy_panorbit_camera_ext::PanOrbitCameraExtPlugin;
use bevy_panorbit_camera_ext::SetFitTarget;
use bevy_panorbit_camera_ext::ZoomToFit;

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

/// Marker for the scene's directional light.
#[derive(Component)]
struct SceneLight;

/// Marker for the HUD text overlay.
#[derive(Component)]
struct HudText;

/// Current lighting/material preset.
#[derive(Resource, Clone, Copy)]
struct LightingPreset(u8);

impl Default for LightingPreset {
    fn default() -> Self { Self(0) }
}

impl LightingPreset {
    const fn is_unlit(self) -> bool { self.0 == 1 || self.0 == 3 }
    const fn lights_on(self) -> bool { self.0 == 0 || self.0 == 1 }

    const fn label(self) -> &'static str {
        match self.0 {
            0 => "[1] Lit + Lights On",
            1 => "[2] Unlit + Lights On",
            2 => "[3] Lit + Lights Off",
            _ => "[4] Unlit + Lights Off",
        }
    }
}

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            PanOrbitCameraPlugin,
            PanOrbitCameraExtPlugin,
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
                toggle_fit_visualization,
                cycle_lighting_preset,
                home_camera,
            ),
        )
        .run();
}

fn zoom_to_panel(
    children: Query<(Entity, &ChildOf), With<Mesh3d>>,
    panels: Query<Entity, With<DiegeticPanel>>,
    cameras: Query<Entity, With<PanOrbitCamera>>,
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

fn toggle_fit_visualization(
    keyboard: Res<ButtonInput<KeyCode>>,
    cameras: Query<(Entity, Option<&FitVisualization>), With<PanOrbitCamera>>,
    mut commands: Commands,
) {
    if !keyboard.just_pressed(KeyCode::KeyD) {
        return;
    }
    for (camera, has_viz) in &cameras {
        if has_viz.is_some() {
            commands.entity(camera).remove::<FitVisualization>();
        } else {
            commands.entity(camera).insert(FitVisualization);
        }
    }
}

/// Cycles through lighting presets with keys 1-4.
fn cycle_lighting_preset(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut preset: ResMut<LightingPreset>,
    mut panels: Query<&mut DiegeticPanel>,
    mut lights: Query<&mut DirectionalLight, With<SceneLight>>,
    mut hud: Query<&mut Text, With<HudText>>,
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
    preset.0 = idx;

    let unlit = preset.is_unlit();
    let lights_visible = preset.lights_on();

    for mut panel in &mut panels {
        let mat = panel.material.get_or_insert_with(StandardMaterial::default);
        mat.unlit = unlit;
        let text_mat = panel
            .text_material
            .get_or_insert_with(StandardMaterial::default);
        text_mat.unlit = unlit;
    }

    // Set illuminance to 0 instead of hiding the light entity.
    // Hiding the entity can cause Bevy to skip rendering passes,
    // which breaks unlit materials that still need the pass to run.
    let default_illuminance = DirectionalLight::default().illuminance;
    for mut light in &mut lights {
        light.illuminance = if lights_visible {
            default_illuminance
        } else {
            0.0
        };
    }

    for mut text in &mut hud {
        **text = format!(
            "{}1: Lit+On  {}2: Unlit+On  {}3: Lit+Off  {}4: Unlit+Off  [H] Home  [D] Fit Viz",
            if idx == 0 { ">" } else { " " },
            if idx == 1 { ">" } else { " " },
            if idx == 2 { ">" } else { " " },
            if idx == 3 { ">" } else { " " },
        );
    }
}

/// Resets the camera to the home position and zooms to fit.
fn home_camera(
    keyboard: Res<ButtonInput<KeyCode>>,
    panels: Query<Entity, With<DiegeticPanel>>,
    cameras: Query<Entity, With<PanOrbitCamera>>,
    mut commands: Commands,
) {
    if !keyboard.just_pressed(KeyCode::KeyH) {
        return;
    }
    let Ok(panel) = panels.single() else { return };
    let Ok(camera) = cameras.single() else { return };
    commands.trigger(
        ZoomToFit::new(camera, panel)
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

fn setup(mut commands: Commands) {
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
                render_mode: RenderMode::Geometry,
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, 0.0),
        ))
        .observe(on_panel_clicked);

    // ── Lighting ────────────────────────────────────────────────────
    commands.spawn((
        SceneLight,
        DirectionalLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // ── HUD ─────────────────────────────────────────────────────────
    commands.spawn((
        HudText,
        Text::new(">1: Lit+On   2: Unlit+On   3: Lit+Off   4: Unlit+Off  [H] Home  [D] Fit Viz"),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgba(0.8, 0.8, 0.8, 0.7)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        },
    ));

    // ── Camera ──────────────────────────────────────────────────────
    commands.spawn((
        PanOrbitCamera {
            focus: Vec3::new(0.0, -0.02, 0.0),
            radius: Some(0.35),
            yaw: Some(0.0),
            pitch: Some(0.0),
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
    ));
}

// ── Panel builders ──────────────────────────────────────────────────

/// Single panel with three cards laid out side by side.
/// No panel-level background — the cards' own backgrounds make them
/// appear as three separate panels within one RTT texture.
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
                            .height(Sizing::grow_min(0.0)),
                        |b| {
                            b.with(
                                El::new()
                                    .background(BLUE_BG)
                                    .border(Border::all(Mm(0.3), BLUE_ACCENT))
                                    .padding(Padding::all(2.0))
                                    .width(Sizing::grow_min(0.0))
                                    .height(Sizing::grow_min(0.0)),
                                |b| {
                                    b.text("A", body_style.clone());
                                },
                            );
                            b.with(
                                El::new()
                                    .background(GREEN_BG)
                                    .border(Border::all(Mm(0.3), GREEN_ACCENT))
                                    .padding(Padding::all(2.0))
                                    .width(Sizing::grow_min(0.0))
                                    .height(Sizing::grow_min(0.0)),
                                |b| {
                                    b.text("B", body_style.clone());
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
