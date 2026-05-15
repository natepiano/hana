//! Capability: small screen-space panels for examples.

mod constants;
mod description;
mod title_bar;

use bevy::prelude::*;
use bevy_diegetic::Border;
use bevy_diegetic::CornerRadius;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::default_panel_material;
pub use description::DescriptionPanel;
pub(crate) use title_bar::ControlActivation;
pub use title_bar::TitleBar;
pub(crate) use title_bar::TitleBarControlState;

use crate::camera_home::CameraHomeConfig;
use crate::constants::BORDER;
use crate::constants::BORDER_ACCENT;
use crate::constants::BORDER_DIM;
use crate::constants::FRAME_PAD;
use crate::constants::INNER_BG;
use crate::constants::INNER_BORDER_WIDTH;
use crate::constants::INNER_PAD;
use crate::constants::INNER_RADIUS;
use crate::constants::RADIUS;
use crate::ensure_plugin;

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
                    .border(Border::all(INNER_BORDER_WIDTH, BORDER_DIM)),
                content,
            );
        },
    );
}
