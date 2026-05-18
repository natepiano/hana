//! Typography overlay demo — visualizes font-level metric lines and
//! per-glyph bounding boxes on a `WorldText` entity using the library's
//! built-in `TypographyOverlay` debug component.
//!
//! Requires the `typography_overlay` feature:
//! ```sh
//! cargo run --example typography --features typography_overlay
//! ```

use std::time::Duration;

use bevy::prelude::*;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::AtlasPreference;
use bevy_diegetic::Border;
use bevy_diegetic::CornerRadius;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticPanelCommands;
use bevy_diegetic::DiegeticPerfStats;
use bevy_diegetic::Direction;
use bevy_diegetic::DistanceField;
use bevy_diegetic::El;
use bevy_diegetic::Font;
use bevy_diegetic::FontId;
use bevy_diegetic::FontRegistered;
use bevy_diegetic::FontRegistry;
use bevy_diegetic::GlyphLoadingPolicy;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::Padding;
use bevy_diegetic::PanelTextChild;
use bevy_diegetic::PendingGlyphs;
use bevy_diegetic::Pt;
use bevy_diegetic::Px;
use bevy_diegetic::RasterBackend;
use bevy_diegetic::RasterQuality;
use bevy_diegetic::Sizing;
use bevy_diegetic::SurfaceShadow;
use bevy_diegetic::TypographyOverlay;
use bevy_diegetic::TypographyOverlayReady;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_lagrange::AnimateToFit;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamPreset;
use bevy_lagrange::ZoomToFit;
use fairy_dust::ControlActivation;
use fairy_dust::SetCameraHomeFromEntity;
use fairy_dust::TitleBar;

const DISPLAY_SIZE: f32 = 0.48;
const DISPLAY_Y: f32 = 0.5;
const DISPLAY_Z: f32 = 2.0;
const COMMENT_SIZE: f32 = 0.15;
const COMMENT_GROUND_LIFT: f32 = 0.005;
/// Front edge of the ground plane (closest to camera).
const GROUND_FRONT_Z: f32 = GROUND_CENTER_Z + GROUND_SIZE * GROUND_DEPTH_SCALE * 0.5;
/// Place the comment halfway between the word and the front of the ground.
const COMMENT_Z: f32 = (DISPLAY_Z + GROUND_FRONT_Z) * 0.5;
/// Mirrors `bevy_diegetic::debug::constants::BBOX_COLOR`, which is `pub(super)`.
const COMMENT_COLOR: Color = Color::srgba(1.0, 1.0, 0.6, 0.7);
const ZOOM_TO_FIT_MARGIN: f32 = 0.05;
const ZOOM_DURATION_MS: u64 = 1000;
const GROUND_SIZE: f32 = 5.4;
const GROUND_DEPTH_SCALE: f32 = 0.7;
const GROUND_CENTER_Z: f32 = GROUND_SIZE * 0.5 * (1.0 - GROUND_DEPTH_SCALE);
const GROUND_COLOR: Color = Color::srgb(0.08, 0.08, 0.08);

const HOME_FOCUS: Vec3 = Vec3::new(-0.001, 0.461, 2.002);
const HOME_RADIUS: f32 = 2.84;
const HOME_YAW: f32 = 0.0;
const HOME_PITCH: f32 = 0.055;

const LIGHT_AIM: Vec3 = Vec3::new(0.0, DISPLAY_Y, DISPLAY_Z);
const KEY_LIGHT_POS: Vec3 = Vec3::new(0.0, 5.0, DISPLAY_Z + 12.0);

const HUD_BACKGROUND: Color = Color::srgba(0.02, 0.03, 0.07, 0.80);
const HUD_FRAME_BACKGROUND: Color = Color::srgba(0.01, 0.01, 0.03, 0.95);
const HUD_BORDER_ACCENT: Color = Color::srgba(0.15, 0.7, 0.9, 0.5);
const HUD_BORDER_DIM: Color = Color::srgba(0.1, 0.4, 0.6, 0.3);
const HUD_TITLE_COLOR: Color = Color::srgb(0.9, 0.95, 1.0);
const HUD_ACTIVE_COLOR: Color = Color::srgb(0.3, 1.0, 0.8);
const HUD_INACTIVE_COLOR: Color = Color::srgba(0.6, 0.65, 0.8, 0.85);

