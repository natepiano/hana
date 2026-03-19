//! Text stress test — add/remove rows to measure per-element rendering cost.
//!
//! Press '+' to add a row, '-' to remove one (hold for continuous).
//! FPS updates on a 1-second timer.
//! Rows fill a column top-to-bottom based on available screen height.
//! When a column fills, a new column is added to the right within the
//! same panel layout.

use bevy::diagnostic::DiagnosticsStore;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_diegetic::Border;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextConfig;

const FONT_SIZE: f32 = 6.0;
const MARGIN: f32 = 0.1;
/// Fixed scale: layout units per world unit. Never changes with window size.
const SCALE: f32 = 300.0 / 3.0;
const ROW_HEIGHT: f32 = FONT_SIZE + 1.0;
/// Header: controls text + divider.
const HEADER_HEIGHT: f32 = ROW_HEIGHT + 1.0;
const COLUMN_PADDING: f32 = 5.0;
const ROW_GAP: f32 = 5.0;

/// Source text to pull words from for row values.
const SOURCE_TEXT: &str = "bevy diegetic layout engine text rendering msdf atlas glyph quad mesh \
    shader pipeline parley shaping font registry plugin system resource component query transform \
    camera projection orthographic perspective viewport world entity spawn despawn commands insert \
    remove compute propagate sizing grow fit fixed percent padding border direction align children \
    element builder tree result render command rectangle scissor culling material texture sampler \
    uniform binding vertex index normal color alpha blend mask discard fragment screen pixel range \
    distance median clamp smooth antialiasing kerning advance baseline ascent descent bearing \
    rasterize bitmap canonical prepopulate allocator shelf etagere unicode bidi cluster glyph";
const KEY_REPEAT_SECS: f32 = 0.05;
const FPS_UPDATE_INTERVAL: f32 = 1.0;

const BORDER_COLOR: bevy::color::Color = bevy::color::Color::srgb(0.39, 0.43, 0.47);
const BG_COLOR: bevy::color::Color = bevy::color::Color::srgb(0.157, 0.173, 0.204);
const DIVIDER_COLOR: bevy::color::Color = bevy::color::Color::srgb(0.235, 0.51, 0.706);

#[derive(Resource)]
struct StressState {
    repeat_timer:  Timer,
    row_count:     usize,
    /// Cached layout/world dimensions from window size.
    layout_width:  f32,
    layout_height: f32,
    world_width:   f32,
    world_height:  f32,
}

impl Default for StressState {
    fn default() -> Self {
        Self {
            repeat_timer:  Timer::from_seconds(KEY_REPEAT_SECS, TimerMode::Repeating),
            row_count:     0,
            layout_width:  200.0,
            layout_height: 300.0,
            world_width:   2.0,
            world_height:  3.0,
        }
    }
}

#[derive(Component)]
struct StressPanel;

#[derive(Component)]
struct StressCamera;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(BrpExtrasPlugin::default())
        .add_plugins(DiegeticUiPlugin)
        .init_resource::<StressState>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                handle_input,
                handle_resize,
                update_fps_overlay,
                update_panel,
            ),
        )
        .run();
}

#[derive(Component)]
struct FpsOverlay;

