//! `cascade` — interactive tour of the text cascade.
//!
//! The cascade rule is: **my own override, else my parent's override, else the
//! global default**. This example shows that rule from both sides:
//!
//! - authoring values with `CascadeDefault<A>`, `override_*`, and `inherit_*`;
//! - reading values back with `resolved_text_alpha` and `resolved_font_unit`.
//!
//! Hotkeys:
//! - `G` — toggle the global text-alpha default: `Blend` / `AlphaToCoverage`.
//! - `P` — toggle the parent panel alpha override: `Add` / inherit.
//! - `L` — toggle one panel label's own alpha override: `Premultiplied` / inherit.
//! - `S` — toggle the standalone alpha override: `Add` / inherit.
//! - `U` — toggle the global font-unit default: `Millimeters` / `Points`.
//! - `F` — toggle the standalone font-unit override: `Points` / inherit.
//! - `H` — home the camera.

use std::time::Duration;

use bevy::prelude::*;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::CascadeDefault;
use bevy_diegetic::CascadeEntityCommandsExt;
use bevy_diegetic::CornerRadius;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticPanelCommands;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::Fit;
use bevy_diegetic::FontUnit;
use bevy_diegetic::InvalidSize;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Mm;
use bevy_diegetic::Padding;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextAlpha;
use bevy_diegetic::TextContent;
use bevy_diegetic::TextStyle;
use bevy_diegetic::Unit;
use bevy_diegetic::WorldText;
use bevy_diegetic::default_panel_material;
use bevy_diegetic::resolved_font_unit;
use bevy_diegetic::resolved_text_alpha;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::CameraHomeTarget;
use fairy_dust::DEFAULT_PANEL_BACKGROUND;
use fairy_dust::LABEL_SIZE;
use fairy_dust::TitleBar;

const EXAMPLE_TITLE: &str = "Cascade";
const STANDALONE_TITLE_TEXT: &str = "Standalone World Text";
const DEFAULT_ALPHA_PREFIX: &str = "global alpha";
const STANDALONE_ALPHA_PREFIX: &str = "standalone alpha";
const STANDALONE_UNIT_PREFIX: &str = "standalone unit";
const PANEL_INHERITED_PREFIX: &str = "inherited alpha";
const PANEL_LOCAL_PREFIX: &str = "local alpha";
const PANEL_UNIT_PREFIX: &str = "panel unit";

const SCENE_Z: f32 = 2.35;
const SCENE_FRAME_Y: f32 = 0.62;
const SCENE_FRAME_Z_OFFSET: f32 = -0.01;
const HOME_PITCH: f32 = 0.055;
const HOME_YAW: f32 = 0.0;
const HOME_MARGIN: f32 = 0.08;
const HOME_DURATION_MS: u64 = 900;

const GLOBAL_ALPHA_A: AlphaMode = AlphaMode::Blend;
const GLOBAL_ALPHA_B: AlphaMode = AlphaMode::AlphaToCoverage;
const PANEL_ALPHA: AlphaMode = AlphaMode::Add;
const LABEL_ALPHA: AlphaMode = AlphaMode::Premultiplied;
const STANDALONE_ALPHA: AlphaMode = AlphaMode::Add;

const GLOBAL_UNIT_A: Unit = Unit::Millimeters;
const GLOBAL_UNIT_B: Unit = Unit::Points;
const STANDALONE_UNIT: Unit = Unit::Points;

const GROUND_PLANE_SIZE: f32 = 3.2;
const GROUND_PLANE_WIDTH_SCALE: f32 = 1.18;
const GROUND_PLANE_DEPTH_SCALE: f32 = 0.42;
const GROUND_PLANE_Z_OFFSET: f32 = -0.35;
const GROUND_PLANE_Z: f32 = SCENE_Z + GROUND_PLANE_Z_OFFSET;

const SCENE_FRAME_WIDTH: Mm = Mm(2300.0);
const SCENE_FRAME_HEIGHT: Mm = Mm(620.0);
const SCENE_FRAME_LEFT_WIDTH: Mm = Mm(1140.0);

