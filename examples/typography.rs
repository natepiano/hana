//! Typography overlay demo — visualizes font-level metric lines and
//! per-glyph bounding boxes on a `WorldText` entity using the library's
//! built-in `TypographyOverlay` debug component.
//!
//! Requires the `typography_overlay` feature:
//! ```sh
//! cargo run --example typography --features typography_overlay
//! ```

use std::time::Duration;

use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_brp_extras::PortDisplay;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Border;
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
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::TypographyOverlay;
use bevy_diegetic::TypographyOverlayReady;
use bevy_diegetic::Unit;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_panorbit_camera::PanOrbitCameraPlugin;
use bevy_panorbit_camera::TrackpadBehavior;
use bevy_panorbit_camera_ext::CameraMove;
use bevy_panorbit_camera_ext::PanOrbitCameraExtPlugin;
use bevy_panorbit_camera_ext::PlayAnimation;
use bevy_panorbit_camera_ext::ZoomToFit;
use bevy_window_manager::WindowManagerPlugin;

const DISPLAY_SIZE: f32 = 0.48;
const ZOOM_TO_FIT_MARGIN: f32 = 0.05;
const ZOOM_DURATION_MS: u64 = 1000;

const HOME_FOCUS: Vec3 = Vec3::new(-0.001, 0.461, 2.002);
const HOME_RADIUS: f32 = 2.84;
const HOME_YAW: f32 = 0.0;
const HOME_PITCH: f32 = 0.055;

const CONTROLS_LAYOUT_W: f32 = 200.0;
const CONTROLS_LAYOUT_H: f32 = 100.0;
const CONTROLS_WORLD_W: f32 = 1.2;
const CONTROLS_FONT_SIZE: f32 = 9.0;
const CONTROLS_TITLE_SIZE: f32 = 10.5;
const CONTROLS_ARROW_SIZE: f32 = CONTROLS_FONT_SIZE * 0.5;
const CONTROLS_ROW_HEIGHT: f32 = CONTROLS_FONT_SIZE * 1.4;
const CONTROLS_TITLE_COLOR: Color = Color::srgb(0.42, 0.5, 0.72);

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

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            PanOrbitCameraPlugin,
            PanOrbitCameraExtPlugin,
            BrpExtrasPlugin::default().port_in_title(PortDisplay::NonDefault),
            WindowManagerPlugin,
            MeshPickingPlugin,
            DiegeticUiPlugin,
        ))
        .insert_resource(WordCycle {
            index: 0,
            timer: Timer::from_seconds(0.15, TimerMode::Repeating),
        })
        .init_resource::<FontHandles>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (toggle_overlay, home_camera, switch_font, cycle_word),
        )
        .add_observer(on_world_text_added)
        .add_observer(on_font_registered)
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
            Mesh3d(meshes.add(Plane3d::default().mesh().size(5.4, 5.4))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.08, 0.08, 0.08),
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
        ))
        .observe(on_ground_clicked)
        .id();

    commands.insert_resource(SceneBounds(ground));

    // Display word with typography overlay.
    commands
        .spawn((
            DisplayText,
            WorldText::new(DISPLAY_WORDS[0]),
            WorldTextStyle::new()
                .with_size(DISPLAY_SIZE)
                .with_color(Color::srgb(0.9, 0.9, 0.9))
                .with_loading_policy(GlyphLoadingPolicy::Progressive),
            TypographyOverlay::default(),
            Transform::from_xyz(0.0, 0.5, 2.0),
        ))
        .observe(
            |trigger: On<TypographyOverlayReady>,
             cameras: Query<Entity, With<PanOrbitCamera>>,
             mut commands: Commands| {
                info!("TypographyOverlayReady: {:?}", trigger.entity);
                for camera in &cameras {
                    commands.trigger(
                        ZoomToFit::new(camera, trigger.entity)
                            .margin(ZOOM_TO_FIT_MARGIN)
                            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
                    );
                }
                // Only zoom on initial load — remove this observer.
                commands.entity(trigger.observer()).despawn();
            },
        );

    // Hint text
    commands.spawn((
        WorldText::new("Click text to zoom in · Click plane to zoom out"),
        WorldTextStyle::new()
            .with_size(0.02)
            .with_color(Color::srgba(0.6, 0.6, 0.6, 0.8)),
        Transform::from_xyz(0.0, 0.0, 3.45),
    ));

    spawn_lights(&mut commands);

    // Controls panel — upper left.
    commands
        .spawn((
            ControlsPanel,
            DiegeticPanel {
                tree: build_controls_panel(),
                width: CONTROLS_LAYOUT_W,
                height: CONTROLS_LAYOUT_H,
                layout_unit: Some(Unit::Custom(CONTROLS_WORLD_W / CONTROLS_LAYOUT_W)),
                ..default()
            },
            Transform::from_xyz(-1.2, 1.5, 0.5),
        ))
        .observe(on_panel_clicked);

    // Fonts panel — upper right (symmetric with controls).
    commands
        .spawn((
            FontsPanel,
            DiegeticPanel {
                tree: build_fonts_panel(&registry),
                width: CONTROLS_LAYOUT_W,
                height: CONTROLS_LAYOUT_H,
                layout_unit: Some(Unit::Custom(CONTROLS_WORLD_W / CONTROLS_LAYOUT_W)),
                ..default()
            },
            Transform::from_xyz(1.2, 1.5, 0.5),
        ))
        .observe(on_panel_clicked);

    // Camera
    commands.spawn((PanOrbitCamera {
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
        trackpad_sensitivity: 0.5,
        trackpad_pinch_to_zoom_enabled: true,
        ..default()
    },));
}

