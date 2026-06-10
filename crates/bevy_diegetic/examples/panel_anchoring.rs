//! Panel-to-panel anchoring example.
//!
//! Left/right cycles which anchor on the dependent panel follows the target.
//! Up/down cycles which anchor on the target panel is followed. Holding
//! minus/equal moves the depth offset continuously along the target's plane
//! normal. The active anchor markers are elements of each panel's layout
//! tree; a gizmo line links the two anchor points when a depth offset
//! separates them. The bottom-left info panel names the active anchors and
//! depth.

use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::AnchoredToPanel;
use bevy_diegetic::Border;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticPanelCommands;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::Fit;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Mm;
use bevy_diegetic::PanelAnchorGeometryParam;
use bevy_diegetic::PanelAnchorOffset;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextStyle;
use bevy_diegetic::Unit;
use bevy_diegetic::default_panel_material;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::CameraHomeTarget;
use fairy_dust::DEFAULT_PANEL_BACKGROUND;
use fairy_dust::TitleBar;
use fairy_dust::screen_panel_frame;
use fairy_dust::screen_panel_material;

const PANEL_WIDTH: f32 = 108.0;
const PANEL_HEIGHT: f32 = 78.0;
const BORDER_WIDTH: f32 = 1.2;
const HOME_MARGIN: f32 = 0.22;
const ANCHOR_MARKER_SIZE: f32 = 10.0;
const MARKER_ALPHA: f32 = 0.72;
const ANCHOR_LINK_MIN_LENGTH: f32 = 0.001;
const INFO_PANEL_WIDTH: f32 = 230.0;
const INFO_TITLE_SIZE: f32 = 16.0;
const INFO_BODY_SIZE: f32 = 12.0;
const INFO_SECTION_GAP: f32 = 8.0;
const INFO_ROW_GAP: f32 = 4.0;
const INFO_COL_GAP: f32 = 10.0;
const INFO_GRID_CELL_SIZE: f32 = 9.0;
const INFO_GRID_GAP: f32 = 2.0;
const INFO_GRID_BORDER_WIDTH: f32 = 1.0;
const INFO_GRID_SIDE: usize = 3;

const TARGET_POSITION: Vec3 = Vec3::ZERO;
const DEPENDENT_AUTHORED_POSITION: Vec3 = Vec3::new(0.30, -0.18, 0.0);
const CAMERA_FOCUS: Vec3 = Vec3::ZERO;
const CAMERA_RADIUS: f32 = 1.75;
const CAMERA_YAW: f32 = 0.0;
const CAMERA_PITCH: f32 = 0.0;

const TARGET_BACKGROUND: Color = Color::srgba(0.07, 0.12, 0.19, 0.92);
const DEPENDENT_BACKGROUND: Color = Color::srgba(0.10, 0.15, 0.10, 0.92);
const TARGET_ACCENT: Color = Color::srgb(0.22, 0.62, 0.94);
const DEPENDENT_ACCENT: Color = Color::srgb(0.42, 0.86, 0.52);
const TITLE_COLOR: Color = Color::WHITE;
const BODY_COLOR: Color = Color::srgba(0.82, 0.88, 0.96, 0.92);
const INFO_LABEL_COLOR: Color = Color::srgba(0.66, 0.72, 0.82, 0.94);
const INFO_GRID_INACTIVE: Color = Color::srgba(0.12, 0.14, 0.18, 0.95);
const INFO_GRID_BORDER: Color = Color::srgba(0.55, 0.62, 0.72, 0.90);

const ANCHOR_POINTS: [Anchor; 9] = [
    Anchor::TopLeft,
    Anchor::TopCenter,
    Anchor::TopRight,
    Anchor::CenterLeft,
    Anchor::Center,
    Anchor::CenterRight,
    Anchor::BottomLeft,
    Anchor::BottomCenter,
    Anchor::BottomRight,
];

const ANCHOR_NAMES: [&str; 9] = [
    "Top Left",
    "Top Center",
    "Top Right",
    "Center Left",
    "Center",
    "Center Right",
    "Bottom Left",
    "Bottom Center",
    "Bottom Right",
];

const DEFAULT_SOURCE_INDEX: usize = 0;
const DEFAULT_TARGET_INDEX: usize = 8;
const DEPTH_RATE_MM_PER_SEC: f32 = 60.0;

#[derive(Component)]
struct AnchorTarget;

#[derive(Component)]
struct AnchoredDemoPanel;

#[derive(Component)]
struct AnchorInfoPanel;

#[derive(Resource, Clone, Copy, Debug, PartialEq)]
struct AnchorSelection {
    source_index: usize,
    target_index: usize,
    depth_mm:     f32,
}