const CAM_HELP_WIDTH: Px = Px(280.0);
const CAM_HELP_RADIUS: Px = Px(15.0);
const CAM_HELP_FRAME_PAD: Px = Px(2.0);
const CAM_HELP_BORDER: Px = Px(2.0);
const CAM_HELP_INSET: Px = Px(CAM_HELP_FRAME_PAD.0 + CAM_HELP_BORDER.0);
const CAM_HELP_INNER_RADIUS: Px = Px(CAM_HELP_RADIUS.0 - CAM_HELP_INSET.0);

const CYCLE_HIGHLIGHT_MIN: Duration = Duration::from_millis(500);

const FONTS_PANEL_WIDTH: Px = CAM_HELP_WIDTH;
const FONTS_PANEL_HEIGHT: Px = Px(208.0);
const FONTS_PANEL_GAP: Px = Px(10.0);
const FONTS_PANEL_ROW_HEIGHT: Px = Px(24.0);
const FONTS_PANEL_KEY_WIDTH: Px = Px(18.0);

const QUALITY_PANEL_WIDTH: Px = CAM_HELP_WIDTH;
const QUALITY_PANEL_HEIGHT: Px = Px(190.0);

/// Quality key bindings: (letter label, `RasterQuality`, `KeyCode`).
///
/// Maps A/B/C/D/E to the five named pixel sizes the
/// [`RasterQuality`] enum exposes. Custom is not bound — apps that
/// want a specific size set `RasterQualityPreference` directly.
const QUALITY_KEYS: &[(&str, RasterQuality, KeyCode)] = &[
    ("A", RasterQuality::Tiny, KeyCode::KeyA),
    ("B", RasterQuality::Small, KeyCode::KeyB),
    ("C", RasterQuality::Medium, KeyCode::KeyC),
    ("D", RasterQuality::Large, KeyCode::KeyD),
    ("E", RasterQuality::Huge, KeyCode::KeyE),
];

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
const CRIMSON_TEXT_REGULAR_FONT_ASSET_PATH: &str = "fonts/CrimsonText-Regular.ttf";
const EB_GARAMOND_REGULAR_FONT_ASSET_PATH: &str = "fonts/EBGaramond-Regular.ttf";
const LIBERATION_SANS_REGULAR_FONT_ASSET_PATH: &str = "fonts/LiberationSans-Regular.ttf";
const LIBERATION_SERIF_REGULAR_FONT_ASSET_PATH: &str = "fonts/LiberationSerif-Regular.ttf";
const NOTO_SANS_REGULAR_FONT_ASSET_PATH: &str = "fonts/NotoSans-Regular.ttf";

const DISPLAY_WORDS: &[(&str, &str)] = &[
    ("Typography", "accented cap above ascent"),
    ("V", "EB Garamond Test"),
    ("Ångström", "ring accent, umlaut"),
    ("fjord", "f-j ligature candidate, j descender"),
    ("Qüixy", "Q descender, umlaut, y descender"),
    ("Éblouir", "accented É above ascent"),
    ("glyph", "g + y descenders, x-height"),
    ("WAVEFORM", "all caps, wide W/M, kerning (AV)"),
    ("Bézier", "accented é, mixed case"),
    ("Señal", "tilde above lowercase ñ"),
    ("Ïjssel", "diaeresis on cap I, IJ digraph"),
    ("Übergrößen", "Ü above cap, ß eszett"),
    ("Sphinx", "ascender curve, x terminal"),
    ("Jäger", "J descender, ä umlaut, g descender"),
    ("Côté", "circumflex + acute above cap"),
    ("pqbd", "mirror descender/ascender letters"),
    ("Ål", "Å ring accent, l ascender, narrow"),
    ("Grüße", "ü umlaut, ß eszett"),
    ("Twiggy", "T overhang, double g + y descender"),
    ("ÀÇÉÎÕÜ", "six accented caps"),
    ("fly", "f ascender, l ascender, y descender"),
    ("difficult", "ffi ligature (liga)"),
    ("::=>!=", "calt sequences (contextual alternates)"),
    ("Thirsty", "Th + st discretionary ligatures (dlig)"),
    ("AVOW Type", "kerning pairs AV, OW, Ty (kern)"),
];

