//! Capability: small screen-space panels for examples.

mod constants;
mod description;
mod help_overlay;
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
pub use title_bar::ControlActivation;
pub use title_bar::TitleBar;
pub use title_bar::TitleBarControl;
pub(crate) use title_bar::TitleBarControlRegistry;
pub(crate) use title_bar::TitleBarControlState;
pub use title_bar::TitleBarOrientation;
pub use title_bar::TitleChip;
pub use title_bar::TitleChipActivation;

use crate::camera_home::CameraHomeConfig;
use crate::constants::BORDER;
use crate::constants::BORDER_ACCENT;
use crate::constants::BORDER_DIM;
use crate::constants::FRAME_PAD;
use crate::constants::INNER_BACKGROUND;
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

pub(crate) fn install_title_bar(app: &mut App, title_bar: TitleBar) {
    ensure_plugin(app, DiegeticUiPlugin);
    help_overlay::install(app);
    app.add_systems(PostUpdate, title_bar::refresh_changed_title_bar);
    app.add_systems(
        Startup,
        move |mut commands: Commands,
              home: Option<Res<CameraHomeConfig>>,
              registry: Option<Res<TitleBarControlRegistry>>| {
            title_bar::spawn_title_bar_with_home_chip(
                &mut commands,
                &title_bar,
                home.as_deref(),
                registry.as_deref(),
            );
        },
    );
}

pub(crate) fn register_title_control(app: &mut App, control: impl Into<TitleBarControl>) {
    let mut registry = app
        .world_mut()
        .get_resource_or_insert_with(TitleBarControlRegistry::default);
    registry.push(control);
}

/// Material used by Fairy Dust screen-space panels.
#[must_use]
pub fn screen_panel_material() -> StandardMaterial {
    StandardMaterial {
        unlit: true,
        ..default_panel_material()
    }
}

/// Adds the standard Fairy Dust screen-panel frame, then lets the caller
/// populate the inner panel contents.
pub fn screen_panel_frame(
    builder: &mut LayoutBuilder,
    width: Sizing,
    height: Sizing,
    background: Color,
    content: impl FnOnce(&mut LayoutBuilder),
) {
    builder.with(
        El::new()
            .width(width)
            .height(height)
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
                    .background(background)
                    .border(Border::all(INNER_BORDER_WIDTH, BORDER_DIM)),
                content,
            );
        },
    );
}

/// Default background color for screen panels — exposed so per-panel
/// builders can substitute it when no override is provided.
pub(super) const fn default_inner_background() -> Color { INNER_BACKGROUND }
