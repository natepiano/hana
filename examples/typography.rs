#![allow(
    clippy::expect_used,
    reason = "demo code; panic on invalid setup is acceptable"
)]

//! Typography overlay demo — visualizes font-level metric lines and
//! per-glyph bounding boxes on a `WorldText` entity using the library's
//! built-in `TypographyOverlay` debug component.
//!
//! Requires the `typography_overlay` feature:
//! ```sh
//! cargo run --example typography --features typography_overlay
//! ```

use std::time::Duration;

use bevy::light::CascadeShadowConfigBuilder;
use bevy::light::DirectionalLightShadowMap;
use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_brp_extras::PortDisplay;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Border;
use bevy_diegetic::CornerRadius;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::Font;
use bevy_diegetic::FontId;
use bevy_diegetic::FontRegistered;
use bevy_diegetic::FontRegistry;
use bevy_diegetic::GlyphLoadingPolicy;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::OverlayBoundingBox;
use bevy_diegetic::Padding;
use bevy_diegetic::Pt;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;
use bevy_diegetic::SurfaceShadow;
use bevy_diegetic::TypographyOverlay;
use bevy_diegetic::TypographyOverlayReady;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_lagrange::AnimateToFit;
use bevy_lagrange::InputControl;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TrackpadBehavior;
use bevy_lagrange::TrackpadInput;
use bevy_lagrange::ZoomToFit;
use bevy_window_manager::WindowManagerPlugin;

const DISPLAY_SIZE: f32 = 0.48;
const DISPLAY_Y: f32 = 0.5;
const DISPLAY_Z: f32 = 2.0;
const ZOOM_TO_FIT_MARGIN: f32 = 0.05;
const ZOOM_DURATION_MS: u64 = 1000;
const KEY_LIGHT_LUX: f32 = 15_000.0;
const KEY_LIGHT_POS: Vec3 = Vec3::new(0.0, 2.5, 6.0);
const KEY_LIGHT_SHADOW_MAP_SIZE: usize = 4096;
const KEY_LIGHT_SHADOW_MAX_DISTANCE: f32 = 16.0;
const KEY_LIGHT_FIRST_CASCADE_FAR_BOUND: f32 = 4.0;
const KEY_LIGHT_SHADOW_MIN_DISTANCE: f32 = 0.2;
const KEY_LIGHT_SHADOW_DEPTH_BIAS: f32 = 0.01;
const KEY_LIGHT_SHADOW_NORMAL_BIAS: f32 = 0.6;
const REFLECTION_LIGHT_LEVEL: f32 = 150_000.0;
const REFLECTION_LIGHT_POS: Vec3 = Vec3::new(0.7, 1.9, 6.2);
const REFLECTION_TARGET: Vec3 = Vec3::new(0.15, 0.0, 1.7);

const GROUND_WIDTH: f32 = 5.4;
const GROUND_FRONT_MARGIN: f32 = 0.7;
const GROUND_BACK_MARGIN: f32 = 2.35;

const HOME_FOCUS: Vec3 = Vec3::new(-0.001, 0.461, 2.002);
const HOME_RADIUS: f32 = 2.84;
const HOME_YAW: f32 = 0.0;
const HOME_PITCH: f32 = 0.055;

const HUD_HEIGHT: Px = Px(48.0);
const CONTROLS_WIDTH: Px = Px(620.0);
const HUD_PADDING: Px = Px(12.0);
const HUD_GAP: Px = Px(14.0);
const HUD_TITLE_SIZE: Pt = Pt(16.0);
const HUD_HINT_SIZE: Pt = Pt(12.0);
const HUD_BACKGROUND: Color = Color::srgba(0.02, 0.03, 0.07, 0.80);
const HUD_FRAME_BACKGROUND: Color = Color::srgba(0.01, 0.01, 0.03, 0.95);
const HUD_BORDER_ACCENT: Color = Color::srgba(0.15, 0.7, 0.9, 0.5);
const HUD_BORDER_DIM: Color = Color::srgba(0.1, 0.4, 0.6, 0.3);
const HUD_TITLE_COLOR: Color = Color::srgb(0.9, 0.95, 1.0);
const HUD_ACTIVE_COLOR: Color = Color::srgb(0.3, 1.0, 0.8);
const HUD_CAMERA_HEADER_COLOR: Color = Color::srgb(1.0, 0.82, 0.52);
const HUD_DIVIDER_COLOR: Color = Color::srgba(0.15, 0.4, 0.6, 0.25);
const HUD_INACTIVE_COLOR: Color = Color::srgba(0.6, 0.65, 0.8, 0.85);

