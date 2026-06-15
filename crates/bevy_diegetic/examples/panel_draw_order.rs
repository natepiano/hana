//! `panel_draw_order` - text-only panel fit probe.

use bevy::prelude::*;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::CornerRadius;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::El;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::In;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Mm;
use bevy_diegetic::Padding;
use bevy_diegetic::PaperSize;
use bevy_diegetic::Pt;
use bevy_diegetic::TextStyle;
use bevy_diegetic::TextWrap;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::CameraHomeTarget;
use fairy_dust::DEFAULT_PANEL_BACKGROUND;
use fairy_dust::TitleBar;
use fairy_dust::screen_panel_material;

const HOME_FOCUS: Vec3 = Vec3::ZERO;
const HOME_MARGIN: f32 = 0.50;
const HOME_PITCH: f32 = 0.08;
const HOME_RADIUS: f32 = 0.30;
const HOME_YAW: f32 = 0.0;
const PAGE_BORDER_COLOR: Color = Color::srgba(0.15, 0.7, 0.9, 0.5);
const PAGE_BORDER_IN: f32 = 0.014;
const PANEL_BACKGROUND_ALPHA: f32 = 0.88;
const PAGE_PADDING_IN: f32 = 0.24;
const PAGE_RADIUS_IN: f32 = 0.08;
const TEXT_SIZE_PT: f32 = 22.0;

const TEXT_COLOR: Color = Color::srgb(0.94, 0.98, 1.0);

const STORY_TEXT: &str = "Alice was beginning to get very tired of sitting by her sister on the bank, and of having nothing to do: once or twice she had peeped into the book her sister was reading, but it had no pictures or conversations in it, and what is the use of a book, thought Alice, without pictures or conversations?";

fn main() {
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_orbit_cam_preset(
            |cam| {
                cam.focus = HOME_FOCUS;
                cam.radius = Some(HOME_RADIUS);
                cam.yaw = Some(HOME_YAW);
                cam.pitch = Some(HOME_PITCH);
            },
            OrbitCamPreset::BlenderLike,
        )
        .with_camera_home()
        .margin(HOME_MARGIN)
        .with_title_bar(TitleBar::new().with_title("Draw Order"))
        .with_camera_control_panel()
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    let material = screen_panel_material();
    let (page_width, page_height) = PaperSize::Photo5x7.landscape();
    let Ok(panel) = DiegeticPanel::world()
        .size(page_width, page_height)
        .anchor(Anchor::Center)
        .material(material.clone())
        .text_material(material)
        .with_tree(build_page(page_width, page_height))
        .build()
    else {
        error!("panel_draw_order: failed to build world text panel");
        return;
    };

    commands.spawn((
        Name::new("Panel draw order world text probe"),
        CameraHomeTarget,
        panel,
        Transform::default(),
    ));
}

fn build_page(width: Mm, height: Mm) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::column()
            .size(width, height)
            .padding(Padding::all(In(PAGE_PADDING_IN)))
            .corner_radius(CornerRadius::all(In(PAGE_RADIUS_IN)))
            .background(DEFAULT_PANEL_BACKGROUND.with_alpha(PANEL_BACKGROUND_ALPHA))
            .border(Border::all(In(PAGE_BORDER_IN), PAGE_BORDER_COLOR)),
    );
    builder.text(
        STORY_TEXT,
        TextStyle::new(Pt(TEXT_SIZE_PT))
            .with_color(TEXT_COLOR)
            .with_shadow_mode(GlyphShadowMode::None)
            .wrap(TextWrap::Words),
    );
    builder.build()
}

