use bevy::app::App;
use bevy::app::Plugin;
use bevy::app::Update;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::SystemSet;
use bevy::prelude::World;

use crate::RegisteredSchemes;
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
    /// Every registered reporter scans before the kernel examines a completed device set.
    Collect,
    /// Reconciliation later diffs reports, computes verdicts, mirrors components, and emits
    /// events after all reporters have run.
    Reconcile,
    /// Consumer systems prepare their requested configurations after authorization but before any
    /// endpoint driver begins an apply.
    Prepare,
    /// Kernel attempt systems later start, poll, retry, and escalate endpoint applies.
    Apply,
}

impl Plugin for RiggingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Drivers>()
            .init_resource::<RegisteredSchemes>()
            .init_resource::<Reporters>()
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
            .add_systems(Update, collect.in_set(RiggingSystems::Collect));
    }
}

fn collect(world: &mut World) {
    world.resource_scope::<Reporters, _>(|world, mut reporters| {
        drop(reporters.scan(world));
    });
}

#[cfg(test)]
mod tests {
    use bevy::app::App;
    use bevy::app::Update;
    use bevy::ecs::schedule::IntoScheduleConfigs;
    use bevy::prelude::ResMut;
    use bevy::prelude::Resource;

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
}