const WORLD_TEXT_X: f32 = -1.02;
const WORLD_TEXT_TITLE_OFFSET_FROM_FRAME: f32 = 0.17;
const WORLD_TEXT_TITLE_BODY_GAP: f32 = 0.10;
const WORLD_TEXT_ROW_GAP: f32 = 0.105;
const WORLD_TEXT_TITLE_Y: f32 = SCENE_FRAME_Y + WORLD_TEXT_TITLE_OFFSET_FROM_FRAME;
const WORLD_TEXT_DEFAULT_Y: f32 = WORLD_TEXT_TITLE_Y - WORLD_TEXT_TITLE_BODY_GAP;
const WORLD_TEXT_ALPHA_Y: f32 = WORLD_TEXT_DEFAULT_Y - WORLD_TEXT_ROW_GAP;
const WORLD_TEXT_UNIT_Y: f32 = WORLD_TEXT_ALPHA_Y - WORLD_TEXT_ROW_GAP;

const WORLD_TITLE_SIZE: f32 = 0.052;
const WORLD_TEXT_SIZE: f32 = 0.040;
const PANEL_TEXT_SIZE: f32 = 40.0;
const PANEL_TITLE_SIZE: f32 = 52.0;
const PANEL_DEMO_WIDTH: Mm = Mm(1040.0);

const DEFAULT_COLOR: Color = Color::srgb(0.55, 0.75, 1.0);
const INHERITED_COLOR: Color = Color::srgb(0.35, 1.0, 0.7);
const OVERRIDE_COLOR: Color = Color::srgb(1.0, 0.8, 0.3);
const HUD_HEADER_COLOR: Color = Color::srgb(0.55, 0.78, 0.95);
const HUD_CONTROL_COLOR: Color = Color::srgb(0.78, 0.84, 0.92);
const HUD_KEY_COLOR: Color = Color::srgb(0.96, 0.76, 0.36);
const PANEL_BORDER: Color = Color::srgba(0.15, 0.7, 0.9, 0.5);
const PANEL_BORDER_ACCENT: Color = Color::srgba(0.15, 0.7, 0.9, 0.85);
const PANEL_BORDER_DIM: Color = Color::srgba(0.1, 0.4, 0.6, 0.3);
const INVISIBLE_FRAME_BACKGROUND: Color = Color::srgba(0.0, 0.0, 0.0, 0.0);
const PANEL_FRAME_BACKGROUND: Color = Color::srgba(0.0, 0.02, 0.04, 0.35);

const PANEL_FRAME_PAD: Mm = Mm(18.0);
const PANEL_INNER_PAD: Mm = Mm(28.0);
const PANEL_INNER_WIDTH: Mm = Mm(930.0);
const PANEL_ROW_GAP: Mm = Mm(40.0);
const PANEL_BORDER_WIDTH: Mm = Mm(6.0);
const PANEL_RADIUS: Mm = Mm(54.0);
const PANEL_INNER_RADIUS: Mm = Mm(34.0);
const PANEL_INNER_BORDER_WIDTH: Mm = Mm(3.0);

const HUD_PADDING: Px = Px(10.0);
const HUD_RADIUS: Px = Px(8.0);
const HUD_BORDER_WIDTH: Px = Px(1.0);
const HUD_ROW_GAP: Px = Px(4.0);
const HUD_CARD_HEIGHT: Px = Px(190.0);
const HUD_CONTROL_KEY_WIDTH: Px = Px(18.0);
const HUD_CONTROL_LABEL_WIDTH: Px = Px(276.0);
const HUD_CONTROL_VALUE_WIDTH: Px = Px(160.0);

fn main() {
    // `bevy_diegetic::DiegeticUiPlugin` is registered automatically by
    // `fairy_dust::sprinkle_example`.
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_ground_plane()
        .size(GROUND_PLANE_SIZE)
        .transform(
            Transform::from_xyz(0.0, 0.0, GROUND_PLANE_Z).with_scale(Vec3::new(
                GROUND_PLANE_WIDTH_SCALE,
                1.0,
                GROUND_PLANE_DEPTH_SCALE,
            )),
        )
        .with_orbit_cam_preset(|_| {}, OrbitCamPreset::BlenderLike)
        .with_camera_home()
        .pitch(HOME_PITCH)
        .yaw(HOME_YAW)
        .margin(HOME_MARGIN)
        .duration(Duration::from_millis(HOME_DURATION_MS))
        .with_title_bar(TitleBar::new().with_title(EXAMPLE_TITLE))
        .with_camera_control_panel()
        .insert_resource(CascadeDefault(TextAlpha(GLOBAL_ALPHA_A)))
        .insert_resource(CascadeDefault(FontUnit(GLOBAL_UNIT_A)))
        .init_resource::<CascadeDemoState>()
        .init_resource::<HudSnapshotCache>()
        .add_systems(Startup, setup)
        .add_systems(Update, handle_cascade_keys)
        .add_systems(PostUpdate, refresh_hud)
        .run();
}