#[derive(Component)]
struct FontsPanel;

#[derive(Component)]
struct QualityPanel;

#[derive(Component)]
struct CommentText;

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
enum OverlayState {
    #[default]
    On,
    Off,
}

#[derive(Resource, Default, Clone, Copy, PartialEq)]
enum CycleState {
    #[default]
    Idle,
    Cycling {
        started_at:    Duration,
        overlay_ready: bool,
    },
}

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

fn main() {
    // `bevy_diegetic::DiegeticUiPlugin` is registered automatically by
    // `fairy_dust::sprinkle_example`.
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .aim_at(LIGHT_AIM)
        .key_light_pos(KEY_LIGHT_POS)
        .with_ground_plane()
        .size(GROUND_SIZE)
        .transform(
            Transform::from_xyz(0.0, 0.0, GROUND_CENTER_Z).with_scale(Vec3::new(
                1.0,
                1.0,
                GROUND_DEPTH_SCALE,
            )),
        )
        .color(GROUND_COLOR)
        .with_orbit_cam(|_| {}, OrbitCamPreset::BlenderLike)
        .with_stable_transparency()
        .with_camera_home(
            Transform::from_translation(HOME_FOCUS).with_scale(Vec3::splat(HOME_RADIUS * 2.0)),
        )
        .yaw(HOME_YAW)
        .pitch(HOME_PITCH)
        .duration(Duration::from_millis(ZOOM_DURATION_MS))
        .margin(ZOOM_TO_FIT_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .control("T Overlay")
                .control("←/→ Cycle Word")
                .control("M MSDF")
                .control("S SDF")
                .control("G GPU"),
        )
        .wire_chip_to_state::<OverlayState, _>("T Overlay", |state| match state {
            OverlayState::On => ControlActivation::Active,
            OverlayState::Off => ControlActivation::Inactive,
        })
        .wire_chip_to_state::<CycleState, _>("←/→ Cycle Word", |state| match state {
            CycleState::Cycling { .. } => ControlActivation::Active,
            CycleState::Idle => ControlActivation::Inactive,
        })
        .wire_chip_to_state::<AtlasPreference, _>("M MSDF", |pref| match pref.distance_field {
            DistanceField::Msdf => ControlActivation::Active,
            DistanceField::Sdf => ControlActivation::Inactive,
        })
        .wire_chip_to_state::<AtlasPreference, _>("S SDF", |pref| match pref.distance_field {
            DistanceField::Sdf => ControlActivation::Active,
            DistanceField::Msdf => ControlActivation::Inactive,
        })
        .wire_chip_to_state::<AtlasPreference, _>("G GPU", |pref| match pref.backend {
            RasterBackend::Gpu => ControlActivation::Active,
            RasterBackend::Cpu => ControlActivation::Inactive,
        })
        .with_camera_control_panel()
        .insert_resource(WordCycle {
            index: 0,
            timer: Timer::from_seconds(0.15, TimerMode::Repeating),
        })
        .insert_resource(SelectedFont(0))
        .init_resource::<FontHandles>()
        .init_resource::<OverlayState>()
        .init_resource::<CycleState>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                toggle_overlay,
                switch_font,
                cycle_word,
                tick_cycle_state,
                toggle_distance_field,
                toggle_backend,
                pick_raster_quality,
                refresh_quality_panel,
                log_steady_state_perf,
            ),
        )
        .add_observer(on_world_text_added)
        .add_observer(on_font_registered)
        .add_observer(on_typography_overlay_ready)
        .run();
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut font_handles: ResMut<FontHandles>,
    font_registry: Res<FontRegistry>,
) {
    load_fonts(&asset_server, &mut font_handles);

    let (initial_word, initial_comment) = DISPLAY_WORDS[0];

    // Display word with typography overlay.
    commands.spawn((
        DisplayText,
        WorldText::new(initial_word),
        WorldTextStyle::new(DISPLAY_SIZE)
            .with_color(Color::srgb(0.9, 0.9, 0.9))
            .with_loading_policy(GlyphLoadingPolicy::Progressive),
        TypographyOverlay::default().with_shadow(SurfaceShadow::On),
        Transform::from_xyz(0.0, DISPLAY_Y, DISPLAY_Z),
    ));

    // Comment text — lies flat in the ground plane, in front of the word,
    // reading toward the camera so the overlay never overlaps it.
    commands.spawn((
        CommentText,
        WorldText::new(initial_comment),
        WorldTextStyle::new(COMMENT_SIZE).with_color(COMMENT_COLOR),
        Transform {
            translation: Vec3::new(0.0, COMMENT_GROUND_LIFT, COMMENT_Z),
            rotation: Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
            ..default()
        },
    ));

    spawn_hud_panels(&mut commands, &font_registry);
}