const CAM_HELP_WIDTH: Px = Px(280.0);
const CAM_HELP_HEIGHT: Px = Px(160.0);
const CAM_HELP_LABEL_SIZE: Pt = Pt(11.0);
const CAM_HELP_HEADER_SIZE: Pt = Pt(13.0);
const CAM_HELP_TITLE_SIZE: Pt = Pt(16.0);
const CAM_HELP_RADIUS: Px = Px(15.0);
const CAM_HELP_FRAME_PAD: Px = Px(2.0);
const CAM_HELP_BORDER: Px = Px(2.0);
const CAM_HELP_INSET: Px = Px(CAM_HELP_FRAME_PAD.0 + CAM_HELP_BORDER.0);
const CAM_HELP_INNER_RADIUS: Px = Px(CAM_HELP_RADIUS.0 - CAM_HELP_INSET.0);

const FONTS_PANEL_WIDTH: Px = CAM_HELP_WIDTH;
const FONTS_PANEL_HEIGHT: Px = Px(208.0);
const FONTS_PANEL_GAP: Px = Px(10.0);
const FONTS_PANEL_ROW_HEIGHT: Px = Px(24.0);
const FONTS_PANEL_KEY_WIDTH: Px = Px(18.0);
const FONTS_KEY_SIZE: Pt = Pt(12.0);
const FONTS_SAMPLE_SIZE: Pt = Pt(15.0);

/// Font key bindings: (digit key label, font family name, `KeyCode`).
/// `JetBrains` Mono is always available; the rest are loaded at runtime.
const FONT_KEYS: &[(&str, &str, KeyCode)] = &[
    ("1", "JetBrains Mono", KeyCode::Digit1),
    ("2", "Noto Sans", KeyCode::Digit2),
    ("3", "EB Garamond", KeyCode::Digit3),
    ("4", "Crimson Text", KeyCode::Digit4),
    ("5", "Liberation Sans", KeyCode::Digit5),
    ("6", "Liberation Serif", KeyCode::Digit6),
];

const DISPLAY_WORDS: &[&str] = &[
    "Typography", // accented cap above ascent
    "Ångström",   // ring accent, umlaut
    "fjord",      // f-j ligature candidate, j descender
    "Qüixy",      // Q descender, umlaut, y descender
    "Éblouir",    // accented É above ascent
    "glyph",      // g + y descenders, x-height
    "WAVEFORM",   // all caps, wide W/M, kerning (AV)
    "Bézier",     // accented é, mixed case
    "Señal",      // tilde above lowercase ñ
    "Ïjssel",     // diaeresis on cap I, IJ digraph
    "Übergrößen", // Ü above cap, ß eszett
    "Sphinx",     // ascender curve, x terminal
    "Jäger",      // J descender, ä umlaut, g descender
    "Côté",       // circumflex + acute above cap
    "pqbd",       // mirror descender/ascender letters
    "Ål",         // Å ring accent, l ascender, narrow
    "Grüße",      // ü umlaut, ß eszett
    "Twiggy",     // T overhang, double g + y descender
    "ÀÇÉÎÕÜ",     // six accented caps
    "fly",        // f ascender, l ascender, y descender
    "fficult",    // ffi ligature (liga)
    "::=>!=",     // calt sequences (contextual alternates)
    "Thirsty",    // Th + st discretionary ligatures (dlig)
    "AVOW Type",  // kerning pairs AV, OW, Ty (kern)
];

#[derive(Resource)]
struct SceneBounds(Entity);

#[derive(Component)]
struct ControlsPanel;

#[derive(Component)]
struct FontsPanel;

/// Marker for the main display text that the overlay toggle targets.
#[derive(Component)]
struct DisplayText;

/// Tracks which word in `DISPLAY_WORDS` is currently shown,
/// with a repeat timer for hold-to-cycle.
#[derive(Resource)]
struct WordCycle {
    index: usize,
    timer: Timer,
}