#[derive(Resource, Clone, Copy)]
struct CascadeDemoEntities {
    default_alpha:    Entity,
    standalone_alpha: Entity,
    standalone_unit:  Entity,
    panel:            Entity,
    hud:              Entity,
}

#[derive(Resource, Clone, Copy, PartialEq)]
struct CascadeDemoState {
    global_alpha:     AlphaMode,
    global_unit:      Unit,
    panel_alpha:      OverrideMode,
    label_alpha:      OverrideMode,
    standalone_alpha: OverrideMode,
    standalone_unit:  OverrideMode,
}

#[derive(Clone, Copy, PartialEq)]
enum OverrideMode {
    Inherit,
    Override,
}

impl OverrideMode {
    const fn toggled(self) -> Self {
        match self {
            Self::Inherit => Self::Override,
            Self::Override => Self::Inherit,
        }
    }

    const fn is_override(self) -> bool { matches!(self, Self::Override) }

    const fn label(self) -> &'static str {
        match self {
            Self::Inherit => "inherit",
            Self::Override => "override",
        }
    }
}

impl Default for CascadeDemoState {
    fn default() -> Self {
        Self {
            global_alpha:     GLOBAL_ALPHA_A,
            global_unit:      GLOBAL_UNIT_A,
            panel_alpha:      OverrideMode::Override,
            label_alpha:      OverrideMode::Override,
            standalone_alpha: OverrideMode::Override,
            standalone_unit:  OverrideMode::Inherit,
        }
    }
}

#[derive(Resource, Default)]
struct HudSnapshotCache(Option<HudSnapshot>);

#[derive(Clone, PartialEq)]
struct HudSnapshot {
    state:                 CascadeDemoState,
    default_alpha:         AlphaMode,
    standalone_alpha:      AlphaMode,
    standalone_unit:       Unit,
    panel_inherited_alpha: Option<AlphaMode>,
    panel_local_alpha:     Option<AlphaMode>,
}

fn setup(mut commands: Commands) {
    let scene_frame = build_scene_frame_panel().map(|panel| {
        commands
            .spawn((
                CameraHomeTarget,
                panel,
                Transform::from_xyz(0.0, SCENE_FRAME_Y, SCENE_Z + SCENE_FRAME_Z_OFFSET),
            ))
            .id()
    });

    commands.spawn(
        WorldText::new(STANDALONE_TITLE_TEXT)
            .size(WORLD_TITLE_SIZE)
            .color(HUD_HEADER_COLOR)
            .anchor(Anchor::TopLeft)
            .unlit()
            .transform(Transform::from_xyz(
                WORLD_TEXT_X,
                WORLD_TEXT_TITLE_Y,
                SCENE_Z,
            ))
            .build(),
    );

    let default_alpha = commands
        .spawn(
            WorldText::new(alpha_line(DEFAULT_ALPHA_PREFIX, GLOBAL_ALPHA_A, "global"))
                .size(WORLD_TEXT_SIZE)
                .color(DEFAULT_COLOR)
                .anchor(Anchor::TopLeft)
                .unlit()
                .transform(Transform::from_xyz(
                    WORLD_TEXT_X,
                    WORLD_TEXT_DEFAULT_Y,
                    SCENE_Z,
                ))
                .build(),
        )
        .id();

    let standalone_alpha = commands
        .spawn(
            WorldText::new(alpha_line(STANDALONE_ALPHA_PREFIX, STANDALONE_ALPHA, "own"))
                .size(WORLD_TEXT_SIZE)
                .color(OVERRIDE_COLOR)
                .anchor(Anchor::TopLeft)
                .unlit()
                .transform(Transform::from_xyz(
                    WORLD_TEXT_X,
                    WORLD_TEXT_ALPHA_Y,
                    SCENE_Z,
                ))
                .build(),
        )
        .override_text_alpha(STANDALONE_ALPHA)
        .id();

    let standalone_unit = commands
        .spawn(
            WorldText::new(unit_line(STANDALONE_UNIT_PREFIX, GLOBAL_UNIT_A, "global"))
                .size(WORLD_TEXT_SIZE)
                .color(DEFAULT_COLOR)
                .anchor(Anchor::TopLeft)
                .unlit()
                .transform(Transform::from_xyz(
                    WORLD_TEXT_X,
                    WORLD_TEXT_UNIT_Y,
                    SCENE_Z,
                ))
                .build(),
        )
        .id();

    let hud =
        build_hud_panel().map(|panel| commands.spawn((HudPanel, panel, Transform::default())).id());

    match (scene_frame, hud) {
        (Ok(scene_frame), Ok(hud)) => {
            commands.insert_resource(CascadeDemoEntities {
                default_alpha,
                standalone_alpha,
                standalone_unit,
                panel: scene_frame,
                hud,
            });
        },
        (Err(error), _) => {
            error!("cascade: failed to build scene frame: {error}");
        },
        (_, Err(error)) => {
            error!("cascade: failed to build HUD panel: {error}");
        },
    }
}