fn spawn_hud_panels(commands: &mut Commands, font_registry: &FontRegistry) {
    let unlit_material = bevy_diegetic::default_panel_material();
    let unlit = StandardMaterial {
        unlit: true,
        ..unlit_material
    };

    let fonts_panel = DiegeticPanel::screen()
        .size(
            Sizing::fixed(FONTS_PANEL_WIDTH),
            Sizing::fixed(FONTS_PANEL_HEIGHT),
        )
        .anchor(bevy_diegetic::Anchor::TopRight)
        .material(unlit.clone())
        .text_material(unlit.clone())
        .with_tree(build_fonts_panel(font_registry, 0))
        .build();
    let Ok(fonts_panel) = fonts_panel else {
        error!("failed to build fonts HUD dimensions");
        return;
    };

    commands.spawn((FontsPanel, fonts_panel, Transform::default()));

    let quality_panel = DiegeticPanel::screen()
        .size(
            Sizing::fixed(QUALITY_PANEL_WIDTH),
            Sizing::fixed(QUALITY_PANEL_HEIGHT),
        )
        .anchor(bevy_diegetic::Anchor::BottomLeft)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(build_quality_panel(RasterQuality::default()))
        .build();
    let Ok(quality_panel) = quality_panel else {
        error!("failed to build quality HUD dimensions");
        return;
    };
    commands.spawn((QualityPanel, quality_panel, Transform::default()));
}

fn build_quality_panel(selected: RasterQuality) -> bevy_diegetic::LayoutTree {
    let row_height = Sizing::fixed(FONTS_PANEL_ROW_HEIGHT);
    let mut builder = LayoutBuilder::new(QUALITY_PANEL_WIDTH, QUALITY_PANEL_HEIGHT);
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .child_align_x(AlignX::Left)
            .child_align_y(AlignY::Bottom),
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
                                "QUALITY",
                                LayoutTextStyle::new(fairy_dust::TITLE_SIZE)
                                    .with_color(HUD_TITLE_COLOR),
                            );
                            b.with(
                                El::new()
                                    .width(Sizing::FIT)
                                    .height(Sizing::FIT)
                                    .direction(Direction::LeftToRight)
                                    .child_gap(FONTS_PANEL_GAP),
                                |b| {
                                    build_quality_keys_column(b, row_height);
                                    build_quality_labels_column(b, row_height, selected);
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

fn build_quality_keys_column(b: &mut LayoutBuilder, row_height: Sizing) {
    b.with(
        El::new()
            .width(Sizing::fixed(FONTS_PANEL_KEY_WIDTH))
            .height(Sizing::FIT)
            .direction(Direction::TopToBottom)
            .child_align_x(AlignX::Center),
        |b| {
            for (label, _, _) in QUALITY_KEYS {
                b.with(
                    El::new()
                        .width(Sizing::GROW)
                        .height(row_height)
                        .child_align_x(AlignX::Center)
                        .child_align_y(AlignY::Center),
                    |b| {
                        b.text(
                            *label,
                            LayoutTextStyle::new(Pt(12.0)).with_color(HUD_TITLE_COLOR),
                        );
                    },
                );
            }
        },
    );
}

fn build_quality_labels_column(b: &mut LayoutBuilder, row_height: Sizing, selected: RasterQuality) {
    b.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .direction(Direction::TopToBottom)
            .child_align_x(AlignX::Left),
        |b| {
            for (_, quality, _) in QUALITY_KEYS {
                let is_selected = *quality == selected;
                b.with(
                    El::new()
                        .width(Sizing::FIT)
                        .height(row_height)
                        .child_align_x(AlignX::Left)
                        .child_align_y(AlignY::Center),
                    |b| {
                        b.text(
                            quality_label(*quality),
                            LayoutTextStyle::new(Pt(12.0)).with_color(row_color(is_selected)),
                        );
                    },
                );
            }
        },
    );
}

const fn quality_label(quality: RasterQuality) -> &'static str {
    match quality {
        RasterQuality::Tiny => "16px  Tiny",
        RasterQuality::Small => "32px  Small",
        RasterQuality::Medium => "64px  Medium",
        RasterQuality::Large => "128px Large",
        RasterQuality::Huge => "256px Huge",
        RasterQuality::Custom(_) => "Custom",
    }
}

