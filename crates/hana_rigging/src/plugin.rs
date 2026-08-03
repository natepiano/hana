use bevy::app::App;
use bevy::app::Plugin;
use bevy::app::Update;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::SystemSet;
use bevy::prelude::World;
use bevy::time::Real;
use bevy::time::Time;

use crate::Attempts;
use crate::BindingEntities;
use crate::Bindings;
use crate::Devices;
use crate::DiscoveryControl;
use crate::DiscoveryLimits;
use crate::DiscoveryStatus;
use crate::HardwareInventory;
use crate::RegisteredSchemes;
use crate::RiggingLimits;
use crate::RiggingRevision;
use crate::binding::BindingTransitionBatch;
use crate::binding::drain_binding_transitions;
use crate::binding::project_binding_entities;
use crate::devices::ReconciledDeviceChanges;
use crate::reconcile::project_device_entities;
use crate::reconcile::reconcile;
use crate::registration::Drivers;
use crate::registration::Reporters;

/// Installs the reporter collection system and the ordering sets that integration crates use.
pub struct RiggingPlugin;

/// Ordered kernel phases in the `Update` schedule.
///
/// This enum is non-exhaustive because later kernel phases can add an ordering boundary without
/// making downstream `RiggingSystems` matches incomplete.
#[derive(Clone, Debug, Hash, PartialEq, Eq, SystemSet)]
#[non_exhaustive]
pub enum RiggingSystems {
    /// Discovery cadence, task admission, and bounded completed reports run before reconciliation.
    Collect,
    /// Reconciliation follows the current collect pass and consumes each independently accepted
    /// reporter change; it does not wait for a global reporter completion barrier.
    Reconcile,
    /// Consumer systems prepare their requested configurations after authorization but before any
    /// endpoint driver begins an apply.
    Prepare,
    /// Kernel attempt systems later start, poll, retry, and escalate endpoint applies.
    Apply,
}

impl Plugin for RiggingPlugin {
    fn build(&self, app: &mut App) {
        // `reconcile` reads `Time<Real>` for the freshness lease, and a missing resource makes
        // Bevy skip the system without reporting anything. The resource is initialized here rather
        // than by adding `TimePlugin`, because `DefaultPlugins` and `MinimalPlugins` both panic on
        // a `TimePlugin` that is already installed, which would make plugin order decide whether an
        // application starts.
        app.init_resource::<Time<Real>>()
            .init_resource::<Attempts>()
            .init_resource::<Drivers>()
            .init_resource::<BindingEntities>()
            .init_resource::<BindingTransitionBatch>()
            .init_resource::<Bindings>()
            .init_resource::<Devices>()
            .init_resource::<DiscoveryControl>()
            .init_resource::<DiscoveryLimits>()
            .init_resource::<DiscoveryStatus>()
            .init_resource::<HardwareInventory>()
            .init_resource::<ReconciledDeviceChanges>()
            .init_resource::<RegisteredSchemes>()
            .init_resource::<Reporters>()
            .init_resource::<RiggingLimits>()
            .init_resource::<RiggingRevision>()
            .configure_sets(
                Update,
                (
                    RiggingSystems::Collect,
                    RiggingSystems::Reconcile,
                    RiggingSystems::Prepare,
                    RiggingSystems::Apply,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    collect.in_set(RiggingSystems::Collect),
                    (drain_binding_transitions, project_binding_entities)
                        .chain()
                        .before(reconcile)
                        .in_set(RiggingSystems::Reconcile),
                    reconcile.in_set(RiggingSystems::Reconcile),
                    project_device_entities
                        .after(reconcile)
                        .in_set(RiggingSystems::Reconcile),
                ),
            );
    }
}

fn collect(world: &mut World) {
    world.resource_scope::<Reporters, _>(|world, mut reporters| {
        let discovery_limits = world.resource::<DiscoveryLimits>().clone();
        world.resource_scope::<DiscoveryControl, _>(|world, mut discovery_control| {
            world.resource_scope::<DiscoveryStatus, _>(|world, mut discovery_status| {
                reporters.collect(
                    world,
                    &mut discovery_control,
                    &discovery_limits,
                    &mut discovery_status,
                );
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use bevy::MinimalPlugins;
    use bevy::app::App;
    use bevy::app::Update;
    use bevy::ecs::schedule::IntoScheduleConfigs;
    use bevy::prelude::ResMut;
    use bevy::prelude::Resource;
    use bevy::time::Real;
    use bevy::time::Time;

    use super::RiggingPlugin;
    use super::RiggingSystems;

    #[derive(Default, Resource)]
    struct StageLog(Vec<&'static str>);

    fn reconcile(mut stage_log: ResMut<StageLog>) { stage_log.0.push("reconcile"); }

    fn prepare(mut stage_log: ResMut<StageLog>) {
        assert_eq!(stage_log.0.as_slice(), ["reconcile"]);
        stage_log.0.push("prepare");
    }

    fn apply(mut stage_log: ResMut<StageLog>) {
        assert_eq!(stage_log.0.as_slice(), ["reconcile", "prepare"]);
        stage_log.0.push("apply");
    }

    #[test]
    fn prepare_runs_after_reconcile_and_before_apply() {
        let mut app = App::new();
        app.add_plugins(RiggingPlugin)
            .init_resource::<StageLog>()
            .add_systems(
                Update,
                (
                    reconcile.in_set(RiggingSystems::Reconcile),
                    prepare.in_set(RiggingSystems::Prepare),
                    apply.in_set(RiggingSystems::Apply),
                ),
            );

        app.update();

        assert_eq!(
            app.world().resource::<StageLog>().0.as_slice(),
            ["reconcile", "prepare", "apply"]
        );
    }

    #[test]
    fn an_application_may_add_the_plugin_before_its_default_plugins() {
        let mut app = App::new();
        app.add_plugins(RiggingPlugin).add_plugins(MinimalPlugins);

        app.update();

        // `MinimalPlugins` brings `TimePlugin`, which panics if the kernel had claimed it first.
        // The lease still has the resource it reads either way.
        assert!(app.world().get_resource::<Time<Real>>().is_some());
    }

    #[test]
    fn a_bare_app_still_gets_the_clock_the_freshness_lease_reads() {
        let mut app = App::new();
        app.add_plugins(RiggingPlugin);

        app.update();

        assert!(app.world().get_resource::<Time<Real>>().is_some());
    }
}
