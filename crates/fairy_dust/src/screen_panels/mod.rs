//! Capability: small screen-space panels for examples.

use bevy::prelude::*;
use bevy_diegetic::Border;
use bevy_diegetic::CornerRadius;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::Padding;
use bevy_diegetic::Pt;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;
use bevy_diegetic::default_panel_material;

use crate::camera_home::CameraHomeConfig;
use crate::ensure_plugin;
use crate::theme::BORDER;
use crate::theme::BORDER_ACCENT;
use crate::theme::BORDER_DIM;
use crate::theme::FRAME_PAD;
use crate::theme::INNER_BG;
use crate::theme::INNER_RADIUS;
use crate::theme::RADIUS;

mod description;
mod title_bar;

pub use description::DescriptionPanel;
pub use title_bar::TitleBar;
pub use title_bar::TitleBarControlState;

const INNER_PAD: Px = Px(10.0);

const BODY_SIZE: Pt = Pt(11.0);
const CONTROL_SIZE: Pt = Pt(12.0);

const DESCRIPTION_WIDTH: Px = Px(330.0);

const BODY_COLOR: Color = Color::srgba(0.68, 0.72, 0.82, 0.9);
const CONTROL_ACTIVE_COLOR: Color = Color::srgb(1.0, 0.9, 0.25);
const CONTROL_INACTIVE_COLOR: Color = Color::srgba(0.68, 0.72, 0.82, 0.9);
const DIVIDER_COLOR: Color = Color::srgba(0.35, 0.8, 1.0, 0.35);

pub(crate) fn install_description(app: &mut App, panel: DescriptionPanel) {
    ensure_plugin(app, DiegeticUiPlugin);
    app.add_systems(Startup, move |mut commands: Commands| {
        description::spawn_description_panel(&mut commands, &panel);
    });
}

pub(crate) fn install_title_bar(app: &mut App, bar: TitleBar) {
    ensure_plugin(app, DiegeticUiPlugin);
    app.add_systems(PostUpdate, title_bar::refresh_changed_title_bar);
    app.add_systems(
        Startup,
        move |mut commands: Commands, home: Option<Res<CameraHomeConfig>>| {
            title_bar::spawn_title_bar_with_home_chip(&mut commands, &bar, home.as_deref());
        },
    );
}

fn unlit_panel_material() -> StandardMaterial {
    StandardMaterial {
        unlit: true,
        ..default_panel_material()
    }
}

fn panel_frame(
    builder: &mut LayoutBuilder,
    width: Sizing,
    content: impl FnOnce(&mut LayoutBuilder),
) {
    builder.with(
        El::new()
            .width(width)
            .height(Sizing::FIT)
            .padding(Padding::all(FRAME_PAD))
            .corner_radius(CornerRadius::all(RADIUS))
            .border(Border::all(BORDER, BORDER_ACCENT)),
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::GROW)
                    .padding(Padding::all(INNER_PAD))
                    .corner_radius(CornerRadius::all(INNER_RADIUS))
                    .background(INNER_BG)
                    .border(Border::all(Px(1.0), BORDER_DIM)),
                content,
            );
        },
    );
}
