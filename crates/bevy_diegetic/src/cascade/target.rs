//! The two 2-tier plugins — [`CascadePanelPlugin`] and
//! [`CascadeEntityPlugin`] — plus the shared machinery they both wire.
//!
//! Grouped as a topology cohort: both plugins share `build_cascade_target`,
//! `on_cascade_target_added`, and `propagate_global_default_to_entity`
//! verbatim. The two plugin types exist so each registration site names its
//! semantic topology — panel-targeted vs. entity-targeted — even though the
//! runtime behavior is identical.

use std::marker::PhantomData;

use bevy::ecs::component::Mutable;
use bevy::prelude::*;

use super::cascade_set::CascadeSet;
use super::defaults;
use super::defaults::CascadeDefaults;
use super::resolved;
use super::resolved::CascadeTarget;
use super::resolved::Resolved;

/// Plugin that wires the 2-tier write paths for an attribute whose override
/// lives on a panel entity. Uses the same internals as
/// [`CascadeEntityPlugin`]; the two plugin types exist so each attribute's
/// registration site names the topology it targets.
pub struct CascadePanelPlugin<A: CascadeTarget>(PhantomData<A>);

impl<A: CascadeTarget> Default for CascadePanelPlugin<A> {
    fn default() -> Self { Self(PhantomData) }
}

impl<A: CascadeTarget> Plugin for CascadePanelPlugin<A>
where
    A::Override: Component<Mutability = Mutable>,
{
    fn build(&self, app: &mut App) { build_cascade_target::<A>(app); }
}

/// Plugin that wires the 2-tier write paths for an attribute whose override
/// lives on an arbitrary entity. Shares machinery with
/// [`CascadePanelPlugin`]; see that plugin's doc for rationale.
pub struct CascadeEntityPlugin<A: CascadeTarget>(PhantomData<A>);

impl<A: CascadeTarget> Default for CascadeEntityPlugin<A> {
    fn default() -> Self { Self(PhantomData) }
}

impl<A: CascadeTarget> Plugin for CascadeEntityPlugin<A>
where
    A::Override: Component<Mutability = Mutable>,
{
    fn build(&self, app: &mut App) { build_cascade_target::<A>(app); }
}

fn build_cascade_target<A: CascadeTarget>(app: &mut App)
where
    A::Override: Component<Mutability = Mutable>,
{
    app.register_type::<Resolved<A>>()
        .add_observer(on_cascade_target_added::<A>)
        .add_systems(
            Update,
            propagate_global_default_to_entity::<A>.in_set(CascadeSet::Propagate),
        );
}

/// Populate the target entity's `Resolved<A>` when its override component is
/// first inserted.
fn on_cascade_target_added<A: CascadeTarget>(
    trigger: On<Add, A::Override>,
    targets: Query<&A::Override>,
    defaults: Res<CascadeDefaults>,
    mut commands: Commands,
) {
    let target = trigger.event_target();
    let Ok(entity_override) = targets.get(target) else {
        return;
    };
    let resolved = resolved::resolve_target::<A>(entity_override, &defaults);
    commands.entity(target).insert(Resolved(resolved));
}