/// Keeps loaded font handles alive so they don't get unloaded.
#[derive(Resource, Default)]
struct FontHandles(Vec<Handle<Font>>);

#[derive(Resource)]
struct SelectedFont(usize);

#[derive(EntityEvent)]
struct OverlayHome {
    #[event_target]
    entity: Entity,
    camera: Entity,
}

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            LagrangePlugin,
            BrpExtrasPlugin::default().port_in_title(PortDisplay::NonDefault),
            WindowManagerPlugin,
            MeshPickingPlugin,
            DiegeticUiPlugin,
        ))
        .insert_resource(DirectionalLightShadowMap {
            size: KEY_LIGHT_SHADOW_MAP_SIZE,
        })
        .insert_resource(WordCycle {
            index: 0,
            timer: Timer::from_seconds(0.15, TimerMode::Repeating),
        })
        .insert_resource(SelectedFont(0))
        .init_resource::<FontHandles>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                toggle_overlay,
                home_camera,
                switch_font,
                cycle_word,
                update_controls_hud,
            ),
        )
        .add_observer(on_world_text_added)
        .add_observer(on_font_registered)
        .add_observer(on_typography_overlay_ready)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    mut font_handles: ResMut<FontHandles>,
    registry: Res<FontRegistry>,
) {
    load_fonts(&asset_server, &mut font_handles);
    // Ground plane — subtle, light gray.
    let ground = commands
        .spawn((
            Mesh3d(
                meshes.add(
                    Plane3d::default()
                        .mesh()
                        .size(GROUND_WIDTH, GROUND_FRONT_MARGIN + GROUND_BACK_MARGIN),
                ),
            ),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.08, 0.08, 0.08),
                perceptual_roughness: 0.15,
                metallic: 0.0,
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
            Transform::from_xyz(
                0.0,
                0.0,
                DISPLAY_Z + (GROUND_FRONT_MARGIN - GROUND_BACK_MARGIN) / 2.0,
            ),
        ))
        .observe(on_ground_clicked)
        .id();

    commands.insert_resource(SceneBounds(ground));

    // Display word with typography overlay.
    commands.spawn((
        DisplayText,
        WorldText::new(DISPLAY_WORDS[0]),
        WorldTextStyle::new(DISPLAY_SIZE)
            .with_color(Color::srgb(0.9, 0.9, 0.9))
            .with_loading_policy(GlyphLoadingPolicy::Progressive),
        TypographyOverlay::default().with_shadow(SurfaceShadow::On),
        Transform::from_xyz(0.0, DISPLAY_Y, DISPLAY_Z),
    ));

    spawn_lights(&mut commands);
    spawn_hud_panels(&mut commands, &registry);

    // Camera
    commands.spawn((OrbitCam {
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
    },));
}

fn spawn_hud_panels(commands: &mut Commands, registry: &FontRegistry) {
    let unlit_material = bevy_diegetic::default_panel_material();
    let unlit = StandardMaterial {
        unlit: true,
        ..unlit_material
    };

    commands.spawn((
        ControlsPanel,
        DiegeticPanel::screen()
            .size(Sizing::fixed(CONTROLS_WIDTH), Sizing::fixed(HUD_HEIGHT))
            .anchor(bevy_diegetic::Anchor::TopLeft)
            .material(unlit.clone())
            .text_material(unlit.clone())
            .layout(|b| build_controls_content(b, true))
            .build()
            .expect("valid controls HUD dimensions"),
        Transform::default(),
    ));

    commands.spawn((
        FontsPanel,
        DiegeticPanel::screen()
            .size(Sizing::fixed(FONTS_PANEL_WIDTH), Sizing::fixed(FONTS_PANEL_HEIGHT))
            .anchor(bevy_diegetic::Anchor::TopRight)
            .material(unlit.clone())
            .text_material(unlit.clone())
            .with_tree(build_fonts_panel(registry, 0))
            .build()
            .expect("valid fonts HUD dimensions"),
        Transform::default(),
    ));

    commands.spawn((
        DiegeticPanel::screen()
            .size(Sizing::fixed(CAM_HELP_WIDTH), Sizing::fixed(CAM_HELP_HEIGHT))
            .anchor(bevy_diegetic::Anchor::BottomRight)
            .material(unlit.clone())
            .text_material(unlit)
            .layout(build_camera_help)
            .build()
            .expect("valid camera help HUD dimensions"),
        Transform::default(),
    ));
}

