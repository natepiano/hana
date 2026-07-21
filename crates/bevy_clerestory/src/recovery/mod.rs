//! One-shot window recovery registration and policy lifecycles.

mod application_controlled;
#[cfg(feature = "monitor-probe")]
mod monitor_probe;
mod registration;

use application_controlled::ApplicationControlledRecoveries;
use bevy::prelude::*;
use bevy::window::ExitSystems;
use bevy::window::close_when_requested;
use registration::FallbackAndReturnRecoveries;
use registration::RecoveryRegistrations;
pub use registration::WindowRecovery;
#[cfg(test)]
pub(crate) use registration::registration_snapshot;

use crate::ClerestoryUpdateSet;
pub use crate::events::CancelWindowRecovery;
pub use crate::events::RestoreWindow;
pub use crate::events::WindowRecoveryAvailable;
pub use crate::events::WindowRecoveryPending;

pub(crate) struct RecoveryPlugin;

impl Plugin for RecoveryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RecoveryRegistrations>()
            .init_resource::<FallbackAndReturnRecoveries>()
            .init_resource::<ApplicationControlledRecoveries>();
        let existing_recoveries: Vec<_> = {
            let mut query = app.world_mut().query::<(Entity, &WindowRecovery)>();
            query
                .iter(app.world())
                .filter_map(|(entity, recovery)| {
                    (*recovery != WindowRecovery::Disabled).then_some((entity, *recovery))
                })
                .collect()
        };
        for (entity, recovery) in existing_recoveries {
            app.world_mut()
                .resource_mut::<RecoveryRegistrations>()
                .begin(entity, recovery);
        }
        app.add_observer(registration::on_window_recovery_added)
            .add_observer(registration::on_window_removed)
            .add_observer(registration::on_cancel_window_recovery)
            .add_observer(application_controlled::on_restore_window)
            .add_observer(application_controlled::on_window_restored)
            .add_observer(application_controlled::on_window_restore_mismatch)
            .add_systems(
                Update,
                application_controlled::evaluate_topology
                    .in_set(ClerestoryUpdateSet::RecoveryTopology),
            )
            .add_systems(
                Update,
                registration::accept_eligible_registrations
                    .in_set(ClerestoryUpdateSet::RecoveryWindow),
            )
            .add_systems(
                Last,
                (
                    registration::record_os_close_intent.after(close_when_requested),
                    application_controlled::emit_topology_notifications,
                )
                    .chain()
                    .before(ExitSystems),
            );
    }
}
