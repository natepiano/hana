//! Capability: small screen-space panels for examples.

mod constants;
mod description;
mod help_overlay;
mod performance;
mod title_bar;

use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy_diegetic::Border;
use bevy_diegetic::CornerRadius;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::default_panel_material;
pub use description::DescriptionPanel;
pub use performance::StatsPanelRow;
pub use performance::StatsPanelSection;
pub use performance::diegetic_stats_panel;
pub use performance::diegetic_stats_sections_panel;
pub use performance::diegetic_stats_sections_tree;
pub use performance::diegetic_stats_tree;
pub use performance::fps_stats_panel;
pub use performance::gpu_meter_panel;
pub use title_bar::ControlActivation;
pub use title_bar::TitleBar;
pub use title_bar::TitleBarControl;
pub(crate) use title_bar::TitleBarControlRegistry;
pub(crate) use title_bar::TitleBarControlState;
pub use title_bar::TitleBarOrientation;
pub use title_bar::TitleBarSegment;
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

#[derive(Component)]
pub(crate) struct FairyDustOverlayPanel;

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

pub(crate) fn install_overlay_picking(app: &mut App) {
    app.add_observer(ignore_screen_panel_picking)
        .add_observer(ignore_overlay_mesh_picking_on_mesh_added)
        .add_observer(ignore_overlay_mesh_picking_on_parent_added);
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

fn ignore_screen_panel_picking(
    trigger: On<Add, DiegeticPanel>,
    panels: Query<&DiegeticPanel>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(panel) = panels.get(entity) else {
        return;
    };
    if panel.coordinate_space().is_screen() {
        commands
            .entity(entity)
            .insert((FairyDustOverlayPanel, Pickable::IGNORE));
    }
}

fn ignore_overlay_mesh_picking_on_mesh_added(
    trigger: On<Add, Mesh3d>,
    parents: Query<&ChildOf>,
    panels: Query<(), With<FairyDustOverlayPanel>>,
    mut commands: Commands,
) {
    ignore_overlay_mesh_picking(trigger.event_target(), &parents, &panels, &mut commands);
}

fn ignore_overlay_mesh_picking_on_parent_added(
    trigger: On<Add, ChildOf>,
    meshes: Query<(), With<Mesh3d>>,
    parents: Query<&ChildOf>,
    panels: Query<(), With<FairyDustOverlayPanel>>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    if meshes.get(entity).is_ok() {
        ignore_overlay_mesh_picking(entity, &parents, &panels, &mut commands);
    }
}

fn ignore_overlay_mesh_picking(
    entity: Entity,
    parents: &Query<&ChildOf>,
    panels: &Query<(), With<FairyDustOverlayPanel>>,
    commands: &mut Commands,
) {
    if has_overlay_panel_ancestor(entity, parents, panels) {
        commands.entity(entity).insert(Pickable::IGNORE);
    }
}

fn has_overlay_panel_ancestor(
    entity: Entity,
    parents: &Query<&ChildOf>,
    panels: &Query<(), With<FairyDustOverlayPanel>>,
) -> bool {
    let mut current = entity;
    while let Ok(parent) = parents.get(current) {
        let parent_entity = parent.parent();
        if panels.get(parent_entity).is_ok() {
            return true;
        }
        current = parent_entity;
    }
    false
}

#[cfg(test)]
mod tests {
    use bevy::picking::Pickable;
    use bevy::prelude::*;
    use bevy_diegetic::DiegeticPanel;
    use bevy_diegetic::Sizing;

    use super::FairyDustOverlayPanel;
    use super::install_overlay_picking;

    #[test]
    fn screen_panel_root_ignores_picking() -> Result<(), String> {
        let mut app = App::new();
        install_overlay_picking(&mut app);

        let panel = spawn_screen_panel(&mut app)?;

        assert_eq!(app.world().get::<Pickable>(panel), Some(&Pickable::IGNORE));
        assert!(app.world().get::<FairyDustOverlayPanel>(panel).is_some());
        Ok(())
    }

    #[test]
    fn mesh_child_under_screen_panel_ignores_picking() -> Result<(), String> {
        let mut app = App::new();
        install_overlay_picking(&mut app);

        let panel = spawn_screen_panel(&mut app)?;
        let child = app
            .world_mut()
            .spawn((Mesh3d::default(), ChildOf(panel)))
            .id();

        assert_eq!(app.world().get::<Pickable>(child), Some(&Pickable::IGNORE));
        Ok(())
    }

    #[test]
    fn mesh_grandchild_under_screen_panel_ignores_picking() -> Result<(), String> {
        let mut app = App::new();
        install_overlay_picking(&mut app);

        let panel = spawn_screen_panel(&mut app)?;
        let child = app.world_mut().spawn(ChildOf(panel)).id();
        let grandchild = app
            .world_mut()
            .spawn((Mesh3d::default(), ChildOf(child)))
            .id();

        assert_eq!(
            app.world().get::<Pickable>(grandchild),
            Some(&Pickable::IGNORE)
        );
        Ok(())
    }

    #[test]
    fn unrelated_mesh_keeps_default_pickability() {
        let mut app = App::new();
        install_overlay_picking(&mut app);

        let entity = app.world_mut().spawn(Mesh3d::default()).id();

        assert!(app.world().get::<Pickable>(entity).is_none());
    }

    fn spawn_screen_panel(app: &mut App) -> Result<Entity, String> {
        let panel = DiegeticPanel::screen()
            .size(Sizing::FIT, Sizing::FIT)
            .build()
            .map_err(|error| format!("{error}"))?;
        Ok(app.world_mut().spawn(panel).id())
    }
}