impl Default for AnchorSelection {
    fn default() -> Self {
        Self {
            source_index: DEFAULT_SOURCE_INDEX,
            target_index: DEFAULT_TARGET_INDEX,
            depth_mm:     0.0,
        }
    }
}

impl AnchorSelection {
    const fn source_anchor(self) -> Anchor { ANCHOR_POINTS[self.source_index] }

    const fn target_anchor(self) -> Anchor { ANCHOR_POINTS[self.target_index] }

    const fn source_label(self) -> &'static str { ANCHOR_NAMES[self.source_index] }

    const fn target_label(self) -> &'static str { ANCHOR_NAMES[self.target_index] }
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
                .with_title("Panel Anchoring")
                .control("Left/Right Source")
                .control("Up/Down Target")
                .control("-/+ Depth")
                .control("R Reset"),
        )
        .with_camera_control_panel()
        .init_resource::<AnchorSelection>()
        .add_systems(Startup, setup)
        .add_systems(Update, cycle_anchor_selection)
        .add_systems(
            PostUpdate,
            draw_anchor_link.after(TransformSystems::Propagate),
        )
        .run();
}

fn setup(mut commands: Commands, selection: Res<AnchorSelection>) {
    let target = build_panel(TARGET_BACKGROUND, TARGET_ACCENT, selection.target_anchor());
    let Ok(target_panel) = target else {
        error!("panel_anchoring: failed to build target panel");
        return;
    };
    let target = commands
        .spawn((
            Name::new("Target panel"),
            AnchorTarget,
            CameraHomeTarget,
            target_panel,
            Transform::from_translation(TARGET_POSITION),
        ))
        .id();

    let dependent = build_panel(
        DEPENDENT_BACKGROUND,
        DEPENDENT_ACCENT,
        selection.source_anchor(),
    );
    let Ok(dependent_panel) = dependent else {
        error!("panel_anchoring: failed to build anchored panel");
        return;
    };

    commands.spawn((
        Name::new("Anchored panel"),
        AnchoredDemoPanel,
        CameraHomeTarget,
        dependent_panel,
        Transform::from_translation(DEPENDENT_AUTHORED_POSITION),
        anchoring_relation(target, *selection),
    ));
    spawn_info_panel(&mut commands, *selection);
}

fn spawn_info_panel(commands: &mut Commands, selection: AnchorSelection) {
    let unlit = screen_panel_material();
    let built = DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::BottomLeft)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(build_info_panel_tree(selection))
        .build();
    match built {
        Ok(panel) => {
            commands.spawn((AnchorInfoPanel, panel, Transform::default()));
        },
        Err(error) => error!("panel_anchoring: failed to build anchor info panel: {error}"),
    }
}

fn cycle_anchor_selection(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut selection: ResMut<AnchorSelection>,
    targets: Query<Entity, With<AnchorTarget>>,
    dependents: Query<Entity, With<AnchoredDemoPanel>>,
    info_panels: Query<Entity, With<AnchorInfoPanel>>,
    mut commands: Commands,
) {
    let mut next = *selection;
    if keyboard.just_pressed(KeyCode::ArrowLeft) {
        next.source_index = previous_anchor(next.source_index);
    }
    if keyboard.just_pressed(KeyCode::ArrowRight) {
        next.source_index = next_anchor(next.source_index);
    }
    if keyboard.just_pressed(KeyCode::ArrowUp) {
        next.target_index = previous_anchor(next.target_index);
    }
    if keyboard.just_pressed(KeyCode::ArrowDown) {
        next.target_index = next_anchor(next.target_index);
    }
    let depth_move = DEPTH_RATE_MM_PER_SEC * time.delta_secs();
    if keyboard.pressed(KeyCode::Minus) {
        next.depth_mm -= depth_move;
    }
    if keyboard.pressed(KeyCode::Equal) {
        next.depth_mm += depth_move;
    }
    if keyboard.just_pressed(KeyCode::KeyR) {
        next = AnchorSelection::default();
    }
    if next == *selection {
        return;
    }

    let Ok(target) = targets.single() else {
        return;
    };
    let Ok(dependent) = dependents.single() else {
        return;
    };

    let anchors_changed =
        next.source_index != selection.source_index || next.target_index != selection.target_index;
    *selection = next;
    commands
        .entity(dependent)
        .insert(anchoring_relation(target, next));
    if let Ok(info_panel) = info_panels.single() {
        commands.set_tree(info_panel, build_info_panel_tree(next));
    }
    if anchors_changed {
        commands.set_tree(
            target,
            build_panel_tree(TARGET_BACKGROUND, TARGET_ACCENT, next.target_anchor()),
        );
        commands.set_tree(
            dependent,
            build_panel_tree(DEPENDENT_BACKGROUND, DEPENDENT_ACCENT, next.source_anchor()),
        );
    }
}