fn build_scene_frame_panel() -> Result<DiegeticPanel, InvalidSize> {
    let unlit = StandardMaterial {
        unlit: true,
        ..default_panel_material()
    };
    DiegeticPanel::world()
        .size(SCENE_FRAME_WIDTH, SCENE_FRAME_HEIGHT)
        .anchor(Anchor::Center)
        .material(unlit.clone())
        .text_material(unlit)
        .text_alpha_mode(PANEL_ALPHA)
        .font_unit(Unit::Millimeters)
        .with_tree(build_scene_frame_tree(None))
        .build()
}

fn build_scene_frame_tree(snapshot: Option<&HudSnapshot>) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .background(INVISIBLE_FRAME_BACKGROUND),
    );
    builder.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .direction(Direction::LeftToRight),
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::fixed(SCENE_FRAME_LEFT_WIDTH))
                    .height(Sizing::GROW),
                |_| {},
            );
            builder.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .child_alignment(AlignX::Center, AlignY::Center),
                |builder| build_panel_layout(builder, snapshot),
            );
        },
    );
    builder.build()
}

fn build_hud_panel() -> Result<DiegeticPanel, InvalidSize> {
    let unlit = StandardMaterial {
        unlit: true,
        ..default_panel_material()
    };
    DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::BottomLeft)
        .material(unlit.clone())
        .text_material(unlit)
        .with_tree(build_hud_tree(None))
        .build()
}

fn build_panel_layout(builder: &mut LayoutBuilder, snapshot: Option<&HudSnapshot>) {
    let title = TextStyle::new(PANEL_TITLE_SIZE)
        .with_color(HUD_HEADER_COLOR)
        .no_wrap();
    let inherited = TextStyle::new(PANEL_TEXT_SIZE)
        .with_color(INHERITED_COLOR)
        .no_wrap();
    let mut own = TextStyle::new(PANEL_TEXT_SIZE)
        .with_color(OVERRIDE_COLOR)
        .no_wrap();
    if snapshot.is_none_or(|snapshot| snapshot.state.label_alpha.is_override()) {
        own = own.with_alpha_mode(LABEL_ALPHA);
    }
    let unit = TextStyle::new(PANEL_TEXT_SIZE)
        .with_color(DEFAULT_COLOR)
        .no_wrap();
    let inherited_text = snapshot.map_or_else(
        || alpha_line(PANEL_INHERITED_PREFIX, PANEL_ALPHA, "panel"),
        panel_inherited_text,
    );
    let local_text = snapshot.map_or_else(
        || alpha_line(PANEL_LOCAL_PREFIX, LABEL_ALPHA, "own"),
        panel_local_text,
    );
    let unit_text = unit_line(PANEL_UNIT_PREFIX, Unit::Millimeters, "panel");

    builder.with(
        El::new()
            .width(Sizing::fixed(PANEL_DEMO_WIDTH))
            .height(Sizing::FIT)
            .direction(Direction::TopToBottom)
            .padding(Padding::all(PANEL_FRAME_PAD))
            .child_alignment(AlignX::Center, AlignY::Center)
            .corner_radius(CornerRadius::all(PANEL_RADIUS))
            .background(PANEL_FRAME_BACKGROUND)
            .border(Border::all(PANEL_BORDER_WIDTH, PANEL_BORDER_ACCENT)),
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::fixed(PANEL_INNER_WIDTH))
                    .height(Sizing::FIT)
                    .direction(Direction::TopToBottom)
                    .padding(Padding::all(PANEL_INNER_PAD))
                    .child_gap(PANEL_ROW_GAP)
                    .corner_radius(CornerRadius::all(PANEL_INNER_RADIUS))
                    .background(DEFAULT_PANEL_BACKGROUND)
                    .border(Border::all(PANEL_INNER_BORDER_WIDTH, PANEL_BORDER_DIM)),
                |builder| {
                    builder.text("Panel Text", title);
                    builder.text(inherited_text, inherited);
                    builder.text(local_text, own);
                    builder.text(unit_text, unit);
                },
            );
        },
    );
}

