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

#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
pub(super) struct WidgetPreorder(usize);

/// Reifies widget entities for every changed computed panel.
pub(super) fn reify_widgets(
    mut changed_panels: Query<
        (
            Entity,
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
        Option<&Cascade<super::WidgetInteractivity>>,
        Option<&CascadeFrom>,
    )>,
    mut commands: Commands,
) {
    for (panel_entity, computed, panel_widgets, mut widget_index) in &mut changed_panels {
        let existing_entities: &[Entity] = panel_widgets.map_or(&[], |widgets| &**widgets);
        let existing_by_id: HashMap<&PanelElementId, Entity> = existing_entities
            .iter()
            .filter_map(|entity| {
                existing_widgets
                    .get(*entity)
                    .ok()
                    .map(|(widget, _, _, _, _, _)| (widget.id(), *entity))
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
                ),
                Some(entity) => {
                    update_widget(
                        &mut commands,
                        entity,
                        record.kind(),
                        record.authored(),
                        record.preorder(),
                        record.interactivity(),
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
                interactivity,
                CascadeFrom::new(panel),
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
    panel: Entity,
    existing_widgets: &Query<(
        &PanelWidget,
        &WidgetKind,
        &WidgetSpec,
        &WidgetPreorder,
        Option<&Cascade<super::WidgetInteractivity>>,
        Option<&CascadeFrom>,
    )>,
) {
    let Ok((
        _,
        existing_kind,
        existing_authored,
        existing_preorder,
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
    if existing_interactivity != Some(&interactivity) {
        widget.insert(interactivity);
    }
    if existing_cascade_from.is_none_or(|relationship| relationship.target() != panel) {
        widget.insert(CascadeFrom::new(panel));
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;

    use super::WidgetPreorder;
    use crate::Button;
    use crate::DiegeticPanel;
    use crate::DiegeticPanelCommands;
    use crate::El;
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
    use crate::text::DiegeticTextMeasurer;
    use crate::widgets::WidgetKind;
    use crate::widgets::WidgetSpec;
    use crate::widgets::WidgetsPlugin;

    fn widget_tree(ids: &[&str]) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        for id in ids {
            builder.with(El::new().button(*id, Button::new()), |_| {});
        }
        builder.build()
    }

    fn slider_tree(id: &str, initial_value: f32) -> Option<LayoutTree> {
        let range = SliderRange::new(0.0, 10.0).ok()?;
        let slider = Slider::new(range, initial_value).ok()?;
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(El::new().slider(id, slider), |_| {});
        Some(builder.build())
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
        let expected_authored = tree
            .computed_widget_records()
            .into_iter()
            .next()
            .map(|record| record.authored().clone());

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
        let expected = slider_tree("level", 8.0)
            .and_then(|tree| tree.computed_widget_records().into_iter().next())
            .map(|record| record.authored().clone());
        assert_eq!(app.world().get::<WidgetSpec>(widget), expected.as_ref());
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
