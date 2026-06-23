//! [`CascadePlugin`] — the one parent-walking cascade plugin, generic over a
//! [`CascadeAttr`]. Registers the attribute's reflection types and runs the
//! per-frame propagation pass that keeps every `Resolved<A>` current.
//!
//! Spawn-time seeding is **not** done here: only the node-kind authoring
//! observers (the `TextStyle` / `DiegeticPanel` bridges and the panel-child
//! alpha seed) know which entities participate in a cascade and which
//! `Resolved<A>` each one needs. They seed via [`resolve_walk`]; this plugin
//! owns reflection registration and runtime propagation.

use std::marker::PhantomData;

use bevy::platform::collections::HashSet;
use bevy::prelude::*;

use super::cascade_set::CascadeSet;
use super::defaults::CascadeDefault;
use super::resolved;
use super::resolved::CascadeAttr;
use super::resolved::Override;
use super::resolved::Resolved;

/// Plugin that wires the cascade for one attribute `A`: registers `A`,
/// `Override<A>`, and `Resolved<A>` for reflection, and adds the propagation
/// pass in [`CascadeSet::Propagate`].
pub(crate) struct CascadePlugin<A: CascadeAttr>(PhantomData<A>);

impl<A: CascadeAttr> Default for CascadePlugin<A> {
    fn default() -> Self { Self(PhantomData) }
}

impl<A: CascadeAttr> Plugin for CascadePlugin<A>
where
    CascadeDefault<A>: Default,
{
    fn build(&self, app: &mut App) {
        app.register_type::<A>()
            .register_type::<Override<A>>()
            .register_type::<Resolved<A>>()
            .register_type::<CascadeDefault<A>>()
            .init_resource::<CascadeDefault<A>>()
            .add_systems(Update, propagate_cascade::<A>.in_set(CascadeSet::Propagate));
    }
}

/// Re-resolve every node whose cached `Resolved<A>` may have gone stale.
///
/// Runs **every frame** (no run condition) so a frame's
/// [`RemovedComponents<Override<A>>`] is always drained before it is cleared.
/// The dirty set is the whole participant set when the attribute's global
/// default changed (sentinel-gated), otherwise the subtrees rooted at nodes
/// whose own `Override<A>` changed or was removed, or whose `ChildOf` changed.
/// Each dirty node that carries `Resolved<A>` is re-walked; non-participants
/// (glyph-mesh children and the like) are skipped. Writes are
/// inequality-guarded, so downstream `Changed<Resolved<A>>` readers wake only
/// on real transitions.
fn propagate_cascade<A: CascadeAttr>(
    default: Res<CascadeDefault<A>>,
    overrides: Query<&Override<A>>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    resolved: Query<(Entity, &Resolved<A>)>,
    changed_overrides: Query<Entity, Changed<Override<A>>>,
    changed_parents: Query<Entity, Changed<ChildOf>>,
    mut removed_overrides: RemovedComponents<Override<A>>,
    mut commands: Commands,
) {
    // Drain removals every frame, even when the default-change path supersedes
    // them, so they are never cleared unread.
    let removed: Vec<Entity> = removed_overrides.read().collect();

    let mut dirty: HashSet<Entity> = HashSet::new();
    if default.is_changed() {
        for (entity, _) in &resolved {
            dirty.insert(entity);
        }
    } else {
        for root in changed_overrides
            .iter()
            .chain(changed_parents.iter())
            .chain(removed.iter().copied())
        {
            collect_subtree(root, &children, &mut dirty);
        }
    }

    for entity in dirty {
        let Ok((_, current)) = resolved.get(entity) else {
            continue;
        };
        let new = resolved::resolve_walk::<A>(entity, &overrides, &parents, default.0.clone());
        if current.0 != new {
            commands.entity(entity).insert(Resolved(new));
        }
    }
}