fn handle_cascade_keys(
    keyboard: Res<ButtonInput<KeyCode>>,
    entities: Option<Res<CascadeDemoEntities>>,
    mut state: ResMut<CascadeDemoState>,
    mut alpha_default: ResMut<CascadeDefault<TextAlpha>>,
    mut unit_default: ResMut<CascadeDefault<FontUnit>>,
    labels: Query<(Entity, &TextContent)>,
    mut commands: Commands,
) {
    let Some(entities) = entities else {
        return;
    };

    if keyboard.just_pressed(KeyCode::KeyG) {
        state.global_alpha = next_alpha_default(state.global_alpha);
        alpha_default.0 = TextAlpha(state.global_alpha);
    }
    if keyboard.just_pressed(KeyCode::KeyU) {
        state.global_unit = next_unit_default(state.global_unit);
        unit_default.0 = FontUnit(state.global_unit);
    }
    if keyboard.just_pressed(KeyCode::KeyP) {
        state.panel_alpha = state.panel_alpha.toggled();
        let mut panel = commands.entity(entities.panel);
        if state.panel_alpha.is_override() {
            panel.override_text_alpha(PANEL_ALPHA);
        } else {
            panel.inherit_text_alpha();
        }
    }
    if keyboard.just_pressed(KeyCode::KeyS) {
        state.standalone_alpha = state.standalone_alpha.toggled();
        let mut standalone = commands.entity(entities.standalone_alpha);
        if state.standalone_alpha.is_override() {
            standalone.override_text_alpha(STANDALONE_ALPHA);
        } else {
            standalone.inherit_text_alpha();
        }
    }
    if keyboard.just_pressed(KeyCode::KeyF) {
        state.standalone_unit = state.standalone_unit.toggled();
        let mut standalone = commands.entity(entities.standalone_unit);
        if state.standalone_unit.is_override() {
            standalone.override_font_unit(STANDALONE_UNIT);
        } else {
            standalone.inherit_font_unit();
        }
    }
    if keyboard.just_pressed(KeyCode::KeyL) {
        let Some(label) = find_label_by_prefix(&labels, PANEL_LOCAL_PREFIX) else {
            warn!("cascade: panel label is not spawned yet");
            return;
        };
        state.label_alpha = state.label_alpha.toggled();
        let mut label_commands = commands.entity(label);
        if state.label_alpha.is_override() {
            label_commands.override_text_alpha(LABEL_ALPHA);
        } else {
            label_commands.inherit_text_alpha();
        }
    }
}

fn next_alpha_default(current: AlphaMode) -> AlphaMode {
    if current == GLOBAL_ALPHA_A {
        GLOBAL_ALPHA_B
    } else {
        GLOBAL_ALPHA_A
    }
}

fn next_unit_default(current: Unit) -> Unit {
    if current == GLOBAL_UNIT_A {
        GLOBAL_UNIT_B
    } else {
        GLOBAL_UNIT_A
    }
}

fn find_label_by_prefix(labels: &Query<(Entity, &TextContent)>, prefix: &str) -> Option<Entity> {
    labels
        .iter()
        .find_map(|(entity, text)| text.text().starts_with(prefix).then_some(entity))
}