fn spawn_lights(commands: &mut Commands) {
    commands.spawn((
        DirectionalLight {
            illuminance: KEY_LIGHT_LUX,
            shadows_enabled: true,
            shadow_depth_bias: KEY_LIGHT_SHADOW_DEPTH_BIAS,
            shadow_normal_bias: KEY_LIGHT_SHADOW_NORMAL_BIAS,
            ..default()
        },
        CascadeShadowConfigBuilder {
            minimum_distance: KEY_LIGHT_SHADOW_MIN_DISTANCE,
            maximum_distance: KEY_LIGHT_SHADOW_MAX_DISTANCE,
            first_cascade_far_bound: KEY_LIGHT_FIRST_CASCADE_FAR_BOUND,
            ..default()
        }
        .build(),
        Transform::from_translation(KEY_LIGHT_POS)
            .looking_at(Vec3::new(0.0, DISPLAY_Y, DISPLAY_Z), Vec3::Y),
    ));
    commands.spawn((
        SpotLight {
            intensity: REFLECTION_LIGHT_LEVEL,
            range: 20.0,
            shadows_enabled: false,
            inner_angle: 0.0,
            outer_angle: core::f32::consts::FRAC_PI_6,
            ..default()
        },
        Transform::from_translation(REFLECTION_LIGHT_POS).looking_at(REFLECTION_TARGET, Vec3::Y),
    ));
}

fn load_fonts(asset_server: &AssetServer, font_handles: &mut FontHandles) {
    for path in [
        "fonts/NotoSans-Regular.ttf",
        "fonts/EBGaramond-Regular.ttf",
        "fonts/CrimsonText-Regular.ttf",
        "fonts/LiberationSans-Regular.ttf",
        "fonts/LiberationSerif-Regular.ttf",
    ] {
        font_handles.0.push(asset_server.load(path));
    }
}

fn on_ground_clicked(click: On<Pointer<Click>>, mut commands: Commands, scene: Res<SceneBounds>) {
    if click.button != PointerButton::Primary {
        return;
    }
    let camera = click.hit.camera;
    commands.trigger(
        ZoomToFit::new(camera, scene.0)
            .margin(ZOOM_TO_FIT_MARGIN)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );
}

fn on_world_text_added(added: On<Add, WorldText>, mut commands: Commands) {
    commands.entity(added.entity).observe(on_text_clicked);
}

fn on_typography_overlay_ready(
    trigger: On<TypographyOverlayReady>,
    cameras: Query<Entity, With<OrbitCam>>,
    mut initialized: Local<bool>,
    mut commands: Commands,
) {
    let target = trigger.event_target();
    info!("TypographyOverlayReady: {target:?}");
    commands.entity(target).observe(on_overlay_home);
    if *initialized {
        return;
    }
    *initialized = true;
    for camera in &cameras {
        commands.trigger(OverlayHome {
            entity: target,
            camera,
        });
    }
}

fn on_overlay_home(event: On<OverlayHome>, mut commands: Commands) {
    commands.trigger(
        AnimateToFit::new(event.camera, event.event_target())
            .yaw(HOME_YAW)
            .pitch(HOME_PITCH)
            .margin(ZOOM_TO_FIT_MARGIN)
            .duration(Duration::from_millis(ZOOM_DURATION_MS))
            .easing(bevy::math::curve::easing::EaseFunction::CubicOut),
    );
}

fn on_text_clicked(mut click: On<Pointer<Click>>, mut commands: Commands) {
    if click.button != PointerButton::Primary {
        return;
    }
    click.propagate(false);
    let camera = click.hit.camera;
    commands.trigger(
        ZoomToFit::new(camera, click.entity)
            .margin(ZOOM_TO_FIT_MARGIN)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );
}

fn build_controls_tree(overlay_on: bool) -> bevy_diegetic::LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::fixed(HUD_HEIGHT)),
    );
    build_controls_content(&mut builder, overlay_on);
    builder.build()
}