fn load_fonts(asset_server: &AssetServer, font_handles: &mut FontHandles) {
    for path in [
        NOTO_SANS_REGULAR_FONT_ASSET_PATH,
        EB_GARAMOND_REGULAR_FONT_ASSET_PATH,
        CRIMSON_TEXT_REGULAR_FONT_ASSET_PATH,
        LIBERATION_SANS_REGULAR_FONT_ASSET_PATH,
        LIBERATION_SERIF_REGULAR_FONT_ASSET_PATH,
    ] {
        font_handles.0.push(asset_server.load(path));
    }
}

fn on_world_text_added(added: On<Add, WorldText>, mut commands: Commands) {
    commands.entity(added.entity).observe(on_text_clicked);
}

fn on_typography_overlay_ready(
    trigger: On<TypographyOverlayReady>,
    cameras: Query<Entity, With<OrbitCam>>,
    mut initialized: Local<bool>,
    mut cycle_state: ResMut<CycleState>,
    mut commands: Commands,
) {
    let target = trigger.event_target();
    info!("TypographyOverlayReady: {target:?}");
    if let CycleState::Cycling {
        started_at,
        overlay_ready: false,
    } = *cycle_state
    {
        *cycle_state = CycleState::Cycling {
            started_at,
            overlay_ready: true,
        };
    }
    commands.trigger(SetCameraHomeFromEntity { source: target });
    if *initialized {
        return;
    }
    *initialized = true;
    for camera in &cameras {
        commands.trigger(
            AnimateToFit::new(camera, target)
                .yaw(HOME_YAW)
                .pitch(HOME_PITCH)
                .margin(ZOOM_TO_FIT_MARGIN)
                .duration(Duration::from_millis(ZOOM_DURATION_MS))
                .easing(bevy::math::curve::easing::EaseFunction::CubicOut),
        );
    }
}

fn tick_cycle_state(time: Res<Time>, mut cycle_state: ResMut<CycleState>) {
    if let CycleState::Cycling {
        started_at,
        overlay_ready: true,
    } = *cycle_state
        && time.elapsed().saturating_sub(started_at) >= CYCLE_HIGHLIGHT_MIN
    {
        *cycle_state = CycleState::Idle;
    }
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
                LayoutTextStyle::new(Pt(12.0)).with_color(row_color(idx == selected_font)),
            )
        })
        .collect()
}

fn build_font_name_cells(
    font_registry: &FontRegistry,
    selected_font: usize,
) -> Vec<ColumnCell<'static>> {
    FONT_KEYS
        .iter()
        .enumerate()
        .map(|(idx, (_, name, _))| {
            let font_id = font_registry
                .font_id_by_name(name)
                .unwrap_or(FontId::MONOSPACE)
                .0;
            ColumnCell::Text(
                name,
                LayoutTextStyle::new(Pt(12.0))
                    .with_font(font_id)
                    .with_color(row_color(idx == selected_font)),
            )
        })
        .collect()
}

