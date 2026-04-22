//! [`CascadePanelChildPlugin`] — the 3-tier (entity → panel → global)
//! plugin. Registers two spawn observers and two propagation systems; the
//! panel's own `Resolved<A>` is reconciled first, then its transitions are
//! fanned out to children without a tier-1 override.

use std::marker::PhantomData;

use bevy::ecs::component::Mutable;
use bevy::prelude::*;

use super::defaults;
use super::defaults::CascadeDefaults;
use super::resolved;
use super::resolved::CascadePanelChild;
use super::resolved::Resolved;
use super::set::CascadeSet;

/// Plugin that wires every write path for a 3-tier cascade `A`.
///
/// Registers two spawn observers (one for the panel's `A::PanelOverride`, one
/// for the child's `A::EntityOverride`) and two propagation systems in
/// [`CascadeSet::Propagate`]: [`reconcile_panel_resolved`] followed by
/// [`propagate_panel_to_children`].
pub struct CascadePanelChildPlugin<A: CascadePanelChild>(PhantomData<A>);

impl<A: CascadePanelChild> Default for CascadePanelChildPlugin<A> {
    fn default() -> Self { Self(PhantomData) }
}

impl<A: CascadePanelChild> Plugin for CascadePanelChildPlugin<A>
where
    A::PanelOverride: Component<Mutability = Mutable>,
    A::EntityOverride: Component<Mutability = Mutable>,
{
    fn build(&self, app: &mut App) {
        app.register_type::<Resolved<A>>()
            .add_observer(on_panel_added::<A>)
            .add_observer(on_panel_child_added::<A>)
            .add_systems(
                Update,
                (
                    reconcile_panel_resolved::<A>,
                    propagate_panel_to_children::<A>,
                )
                    .chain()
                    .in_set(CascadeSet::Propagate),
            );
    }
}

/// Populate the panel's own `Resolved<A>` when its tier-2 override component
/// is first inserted.
fn on_panel_added<A: CascadePanelChild>(
    trigger: On<Add, A::PanelOverride>,
    panels: Query<&A::PanelOverride>,
    defaults: Res<CascadeDefaults>,
    mut commands: Commands,
) {
    let panel = trigger.event_target();
    let Ok(panel_override) = panels.get(panel) else {
        return;
    };
    let resolved = A::panel_value(panel_override).unwrap_or_else(|| A::global_default(&defaults));
    commands.entity(panel).insert(Resolved(resolved));
}

/// Populate a child's `Resolved<A>` when its tier-1 override component is
/// first inserted.
///
/// Reads the parent panel's **raw** `A::PanelOverride` (not the panel's
/// `Resolved<A>`): if panel and child are spawned in the same command batch,
/// the panel's queued `insert(Resolved(...))` hasn't flushed yet when this
/// observer fires, but the panel's override component is already visible.
/// Resolving from raw inputs makes this observer race-free regardless of
/// spawn ordering.
fn on_panel_child_added<A: CascadePanelChild>(
    trigger: On<Add, A::EntityOverride>,
    children: Query<(&A::EntityOverride, &ChildOf)>,
    panel_overrides: Query<&A::PanelOverride>,
    defaults: Res<CascadeDefaults>,
    mut commands: Commands,
) {
    let child = trigger.event_target();
    let Ok((child_override, child_of)) = children.get(child) else {
        return;
    };
    let panel_override = panel_overrides.get(child_of.parent()).ok();
    let resolved = resolved::resolve_panel_child::<A>(child_override, panel_override, &defaults);
    commands.entity(child).insert(Resolved(resolved));
}

/// Keep each panel's `Resolved<A>` in sync with its two input sources — the
/// panel's own `A::PanelOverride` and [`CascadeDefaults`].
///
/// Runs every frame; does real work only when either source changed. When
/// the global default shifts, every panel is re-checked; when a single panel
/// mutates its override, only that panel is re-checked. Writes skip when the
/// resolved value didn't actually transition, so downstream
/// `Changed<Resolved<A>>` readers only wake on real transitions.
fn reconcile_panel_resolved<A: CascadePanelChild>(
    defaults: Res<CascadeDefaults>,
    mut sentinel: Local<Option<A>>,
    panels_changed: Query<Entity, Changed<A::PanelOverride>>,
    all_panels: Query<(Entity, &A::PanelOverride, &Resolved<A>)>,
    mut commands: Commands,
) {
    let current_global = A::global_default(&defaults);
    let global_changed = defaults::should_propagate_defaults(current_global, &mut sentinel);

    let update = |panel_entity: Entity,
                  panel_override: &A::PanelOverride,
                  old: &Resolved<A>,
                  commands: &mut Commands| {
        let new = A::panel_value(panel_override).unwrap_or(current_global);
        if old.0 != new {
            commands.entity(panel_entity).insert(Resolved(new));
        }
    };

    if global_changed {
        for (panel_entity, panel_override, old) in &all_panels {
            update(panel_entity, panel_override, old, &mut commands);
        }
    } else {
        for panel_entity in &panels_changed {
            if let Ok((_, panel_override, old)) = all_panels.get(panel_entity) {
                update(panel_entity, panel_override, old, &mut commands);
            }
        }
    }
}