fn spawn_lights(commands: &mut Commands) {
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(0.0, 1.5, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.0, 1.5, -3.0).looking_at(Vec3::ZERO, Vec3::Y),
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

fn on_panel_clicked(mut click: On<Pointer<Click>>, mut commands: Commands) {
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

fn build_controls_panel() -> bevy_diegetic::LayoutTree {
    let border_color = Color::srgb(0.4, 0.4, 0.45);
    let divider_color = Color::srgb(0.45, 0.45, 0.5);
    let row_h = Sizing::fixed(CONTROLS_ROW_HEIGHT);
    let cfg = LayoutTextStyle::new(CONTROLS_FONT_SIZE);
    let arrow_cfg = LayoutTextStyle::new(CONTROLS_ARROW_SIZE);

    let mut builder = LayoutBuilder::new(CONTROLS_LAYOUT_W, CONTROLS_LAYOUT_H);
    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .padding(Padding::all(2.5))
            .direction(Direction::TopToBottom)
            .child_gap(1.5)
            .background(Color::srgba(0.1, 0.1, 0.12, 0.85))
            .border(Border::all(1.0, border_color)),
        |b| {
            b.text(
                "controls",
                LayoutTextStyle::new(CONTROLS_TITLE_SIZE).with_color(CONTROLS_TITLE_COLOR),
            );
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::fixed(0.3))
                    .background(divider_color),
                |_| {},
            );
            // Three columns with fixed row heights.
            b.with(
                El::new()
                    .width(Sizing::FIT)
                    .height(Sizing::FIT)
                    .direction(Direction::LeftToRight)
                    .child_gap(2.0),
                |b| {
                    // Key column (centered).
                    column(
                        b,
                        AlignX::Center,
                        row_h,
                        &[
                            ColumnCell::Text("t", cfg.clone()),
                            ColumnCell::Text("h", cfg.clone()),
                            ColumnCell::Text("\u{2190} \u{2192}", cfg.clone()),
                        ],
                    );
                    // Arrow column (centered).
                    column(
                        b,
                        AlignX::Center,
                        row_h,
                        &[
                            ColumnCell::Text("  ->  ", arrow_cfg.clone()),
                            ColumnCell::Text("  ->  ", arrow_cfg.clone()),
                            ColumnCell::Text("  ->  ", arrow_cfg.clone()),
                        ],
                    );
                    // Description column (left-aligned).
                    column(
                        b,
                        AlignX::Left,
                        row_h,
                        &[
                            ColumnCell::Text("toggle overlay", cfg.clone()),
                            ColumnCell::Text("home camera", cfg.clone()),
                            ColumnCell::Text("cycle word", cfg.clone()),
                        ],
                    );
                },
            );
        },
    );
    builder.build()
}