fn build_fonts_panel(
    font_registry: &FontRegistry,
    selected_font: usize,
) -> bevy_diegetic::LayoutTree {
    let row_height = Sizing::fixed(FONTS_PANEL_ROW_HEIGHT);
    let key_cells = build_font_key_cells(selected_font);
    let name_cells = build_font_name_cells(font_registry, selected_font);

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
                                LayoutTextStyle::new(fairy_dust::TITLE_SIZE)
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
                                                        .height(row_height)
                                                        .child_align_x(AlignX::Center)
                                                        .child_align_y(AlignY::Center),
                                                    |b| {
                                                        b.text(*text, config.clone());
                                                    },
                                                );
                                            }
                                        },
                                    );
                                    column(b, AlignX::Left, row_height, &name_cells);
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
    panels: Query<Entity, With<FontsPanel>>,
    font_registry: Res<FontRegistry>,
    selected_font: Res<SelectedFont>,
    mut commands: Commands,
) {
    info!(
        "FontRegistered: {} (id: {}, {:?})",
        trigger.name, trigger.id.0, trigger.source
    );
    for entity in &panels {
        info!("Rebuilding fonts panel");
        commands.set_tree(entity, build_fonts_panel(&font_registry, selected_font.0));
    }
}

/// `M` selects MSDF rasterization; `S` selects SDF. Mutates
/// [`AtlasPreference::distance_field`]; the driver picks up the
/// mismatch on the next `PostUpdate` and starts the parallel-atlas
/// swap.
fn toggle_distance_field(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut preference: ResMut<AtlasPreference>,
) {
    if keyboard.just_pressed(KeyCode::KeyM) && preference.distance_field != DistanceField::Msdf {
        preference.distance_field = DistanceField::Msdf;
    } else if keyboard.just_pressed(KeyCode::KeyS)
        && preference.distance_field != DistanceField::Sdf
    {
        preference.distance_field = DistanceField::Sdf;
    }
}

/// `G` flips the rasterizer backend between CPU (`fdsm`) and GPU
/// (wgpu compute shader). Mutates [`AtlasPreference::backend`]; the
/// driver picks up the mismatch on the next `PostUpdate` and starts
/// the parallel-atlas swap into an atlas built with the new backend.
/// Visual difference signals WGSL kernel bugs — when the GPU path is
/// correct, rendered glyphs should be byte-identical to CPU.
fn toggle_backend(keyboard: Res<ButtonInput<KeyCode>>, mut preference: ResMut<AtlasPreference>) {
    if keyboard.just_pressed(KeyCode::KeyG) {
        preference.backend = match preference.backend {
            RasterBackend::Cpu => RasterBackend::Gpu,
            RasterBackend::Gpu => RasterBackend::Cpu,
        };
    }
}

/// `A`/`B`/`C`/`D`/`E` pick a canonical rasterization size. Mutates
/// [`AtlasPreference::quality`]; the driver runs the same parallel-
/// atlas swap as MSDF↔SDF when the size differs from the active atlas.
fn pick_raster_quality(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut preference: ResMut<AtlasPreference>,
) {
    for (_, quality, key) in QUALITY_KEYS {
        if keyboard.just_pressed(*key) && preference.quality != *quality {
            preference.quality = *quality;
            return;
        }
    }
}