fn setup(mut commands: Commands) {
    // 2D FPS overlay — uses Bevy's native text, no panel rebuild.
    commands.spawn((
        FpsOverlay,
        Text::new("fps: --  ms: --"),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            right: Val::Px(8.0),
            ..default()
        },
    ));

    commands.spawn((
        StressCamera,
        Camera3d::default(),
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::camera::ScalingMode::FixedVertical {
                viewport_height: 5.0,
            },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(0.0, 0.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((
        StressPanel,
        DiegeticPanel {
            tree:          LayoutBuilder::new(200.0, 300.0).build(),
            layout_width:  200.0,
            layout_height: 300.0,
            world_width:   2.0,
            world_height:  3.0,
        },
        Transform::IDENTITY,
    ));
}

fn handle_resize(
    mut camera: Query<(&mut Projection, &mut Transform), With<StressCamera>>,
    mut state: ResMut<StressState>,
    mut panels: Query<
        (&mut DiegeticPanel, &mut Transform),
        (With<StressPanel>, Without<StressCamera>),
    >,
    windows: Query<&Window>,
    mut last_size: Local<(f32, f32)>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let (w, h) = (window.width(), window.height());
    if (w - last_size.0).abs() < 1.0 && (h - last_size.1).abs() < 1.0 {
        return;
    }
    *last_size = (w, h);

    // Fixed ratio: world units per screen pixel. This keeps text the same
    // physical size on screen regardless of window dimensions.
    let world_per_pixel = 5.0 / 1080.0;
    let viewport_height = h * world_per_pixel;
    let viewport_width = w * world_per_pixel;

    state.world_height = viewport_height - MARGIN * 2.0;
    state.world_width = viewport_width - MARGIN * 2.0;
    // Derive layout dimensions from fixed scale — text stays the same
    // physical size regardless of window height.
    state.layout_height = state.world_height * SCALE;
    state.layout_width = state.world_width * SCALE;

    let Ok((mut proj, mut cam_transform)) = camera.single_mut() else {
        return;
    };
    *proj = Projection::Orthographic(OrthographicProjection {
        scaling_mode: bevy::camera::ScalingMode::FixedVertical { viewport_height },
        ..OrthographicProjection::default_3d()
    });
    cam_transform.translation.x = viewport_width * 0.5;
    cam_transform.translation.y = viewport_height * 0.5;

    // Reposition panel: top-left at (MARGIN, viewport_height - MARGIN).
    for (mut panel, mut transform) in &mut panels {
        panel.layout_width = state.layout_width;
        panel.layout_height = state.layout_height;
        panel.world_width = state.world_width;
        panel.world_height = state.world_height;
        transform.translation.x = MARGIN + state.world_width * 0.5;
        transform.translation.y = viewport_height - MARGIN - state.world_height * 0.5;
    }
}

fn handle_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut state: ResMut<StressState>,
) {
    if keyboard.just_pressed(KeyCode::Equal) {
        state.row_count += 1;
        state.repeat_timer.reset();
        return;
    }
    if keyboard.just_pressed(KeyCode::Minus) && state.row_count > 0 {
        state.row_count -= 1;
        state.repeat_timer.reset();
        return;
    }
    if keyboard.pressed(KeyCode::Equal) || keyboard.pressed(KeyCode::Minus) {
        state.repeat_timer.tick(time.delta());
        if state.repeat_timer.just_finished() {
            if keyboard.pressed(KeyCode::Equal) {
                state.row_count += 1;
            } else if state.row_count > 0 {
                state.row_count -= 1;
            }
        }
    }
}

fn rows_per_column(layout_height: f32, is_first: bool) -> usize {
    let available = if is_first {
        layout_height - HEADER_HEIGHT
    } else {
        layout_height
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = (available / ROW_HEIGHT) as usize;
    count.max(1)
}

fn update_fps_overlay(
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    state: Res<StressState>,
    mut overlay: Query<&mut Text, With<FpsOverlay>>,
    mut timer: Local<Option<Timer>>,
) {
    let timer =
        timer.get_or_insert_with(|| Timer::from_seconds(FPS_UPDATE_INTERVAL, TimerMode::Repeating));
    timer.tick(time.delta());
    if !timer.just_finished() {
        return;
    }
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(bevy::diagnostic::Diagnostic::smoothed);
    let frame_ms = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(bevy::diagnostic::Diagnostic::smoothed);
    let fps_str = fps.map_or_else(|| "--".to_string(), |v| format!("{v:.0}"));
    let ms_str = frame_ms.map_or_else(|| "--".to_string(), |v| format!("{v:.1}"));
    for mut text in &mut overlay {
        **text = format!("fps: {fps_str}  ms: {ms_str}  rows: {}", state.row_count);
    }
}

fn update_panel(state: Res<StressState>, mut panels: Query<&mut DiegeticPanel, With<StressPanel>>) {
    if !state.is_changed() {
        return;
    }

    let t0 = std::time::Instant::now();
    let tree = build_tree(&state);
    let tree_elapsed = t0.elapsed();
    bevy::log::info!("build_tree: {} rows | {tree_elapsed:?}", state.row_count);
    for mut panel in &mut panels {
        panel.tree = tree.clone();
    }
}

fn build_tree(state: &StressState) -> bevy_diegetic::LayoutTree {
    let first_col_rows = rows_per_column(state.layout_height, true);
    let other_col_rows = rows_per_column(state.layout_height, false);

    let needed_cols = if state.row_count <= first_col_rows {
        1
    } else {
        let remaining = state.row_count - first_col_rows;
        1 + (remaining + other_col_rows - 1) / other_col_rows
    };

    let mut builder = LayoutBuilder::new(state.layout_width, state.layout_height);

    let words: Vec<&str> = SOURCE_TEXT.split_whitespace().collect();

    // Root: horizontal flow of columns.
    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight)
            .child_gap(COLUMN_PADDING)
            .padding(Padding::all(2.0))
            .background(BG_COLOR)
            .border(Border::all(1.0, BORDER_COLOR)),
        |b| {
            for col_idx in 0..needed_cols {
                // Each column: vertical flow of rows.
                b.with(
                    El::new()
                        .width(Sizing::FIT)
                        .height(Sizing::GROW)
                        .direction(Direction::TopToBottom)
                        .child_gap(1.0)
                        .padding(Padding::all(2.0))
                        .border(Border::all(1.0, BORDER_COLOR)),
                    |b| {
                        // First column gets the header.
                        if col_idx == 0 {
                            b.text("'+' add  '-' remove", TextConfig::new(FONT_SIZE));
                            b.with(
                                El::new()
                                    .width(Sizing::GROW)
                                    .height(Sizing::fixed(1.0))
                                    .background(DIVIDER_COLOR),
                                |_| {},
                            );
                        }

                        // Data rows for this column.
                        let (start, end) = if col_idx == 0 {
                            (0, first_col_rows.min(state.row_count))
                        } else {
                            let s = first_col_rows + (col_idx - 1) * other_col_rows;
                            let e = (s + other_col_rows).min(state.row_count);
                            (s, e)
                        };

                        for i in start..end {
                            let label = format!("item {i}:");
                            let value = words[i % words.len()];
                            // Rainbow hue based on item index across total row count.
                            #[allow(clippy::cast_precision_loss)]
                            let hue = if state.row_count > 0 {
                                360.0 * (i as f32 / state.row_count as f32)
                            } else {
                                0.0
                            };
                            let color = Color::hsl(hue, 0.8, 0.6);
                            key_value_row_colored(b, &label, value, color);
                        }
                    },
                );
            }
        },
    );

    builder.build()
}

fn key_value_row_colored(b: &mut LayoutBuilder, label: &str, value: &str, color: Color) {
    let config = TextConfig::new(FONT_SIZE).with_color(color);
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight)
            .child_gap(ROW_GAP),
        |b| {
            b.text(label, config.clone());
            b.with(
                El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                |_| {},
            );
            b.text(value, config);
        },
    );
}
