//! `draw_layer` — authored draw order for text runs relative to panel
//! backings.
//!
//! Two content panels show the same three text tiers while a translucent
//! shade panel slides across each one:
//!
//! - body runs authored at draw layer 8 sit below the shade's backing and dim behind it in passing
//! - default-layer runs keep the text-above-backings default and stay bright over the shade
//! - one run authored at layer 72 is explicitly above everything (on OIT views, layers at or above
//!   the default clamp to the default's offset, so it composites like default text there)
//!
//! The world panel renders under order-independent transparency
//! ([`StableTransparency`] on the orbit camera); the screen panel renders on
//! the sorted screen view. One authored ordinal derives both the
//! `Transparent3d` sort bias and the OIT fragment depth offset, so the tiers
//! order the same way on both views.
//!
//! Cross-panel order combines each draw slot's bias with the shade's depth
//! offset. On the screen view both are logical pixels (8 < 16 < 64). On the
//! world view the 6 mm separation becomes an NDC depth delta of one OIT step
//! per millimeter per meter of orbit radius (the near plane tracks the
//! radius), so it must land between the body layer's 8 steps and the default
//! layer's 64: far enough out the body text stops dimming, close enough in
//! default text dips under the shade. The within-panel tiers are
//! distance-independent.
//!
//! Hotkeys:
//! - `H` — home the camera.
//! - `K` — toggle the default-layer run: `override_draw_layer` drops it to the body layer (it dims
//!   behind the shade), `inherit_draw_layer` removes the override so it resolves back to the
//!   cascade default. The authored tree carries no memory of a removed override — inherit always
//!   lands on the default, which is why the toggle targets the run that starts without one.

use bevy::prelude::*;
use bevy_diegetic::Anchor;
use bevy_diegetic::AnchoredToPanel;
use bevy_diegetic::Border;
use bevy_diegetic::CascadeEntityCommandsExt;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::Direction;
use bevy_diegetic::DrawZIndex;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Mm;
use bevy_diegetic::Padding;
use bevy_diegetic::PanelAnchorOffset;
use bevy_diegetic::PanelBuildError;
use bevy_diegetic::PanelFieldId;
use bevy_diegetic::PanelTextLayout;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextStyle;
use bevy_diegetic::Unit;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::CameraHomeTarget;
use fairy_dust::TitleBar;
use fairy_dust::screen_panel_material;

// camera
const CAMERA_FOCUS: Vec3 = Vec3::ZERO;
const CAMERA_PITCH: f32 = 0.0;
/// Camera-home refits to the world panel on startup (≈0.3 m), so this only
/// seeds the pre-fit pose.
const CAMERA_RADIUS: f32 = 0.4;
const CAMERA_YAW: f32 = 0.0;
const HOME_MARGIN: f32 = 0.3;

// colors
const BODY_COLOR: Color = Color::srgba(0.82, 0.88, 0.96, 0.95);
const CONTENT_ACCENT: Color = Color::srgb(0.25, 0.65, 0.95);
const CONTENT_BACKGROUND: Color = Color::srgba(0.07, 0.11, 0.17, 0.94);
const DIVIDER_COLOR: Color = Color::srgba(0.25, 0.55, 0.80, 0.45);
const SHADE_ACCENT: Color = Color::srgba(0.85, 0.65, 0.25, 0.90);
const SHADE_BACKGROUND: Color = Color::srgba(0.03, 0.05, 0.09, 0.80);
const STATUS_COLOR: Color = Color::srgba(0.60, 0.68, 0.78, 0.90);
const TITLE_COLOR: Color = Color::srgb(0.92, 0.96, 1.0);
const TOP_COLOR: Color = Color::srgb(1.0, 0.78, 0.30);

// draw layers
/// Body runs: above the content panel's own backing geometry (background,
/// divider, border — draw slots 0..=2) and below the shade backing's
/// effective bias (its slot 0 plus the depth offset).
const DIM_TEXT_LAYER: DrawZIndex = DrawZIndex(8);
/// Above the default layer (64). The sorted screen view honors the full
/// value; OIT views clamp layers at or above the default to the default's
/// offset, so this run composites like default text there.
const TOP_TEXT_LAYER: DrawZIndex = DrawZIndex(72);