fn build_controls_content(b: &mut LayoutBuilder, overlay_on: bool) {
    let title = LayoutTextStyle::new(HUD_TITLE_SIZE)
        .with_font(FontId::MONOSPACE.0)
        .with_color(HUD_TITLE_COLOR);
    let hint = LayoutTextStyle::new(HUD_HINT_SIZE)
        .with_font(FontId::MONOSPACE.0)
        .with_color(HUD_INACTIVE_COLOR);

    b.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::GROW)
            .padding(Padding::all(Px(2.0)))
            .corner_radius(CornerRadius::new(
                CAM_HELP_RADIUS,
                CAM_HELP_RADIUS,
                CAM_HELP_RADIUS,
                CAM_HELP_RADIUS,
            ))
            .background(HUD_FRAME_BACKGROUND)
            .border(Border::all(Px(2.0), HUD_BORDER_ACCENT)),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::FIT)
                    .height(Sizing::GROW)
                    .direction(Direction::LeftToRight)
                    .padding(Padding::new(Px(8.0), HUD_PADDING, Px(8.0), HUD_PADDING))
                    .child_gap(HUD_GAP)
                    .child_align_y(AlignY::Center)
                    .clip()
                    .corner_radius(CornerRadius::new(
                        CAM_HELP_INNER_RADIUS,
                        CAM_HELP_INNER_RADIUS,
                        CAM_HELP_INNER_RADIUS,
                        CAM_HELP_INNER_RADIUS,
                    ))
                    .background(HUD_BACKGROUND)
                    .border(Border::all(Px(1.0), HUD_BORDER_DIM)),
                |b| {
                    b.text("CONTROLS", title);
                    hud_separator(b);

                    b.text("H Home", hint.clone());
                    hud_separator(b);

                    let overlay_label = if overlay_on {
                        "T Overlay On"
                    } else {
                        "T Overlay Off"
                    };
                    let overlay_color = if overlay_on {
                        HUD_ACTIVE_COLOR
                    } else {
                        HUD_INACTIVE_COLOR
                    };
                    b.text(overlay_label, hint.clone().with_color(overlay_color));
                    hud_separator(b);

                    b.text("←/→ Cycle Word", hint);
                },
            );
        },
    );
}

const fn row_color(active: bool) -> Color {
    if active {
        HUD_ACTIVE_COLOR
    } else {
        HUD_INACTIVE_COLOR
    }
}

fn build_font_key_cells(selected_font: usize) -> Vec<ColumnCell<'static>> {
    FONT_KEYS
        .iter()
        .enumerate()
        .map(|(idx, (label, _, _))| {
            ColumnCell::Text(
                label,
                LayoutTextStyle::new(FONTS_KEY_SIZE).with_color(row_color(idx == selected_font)),
            )
        })
        .collect()
}

fn build_font_name_cells(
    registry: &FontRegistry,
    selected_font: usize,
) -> Vec<ColumnCell<'static>> {
    FONT_KEYS
        .iter()
        .enumerate()
        .map(|(idx, (_, name, _))| {
            let font_id = registry
                .font_id_by_name(name)
                .unwrap_or(FontId::MONOSPACE)
                .0;
            ColumnCell::Text(
                name,
                LayoutTextStyle::new(FONTS_SAMPLE_SIZE)
                    .with_font(font_id)
                    .with_color(row_color(idx == selected_font)),
            )
        })
        .collect()
}