/// Logs steady-state per-frame work counts once per second so we can
/// see which subsystem is actually burning CPU. Watch:
///
/// - text-shaping wall time / mesh-build wall time — if non-zero in steady state, some text entity
///   is being re-processed every frame.
/// - `pending_panel`/`pending_world` — entities stuck with `PendingGlyphs`; each forces the
///   text-shaping pass to re-run on it every frame.
/// - `atlas.poll_ms` — async glyph polling, should be near-zero when no rasterization is in flight.
/// - `atlas.in_flight_glyphs` — should be 0 in steady state.
#[allow(
    clippy::needless_pass_by_value,
    reason = "Bevy system parameters are taken by value"
)]
fn log_steady_state_perf(
    perf: Res<DiegeticPerfStats>,
    time: Res<Time>,
    pending_panel: Query<(), (With<PanelTextChild>, With<PendingGlyphs>)>,
    pending_world: Query<(), (With<bevy_diegetic::WorldText>, With<PendingGlyphs>)>,
    mut last_log: Local<f32>,
) {
    let now = time.elapsed_secs();
    if now - *last_log < 1.0 {
        return;
    }
    *last_log = now;
    let pending_panel = pending_panel.iter().count();
    let pending_world = pending_world.iter().count();
    let pt = &perf.panel_text;
    let atlas = &perf.atlas;
    let shaping_ms = pt.shape_ms; // allow-banned: DiegeticPerfStats field name
    let panels_processed = pt.shaped_panels; // allow-banned: DiegeticPerfStats field name
    info!(
        target: "typography_perf",
        "shaping_ms={:.2} parley_ms={:.2} atlas_lookup_ms={:.2} mesh_build_ms={:.2} \
         panels_processed={} pending_panel={} pending_world={} atlas_poll_ms={:.2} \
         atlas_sync_ms={:.2} in_flight={}",
        shaping_ms,
        pt.parley_ms,
        pt.atlas_lookup_ms,
        pt.mesh_build_ms,
        panels_processed,
        pending_panel,
        pending_world,
        atlas.poll_ms,
        atlas.sync_ms,
        atlas.in_flight_glyphs,
    );
}

/// Rebuilds the quality panel tree when the preference changes so the
/// selected row highlights match.
fn refresh_quality_panel(
    preference: Res<AtlasPreference>,
    panels: Query<Entity, With<QualityPanel>>,
    mut commands: Commands,
) {
    if !preference.is_changed() {
        return;
    }
    for entity in &panels {
        commands.set_tree(entity, build_quality_panel(preference.quality));
    }
}

fn toggle_overlay(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut overlay_state: ResMut<OverlayState>,
    with_overlay: Query<Entity, (With<DisplayText>, With<TypographyOverlay>)>,
    without_overlay: Query<Entity, (With<DisplayText>, Without<TypographyOverlay>)>,
) {
    if !keyboard.just_pressed(KeyCode::KeyT) {
        return;
    }
    if with_overlay.is_empty() {
        for entity in &without_overlay {
            commands
                .entity(entity)
                .insert(TypographyOverlay::default().with_shadow(SurfaceShadow::On));
        }
        *overlay_state = OverlayState::On;
    } else {
        for entity in &with_overlay {
            commands.entity(entity).remove::<TypographyOverlay>();
        }
        *overlay_state = OverlayState::Off;
    }
}

fn cycle_word(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut cycle: ResMut<WordCycle>,
    mut cycle_state: ResMut<CycleState>,
    mut texts: Query<&mut WorldText, (With<DisplayText>, Without<CommentText>)>,
    mut comments: Query<&mut WorldText, (With<CommentText>, Without<DisplayText>)>,
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
    let (word, comment) = DISPLAY_WORDS[cycle.index];
    for mut text in &mut comments {
        text.0 = comment.to_string();
    }
    for mut text in &mut texts {
        text.0 = word.to_string();
    }
    *cycle_state = CycleState::Cycling {
        started_at:    time.elapsed(),
        overlay_ready: false,
    };
}

fn switch_font(
    keyboard: Res<ButtonInput<KeyCode>>,
    font_registry: Res<FontRegistry>,
    mut selected_font: ResMut<SelectedFont>,
    panels: Query<Entity, With<FontsPanel>>,
    mut texts: Query<&mut WorldTextStyle, With<DisplayText>>,
    mut commands: Commands,
) {
    let pressed = FONT_KEYS
        .iter()
        .enumerate()
        .find(|(_, (_, _, key))| keyboard.just_pressed(*key));
    let Some((idx, (_, name, _))) = pressed else {
        return;
    };
    selected_font.0 = idx;
    let font_id = font_registry
        .font_id_by_name(name)
        .unwrap_or(FontId::MONOSPACE)
        .0;
    for mut style in &mut texts {
        *style = WorldTextStyle::new(DISPLAY_SIZE)
            .with_font(font_id)
            .with_color(Color::srgb(0.9, 0.9, 0.9));
    }
    for entity in &panels {
        commands.set_tree(entity, build_fonts_panel(&font_registry, selected_font.0));
    }
}