// layout (content units: millimeters on the world panel, scaled to logical
// pixels on the screen panel)
const BODY_SIZE: f32 = 6.5;
const BORDER_WIDTH: f32 = 1.2;
const CONTENT_HEIGHT: f32 = 130.0;
const CONTENT_WIDTH: f32 = 150.0;
const DIVIDER_HEIGHT: f32 = 1.2;
const LINE_GAP: f32 = 2.5;
const PANEL_PADDING: f32 = 8.0;
const SCREEN_SCALE: f32 = 2.5;
const SECTION_GAP: f32 = 5.0;
const STATUS_SIZE: f32 = 5.0;
const TITLE_SIZE: f32 = 10.0;

// shade
/// Depth offsets sit between the body layer's bias and the default text
/// layer's. Screen view: logical pixels, 8 < 16 < 64 exactly. World view:
/// `bevy_lagrange` syncs the near plane to `radius × 0.001`, so the shade's
/// NDC depth delta is one `OIT_DEPTH_STEP` per millimeter per meter of
/// radius; 6 mm sits between the 8-step and 64-step bounds for radii of
/// 0.094 m to 0.75 m (camera-home lands near 0.24 m, ~25 steps).
const SHADE_DEPTH_MM: f32 = 6.0;
const SHADE_DEPTH_PX: f32 = 16.0;
const SHADE_HEIGHT: f32 = 142.0;
const SHADE_SWING: f32 = 55.0;
const SHADE_WIDTH: f32 = 56.0;
const SLIDE_RATE: f32 = 0.7;

// text
/// Panel-local id of the default-layer run; the `K` toggle finds the run's
/// line entities by it (a wrapped run spawns one label entity per line, all
/// carrying the same run id in [`PanelTextLayout`]).
const DEFAULT_RUN_ID: &str = "default-run";

/// Which view a panel pair renders on: the OIT world camera or the sorted
/// screen camera.
#[derive(Clone, Copy)]
enum ViewSide {
    World,
    Screen,
}

impl ViewSide {
    /// Content-unit multiplier: world trees are authored in millimeters,
    /// screen trees in logical pixels.
    const fn scale(self) -> f32 {
        match self {
            Self::World => 1.0,
            Self::Screen => SCREEN_SCALE,
        }
    }

    /// Shade depth offset toward the viewer, in the content panel's layout
    /// unit.
    const fn shade_depth(self) -> f32 {
        match self {
            Self::World => SHADE_DEPTH_MM,
            Self::Screen => SHADE_DEPTH_PX,
        }
    }

    /// Horizontal swing amplitude in the content panel's layout unit.
    const fn shade_swing(self) -> f32 { SHADE_SWING * self.scale() }

    const fn label(self) -> &'static str {
        match self {
            Self::World => "world",
            Self::Screen => "screen",
        }
    }

    const fn view_line(self) -> &'static str {
        match self {
            Self::World => "world view / OIT",
            Self::Screen => "screen view / sorted",
        }
    }

    fn layout_builder(self, width: f32, height: f32) -> LayoutBuilder {
        match self {
            Self::World => LayoutBuilder::new(Mm(width), Mm(height)),
            Self::Screen => LayoutBuilder::new(Px(width * SCREEN_SCALE), Px(height * SCREEN_SCALE)),
        }
    }
}

/// Per-frame anchor-offset animation for a shade panel.
#[derive(Component)]
struct SlidingShade {
    /// Horizontal swing amplitude in the target panel's layout unit.
    swing: f32,
    /// Depth offset toward the viewer in the target panel's layout unit.
    depth: f32,
}

/// Whether the default-layer run currently carries a draw-layer override
/// (`K` toggle). `Inherited` resolves to the cascade default; `Dropped`
/// overrides to [`DIM_TEXT_LAYER`], putting the run behind the shade.
#[derive(Resource, Clone, Copy, Default, PartialEq)]
enum DefaultRunLayer {
    #[default]
    Inherited,
    Dropped,
}