fn build_fonts_panel(registry: &FontRegistry, selected_font: usize) -> bevy_diegetic::LayoutTree {
    let row_h = Sizing::fixed(FONTS_PANEL_ROW_HEIGHT);
    let key_cells = build_font_key_cells(selected_font);
    let name_cells = build_font_name_cells(registry, selected_font);

    let mut builder = LayoutBuilder::new(FONTS_PANEL_WIDTH, FONTS_PANEL_HEIGHT);
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .child_align_x(AlignX::Right),
        |b| {
            b.with(
                El::new()
                    .width(Sizing::FIT)
                    .height(Sizing::FIT)
                    .padding(Padding::all(CAM_HELP_FRAME_PAD))
                    .corner_radius(CornerRadius::new(
                        CAM_HELP_RADIUS,
                        CAM_HELP_RADIUS,
                        CAM_HELP_RADIUS,
                        CAM_HELP_RADIUS,
                    ))
                    .background(HUD_FRAME_BACKGROUND)
                    .border(Border::all(CAM_HELP_BORDER, HUD_BORDER_ACCENT)),
                |b| {
                    b.with(
                        El::new()
                            .width(Sizing::FIT)
                            .height(Sizing::FIT)
                            .padding(Padding::all(Px(10.0)))
                            .direction(Direction::TopToBottom)
                            .child_gap(Px(6.0))
                            .corner_radius(CornerRadius::new(
                                CAM_HELP_INNER_RADIUS,
                                CAM_HELP_INNER_RADIUS,
                                CAM_HELP_INNER_RADIUS,
                                CAM_HELP_INNER_RADIUS,
                            ))
                            .background(HUD_BACKGROUND)
                            .border(Border::all(Px(1.0), HUD_BORDER_DIM)),
                        |b| {
                            b.text(
                                "FONTS",
                                LayoutTextStyle::new(CAM_HELP_TITLE_SIZE)
                                    .with_color(HUD_TITLE_COLOR),
                            );
                            b.with(
                                El::new()
                                    .width(Sizing::FIT)
                                    .height(Sizing::FIT)
                                    .direction(Direction::LeftToRight)
                                    .child_gap(FONTS_PANEL_GAP),
                                |b| {
                                    b.with(
                                        El::new()
                                            .width(Sizing::fixed(FONTS_PANEL_KEY_WIDTH))
                                            .height(Sizing::FIT)
                                            .direction(Direction::TopToBottom)
                                            .child_align_x(AlignX::Center),
                                        |b| {
                                            for cell in &key_cells {
                                                let ColumnCell::Text(text, config) = cell;
                                                b.with(
                                                    El::new()
                                                        .width(Sizing::GROW)
                                                        .height(row_h)
                                                        .child_align_x(AlignX::Center)
                                                        .child_align_y(AlignY::Center),
                                                    |b| {
                                                        b.text(*text, config.clone());
                                                    },
                                                );
                                            }
                                        },
                                    );
                                    column(b, AlignX::Left, row_h, &name_cells);
                                },
                            );
                        },
                    );
                },
            );
        },
    );
    builder.build()
}

fn build_camera_help(b: &mut LayoutBuilder) {
    let title = LayoutTextStyle::new(CAM_HELP_TITLE_SIZE).with_color(HUD_TITLE_COLOR);
    let header = LayoutTextStyle::new(CAM_HELP_HEADER_SIZE).with_color(HUD_CAMERA_HEADER_COLOR);
    let label = LayoutTextStyle::new(CAM_HELP_LABEL_SIZE).with_color(HUD_INACTIVE_COLOR);

    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(CAM_HELP_FRAME_PAD))
            .corner_radius(CornerRadius::new(
                CAM_HELP_RADIUS,
                CAM_HELP_RADIUS,
                CAM_HELP_RADIUS,
                CAM_HELP_RADIUS,
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
                        CAM_HELP_INNER_RADIUS,
                        CAM_HELP_INNER_RADIUS,
                        CAM_HELP_INNER_RADIUS,
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
                                    b.text("MMB drag → Orbit", label.clone());
                                    b.text("Shift+MMB → Pan", label.clone());
                                    b.text("Scroll → Zoom", label.clone());
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
                                    b.text("Scroll → Orbit", label.clone());
                                    b.text("Shift+Scroll → Pan", label.clone());
                                    b.text("Ctrl+Scroll → Zoom", label.clone());
                                    b.text("Pinch → Zoom", label.clone());
                                },
                            );
                        },
                    );
                },
            );
        },
    );
}

fn hud_separator(b: &mut LayoutBuilder) {
    b.with(
        El::new()
            .width(Sizing::fixed(Px(1.0)))
            .height(Sizing::GROW)
            .background(HUD_DIVIDER_COLOR),
        |_| {},
    );
}

/// Cell content for a column.
enum ColumnCell<'a> {
    Text(&'a str, LayoutTextStyle),
}

/// Builds a column of fixed-height rows.
fn column(b: &mut LayoutBuilder, align: AlignX, row_height: Sizing, cells: &[ColumnCell<'_>]) {
    b.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .direction(Direction::TopToBottom)
            .child_align_x(align),
        |b| {
            for cell in cells {
                let ColumnCell::Text(text, config) = cell;
                b.with(
                    El::new()
                        .width(Sizing::FIT)
                        .height(row_height)
                        .child_align_y(AlignY::Center),
                    |b| {
                        b.text(*text, config.clone());
                    },
                );
            }
        },
    );
}