fn build_fonts_panel(registry: &FontRegistry) -> bevy_diegetic::LayoutTree {
    let border_color = Color::srgb(0.4, 0.4, 0.45);
    let divider_color = Color::srgb(0.45, 0.45, 0.5);
    let row_h = Sizing::fixed(CONTROLS_ROW_HEIGHT);
    let cfg = LayoutTextStyle::new(CONTROLS_FONT_SIZE);
    let arrow_cfg = LayoutTextStyle::new(CONTROLS_ARROW_SIZE);

    let key_cells: Vec<ColumnCell> = FONT_KEYS
        .iter()
        .map(|(label, _, _)| ColumnCell::Text(label, cfg.clone()))
        .collect();
    let arrow_cells: Vec<ColumnCell> = FONT_KEYS
        .iter()
        .map(|_| ColumnCell::Text("  ->  ", arrow_cfg.clone()))
        .collect();
    let name_cells: Vec<ColumnCell> = FONT_KEYS
        .iter()
        .map(|(_, name, _)| {
            let font_id = registry
                .font_id_by_name(name)
                .unwrap_or(FontId::MONOSPACE)
                .0;
            ColumnCell::Text(name, cfg.clone().with_font(font_id))
        })
        .collect();

    let mut builder = LayoutBuilder::new(CONTROLS_LAYOUT_W, CONTROLS_LAYOUT_H);
    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .padding(Padding::all(2.5))
            .direction(Direction::TopToBottom)
            .child_gap(1.5)
            .background(Color::srgba(0.1, 0.1, 0.12, 0.85))
            .border(Border::all(1.0, border_color)),
        |b| {
            b.text(
                "fonts",
                LayoutTextStyle::new(CONTROLS_TITLE_SIZE).with_color(CONTROLS_TITLE_COLOR),
            );
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::fixed(0.3))
                    .background(divider_color),
                |_| {},
            );
            b.with(
                El::new()
                    .width(Sizing::FIT)
                    .height(Sizing::FIT)
                    .direction(Direction::LeftToRight)
                    .child_gap(2.0),
                |b| {
                    column(b, AlignX::Center, row_h, &key_cells);
                    column(b, AlignX::Center, row_h, &arrow_cells);
                    column(b, AlignX::Left, row_h, &name_cells);
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
    mut panels: Query<&mut DiegeticPanel, With<FontsPanel>>,
    registry: Res<FontRegistry>,
) {
    info!(
        "FontRegistered: {} (id: {}, {:?})",
        trigger.name, trigger.id.0, trigger.source
    );
    for mut panel in &mut panels {
        info!("Rebuilding fonts panel");
        panel.tree = build_fonts_panel(&registry);
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
    cameras: Query<Entity, With<PanOrbitCamera>>,
    mut commands: Commands,
) {
    if !keyboard.just_pressed(KeyCode::KeyH) {
        return;
    }
    for camera in &cameras {
        commands.trigger(PlayAnimation::new(
            camera,
            [CameraMove::ToOrbit {
                focus:    HOME_FOCUS,
                yaw:      HOME_YAW,
                pitch:    HOME_PITCH,
                radius:   HOME_RADIUS,
                duration: Duration::from_millis(ZOOM_DURATION_MS),
                easing:   bevy::math::curve::easing::EaseFunction::CubicOut,
            }],
        ));
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
    mut texts: Query<&mut WorldTextStyle, With<DisplayText>>,
) {
    let pressed = FONT_KEYS
        .iter()
        .find(|(_, _, key)| keyboard.just_pressed(*key));
    let Some((_, name, _)) = pressed else {
        return;
    };
    let font_id = registry
        .font_id_by_name(name)
        .unwrap_or(FontId::MONOSPACE)
        .0;
    for mut style in &mut texts {
        *style = WorldTextStyle::new()
            .with_font(font_id)
            .with_size(DISPLAY_SIZE)
            .with_color(Color::srgb(0.9, 0.9, 0.9));
    }
}