fn refresh_hud(world: &mut World) {
    let Some(entities) = world.get_resource::<CascadeDemoEntities>().copied() else {
        return;
    };
    let state = *world.resource::<CascadeDemoState>();
    let snapshot = HudSnapshot {
        state,
        default_alpha: resolved_text_alpha(world, entities.default_alpha),
        standalone_alpha: resolved_text_alpha(world, entities.standalone_alpha),
        standalone_unit: resolved_font_unit(world, entities.standalone_unit),
        panel_inherited_alpha: find_text_entity_by_prefix(world, PANEL_INHERITED_PREFIX)
            .map(|entity| resolved_text_alpha(world, entity)),
        panel_local_alpha: find_text_entity_by_prefix(world, PANEL_LOCAL_PREFIX)
            .map(|entity| resolved_text_alpha(world, entity)),
    };

    let mut cache = world.resource_mut::<HudSnapshotCache>();
    if cache.0.as_ref() == Some(&snapshot) {
        return;
    }
    cache.0 = Some(snapshot.clone());
    update_world_text(
        world,
        entities.default_alpha,
        alpha_line(DEFAULT_ALPHA_PREFIX, snapshot.default_alpha, "global"),
    );
    update_world_text(
        world,
        entities.standalone_alpha,
        standalone_alpha_text(&snapshot),
    );
    update_world_text(
        world,
        entities.standalone_unit,
        standalone_unit_text(&snapshot),
    );
    world
        .commands()
        .set_tree(entities.panel, build_scene_frame_tree(Some(&snapshot)));
    world
        .commands()
        .set_tree(entities.hud, build_hud_tree(Some(&snapshot)));
}

fn find_text_entity_by_prefix(world: &mut World, prefix: &str) -> Option<Entity> {
    let mut labels = world.query::<(Entity, &TextContent)>();
    labels
        .iter(world)
        .find_map(|(entity, text)| text.text().starts_with(prefix).then_some(entity))
}

fn update_world_text(world: &mut World, entity: Entity, text: String) {
    let Some(mut world_text) = world.get_mut::<TextContent>(entity) else {
        return;
    };
    if world_text.text() != text {
        world_text.set_text(text);
    }
}

#[derive(Component)]
struct HudPanel;

fn build_hud_tree(snapshot: Option<&HudSnapshot>) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(El::new().width(Sizing::FIT).height(Sizing::FIT));
    let header = TextStyle::new(LABEL_SIZE)
        .with_color(HUD_HEADER_COLOR)
        .no_wrap();
    let control_key = TextStyle::new(LABEL_SIZE)
        .with_color(HUD_KEY_COLOR)
        .no_wrap();
    let control_text = TextStyle::new(LABEL_SIZE)
        .with_color(HUD_CONTROL_COLOR)
        .no_wrap();

    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight),
        |builder| match snapshot {
            Some(snapshot) => {
                build_hud_card(builder, |builder| {
                    build_controls_table(builder, snapshot, &header, &control_key, &control_text);
                });
            },
            None => {
                build_hud_card(builder, |builder| {
                    builder.text("Cascade controls", header);
                });
            },
        },
    );
    builder.build()
}

fn build_hud_card(builder: &mut LayoutBuilder, build: impl FnOnce(&mut LayoutBuilder)) {
    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::fixed(HUD_CARD_HEIGHT))
            .direction(Direction::TopToBottom)
            .padding(Padding::all(HUD_PADDING))
            .child_gap(HUD_ROW_GAP)
            .corner_radius(CornerRadius::all(HUD_RADIUS))
            .background(DEFAULT_PANEL_BACKGROUND)
            .border(Border::all(HUD_BORDER_WIDTH, PANEL_BORDER)),
        build,
    );
}