/// Collect `root` and every descendant into `dirty`.
///
/// Iterative DFS with `dirty` doubling as the visited set, so a `ChildOf`
/// cycle (which makes a `Children` list self-referential) terminates instead
/// of looping forever.
fn collect_subtree(root: Entity, children: &Query<&Children>, dirty: &mut HashSet<Entity>) {
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if !dirty.insert(entity) {
            continue;
        }
        if let Ok(child_list) = children.get(entity) {
            stack.extend(child_list.iter());
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
    use crate::cascade::constants::CASCADE_DEPTH_CAP;
    use crate::cascade::defaults::CascadeDefaults;
    use crate::cascade::resolved::TestUnit;
    use crate::layout::Unit;

    /// Marker for a cascade-participating test node.
    #[derive(Component)]
    struct TestNode;

    /// Stand-in for a real node-kind authoring bridge: seeds `Resolved<TestUnit>`
    /// at spawn by walking from the just-added node.
    fn seed_test_node(
        trigger: On<Add, TestNode>,
        overrides: Query<&Override<TestUnit>>,
        parents: Query<&ChildOf>,
        default: Res<CascadeDefault<TestUnit>>,
        mut commands: Commands,
    ) {
        let entity = trigger.event_target();
        let resolved = resolved::resolve_walk::<TestUnit>(entity, &overrides, &parents, default.0);
        commands.entity(entity).insert(Resolved(resolved));
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<CascadeDefaults>()
            .add_plugins(CascadePlugin::<TestUnit>::default())
            .add_observer(seed_test_node);
        app
    }

    fn read(app: &App, entity: Entity) -> Unit {
        app.world()
            .get::<Resolved<TestUnit>>(entity)
            .expect("Resolved<TestUnit> should be present")
            .0
            .0
    }

    #[test]
    fn root_with_override_resolves_to_override() {
        let mut app = test_app();
        let entity = app
            .world_mut()
            .spawn((TestNode, Override(TestUnit(Unit::Millimeters))))
            .id();
        app.update();
        assert_eq!(read(&app, entity), Unit::Millimeters);
    }

    #[test]
    fn root_without_override_resolves_to_global() {
        let mut app = test_app();
        let entity = app.world_mut().spawn(TestNode).id();
        app.update();
        assert_eq!(read(&app, entity), Unit::Meters);
    }

    #[test]
    fn child_inherits_parent_override() {
        let mut app = test_app();
        let parent = app
            .world_mut()
            .spawn((TestNode, Override(TestUnit(Unit::Inches))))
            .id();
        let child = app.world_mut().spawn((TestNode, ChildOf(parent))).id();
        app.update();
        assert_eq!(read(&app, child), Unit::Inches);
    }

    #[test]
    fn world_resolve_helper_inherits_parent_override() {
        let mut app = test_app();
        let parent = app
            .world_mut()
            .spawn((TestNode, Override(TestUnit(Unit::Inches))))
            .id();
        let child = app.world_mut().spawn((TestNode, ChildOf(parent))).id();
        app.update();

        assert_eq!(
            resolved::resolve::<TestUnit>(app.world(), child, TestUnit(Unit::Meters)).0,
            Unit::Inches
        );
    }

    #[test]
    fn child_override_wins_over_parent() {
        let mut app = test_app();
        let parent = app
            .world_mut()
            .spawn((TestNode, Override(TestUnit(Unit::Inches))))
            .id();
        let child = app
            .world_mut()
            .spawn((
                TestNode,
                Override(TestUnit(Unit::Millimeters)),
                ChildOf(parent),
            ))
            .id();
        app.update();
        assert_eq!(read(&app, child), Unit::Millimeters);
    }

    #[test]
    fn parent_override_mutation_propagates_to_child() {
        let mut app = test_app();
        let parent = app
            .world_mut()
            .spawn((TestNode, Override(TestUnit(Unit::Inches))))
            .id();
        let child = app.world_mut().spawn((TestNode, ChildOf(parent))).id();
        app.update();
        assert_eq!(read(&app, child), Unit::Inches);

        app.world_mut()
            .entity_mut(parent)
            .get_mut::<Override<TestUnit>>()
            .expect("parent override")
            .0 = TestUnit(Unit::Points);
        app.update();
        assert_eq!(read(&app, child), Unit::Points);
    }

    #[test]
    fn global_default_mutation_updates_node_without_override() {
        let mut app = test_app();
        let entity = app.world_mut().spawn(TestNode).id();
        app.update();
        assert_eq!(read(&app, entity), Unit::Meters);

        app.world_mut().resource_mut::<CascadeDefault<TestUnit>>().0 = TestUnit(Unit::Inches);
        app.update();
        assert_eq!(read(&app, entity), Unit::Inches);
    }

    #[test]
    fn global_default_mutation_skips_node_with_override() {
        let mut app = test_app();
        let entity = app
            .world_mut()
            .spawn((TestNode, Override(TestUnit(Unit::Millimeters))))
            .id();
        app.update();

        app.world_mut().resource_mut::<CascadeDefault<TestUnit>>().0 = TestUnit(Unit::Inches);
        app.update();
        assert_eq!(read(&app, entity), Unit::Millimeters);
    }

    #[test]
    fn override_removal_reinherits_from_parent() {
        let mut app = test_app();
        let parent = app
            .world_mut()
            .spawn((TestNode, Override(TestUnit(Unit::Inches))))
            .id();
        let child = app
            .world_mut()
            .spawn((
                TestNode,
                Override(TestUnit(Unit::Millimeters)),
                ChildOf(parent),
            ))
            .id();
        app.update();
        assert_eq!(read(&app, child), Unit::Millimeters);

        app.world_mut()
            .entity_mut(child)
            .remove::<Override<TestUnit>>();
        app.update();
        assert_eq!(read(&app, child), Unit::Inches);
    }

    #[test]
    fn reparent_reresolves_against_new_parent() {
        let mut app = test_app();
        let parent_a = app
            .world_mut()
            .spawn((TestNode, Override(TestUnit(Unit::Inches))))
            .id();
        let parent_b = app
            .world_mut()
            .spawn((TestNode, Override(TestUnit(Unit::Points))))
            .id();
        let child = app.world_mut().spawn((TestNode, ChildOf(parent_a))).id();
        app.update();
        assert_eq!(read(&app, child), Unit::Inches);

        app.world_mut().entity_mut(child).insert(ChildOf(parent_b));
        app.update();
        assert_eq!(read(&app, child), Unit::Points);
    }

    #[test]
    fn self_parent_terminates_at_global_default() {
        let mut app = test_app();
        let entity = app.world_mut().spawn(TestNode).id();
        app.update();

        app.world_mut().entity_mut(entity).insert(ChildOf(entity));
        app.update();
        assert_eq!(read(&app, entity), Unit::Meters);
    }

    #[test]
    fn two_node_childof_cycle_terminates_at_global_default() {
        let mut app = test_app();
        let a = app.world_mut().spawn(TestNode).id();
        let b = app.world_mut().spawn(TestNode).id();

        // Form a two-node `ChildOf` cycle: a → b → a. Neither node carries an
        // override, so the walk must traverse the whole cycle before giving up
        // — the visited-set (debug) / depth-cap (release) guard terminates it
        // at the global default instead of looping forever.
        app.world_mut().entity_mut(a).insert(ChildOf(b));
        app.world_mut().entity_mut(b).insert(ChildOf(a));
        app.update();

        assert_eq!(read(&app, a), Unit::Meters);
        assert_eq!(read(&app, b), Unit::Meters);
    }

    #[test]
    fn chain_beyond_depth_cap_falls_to_global_default() {
        let mut app = test_app();
        let root = app
            .world_mut()
            .spawn((TestNode, Override(TestUnit(Unit::Millimeters))))
            .id();

        // A `ChildOf` chain deeper than the cap. The root's override sits above
        // the cap from the deepest node's vantage.
        let mut parent = root;
        let mut nodes = vec![root];
        for _ in 0..CASCADE_DEPTH_CAP + 4 {
            let child = app.world_mut().spawn((TestNode, ChildOf(parent))).id();
            nodes.push(child);
            parent = child;
        }
        app.update();

        // A node within the cap reaches the root override.
        assert_eq!(read(&app, nodes[3]), Unit::Millimeters);
        // The deepest node exhausts the cap before reaching the root, so it
        // resolves to the global default — no hang, no panic.
        let deepest = *nodes.last().expect("chain is non-empty");
        assert_eq!(read(&app, deepest), Unit::Meters);
    }

    #[test]
    fn non_cascade_default_resource_does_not_fire_propagation() {
        let mut app = test_app();
        let entity = app.world_mut().spawn(TestNode).id();
        app.update();

        let before = app
            .world()
            .get_resource_ref::<CascadeDefaults>()
            .expect("CascadeDefaults should exist")
            .last_changed();

        // Mutate a separate non-cascade default resource.
        app.world_mut()
            .resource_mut::<CascadeDefaults>()
            .layout_unit = Unit::Inches;
        app.update();

        assert_eq!(read(&app, entity), Unit::Meters);
        let after = app
            .world()
            .get_resource_ref::<CascadeDefaults>()
            .expect("CascadeDefaults should exist")
            .last_changed();
        assert!(after.get() > before.get());
    }
}