impl DefaultRunLayer {
    const fn toggled(self) -> Self {
        match self {
            Self::Inherited => Self::Dropped,
            Self::Dropped => Self::Inherited,
        }
    }
}

fn main() {
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_orbit_cam_preset(
            |cam| {
                cam.focus = CAMERA_FOCUS;
                cam.radius = Some(CAMERA_RADIUS);
                cam.yaw = Some(CAMERA_YAW);
                cam.pitch = Some(CAMERA_PITCH);
            },
            OrbitCamPreset::BlenderLike,
        )
        .with_stable_transparency()
        .with_camera_home()
        .margin(HOME_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .with_title("Text Draw Layer")
                .control("H Home")
                .control("K Drop Default Run"),
        )
        .with_camera_control_panel()
        .init_resource::<DefaultRunLayer>()
        .add_systems(Startup, setup)
        .add_systems(Update, slide_shades)
        .with_shortcut(KeyCode::KeyK, toggle_default_run_layer)
        .run();
}

fn setup(mut commands: Commands) {
    spawn_view(&mut commands, ViewSide::World);
    spawn_view(&mut commands, ViewSide::Screen);
}

/// Spawns one content panel and its sliding shade on the given view.
fn spawn_view(commands: &mut Commands, side: ViewSide) {
    let Ok(content_panel) = build_content_panel(side) else {
        error!("draw_layer: failed to build {} content panel", side.label());
        return;
    };
    let target = match side {
        ViewSide::World => commands
            .spawn((
                Name::new("World content panel"),
                CameraHomeTarget,
                content_panel,
                Transform::default(),
            ))
            .id(),
        ViewSide::Screen => commands
            .spawn((
                Name::new("Screen content panel"),
                content_panel,
                Transform::default(),
            ))
            .id(),
    };

    let Ok(shade_panel) = build_shade_panel(side) else {
        error!("draw_layer: failed to build {} shade panel", side.label());
        return;
    };
    commands.spawn((
        Name::new(format!("{} shade panel", side.label())),
        shade_panel,
        Transform::default(),
        AnchoredToPanel::new(target, Anchor::Center, Anchor::Center)
            .with_offset(PanelAnchorOffset::new(0.0, 0.0).with_z(side.shade_depth())),
        SlidingShade {
            swing: side.shade_swing(),
            depth: side.shade_depth(),
        },
    ));
}

/// Sweeps each shade across its target panel by re-inserting the anchor
/// relationship with a new horizontal offset.
fn slide_shades(
    time: Res<Time>,
    shades: Query<(Entity, &AnchoredToPanel, &SlidingShade)>,
    mut commands: Commands,
) {
    let phase = (time.elapsed_secs() * SLIDE_RATE).sin();
    for (entity, anchored, sliding_shade) in &shades {
        let offset =
            PanelAnchorOffset::new(phase * sliding_shade.swing, 0.0).with_z(sliding_shade.depth);
        commands.entity(entity).insert(anchored.with_offset(offset));
    }
}

/// `K` — toggles the default-layer run on both panels between an explicit
/// [`DIM_TEXT_LAYER`] override (dims behind the shade) and no override
/// (resolves to the cascade default and composites above it again). Each
/// line of the wrapped run is its own label entity; all carry the run id.
fn toggle_default_run_layer(
    mut state: ResMut<DefaultRunLayer>,
    lines: Query<(Entity, &PanelTextLayout)>,
    mut commands: Commands,
) {
    *state = state.toggled();
    let default_run_id = PanelFieldId::named(DEFAULT_RUN_ID);
    for (entity, layout) in &lines {
        if layout.id != default_run_id {
            continue;
        }
        let mut label = commands.entity(entity);
        match *state {
            DefaultRunLayer::Dropped => {
                label.override_draw_layer(DIM_TEXT_LAYER);
            },
            DefaultRunLayer::Inherited => {
                label.inherit_draw_layer();
            },
        }
    }
}