fn draw_anchor_link(
    selection: Res<AnchorSelection>,
    geometry: PanelAnchorGeometryParam,
    targets: Query<Entity, With<AnchorTarget>>,
    dependents: Query<Entity, With<AnchoredDemoPanel>>,
    mut gizmos: Gizmos,
) {
    let (Ok(target), Ok(dependent)) = (targets.single(), dependents.single()) else {
        return;
    };
    let (Ok(target_geometry), Ok(dependent_geometry)) =
        (geometry.get(target), geometry.get(dependent))
    else {
        return;
    };
    let (Some(target_point), Some(source_point)) = (
        target_geometry.point(selection.target_anchor()).as_world(),
        dependent_geometry
            .point(selection.source_anchor())
            .as_world(),
    ) else {
        return;
    };
    if target_point.distance_squared(source_point)
        <= ANCHOR_LINK_MIN_LENGTH * ANCHOR_LINK_MIN_LENGTH
    {
        return;
    }
    gizmos.line_gradient(target_point, source_point, TARGET_ACCENT, DEPENDENT_ACCENT);
}

fn anchoring_relation(target: Entity, selection: AnchorSelection) -> AnchoredToPanel {
    AnchoredToPanel::new(target, selection.source_anchor(), selection.target_anchor())
        .with_offset(PanelAnchorOffset::ZERO.with_z(Mm(selection.depth_mm)))
}

const fn next_anchor(index: usize) -> usize { (index + 1) % ANCHOR_POINTS.len() }

const fn previous_anchor(index: usize) -> usize {
    if index == 0 {
        ANCHOR_POINTS.len() - 1
    } else {
        index - 1
    }
}

fn build_panel(
    background: Color,
    accent: Color,
    marker_anchor: Anchor,
) -> Result<DiegeticPanel, bevy_diegetic::PanelBuildError> {
    DiegeticPanel::world()
        .size(Mm(PANEL_WIDTH), Mm(PANEL_HEIGHT))
        .font_unit(Unit::Millimeters)
        .material(panel_material())
        .text_material(text_material())
        .anchor(Anchor::Center)
        .with_tree(build_panel_tree(background, accent, marker_anchor))
        .build()
}

fn build_panel_tree(background: Color, accent: Color, marker_anchor: Anchor) -> LayoutTree {
    let (align_x, align_y) = anchor_alignment(marker_anchor);
    let mut builder = LayoutBuilder::new(Mm(PANEL_WIDTH), Mm(PANEL_HEIGHT));
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .background(background)
            .border(Border::all(BORDER_WIDTH, accent))
            .child_alignment(align_x, align_y),
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::fixed(ANCHOR_MARKER_SIZE))
                    .height(Sizing::fixed(ANCHOR_MARKER_SIZE))
                    .background(accent.with_alpha(MARKER_ALPHA)),
                |_| {},
            );
        },
    );
    builder.build()
}

const fn anchor_alignment(anchor: Anchor) -> (AlignX, AlignY) {
    match anchor {
        Anchor::TopLeft => (AlignX::Left, AlignY::Top),
        Anchor::TopCenter => (AlignX::Center, AlignY::Top),
        Anchor::TopRight => (AlignX::Right, AlignY::Top),
        Anchor::CenterLeft => (AlignX::Left, AlignY::Center),
        Anchor::Center => (AlignX::Center, AlignY::Center),
        Anchor::CenterRight => (AlignX::Right, AlignY::Center),
        Anchor::BottomLeft => (AlignX::Left, AlignY::Bottom),
        Anchor::BottomCenter => (AlignX::Center, AlignY::Bottom),
        Anchor::BottomRight => (AlignX::Right, AlignY::Bottom),
    }
}

fn build_info_panel_tree(selection: AnchorSelection) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
    screen_panel_frame(
        &mut builder,
        Sizing::fixed(INFO_PANEL_WIDTH),
        Sizing::FIT,
        DEFAULT_PANEL_BACKGROUND,
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::FIT)
                    .direction(Direction::TopToBottom)
                    .child_gap(INFO_SECTION_GAP),
                |builder| {
                    info_section(
                        builder,
                        PanelRole::Target,
                        "followed:",
                        selection.target_label(),
                        selection.target_index,
                        None,
                    );
                    info_section(
                        builder,
                        PanelRole::Dependent,
                        "following:",
                        selection.source_label(),
                        selection.source_index,
                        Some(selection.depth_mm),
                    );
                },
            );
        },
    );
    builder.build()
}