fn on_font_registered(
    trigger: On<FontRegistered>,
    mut panels: Query<&mut DiegeticPanel, With<FontsPanel>>,
    registry: Res<FontRegistry>,
    selected_font: Res<SelectedFont>,
) {
    info!(
        "FontRegistered: {} (id: {}, {:?})",
        trigger.name, trigger.id.0, trigger.source
    );
    for mut panel in &mut panels {
        info!("Rebuilding fonts panel");
        panel.set_tree(build_fonts_panel(&registry, selected_font.0));
    }
}

fn update_controls_hud(
    mut huds: Query<&mut DiegeticPanel, With<ControlsPanel>>,
    with_overlay: Query<Entity, (With<DisplayText>, With<TypographyOverlay>)>,
    mut previous_state: Local<bool>,
) {
    let overlay_on = !with_overlay.is_empty();
    if *previous_state == overlay_on {
        return;
    }
    *previous_state = overlay_on;

    for mut panel in &mut huds {
        panel.set_tree(build_controls_tree(overlay_on));
    }
}

fn toggle_overlay(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    with_overlay: Query<Entity, (With<DisplayText>, With<TypographyOverlay>)>,
    without_overlay: Query<Entity, (With<DisplayText>, Without<TypographyOverlay>)>,
) {
    if !keyboard.just_pressed(KeyCode::KeyT) {
        return;
    }
    if with_overlay.is_empty() {
        for entity in &without_overlay {
            commands.entity(entity).insert(TypographyOverlay::default());
        }
    } else {
        for entity in &with_overlay {
            commands.entity(entity).remove::<TypographyOverlay>();
        }
    }
}

fn home_camera(
    keyboard: Res<ButtonInput<KeyCode>>,
    cameras: Query<Entity, With<OrbitCam>>,
    overlay_bounds: Query<Entity, With<OverlayBoundingBox>>,
    mut commands: Commands,
) {
    if !keyboard.just_pressed(KeyCode::KeyH) {
        return;
    }
    let Some(target) = overlay_bounds.iter().next() else {
        return;
    };
    for camera in &cameras {
        commands.trigger(OverlayHome {
            entity: target,
            camera,
        });
    }
}

fn cycle_word(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut cycle: ResMut<WordCycle>,
    mut texts: Query<&mut WorldText, With<DisplayText>>,
) {
    let forward = keyboard.pressed(KeyCode::ArrowRight);
    let backward = keyboard.pressed(KeyCode::ArrowLeft);
    if !forward && !backward {
        cycle.timer.reset();
        return;
    }

    // Advance immediately on first press, then on timer ticks while held.
    let just =
        keyboard.just_pressed(KeyCode::ArrowRight) || keyboard.just_pressed(KeyCode::ArrowLeft);
    let should_advance = just || cycle.timer.tick(time.delta()).just_finished();
    if !should_advance {
        return;
    }

    let len = DISPLAY_WORDS.len();
    if forward {
        cycle.index = (cycle.index + 1) % len;
    } else {
        cycle.index = (cycle.index + len - 1) % len;
    }
    let word = DISPLAY_WORDS[cycle.index];
    for mut text in &mut texts {
        text.0 = word.to_string();
    }
}

fn switch_font(
    keyboard: Res<ButtonInput<KeyCode>>,
    registry: Res<FontRegistry>,
    mut selected_font: ResMut<SelectedFont>,
    mut panels: Query<&mut DiegeticPanel, With<FontsPanel>>,
    mut texts: Query<&mut WorldTextStyle, With<DisplayText>>,
) {
    let pressed = FONT_KEYS
        .iter()
        .enumerate()
        .find(|(_, (_, _, key))| keyboard.just_pressed(*key));
    let Some((idx, (_, name, _))) = pressed else {
        return;
    };
    selected_font.0 = idx;
    let font_id = registry
        .font_id_by_name(name)
        .unwrap_or(FontId::MONOSPACE)
        .0;
    for mut style in &mut texts {
        *style = WorldTextStyle::new(DISPLAY_SIZE)
            .with_font(font_id)
            .with_color(Color::srgb(0.9, 0.9, 0.9));
    }
    for mut panel in &mut panels {
        panel.set_tree(build_fonts_panel(&registry, selected_font.0));
    }
}
