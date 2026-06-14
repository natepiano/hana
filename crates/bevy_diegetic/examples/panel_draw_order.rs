//! `panel_draw_order` — one panel tree ordered by `DrawZIndex`.
//!
//! Press `K` to rebuild the panel tree with the overlay rectangle below or
//! above the text leaf. Both elements live in the same layout tree: the text
//! uses `text_element(El::new().z_index(...), ...)`, and the overlay uses
//! `El::z_index`.

use bevy::prelude::*;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticPanelCommands;
use bevy_diegetic::Direction;
use bevy_diegetic::DrawZIndex;
use bevy_diegetic::El;
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
use fairy_dust::TitleBar;

// camera
const CAMERA_FOCUS: Vec3 = Vec3::ZERO;
const CAMERA_PITCH: f32 = 0.1;
const CAMERA_RADIUS: f32 = 0.34;
const CAMERA_YAW: f32 = 0.0;
const HOME_MARGIN: f32 = 0.18;

// colors
const BORDER_COLOR: Color = Color::srgb(0.20, 0.78, 0.82);
const PANEL_COLOR: Color = Color::srgba(0.05, 0.08, 0.10, 0.94);
const OVERLAY_FRONT_COLOR: Color = Color::srgba(1.0, 0.36, 0.16, 0.58);
const OVERLAY_BACK_COLOR: Color = Color::srgba(0.13, 0.58, 0.92, 0.44);
const TEXT_COLOR: Color = Color::srgb(0.94, 0.98, 1.0);

// draw order
const OVERLAY_BACK_LAYER: DrawZIndex = DrawZIndex(-1);
const OVERLAY_FRONT_LAYER: DrawZIndex = DrawZIndex(1);
const TEXT_LAYER: DrawZIndex = DrawZIndex(0);

// layout
const OVERLAY_HEIGHT: f32 = 34.0;
const OVERLAY_WIDTH: f32 = 122.0;
const PANEL_HEIGHT: f32 = 74.0;
const PANEL_PADDING: f32 = 9.0;
const PANEL_WIDTH: f32 = 150.0;
const STACK_OVERLAP_GAP: f32 = -OVERLAY_HEIGHT;
const TEXT_SIZE: f32 = 7.0;

#[derive(Component)]
struct DrawOrderPanel;

#[derive(Resource, Clone, Copy, Default, PartialEq)]
enum OverlayOrder {
    #[default]
    BehindText,
    InFrontOfText,
}

impl OverlayOrder {
    const fn toggled(self) -> Self {
        match self {
            Self::BehindText => Self::InFrontOfText,
            Self::InFrontOfText => Self::BehindText,
        }
    }

    const fn layer(self) -> DrawZIndex {
        match self {
            Self::BehindText => OVERLAY_BACK_LAYER,
            Self::InFrontOfText => OVERLAY_FRONT_LAYER,
        }
    }

    const fn overlay_color(self) -> Color {
        match self {
            Self::BehindText => OVERLAY_BACK_COLOR,
            Self::InFrontOfText => OVERLAY_FRONT_COLOR,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::BehindText => "Text z=0\nOverlay z=-1",
            Self::InFrontOfText => "Text z=0\nOverlay z=+1",
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
                .with_title("Panel Draw Order")
                .control("H Home")
                .control("K Overlay"),
        )
        .with_camera_control_panel()
        .init_resource::<OverlayOrder>()
        .add_systems(Startup, setup)
        .with_shortcut(KeyCode::KeyK, toggle_overlay_order)
        .run();
}

fn setup(mut commands: Commands, overlay_order: Res<OverlayOrder>) {
    let Ok(panel) = build_panel(*overlay_order) else {
        error!("panel_draw_order: failed to build demo panel");
        return;
    };
    commands.spawn((
        Name::new("Panel draw order demo"),
        CameraHomeTarget,
        DrawOrderPanel,
        panel,
        Transform::default(),
    ));
}

fn toggle_overlay_order(
    mut overlay_order: ResMut<OverlayOrder>,
    panel: Single<Entity, With<DrawOrderPanel>>,
    mut commands: Commands,
) {
    *overlay_order = overlay_order.toggled();
    commands.set_tree(*panel, draw_order_tree(*overlay_order));
}

fn build_panel(overlay_order: OverlayOrder) -> Result<DiegeticPanel, PanelBuildError> {
    DiegeticPanel::world()
        .size(Mm(PANEL_WIDTH), Mm(PANEL_HEIGHT))
        .font_unit(Unit::Millimeters)
        .anchor(Anchor::Center)
        .with_tree(draw_order_tree(overlay_order))
        .build()
}

fn draw_order_tree(overlay_order: OverlayOrder) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::GROW)
            .padding(Padding::all(PANEL_PADDING))
            .direction(Direction::TopToBottom)
            .child_gap(STACK_OVERLAP_GAP)
            .child_alignment(AlignX::Center, AlignY::Center)
            .background(PANEL_COLOR)
            .border(Border::all(0.8, BORDER_COLOR)),
    );
    builder.text_element(
        El::new()
            .width(Sizing::fixed(OVERLAY_WIDTH))
            .height(Sizing::fixed(OVERLAY_HEIGHT))
            .z_index(TEXT_LAYER),
        overlay_order.label(),
        TextStyle::new(TEXT_SIZE)
            .with_color(TEXT_COLOR)
            .with_align(TextAlign::Center),
    );
    builder.with(
        El::new()
            .width(Sizing::fixed(OVERLAY_WIDTH))
            .height(Sizing::fixed(OVERLAY_HEIGHT))
            .background(overlay_order.overlay_color())
            .z_index(overlay_order.layer()),
        |_| {},
    );
    builder.build()
}