/*
//! `panel_draw_order` - one panel tree ordered by `DrawZIndex`.
//!
//! The moving sweep and the text live in the same panel tree. Press `B` or `F`
//! to move the sweep behind or in front of the text, proving that a normal
//! sibling element can pass behind or in front of text without leaving the panel.

use std::time::Duration;

use bevy::prelude::*;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::CornerRadius;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticPanelCommands;
use bevy_diegetic::DrawZIndex;
use bevy_diegetic::El;
use bevy_diegetic::FitMax;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Mm;
use bevy_diegetic::Padding;
use bevy_diegetic::PanelBuildError;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextAlign;
use bevy_diegetic::TextStyle;
use bevy_diegetic::Unit;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::CameraHomeTarget;
use fairy_dust::ControlActivation;
use fairy_dust::DEFAULT_PANEL_BACKGROUND;
use fairy_dust::DescriptionPanel;
use fairy_dust::LABEL_SIZE;
use fairy_dust::TitleBar;
use fairy_dust::TitleBarControl;
use fairy_dust::TitleBarSegment;

const HOME_FOCUS: Vec3 = Vec3::ZERO;
const HOME_MARGIN: f32 = 0.12;
const HOME_PITCH: f32 = 0.08;
const HOME_RADIUS: f32 = 0.50;
const HOME_YAW: f32 = 0.0;
const ZOOM_DURATION_MS: u64 = 650;

const PANEL_PADDING: f32 = 4.0;
const PANEL_RADIUS: f32 = 2.8;
const TEXT_WIDTH: f32 = 112.0;
const SWEEP_OVERHANG: f32 = PANEL_PADDING;
const TEXT_VERTICAL_INSET: f32 = PANEL_PADDING;
const SWEEP_LANE_WIDTH: f32 = TEXT_WIDTH + 2.0 * SWEEP_OVERHANG;
const PANEL_MAX_WIDTH: f32 = SWEEP_LANE_WIDTH + 2.0 * PANEL_PADDING;
const SWEEP_WIDTH: f32 = 31.0;
const SWEEP_TRACK: f32 = SWEEP_LANE_WIDTH - SWEEP_WIDTH;
const SWEEP_SPEED: f32 = 16.0;
const TEXT_SIZE: f32 = 3.25;
const PANEL_TRANSLATION: Vec3 = Vec3::new(0.0, 0.015, 0.0);

const TEXT_Z: DrawZIndex = DrawZIndex(10);
const BEHIND_TEXT_Z: DrawZIndex = DrawZIndex(9);
const IN_FRONT_OF_TEXT_Z: DrawZIndex = DrawZIndex(11);

const PANEL_COLOR: Color = Color::srgba(0.025, 0.035, 0.045, 0.90);
const PANEL_BORDER: Color = Color::srgba(0.20, 0.78, 0.82, 0.76);
const TEXT_COLOR: Color = Color::srgb(0.94, 0.98, 1.0);
const GLASS_SWEEP: Color = Color::srgba(0.96, 1.0, 1.0, 0.64);
const MATTE_COLOR: Color = Color::srgba(0.04, 0.025, 0.018, 0.46);
const SWEEP_BORDER_INNER: Color = Color::srgba(1.0, 0.93, 0.84, 0.52);
const SWEEP_BORDER_OUTER: Color = Color::srgba(1.0, 0.96, 0.88, 0.72);
const TINT_SWEEP: Color = Color::srgba(1.0, 0.34, 0.12, 0.78);

const BEHIND_SEGMENT: &str = "layer-behind";
const FRONT_SEGMENT: &str = "layer-front";
const SLIDE_CONTROL: &str = "S Slide";
const TINT_SEGMENT: &str = "style-tint";
const GLASS_SEGMENT: &str = "style-glass";

const DESCRIPTION_HEADING: &str = "Panel Draw Order";
const DESCRIPTION_LINES: [&str; 5] = [
    "Source: Alice's Adventures in Wonderland.",
    "The text lane grows to the measured wrapped paragraph.",
    "B Behind and F Front change only the sweep DrawZIndex.",
    "T Tint and G Glass change only the sweep material.",
    "bevy_diegetic names it DrawZIndex to avoid Bevy UI ZIndex.",
];
const STORY_TEXT: &str = "Alice was beginning to get very tired of sitting by her sister on the bank, and of having nothing to do: once or twice she had peeped into the book her sister was reading, but it had no pictures or conversations in it, and what is the use of a book, thought Alice, without pictures or conversations?";

#[derive(Component)]
struct DrawOrderPanel;

#[derive(Resource, Clone, Copy, PartialEq)]
struct DemoState {
    layer:  LayerMode,
    motion: MotionMode,
    style:  SweepStyle,
}

impl Default for DemoState {
    fn default() -> Self {
        Self {
            layer:  LayerMode::BehindText,
            motion: MotionMode::Sliding,
            style:  SweepStyle::Tint,
        }
    }
}

#[derive(Resource, Clone, Copy, PartialEq)]
struct SweepPosition {
    direction: f32,
    x:         f32,
}

impl Default for SweepPosition {
    fn default() -> Self {
        Self {
            direction: 1.0,
            x:         0.0,
        }
    }
}

impl SweepPosition {
    fn advance(&mut self, delta_secs: f32) {
        self.x = (self.direction * SWEEP_SPEED).mul_add(delta_secs, self.x);
        if self.x >= SWEEP_TRACK {
            self.x = SWEEP_TRACK;
            self.direction = -1.0;
        } else if self.x <= 0.0 {
            self.x = 0.0;
            self.direction = 1.0;
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum LayerMode {
    BehindText,
    InFrontOfText,
}

impl LayerMode {
    const fn z_index(self) -> DrawZIndex {
        match self {
            Self::BehindText => BEHIND_TEXT_Z,
            Self::InFrontOfText => IN_FRONT_OF_TEXT_Z,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum MotionMode {
    Sliding,
    Paused,
}

impl MotionMode {
    const fn toggled(self) -> Self {
        match self {
            Self::Sliding => Self::Paused,
            Self::Paused => Self::Sliding,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SweepStyle {
    Tint,
    Glass,
}

#[derive(Clone, Copy, PartialEq)]
struct RenderSnapshot {
    state:    DemoState,
    x_bucket: f32,
}

fn main() {
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_orbit_cam_preset(
            |cam| {
                cam.focus = HOME_FOCUS;
                cam.radius = Some(HOME_RADIUS);
                cam.yaw = Some(HOME_YAW);
                cam.pitch = Some(HOME_PITCH);
            },
            OrbitCamPreset::BlenderLike,
        )
        .with_stable_transparency()
        .with_camera_home()
        .duration(Duration::from_millis(ZOOM_DURATION_MS))
        .margin(HOME_MARGIN)
        .with_title_bar(title_bar())
        .wire_chip_to_state::<DemoState, _>(BEHIND_SEGMENT, |state| {
            chip_activation(state.layer == LayerMode::BehindText)
        })
        .wire_chip_to_state::<DemoState, _>(FRONT_SEGMENT, |state| {
            chip_activation(state.layer == LayerMode::InFrontOfText)
        })
        .wire_chip_to_state::<DemoState, _>(SLIDE_CONTROL, |state| {
            chip_activation(state.motion == MotionMode::Sliding)
        })
        .wire_chip_to_state::<DemoState, _>(TINT_SEGMENT, |state| {
            chip_activation(state.style == SweepStyle::Tint)
        })
        .wire_chip_to_state::<DemoState, _>(GLASS_SEGMENT, |state| {
            chip_activation(state.style == SweepStyle::Glass)
        })
        .with_description_panel(description_panel())
        .with_camera_control_panel()
        .with_camera_control_panel_background_color(DEFAULT_PANEL_BACKGROUND.with_alpha(0.88))
        .init_resource::<DemoState>()
        .init_resource::<SweepPosition>()
        .add_systems(Startup, setup)
        .add_systems(Update, update_panel)
        .with_shortcut(KeyCode::KeyB, set_behind_text)
        .with_shortcut(KeyCode::KeyF, set_in_front_of_text)
        .with_shortcut(KeyCode::KeyS, toggle_motion)
        .with_shortcut(KeyCode::KeyT, set_tint)
        .with_shortcut(KeyCode::KeyG, set_glass)
        .run();
}

fn title_bar() -> TitleBar {
    TitleBar::new()
        .with_title("Draw Order")
        .with_background_color(DEFAULT_PANEL_BACKGROUND.with_alpha(0.88))
        .control(TitleBarControl::segmented(
            "Layer:",
            [
                TitleBarSegment::new(BEHIND_SEGMENT, "B Behind"),
                TitleBarSegment::new(FRONT_SEGMENT, "F Front"),
            ],
        ))
        .control(SLIDE_CONTROL)
        .control(TitleBarControl::segmented(
            "Style:",
            [
                TitleBarSegment::new(TINT_SEGMENT, "T Tint"),
                TitleBarSegment::new(GLASS_SEGMENT, "G Glass"),
            ],
        ))
}

fn description_panel() -> DescriptionPanel {
    DescriptionPanel::new(DESCRIPTION_HEADING)
        .with_body_size(LABEL_SIZE.0)
        .lines(DESCRIPTION_LINES)
}

fn setup(mut commands: Commands, state: Res<DemoState>, sweep: Res<SweepPosition>) {
    let Ok(panel) = build_panel(*state, sweep.x) else {
        error!("panel_draw_order: failed to build demo panel");
        return;
    };
    commands.spawn((
        Name::new("Panel draw order demo"),
        CameraHomeTarget,
        DrawOrderPanel,
        panel,
        Transform::from_translation(PANEL_TRANSLATION),
    ));
}

fn update_panel(
    mut commands: Commands,
    state: Res<DemoState>,
    time: Res<Time>,
    mut sweep: ResMut<SweepPosition>,
    panel: Single<Entity, With<DrawOrderPanel>>,
    mut previous: Local<Option<RenderSnapshot>>,
) {
    if state.motion == MotionMode::Sliding {
        sweep.advance(time.delta_secs());
    }
    let x = sweep.x;
    let snapshot = RenderSnapshot {
        state:    *state,
        x_bucket: x.round(),
    };
    if previous.is_some_and(|previous| previous == snapshot) {
        return;
    }

    *previous = Some(snapshot);
    commands.set_tree(*panel, draw_order_tree(*state, x));
}

fn toggle_motion(mut state: ResMut<DemoState>) { state.motion = state.motion.toggled(); }

fn set_behind_text(mut state: ResMut<DemoState>) { state.layer = LayerMode::BehindText; }

fn set_in_front_of_text(mut state: ResMut<DemoState>) { state.layer = LayerMode::InFrontOfText; }

fn set_tint(mut state: ResMut<DemoState>) { state.style = SweepStyle::Tint; }

fn set_glass(mut state: ResMut<DemoState>) { state.style = SweepStyle::Glass; }

const fn chip_activation(active: bool) -> ControlActivation {
    if active {
        ControlActivation::Active
    } else {
        ControlActivation::Inactive
    }
}

fn build_panel(state: DemoState, sweep_x: f32) -> Result<DiegeticPanel, PanelBuildError> {
    DiegeticPanel::world()
        .size(
            FitMax(Mm(PANEL_MAX_WIDTH).into()),
            FitMax(Mm(1000.0).into()),
        )
        .font_unit(Unit::Millimeters)
        .anchor(Anchor::Center)
        .with_tree(draw_order_tree(state, sweep_x))
        .build()
}

fn draw_order_tree(state: DemoState, sweep_x: f32) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::column()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .padding(Padding::all(PANEL_PADDING))

            .alignment(AlignX::Center, AlignY::Center)
            .corner_radius(CornerRadius::all(PANEL_RADIUS))
            .background(PANEL_COLOR)
            .border(Border::all(0.7, PANEL_BORDER)),
    );
    demo_stack(&mut builder, state, sweep_x);
    builder.build()
}

fn demo_stack(builder: &mut LayoutBuilder, state: DemoState, sweep_x: f32) {
    builder.with(
        El::row()
            .width(Sizing::fixed(SWEEP_LANE_WIDTH))
            .height(Sizing::FIT)

            .gap(-SWEEP_LANE_WIDTH)
            .alignment(AlignX::Center, AlignY::Center),
        |builder| {
            text_lane(builder);
            sweep_band(builder, state, sweep_x);
        },
    );
}

fn text_lane(builder: &mut LayoutBuilder) {
    builder.with(
        El::row()
            .width(Sizing::fixed(SWEEP_LANE_WIDTH))
            .height(Sizing::FIT)
            .padding(Padding::xy(0.0, TEXT_VERTICAL_INSET))

            .align_y(AlignY::Center),
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::fixed(SWEEP_OVERHANG))
                    .height(Sizing::FIT),
                |_| {},
            );
            text_band(builder);
            builder.with(El::new().width(Sizing::GROW).height(Sizing::FIT), |_| {});
        },
    );
}

fn text_band(builder: &mut LayoutBuilder) {
    let body = TextStyle::new(TEXT_SIZE)
        .with_color(TEXT_COLOR)
        .with_align(TextAlign::Left)
        .with_shadow_mode(GlyphShadowMode::None);

    builder.with(
        El::new()
            .width(Sizing::fixed(TEXT_WIDTH))
            .height(Sizing::FIT),
        |builder| {
            builder.text_element(
                El::new()
                    .width(Sizing::FIT)
                    .height(Sizing::FIT)
                    .z_index(TEXT_Z),
                STORY_TEXT,
                body,
            );
        },
    );
}

fn sweep_band(builder: &mut LayoutBuilder, state: DemoState, sweep_x: f32) {
    builder.with(
        El::row()
            .width(Sizing::fixed(SWEEP_LANE_WIDTH))
            .height(Sizing::GROW)

            .align_y(AlignY::Center),
        |builder| {
            builder.with(
                El::new().width(Sizing::fixed(sweep_x)).height(Sizing::GROW),
                |_| {},
            );
            sweep_chip(builder, state);
            builder.with(El::new().width(Sizing::GROW).height(Sizing::GROW), |_| {});
        },
    );
}

fn sweep_chip(builder: &mut LayoutBuilder, state: DemoState) {
    let fill = match state.style {
        SweepStyle::Tint => TINT_SWEEP,
        SweepStyle::Glass => GLASS_SWEEP,
    };
    builder.with(
        El::new()
            .width(Sizing::fixed(SWEEP_WIDTH))
            .height(Sizing::GROW)
            .padding(Padding::all(0.65))
            .corner_radius(CornerRadius::all(1.4))
            .background(fill)
            .border(Border::all(0.28, SWEEP_BORDER_OUTER))
            .z_index(state.layer.z_index()),
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .corner_radius(CornerRadius::all(1.0))
                    .background(MATTE_COLOR)
                    .border(Border::all(0.16, SWEEP_BORDER_INNER))
                    .z_index(state.layer.z_index()),
                |_| {},
            );
        },
    );
}
*/