fn info_section(
    builder: &mut LayoutBuilder,
    role: PanelRole,
    label: &str,
    value: &str,
    active_index: usize,
    depth_mm: Option<f32>,
) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .direction(Direction::TopToBottom)
            .child_gap(INFO_ROW_GAP),
        |builder| {
            builder.text(role.title(), info_title_style(role.accent_color()));
            info_anchor_row(builder, label, value, active_index, role.accent_color());
            if let Some(depth_mm) = depth_mm {
                info_depth_row(builder, depth_mm, role.accent_color());
            }
        },
    );
}

fn info_depth_row(builder: &mut LayoutBuilder, depth_mm: f32, accent: Color) {
    let value_style = if depth_mm == 0.0 {
        info_label_style()
    } else {
        info_value_style(accent)
    };
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight)
            .child_gap(INFO_COL_GAP)
            .child_alignment(AlignX::Left, AlignY::Center),
        |builder| {
            builder.text("depth:", info_label_style());
            builder.text(format_depth_mm(depth_mm), value_style);
        },
    );
}

fn format_depth_mm(depth_mm: f32) -> String {
    if depth_mm == 0.0 {
        "0 mm".to_owned()
    } else {
        format!("{depth_mm:+.0} mm")
    }
}

fn info_anchor_row(
    builder: &mut LayoutBuilder,
    label: &str,
    value: &str,
    active_index: usize,
    accent: Color,
) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight)
            .child_gap(INFO_COL_GAP)
            .child_alignment(AlignX::Left, AlignY::Center),
        |builder| {
            info_anchor_text(builder, label, value, accent);
            info_anchor_grid(builder, active_index, accent);
        },
    );
}

fn info_anchor_text(builder: &mut LayoutBuilder, label: &str, value: &str, accent: Color) {
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .direction(Direction::TopToBottom)
            .child_gap(1.0)
            .child_alignment(AlignX::Left, AlignY::Center),
        |builder| {
            builder.text(label, info_label_style());
            builder.text(value, info_value_style(accent));
        },
    );
}

fn info_anchor_grid(builder: &mut LayoutBuilder, active_index: usize, accent: Color) {
    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .direction(Direction::TopToBottom)
            .child_gap(INFO_GRID_GAP),
        |builder| {
            for row in 0..INFO_GRID_SIDE {
                info_anchor_grid_row(builder, row, active_index, accent);
            }
        },
    );
}

fn info_anchor_grid_row(
    builder: &mut LayoutBuilder,
    row: usize,
    active_index: usize,
    accent: Color,
) {
    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight)
            .child_gap(INFO_GRID_GAP),
        |builder| {
            for column in 0..INFO_GRID_SIDE {
                let index = row * INFO_GRID_SIDE + column;
                info_anchor_grid_cell(builder, index == active_index, accent);
            }
        },
    );
}

fn info_anchor_grid_cell(builder: &mut LayoutBuilder, active: bool, accent: Color) {
    let background = if active { accent } else { INFO_GRID_INACTIVE };
    builder.with(
        El::new()
            .width(Sizing::fixed(INFO_GRID_CELL_SIZE))
            .height(Sizing::fixed(INFO_GRID_CELL_SIZE))
            .background(background)
            .border(Border::all(INFO_GRID_BORDER_WIDTH, INFO_GRID_BORDER)),
        |_| {},
    );
}

fn panel_material() -> StandardMaterial { default_panel_material() }

fn text_material() -> StandardMaterial {
    StandardMaterial {
        unlit: true,
        ..default_panel_material()
    }
}

fn info_title_style(accent: Color) -> TextStyle {
    TextStyle::new(INFO_TITLE_SIZE)
        .with_color(TITLE_COLOR.mix(&accent, 0.18))
        .with_shadow_mode(GlyphShadowMode::None)
}

fn info_label_style() -> TextStyle {
    TextStyle::new(INFO_BODY_SIZE)
        .with_color(INFO_LABEL_COLOR)
        .with_shadow_mode(GlyphShadowMode::None)
}

fn info_value_style(accent: Color) -> TextStyle {
    TextStyle::new(INFO_BODY_SIZE)
        .with_color(BODY_COLOR.mix(&accent, 0.22))
        .with_shadow_mode(GlyphShadowMode::None)
}

#[derive(Clone, Copy)]
enum PanelRole {
    Target,
    Dependent,
}

impl PanelRole {
    const fn title(self) -> &'static str {
        match self {
            Self::Target => "Target Panel",
            Self::Dependent => "Anchored Panel",
        }
    }

    const fn accent_color(self) -> Color {
        match self {
            Self::Target => TARGET_ACCENT,
            Self::Dependent => DEPENDENT_ACCENT,
        }
    }
}
