use std::collections::HashMap;

use bevy::prelude::*;

use super::PanelWidget;
use super::PanelWidgetIndex;
use super::PanelWidgets;
use super::WidgetKind;
use super::WidgetOf;
use super::WidgetSpec;
use crate::PanelElementId;
use crate::cascade::Cascade;
use crate::cascade::CascadeFrom;
use crate::panel::ComputedDiegeticPanel;
use crate::panel::DiegeticPanel;
use crate::panel::PanelOwned;

#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
pub(super) struct WidgetPreorder(usize);

/// Reifies widget entities for every changed computed panel.
pub(super) fn reify_widgets(
    mut changed_panels: Query<
        (
            Entity,
            &DiegeticPanel,
            &ComputedDiegeticPanel,
            Option<&PanelWidgets>,
            &mut PanelWidgetIndex,
        ),
        Changed<ComputedDiegeticPanel>,
    >,
    existing_widgets: Query<(
        &PanelWidget,
        &WidgetKind,
        &WidgetSpec,
        &WidgetPreorder,
        &Transform,
        Option<&Cascade<super::WidgetInteractivity>>,
        Option<&CascadeFrom>,
    )>,
    mut commands: Commands,
) {
    for (panel_entity, panel, computed, panel_widgets, mut widget_index) in &mut changed_panels {
        let existing_entities: &[Entity] = panel_widgets.map_or(&[], |widgets| &**widgets);
        let existing_by_id: HashMap<&PanelElementId, Entity> = existing_entities
            .iter()
            .filter_map(|entity| {
                existing_widgets
                    .get(*entity)
                    .ok()
                    .map(|(widget, _, _, _, _, _, _)| (widget.id(), *entity))
            })
            .collect();

        let mut visited = Vec::with_capacity(computed.widget_records().len());
        let mut next_widget_index = HashMap::with_capacity(computed.widget_records().len());
        for record in computed.widget_records() {
            let entity = match existing_by_id.get(record.id()).copied() {
                None => spawn_widget(
                    &mut commands,
                    panel_entity,
                    record.id().clone(),
                    record.kind(),
                    record.authored().clone(),
                    record.preorder(),
                    record.interactivity(),
                    widget_transform(panel, record.rect()),
                ),
                Some(entity) => {
                    update_widget(
                        &mut commands,
                        entity,
                        record.kind(),
                        record.authored(),
                        record.preorder(),
                        record.interactivity(),
                        widget_transform(panel, record.rect()),
                        panel_entity,
                        &existing_widgets,
                    );
                    entity
                },
            };
            visited.push(entity);
            next_widget_index.insert(record.id().clone(), entity);
        }

        for &entity in existing_entities {
            if !visited.contains(&entity) {
                commands.entity(entity).despawn();
            }
        }

        widget_index.replace(next_widget_index);
    }
}

fn spawn_widget(
    commands: &mut Commands<'_, '_>,
    panel: Entity,
    id: PanelElementId,
    kind: WidgetKind,
    authored: WidgetSpec,
    preorder: usize,
    interactivity: Cascade<super::WidgetInteractivity>,
    transform: Transform,
) -> Entity {
    let mut spawned = Entity::PLACEHOLDER;
    commands.entity(panel).with_children(|children| {
        spawned = children
            .spawn((
                PanelWidget::new(id),
                WidgetOf::new(panel),
                kind,
                authored,
                WidgetPreorder(preorder),
                transform,
                interactivity,
                CascadeFrom::new(panel),
                PanelOwned::from(panel),
            ))
            .id();
    });
    spawned
}

fn update_widget(
    commands: &mut Commands<'_, '_>,
    entity: Entity,
    kind: WidgetKind,
    authored: &WidgetSpec,
    preorder: usize,
    interactivity: Cascade<super::WidgetInteractivity>,
    transform: Transform,
    panel: Entity,
    existing_widgets: &Query<(
        &PanelWidget,
        &WidgetKind,
        &WidgetSpec,
        &WidgetPreorder,
        &Transform,
        Option<&Cascade<super::WidgetInteractivity>>,
        Option<&CascadeFrom>,
    )>,
) {
    let Ok((
        _,
        existing_kind,
        existing_authored,
        existing_preorder,
        existing_transform,
        existing_interactivity,
        existing_cascade_from,
    )) = existing_widgets.get(entity)
    else {
        return;
    };
    let mut widget = commands.entity(entity);
    if *existing_kind != kind {
        widget.insert(kind);
    }
    if existing_authored != authored {
        widget.insert(authored.clone());
    }
    if existing_preorder.0 != preorder {
        widget.insert(WidgetPreorder(preorder));
    }
    if *existing_transform != transform {
        widget.insert(transform);
    }
    if existing_interactivity != Some(&interactivity) {
        widget.insert(interactivity);
    }
    if existing_cascade_from.is_none_or(|relationship| relationship.target() != panel) {
        widget.insert(CascadeFrom::new(panel));
    }
}