/// Fan panel-level `Resolved<A>` changes out to non-tier-1 children.
///
/// Native `Changed<Resolved<A>>` on the panel is the trigger. Children with
/// a tier-1 override are skipped; for the rest, the child's `Resolved<A>` is
/// updated to match the panel's, inequality-guarded so downstream readers
/// only wake on real transitions.
fn propagate_panel_to_children<A: CascadePanelChild>(
    panels: Query<(&Resolved<A>, &Children), Changed<Resolved<A>>>,
    children: Query<(&A::EntityOverride, &Resolved<A>)>,
    mut commands: Commands,
) {
    for (panel_resolved, children_component) in &panels {
        for child_entity in children_component.iter() {
            let Ok((child_override, old)) = children.get(child_entity) else {
                continue;
            };
            if A::entity_value(child_override).is_some() {
                continue;
            }
            let new = panel_resolved.0;
            if old.0 != new {
                commands.entity(child_entity).insert(Resolved(new));
            }
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use super::*;

    // A throwaway 3-tier test attribute exercising the `CascadePanelChild`
    // path (panel's own Resolved + child fan-out on Changed<Resolved>).

    #[derive(Clone, Copy, Debug, PartialEq, Reflect)]
    struct TestAlpha(AlphaMode);

    #[derive(Component, Clone, Copy, Debug, Reflect)]
    struct TestPanelOverride(Option<AlphaMode>);

    #[derive(Component, Clone, Copy, Debug, Reflect)]
    struct TestChildOverride(Option<AlphaMode>);

    impl CascadePanelChild for TestAlpha {
        type EntityOverride = TestChildOverride;
        type PanelOverride = TestPanelOverride;

        fn entity_value(c: &TestChildOverride) -> Option<Self> { c.0.map(Self) }
        fn panel_value(c: &TestPanelOverride) -> Option<Self> { c.0.map(Self) }
        fn global_default(d: &CascadeDefaults) -> Self { Self(d.text_alpha) }
    }

    fn panel_child_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<CascadeDefaults>()
            .add_plugins(CascadePanelChildPlugin::<TestAlpha>::default());
        app
    }

    fn read_resolved(app: &App, entity: Entity) -> TestAlpha {
        app.world()
            .get::<Resolved<TestAlpha>>(entity)
            .expect("Resolved<TestAlpha> should be present")
            .0
    }

    #[test]
    fn spawn_with_no_overrides_resolves_to_global() {
        let mut app = panel_child_app();
        let panel = app.world_mut().spawn(TestPanelOverride(None)).id();
        let child = app
            .world_mut()
            .spawn((TestChildOverride(None), ChildOf(panel)))
            .id();
        app.update();

        assert_eq!(read_resolved(&app, panel), TestAlpha(AlphaMode::Blend));
        assert_eq!(read_resolved(&app, child), TestAlpha(AlphaMode::Blend));
    }

    #[test]
    fn panel_override_resolves_for_child_without_tier1() {
        let mut app = panel_child_app();
        let panel = app
            .world_mut()
            .spawn(TestPanelOverride(Some(AlphaMode::Opaque)))
            .id();
        let child = app
            .world_mut()
            .spawn((TestChildOverride(None), ChildOf(panel)))
            .id();
        app.update();

        assert_eq!(read_resolved(&app, panel), TestAlpha(AlphaMode::Opaque));
        assert_eq!(read_resolved(&app, child), TestAlpha(AlphaMode::Opaque));
    }

    #[test]
    fn child_tier1_override_wins_over_panel() {
        let mut app = panel_child_app();
        let panel = app
            .world_mut()
            .spawn(TestPanelOverride(Some(AlphaMode::Opaque)))
            .id();
        let child = app
            .world_mut()
            .spawn((TestChildOverride(Some(AlphaMode::Multiply)), ChildOf(panel)))
            .id();
        app.update();

        assert_eq!(read_resolved(&app, child), TestAlpha(AlphaMode::Multiply));
    }

    #[test]
    fn panel_override_mutation_propagates_to_child() {
        let mut app = panel_child_app();
        let panel = app.world_mut().spawn(TestPanelOverride(None)).id();
        let child = app
            .world_mut()
            .spawn((TestChildOverride(None), ChildOf(panel)))
            .id();
        app.update();
        assert_eq!(read_resolved(&app, child), TestAlpha(AlphaMode::Blend));

        app.world_mut()
            .entity_mut(panel)
            .get_mut::<TestPanelOverride>()
            .expect("panel should have override")
            .0 = Some(AlphaMode::Opaque);
        // reconcile_panel_resolved writes the panel's new Resolved; the
        // chained propagate_panel_to_children fans it to the child within
        // the same frame.
        app.update();

        assert_eq!(read_resolved(&app, panel), TestAlpha(AlphaMode::Opaque));
        assert_eq!(read_resolved(&app, child), TestAlpha(AlphaMode::Opaque));
    }

    #[test]
    fn panel_override_mutation_skips_tier1_children() {
        let mut app = panel_child_app();
        let panel = app.world_mut().spawn(TestPanelOverride(None)).id();
        let child = app
            .world_mut()
            .spawn((TestChildOverride(Some(AlphaMode::Multiply)), ChildOf(panel)))
            .id();
        app.update();

        app.world_mut()
            .entity_mut(panel)
            .get_mut::<TestPanelOverride>()
            .expect("panel should have override")
            .0 = Some(AlphaMode::Opaque);
        app.update();

        assert_eq!(read_resolved(&app, panel), TestAlpha(AlphaMode::Opaque));
        assert_eq!(read_resolved(&app, child), TestAlpha(AlphaMode::Multiply));
    }

    #[test]
    fn global_default_mutation_propagates_to_panel_and_child() {
        let mut app = panel_child_app();
        let panel = app.world_mut().spawn(TestPanelOverride(None)).id();
        let child = app
            .world_mut()
            .spawn((TestChildOverride(None), ChildOf(panel)))
            .id();
        app.update();

        app.world_mut().resource_mut::<CascadeDefaults>().text_alpha = AlphaMode::Opaque;
        app.update();

        assert_eq!(read_resolved(&app, panel), TestAlpha(AlphaMode::Opaque));
        assert_eq!(read_resolved(&app, child), TestAlpha(AlphaMode::Opaque));
    }
}