fn build_content_panel(side: ViewSide) -> Result<DiegeticPanel, PanelBuildError> {
    let tree = content_tree(side);
    match side {
        ViewSide::World => DiegeticPanel::world()
            .size(Mm(CONTENT_WIDTH), Mm(CONTENT_HEIGHT))
            .font_unit(Unit::Millimeters)
            .anchor(Anchor::Center)
            .with_tree(tree)
            .build(),
        ViewSide::Screen => {
            let unlit = screen_panel_material();
            DiegeticPanel::screen()
                .size(
                    Px(CONTENT_WIDTH * SCREEN_SCALE),
                    Px(CONTENT_HEIGHT * SCREEN_SCALE),
                )
                .font_unit(Unit::Pixels)
                .anchor(Anchor::TopRight)
                .material(unlit.clone())
                .text_material(unlit)
                .with_tree(tree)
                .build()
        },
    }
}

fn build_shade_panel(side: ViewSide) -> Result<DiegeticPanel, PanelBuildError> {
    let tree = shade_tree(side);
    match side {
        ViewSide::World => DiegeticPanel::world()
            .size(Mm(SHADE_WIDTH), Mm(SHADE_HEIGHT))
            .anchor(Anchor::Center)
            .with_tree(tree)
            .build(),
        ViewSide::Screen => DiegeticPanel::screen()
            .size(
                Px(SHADE_WIDTH * SCREEN_SCALE),
                Px(SHADE_HEIGHT * SCREEN_SCALE),
            )
            .anchor(Anchor::Center)
            .material(screen_panel_material())
            .with_tree(tree)
            .build(),
    }
}

fn content_tree(side: ViewSide) -> LayoutTree {
    let s = side.scale();
    let title_style = TextStyle::new(TITLE_SIZE * s).with_color(TITLE_COLOR);
    let dim_style = TextStyle::new(BODY_SIZE * s)
        .with_color(BODY_COLOR)
        .with_draw_layer(DIM_TEXT_LAYER);
    let top_style = TextStyle::new(BODY_SIZE * s)
        .with_color(TOP_COLOR)
        .with_draw_layer(TOP_TEXT_LAYER);
    let default_style = TextStyle::new(BODY_SIZE * s).with_color(BODY_COLOR);
    let status_style = TextStyle::new(STATUS_SIZE * s).with_color(STATUS_COLOR);

    let mut builder = side.layout_builder(CONTENT_WIDTH, CONTENT_HEIGHT);
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(PANEL_PADDING * s))
            .direction(Direction::TopToBottom)
            .child_gap(SECTION_GAP * s)
            .background(CONTENT_BACKGROUND)
            .border(Border::all(BORDER_WIDTH * s, CONTENT_ACCENT)),
        |b| {
            b.text("TEXT DRAW LAYER", title_style);
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::fixed(DIVIDER_HEIGHT * s))
                    .background(DIVIDER_COLOR),
                |_| {},
            );
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .direction(Direction::TopToBottom)
                    .child_gap(LINE_GAP * s),
                |b| {
                    b.text(
                        format!(
                            "Body runs are authored at draw layer {}, below the sliding shade's \
                             backing, so the shade dims them in passing.",
                            DIM_TEXT_LAYER.0
                        ),
                        dim_style,
                    );
                    b.text(
                        format!("AUTHORED AT LAYER {} - ABOVE ALL", TOP_TEXT_LAYER.0),
                        top_style,
                    );
                    b.text_id(
                        PanelFieldId::named(DEFAULT_RUN_ID),
                        "Default-layer text stays above the shade.",
                        default_style,
                    );
                },
            );
            b.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
            b.text(side.view_line(), status_style);
        },
    );
    builder.build()
}

fn shade_tree(side: ViewSide) -> LayoutTree {
    let s = side.scale();
    let mut builder = side.layout_builder(SHADE_WIDTH, SHADE_HEIGHT);
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .background(SHADE_BACKGROUND)
            .border(Border::all(BORDER_WIDTH * s, SHADE_ACCENT)),
        |_| {},
    );
    builder.build()
}