fn widget_transform(panel: &DiegeticPanel, rect: crate::BoundingBox) -> Transform {
    let scale = panel.points_to_world();
    let (x_offset, y_offset) = panel.anchor_offsets();
    Transform::from_xyz(
        rect.x.mul_add(scale, -x_offset),
        (-rect.y).mul_add(scale, y_offset),
        0.0,
    )
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;
    use bevy::window::PrimaryWindow;

    use super::WidgetPreorder;
    use crate::Anchor;
    use crate::Button;
    use crate::ComputedDiegeticPanel;
    use crate::DiegeticPanel;
    use crate::DiegeticPanelCommands;
    use crate::El;
    use crate::Fit;
    use crate::HeadlessLayoutPlugin;
    use crate::LayoutBuilder;
    use crate::LayoutTree;
    use crate::Mm;
    use crate::PanelElementId;
    use crate::PanelWidget;
    use crate::PanelWidgetReader;
    use crate::PanelWidgets;
    use crate::Slider;
    use crate::SliderRange;
    use crate::WidgetInteractivity;
    use crate::WidgetOf;
    use crate::cascade::Cascade;
    use crate::cascade::CascadeFrom;
    use crate::screen_space::ScreenSpacePlugin;
    use crate::text::DiegeticTextMeasurer;
    use crate::widgets::PanelWidgetIndex;
    use crate::widgets::WidgetKind;
    use crate::widgets::WidgetSpec;
    use crate::widgets::WidgetsPlugin;

    const SCREEN_FIT_SPACER_WIDTH: f32 = 30.0;
    const SCREEN_FIT_WIDGET_HEIGHT: f32 = 10.0;
    const SCREEN_FIT_WIDGET_WIDTH: f32 = 20.0;

    #[derive(Component)]
    struct ApplicationData;

    fn widget_tree(ids: &[&str]) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        for id in ids {
            builder.with(El::new().button(*id, Button::new()), |_| {});
        }
        builder.build()
    }

    fn slider_tree(id: &str, initial_value: f32) -> Option<LayoutTree> {
        let WidgetSpec::Slider(slider) = slider_spec(initial_value)? else {
            return None;
        };
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(El::new().slider(id, slider), |_| {});
        Some(builder.build())
    }

    fn ranked_slider_tree(initial_value: f32, z_index: i8) -> Option<LayoutTree> {
        let WidgetSpec::Slider(slider) = slider_spec(initial_value)? else {
            return None;
        };
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(
            El::new()
                .size(20.0, 10.0)
                .slider("level", slider)
                .widget_interactivity(WidgetInteractivity::Disabled)
                .z_index(z_index),
            |_| {},
        );
        builder.with(
            El::new().size(20.0, 10.0).button("peer", Button::new()),
            |_| {},
        );
        Some(builder.build())
    }

    fn slider_spec(initial_value: f32) -> Option<WidgetSpec> {
        let range = SliderRange::new(0.0, 10.0).ok()?;
        Slider::new(range, initial_value)
            .ok()
            .map(WidgetSpec::Slider)
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(DiegeticTextMeasurer::default())
            .add_plugins((HeadlessLayoutPlugin, WidgetsPlugin));
        app
    }

    fn spawn_panel(app: &mut App, tree: LayoutTree) -> Option<Entity> {
        let result = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .with_tree(tree)
            .build();
        assert!(result.is_ok());
        let Ok(panel) = result else {
            return None;
        };
        Some(app.world_mut().spawn(panel).id())
    }

    fn resolve_widget(app: &mut App, panel: Entity, id: PanelElementId) -> Option<Entity> {
        app.world_mut()
            .run_system_once(move |reader: PanelWidgetReader| reader.entity(panel, &id))
            .ok()
            .flatten()
    }

    #[track_caller]
    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-6,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn reify_creates_child_relationship_and_lookup() {
        let mut app = test_app();
        let Some(panel) = spawn_panel(&mut app, widget_tree(&["action"])) else {
            return;
        };

        assert!(
            resolve_widget(&mut app, panel, PanelElementId::named("action")).is_none(),
            "a widget must not resolve before its first computed reification",
        );
        app.update();

        let Some(widget) = resolve_widget(&mut app, panel, PanelElementId::named("action")) else {
            return;
        };
        assert_eq!(
            app.world().get::<PanelWidget>(widget).map(PanelWidget::id),
            Some(&PanelElementId::named("action"))
        );
        assert_eq!(
            app.world().get::<WidgetOf>(widget).map(WidgetOf::panel),
            Some(panel)
        );
        assert_eq!(
            app.world().get::<ChildOf>(widget).map(ChildOf::parent),
            Some(panel)
        );
        assert!(
            app.world()
                .get::<PanelWidgets>(panel)
                .is_some_and(|widgets| widgets.contains(&widget))
        );
        assert!(resolve_widget(&mut app, panel, PanelElementId::named("missing")).is_none());
    }

    #[test]
    fn reify_places_widget_transform_at_its_solved_panel_offset() {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(El::row().size(100.0, 50.0), |builder| {
            builder.with(El::new().size(30.0, 10.0), |_| {});
            builder.with(
                El::new().size(20.0, 10.0).button("offset", Button::new()),
                |_| {},
            );
        });
        let mut app = test_app();
        let panel = spawn_panel(&mut app, builder.build()).expect("panel should build");
        app.update();
        let widget = resolve_widget(&mut app, panel, PanelElementId::named("offset"))
            .expect("widget should be reified");
        let panel_component = app
            .world()
            .get::<DiegeticPanel>(panel)
            .expect("panel should remain live");
        let transform = app
            .world()
            .get::<Transform>(widget)
            .expect("widget should carry a transform");
        let (anchor_x, anchor_y) = panel_component.anchor_offsets();
        let scale = panel_component.points_to_world();
        let rect = app
            .world()
            .get::<ComputedDiegeticPanel>(panel)
            .and_then(|computed| {
                computed
                    .widget_records()
                    .iter()
                    .find(|record| record.id() == &PanelElementId::named("offset"))
            })
            .map(crate::widgets::ComputedWidgetRecord::rect);
        let rect = rect.expect("widget should have computed bounds");
        assert_close(transform.translation.x, rect.x.mul_add(scale, -anchor_x));
        assert_close(transform.translation.y, rect.y.mul_add(-scale, anchor_y));
        assert!(app.world().get::<Mesh3d>(widget).is_none());
        assert!(
            app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(widget)
                .is_none()
        );
    }

    #[test]
    fn centered_fit_panel_reifies_from_final_dimensions() {
        let mut tree = LayoutBuilder::with_root(El::column());
        tree.with(
            El::new().size(20.0, 10.0).button("centered", Button::new()),
            |_| {},
        );
        let panel = DiegeticPanel::world()
            .size(Fit, Fit)
            .anchor(Anchor::Center)
            .with_tree(tree.build())
            .build()
            .expect("fit panel should build");
        let mut app = test_app();
        let panel = app.world_mut().spawn(panel).id();

        app.update();

        let widget = resolve_widget(&mut app, panel, PanelElementId::named("centered"))
            .expect("centered widget should be reified");
        let panel_component = app
            .world()
            .get::<DiegeticPanel>(panel)
            .expect("panel should remain live");
        let transform = app
            .world()
            .get::<Transform>(widget)
            .expect("widget should carry a transform");
        let (anchor_x, anchor_y) = panel_component.anchor_offsets();
        assert!(anchor_x > 0.0);
        assert!(anchor_y > 0.0);
        assert_close(transform.translation.x, -anchor_x);
        assert_close(transform.translation.y, anchor_y);
    }

    #[test]
    fn centered_screen_fit_reifies_off_origin_widget_from_final_dimensions() {
        let mut tree = LayoutBuilder::with_root(El::row());
        tree.with(
            El::new().size(SCREEN_FIT_SPACER_WIDTH, SCREEN_FIT_WIDGET_HEIGHT),
            |_| {},
        );
        tree.with(
            El::new()
                .size(SCREEN_FIT_WIDGET_WIDTH, SCREEN_FIT_WIDGET_HEIGHT)
                .button("screen-fit", Button::new()),
            |_| {},
        );
        let panel = DiegeticPanel::screen()
            .size(Fit, Fit)
            .anchor(Anchor::Center)
            .with_tree(tree.build())
            .build()
            .expect("screen Fit panel should build");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(DiegeticTextMeasurer::default())
            .add_plugins((HeadlessLayoutPlugin, WidgetsPlugin, ScreenSpacePlugin));
        app.world_mut().spawn((Window::default(), PrimaryWindow));
        let panel = app.world_mut().spawn(panel).id();

        app.update();

        let (scale, anchor_x, anchor_y, rect) = {
            let panel_component = app
                .world()
                .get::<DiegeticPanel>(panel)
                .expect("screen Fit panel should remain live");
            assert_close(
                panel_component.width(),
                SCREEN_FIT_SPACER_WIDTH + SCREEN_FIT_WIDGET_WIDTH,
            );
            assert_close(panel_component.height(), SCREEN_FIT_WIDGET_HEIGHT);
            let computed = app
                .world()
                .get::<ComputedDiegeticPanel>(panel)
                .expect("screen Fit panel should have computed output");
            computed
                .result()
                .expect("screen Fit panel should have a computed layout result");
            let record = computed
                .widget_records()
                .iter()
                .find(|record| record.id() == &PanelElementId::named("screen-fit"))
                .expect("screen Fit widget should have a computed record");
            assert!(
                record.rect().x > 0.0,
                "widget should be off the panel origin"
            );
            let (anchor_x, anchor_y) = panel_component.anchor_offsets();
            assert_close(anchor_x, panel_component.width() * 0.5);
            assert_close(anchor_y, panel_component.height() * 0.5);
            (
                panel_component.points_to_world(),
                anchor_x,
                anchor_y,
                record.rect(),
            )
        };
        let widget = resolve_widget(&mut app, panel, PanelElementId::named("screen-fit"))
            .expect("screen Fit widget should be reified");
        let transform = app
            .world()
            .get::<Transform>(widget)
            .expect("screen Fit widget should carry a transform");
        assert_close(transform.translation.x, rect.x.mul_add(scale, -anchor_x));
        assert_close(transform.translation.y, (-rect.y).mul_add(scale, anchor_y));
    }

    #[test]
    fn removing_panel_role_despawns_owned_children_and_preserves_application_state() {
        let mut app = test_app();
        let panel = spawn_panel(&mut app, widget_tree(&["action"])).expect("panel should build");
        app.world_mut().entity_mut(panel).insert(ApplicationData);
        let application_child = app
            .world_mut()
            .spawn((ApplicationData, ChildOf(panel)))
            .id();
        app.update();
        let widget = resolve_widget(&mut app, panel, PanelElementId::named("action"))
            .expect("widget should be reified");

        app.world_mut().entity_mut(panel).remove::<DiegeticPanel>();
        app.update();

        assert!(app.world().get_entity(panel).is_ok());
        assert!(app.world().get::<ApplicationData>(panel).is_some());
        assert!(app.world().get_entity(application_child).is_ok());
        assert!(app.world().get::<PanelWidgetIndex>(panel).is_none());
        assert!(app.world().get::<ComputedDiegeticPanel>(panel).is_none());
        assert!(app.world().get::<PanelWidgets>(panel).is_none());
        assert!(app.world().get_entity(widget).is_err());
        assert!(resolve_widget(&mut app, panel, PanelElementId::named("action")).is_none());
    }

    #[test]
    fn identical_ids_resolve_independently_per_panel() {
        let mut app = test_app();
        let Some(first_panel) = spawn_panel(&mut app, widget_tree(&["action"])) else {
            return;
        };
        let Some(second_panel) = spawn_panel(&mut app, widget_tree(&["action"])) else {
            return;
        };
        app.update();

        let first = resolve_widget(&mut app, first_panel, PanelElementId::named("action"));
        let second = resolve_widget(&mut app, second_panel, PanelElementId::named("action"));
        assert!(first.is_some());
        assert!(second.is_some());
        assert_ne!(first, second);
    }

    #[test]
    fn identical_tree_replacement_preserves_widget_lookup() {
        let mut app = test_app();
        let tree = widget_tree(&["action"]);
        let Some(panel) = spawn_panel(&mut app, tree.clone()) else {
            return;
        };
        app.update();

        let before = resolve_widget(&mut app, panel, PanelElementId::named("action"));
        assert!(before.is_some());
        let revision = app
            .world()
            .get::<DiegeticPanel>(panel)
            .map(DiegeticPanel::tree_revision)
            .map(u64::from);

        let result = app.world_mut().commands().set_tree(panel, tree);
        assert!(result.is_ok());
        app.update();

        let after = resolve_widget(&mut app, panel, PanelElementId::named("action"));
        assert_eq!(after, before);
        assert_eq!(
            app.world()
                .get::<DiegeticPanel>(panel)
                .map(DiegeticPanel::tree_revision)
                .map(u64::from),
            revision.map(|revision| revision + 1)
        );
    }

    #[test]
    fn reorder_reuses_entities_and_refreshes_preorder() {
        let mut app = test_app();
        let Some(panel) = spawn_panel(&mut app, widget_tree(&["first", "second"])) else {
            return;
        };
        app.update();
        let first_before = resolve_widget(&mut app, panel, PanelElementId::named("first"));
        let second_before = resolve_widget(&mut app, panel, PanelElementId::named("second"));

        let result = app
            .world_mut()
            .commands()
            .set_tree(panel, widget_tree(&["second", "first"]));
        assert!(result.is_ok());
        app.update();

        let first_after = resolve_widget(&mut app, panel, PanelElementId::named("first"));
        let second_after = resolve_widget(&mut app, panel, PanelElementId::named("second"));
        assert_eq!(first_before, first_after);
        assert_eq!(second_before, second_after);
        let Some(first) = first_after else {
            return;
        };
        let Some(second) = second_after else {
            return;
        };
        assert!(
            app.world()
                .get::<WidgetPreorder>(second)
                .zip(app.world().get::<WidgetPreorder>(first))
                .is_some_and(|(second_order, first_order)| second_order.0 < first_order.0)
        );
    }

    #[test]
    fn removal_sweeps_widget_and_stale_lookup_returns_none() {
        let mut app = test_app();
        let Some(panel) = spawn_panel(&mut app, widget_tree(&["keep", "remove"])) else {
            return;
        };
        app.update();
        let removed = resolve_widget(&mut app, panel, PanelElementId::named("remove"));
        assert!(removed.is_some());

        let result = app
            .world_mut()
            .commands()
            .set_tree(panel, widget_tree(&["keep"]));
        assert!(result.is_ok());
        app.update();

        assert!(app.world().get_entity(panel).is_ok());
        assert!(resolve_widget(&mut app, panel, PanelElementId::named("remove")).is_none());
        assert!(removed.is_some_and(|entity| app.world().get_entity(entity).is_err()));
    }

    #[test]
    fn kind_replacement_retains_entity_and_replaces_authored_snapshot() {
        let mut app = test_app();
        let panel = spawn_panel(&mut app, widget_tree(&["control"]));
        assert!(panel.is_some());
        let Some(panel) = panel else {
            return;
        };
        app.update();
        let before = resolve_widget(&mut app, panel, PanelElementId::named("control"));
        assert!(before.is_some());
        let Some(before) = before else {
            return;
        };
        let interactivity_tick = app
            .world()
            .entity(before)
            .get_ref::<Cascade<WidgetInteractivity>>()
            .map(|authored| authored.last_changed());
        let relationship_tick = app
            .world()
            .entity(before)
            .get_ref::<CascadeFrom>()
            .map(|relationship| relationship.last_changed());
        let tree = slider_tree("control", 4.0);
        assert!(tree.is_some());
        let Some(tree) = tree else {
            return;
        };
        let expected_authored = slider_spec(4.0);

        let result = app.world_mut().commands().set_tree(panel, tree);
        assert!(result.is_ok());
        app.update();

        let after = resolve_widget(&mut app, panel, PanelElementId::named("control"));
        assert_eq!(after, Some(before));
        let Some(widget) = after else {
            return;
        };
        assert_eq!(
            app.world().get::<WidgetKind>(widget),
            Some(&WidgetKind::Slider)
        );
        assert_eq!(
            app.world().get::<WidgetSpec>(widget),
            expected_authored.as_ref()
        );
        assert!(matches!(expected_authored, Some(WidgetSpec::Slider(_))));
        assert_eq!(
            app.world()
                .entity(widget)
                .get_ref::<Cascade<WidgetInteractivity>>()
                .map(|authored| authored.last_changed()),
            interactivity_tick
        );
        assert_eq!(
            app.world()
                .entity(widget)
                .get_ref::<CascadeFrom>()
                .map(|relationship| relationship.last_changed()),
            relationship_tick
        );
    }

    #[test]
    fn visual_only_slider_edit_refreshes_snapshot_without_replacing_entity() {
        let mut app = test_app();
        let Some(first_tree) = slider_tree("level", 2.0) else {
            return;
        };
        let Some(panel) = spawn_panel(&mut app, first_tree) else {
            return;
        };
        app.update();
        let before = resolve_widget(&mut app, panel, PanelElementId::named("level"));
        let Some(next_tree) = slider_tree("level", 8.0) else {
            return;
        };

        let result = app.world_mut().commands().set_tree(panel, next_tree);
        assert!(result.is_ok());
        app.update();

        let after = resolve_widget(&mut app, panel, PanelElementId::named("level"));
        assert_eq!(before, after);
        let Some(widget) = after else {
            return;
        };
        let expected = slider_spec(8.0);
        assert_eq!(app.world().get::<WidgetSpec>(widget), expected.as_ref());
    }

    #[test]
    fn visual_only_refresh_preserves_geometry_and_updates_rank_without_layout() {
        let mut app = test_app();
        let first_tree = ranked_slider_tree(2.0, -1).expect("slider tree should build");
        let panel = spawn_panel(&mut app, first_tree).expect("panel should build");
        app.update();
        let before_entity = resolve_widget(&mut app, panel, PanelElementId::named("level"))
            .expect("slider should be reified");
        let (rect, clipped_rect, rank, interactivity, layout_solves) = {
            let computed = app
                .world()
                .get::<ComputedDiegeticPanel>(panel)
                .expect("panel should have computed output");
            let record = computed
                .widget_records()
                .iter()
                .find(|record| record.id() == &PanelElementId::named("level"))
                .expect("slider should have a computed record");
            (
                record.rect(),
                record.clipped_rect(),
                record.interaction_rank(),
                record.interactivity(),
                computed.layout_solves(),
            )
        };
        let next_tree = ranked_slider_tree(8.0, 1).expect("slider tree should build");

        app.world_mut()
            .commands()
            .set_tree(panel, next_tree)
            .expect("visual-only replacement should be accepted");
        app.update();

        let after_entity = resolve_widget(&mut app, panel, PanelElementId::named("level"))
            .expect("slider should remain reified");
        assert_eq!(after_entity, before_entity);
        let expected = slider_spec(8.0).expect("slider specification should be valid");
        let computed = app
            .world()
            .get::<ComputedDiegeticPanel>(panel)
            .expect("panel should retain computed output");
        let record = computed
            .widget_records()
            .iter()
            .find(|record| record.id() == &PanelElementId::named("level"))
            .expect("slider should retain its computed record");
        assert_eq!(record.rect(), rect);
        assert_eq!(record.clipped_rect(), clipped_rect);
        assert_ne!(record.interaction_rank(), rank);
        assert_eq!(record.authored(), &expected);
        assert_eq!(record.interactivity(), interactivity);
        assert_eq!(computed.layout_solves(), layout_solves);
    }

    #[test]
    fn stale_entity_and_panel_despawn_are_safe() {
        let mut app = test_app();
        let Some(panel) = spawn_panel(&mut app, widget_tree(&["action"])) else {
            return;
        };
        app.update();
        let widget = resolve_widget(&mut app, panel, PanelElementId::named("action"));
        let Some(widget) = widget else {
            return;
        };
        app.world_mut().entity_mut(widget).despawn();
        assert!(resolve_widget(&mut app, panel, PanelElementId::named("action")).is_none());

        let result = app
            .world_mut()
            .commands()
            .set_tree(panel, widget_tree(&["replacement"]));
        assert!(result.is_ok());
        app.update();
        let replacement = resolve_widget(&mut app, panel, PanelElementId::named("replacement"));
        assert!(replacement.is_some());
        app.world_mut().entity_mut(panel).despawn();
        assert!(replacement.is_some_and(|entity| app.world().get_entity(entity).is_err()));
    }
}