/// Recompute every target entity's `Resolved<A>` when [`CascadeDefaults`]
/// transitions for this cascade's projected field. Sentinel-gated; no-op on
/// frames where unrelated `CascadeDefaults` fields changed.
fn propagate_global_default_to_entity<A: CascadeTarget>(
    defaults: Res<CascadeDefaults>,
    mut last_seen: Local<Option<A>>,
    targets: Query<(Entity, &A::Override, &Resolved<A>)>,
    mut commands: Commands,
) {
    let current = A::global_default(&defaults);
    if !defaults::should_propagate_defaults(current, &mut last_seen) {
        return;
    }
    for (entity, entity_override, old) in &targets {
        let new = resolved::resolve_target::<A>(entity_override, &defaults);
        if old.0 != new {
            commands.entity(entity).insert(Resolved(new));
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
    use crate::layout::Unit;

    // A throwaway 2-tier test attribute projected onto
    // `CascadeDefaults.layout_unit`. Picked because mutating `layout_unit`
    // has no cascade propagation of its own (it's read at panel
    // construction, not cascade-watched), so the test is hermetic even
    // inside an app wired with every real cascade.

    #[derive(Clone, Copy, Debug, PartialEq, Reflect)]
    struct TestUnit(Unit);

    #[derive(Component, Clone, Copy, Debug, Reflect)]
    struct TestOverride(Option<Unit>);

    impl CascadeTarget for TestUnit {
        type Override = TestOverride;

        fn override_value(c: &TestOverride) -> Option<Self> { c.0.map(Self) }
        fn global_default(d: &CascadeDefaults) -> Self { Self(d.layout_unit) }
    }

    fn entity_plugin_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<CascadeDefaults>()
            .add_plugins(CascadeEntityPlugin::<TestUnit>::default());
        app
    }

    fn read_resolved(app: &App, entity: Entity) -> TestUnit {
        app.world()
            .get::<Resolved<TestUnit>>(entity)
            .expect("Resolved<TestUnit> should be present")
            .0
    }

    #[test]
    fn spawn_without_override_resolves_to_global_default() {
        let mut app = entity_plugin_app();
        let entity = app.world_mut().spawn(TestOverride(None)).id();
        app.update();

        assert_eq!(read_resolved(&app, entity), TestUnit(Unit::Meters));
    }

    #[test]
    fn spawn_with_override_resolves_to_override_value() {
        let mut app = entity_plugin_app();
        let entity = app
            .world_mut()
            .spawn(TestOverride(Some(Unit::Millimeters)))
            .id();
        app.update();

        assert_eq!(read_resolved(&app, entity), TestUnit(Unit::Millimeters));
    }

    #[test]
    fn global_default_mutation_updates_entities_without_override() {
        let mut app = entity_plugin_app();
        let entity = app.world_mut().spawn(TestOverride(None)).id();
        app.update();
        assert_eq!(read_resolved(&app, entity), TestUnit(Unit::Meters));

        app.world_mut()
            .resource_mut::<CascadeDefaults>()
            .layout_unit = Unit::Inches;
        app.update();

        assert_eq!(read_resolved(&app, entity), TestUnit(Unit::Inches));
    }

    #[test]
    fn global_default_mutation_does_not_affect_entities_with_override() {
        let mut app = entity_plugin_app();
        let entity = app
            .world_mut()
            .spawn(TestOverride(Some(Unit::Millimeters)))
            .id();
        app.update();

        app.world_mut()
            .resource_mut::<CascadeDefaults>()
            .layout_unit = Unit::Inches;
        app.update();

        assert_eq!(read_resolved(&app, entity), TestUnit(Unit::Millimeters));
    }

    #[test]
    fn unrelated_cascade_defaults_mutation_does_not_fire_propagation() {
        // The Local<Option<A>> sentinel in propagate_global_default_to_entity
        // should short-circuit when the projected field (layout_unit) hasn't
        // transitioned, even when some other CascadeDefaults field changed.
        let mut app = entity_plugin_app();
        let entity = app.world_mut().spawn(TestOverride(None)).id();
        app.update();

        let before_tick = app
            .world()
            .get_resource_ref::<CascadeDefaults>()
            .expect("CascadeDefaults should exist")
            .last_changed();

        // Mutate a different field — layout_unit stays at Meters.
        app.world_mut().resource_mut::<CascadeDefaults>().text_alpha = AlphaMode::Opaque;
        app.update();

        // Resolved<TestUnit> value is still the default.
        assert_eq!(read_resolved(&app, entity), TestUnit(Unit::Meters));
        // Sanity-check: the mutation did register on the resource, so the
        // "no propagation" above is attributable to the sentinel, not to a
        // silently-failing mutation.
        let after_tick = app
            .world()
            .get_resource_ref::<CascadeDefaults>()
            .expect("CascadeDefaults should exist")
            .last_changed();
        assert!(after_tick.get() > before_tick.get());
    }

    // Second 2-tier attribute, registered under `CascadePanelPlugin`, to
    // confirm both plugin types wire identical machinery.

    #[derive(Clone, Copy, Debug, PartialEq, Reflect)]
    struct TestPanelUnit(Unit);

    #[derive(Component, Clone, Copy, Debug, Reflect)]
    struct TestPanelUnitOverride(Option<Unit>);

    impl CascadeTarget for TestPanelUnit {
        type Override = TestPanelUnitOverride;

        fn override_value(c: &TestPanelUnitOverride) -> Option<Self> { c.0.map(Self) }
        fn global_default(d: &CascadeDefaults) -> Self { Self(d.panel_font_unit) }
    }

    #[test]
    fn cascade_panel_plugin_uses_shared_machinery() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<CascadeDefaults>()
            .add_plugins(CascadePanelPlugin::<TestPanelUnit>::default());

        let entity = app.world_mut().spawn(TestPanelUnitOverride(None)).id();
        app.update();
        let resolved = app
            .world()
            .get::<Resolved<TestPanelUnit>>(entity)
            .expect("Resolved<TestPanelUnit> should be present");
        assert_eq!(resolved.0, TestPanelUnit(Unit::Points));

        app.world_mut()
            .resource_mut::<CascadeDefaults>()
            .panel_font_unit = Unit::Millimeters;
        app.update();
        let resolved = app
            .world()
            .get::<Resolved<TestPanelUnit>>(entity)
            .expect("Resolved<TestPanelUnit> should be present");
        assert_eq!(resolved.0, TestPanelUnit(Unit::Millimeters));
    }
}