fn build_controls_table(
    builder: &mut LayoutBuilder,
    snapshot: &HudSnapshot,
    header: &TextStyle,
    control_key: &TextStyle,
    control_text: &TextStyle,
) {
    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .direction(Direction::TopToBottom)
            .child_gap(HUD_ROW_GAP),
        |builder| {
            builder.text("Cascade controls", header.clone());
            controls_row(
                builder,
                "G",
                "global alpha default:",
                alpha_label(snapshot.state.global_alpha),
                control_key,
                control_text,
            );
            controls_row(
                builder,
                "P",
                "parent panel override:",
                snapshot.state.panel_alpha.label(),
                control_key,
                control_text,
            );
            controls_row(
                builder,
                "L",
                "label override:",
                snapshot.state.label_alpha.label(),
                control_key,
                control_text,
            );
            controls_row(
                builder,
                "S",
                "standalone alpha override:",
                snapshot.state.standalone_alpha.label(),
                control_key,
                control_text,
            );
            controls_row(
                builder,
                "U",
                "global font unit:",
                unit_label(snapshot.state.global_unit),
                control_key,
                control_text,
            );
            controls_row(
                builder,
                "F",
                "standalone font unit override:",
                snapshot.state.standalone_unit.label(),
                control_key,
                control_text,
            );
        },
    );
}

fn controls_row(
    builder: &mut LayoutBuilder,
    key: &str,
    label: &str,
    value: &str,
    key_style: &TextStyle,
    text_style: &TextStyle,
) {
    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .direction(Direction::LeftToRight)
            .child_gap(Px(6.0)),
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::fixed(HUD_CONTROL_KEY_WIDTH))
                    .height(Sizing::FIT),
                |builder| {
                    builder.text(key, key_style.clone());
                },
            );
            builder.with(
                El::new()
                    .width(Sizing::fixed(HUD_CONTROL_LABEL_WIDTH))
                    .height(Sizing::FIT),
                |builder| {
                    builder.text(label, text_style.clone());
                },
            );
            builder.with(
                El::new()
                    .width(Sizing::fixed(HUD_CONTROL_VALUE_WIDTH))
                    .height(Sizing::FIT),
                |builder| {
                    builder.text(value, text_style.clone());
                },
            );
        },
    );
}

fn standalone_alpha_text(snapshot: &HudSnapshot) -> String {
    let source = if snapshot.state.standalone_alpha.is_override() {
        "own"
    } else {
        "global"
    };
    alpha_line(STANDALONE_ALPHA_PREFIX, snapshot.standalone_alpha, source)
}

fn standalone_unit_text(snapshot: &HudSnapshot) -> String {
    let source = if snapshot.state.standalone_unit.is_override() {
        "own"
    } else {
        "global"
    };
    unit_line(STANDALONE_UNIT_PREFIX, snapshot.standalone_unit, source)
}

fn panel_inherited_text(snapshot: &HudSnapshot) -> String {
    let source = if snapshot.state.panel_alpha.is_override() {
        "panel"
    } else {
        "global"
    };
    alpha_line(
        PANEL_INHERITED_PREFIX,
        snapshot
            .panel_inherited_alpha
            .unwrap_or(snapshot.state.global_alpha),
        source,
    )
}

fn panel_local_text(snapshot: &HudSnapshot) -> String {
    let source = if snapshot.state.label_alpha.is_override() {
        "own"
    } else if snapshot.state.panel_alpha.is_override() {
        "panel"
    } else {
        "global"
    };
    alpha_line(
        PANEL_LOCAL_PREFIX,
        snapshot
            .panel_local_alpha
            .unwrap_or(snapshot.state.global_alpha),
        source,
    )
}

fn alpha_line(label: &str, alpha: AlphaMode, source: &str) -> String {
    format!("{label} = {} ({source})", alpha_label(alpha))
}

fn unit_line(label: &str, unit: Unit, source: &str) -> String {
    format!("{label} = {} ({source})", unit_label(unit))
}

const fn alpha_label(alpha: AlphaMode) -> &'static str {
    match alpha {
        AlphaMode::Opaque => "Opaque",
        AlphaMode::Mask(_) => "Mask",
        AlphaMode::Blend => "Blend",
        AlphaMode::Premultiplied => "Premultiplied",
        AlphaMode::Add => "Add",
        AlphaMode::Multiply => "Multiply",
        AlphaMode::AlphaToCoverage => "AlphaToCoverage",
    }
}

const fn unit_label(unit: Unit) -> &'static str {
    match unit {
        Unit::Meters => "Meters",
        Unit::Millimeters => "Millimeters",
        Unit::Points => "Points",
        Unit::Pixels => "Pixels",
        Unit::Inches => "Inches",
        Unit::Custom(_) => "Custom",
    }
}
