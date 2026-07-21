//! One-shot window recovery registration and policy lifecycles.

mod application_controlled;
mod fallback_and_return;
#[cfg(feature = "monitor-probe")]
mod monitor_probe;
mod registration;

pub(crate) use application_controlled::ApplicationControlledRecoveries;
pub(crate) use application_controlled::ExplicitRestoreRequests;
use bevy::prelude::*;
use bevy::window::ExitSystems;
use bevy::window::close_when_requested;
pub(crate) use fallback_and_return::AutomaticRestoreIntent;
pub(crate) use fallback_and_return::AutomaticRestoreIntents;
#[cfg(test)]
pub(crate) use fallback_and_return::FallbackAndReturnPhaseSnapshot;
pub(crate) use fallback_and_return::FallbackAndReturnRecoveries;
pub(crate) use fallback_and_return::advance_fallback_windows;
#[cfg(test)]
pub(crate) use fallback_and_return::fallback_and_return_snapshot;
pub(crate) use registration::CanonicalWindowRole;
pub(crate) use registration::PrimaryPresence;
pub(crate) use registration::RecoveryGeneration;
pub(crate) use registration::RecoveryRegistrations;
pub use registration::WindowRecovery;
pub(crate) use registration::accept_eligible_registrations;
pub(crate) use registration::canonical_window;
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
            .init_resource::<AutomaticRestoreIntents>()
            .init_resource::<ApplicationControlledRecoveries>()
            .init_resource::<ExplicitRestoreRequests>();
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
            .add_systems(
                Update,
                (
                    application_controlled::evaluate_topology,
                    fallback_and_return::evaluate_topology,
                    ApplyDeferred,
                )
                    .chain()
                    .in_set(ClerestoryUpdateSet::RecoveryTopology),
            )
            .add_systems(
                Update,
                (
                    registration::accept_eligible_registrations,
                    fallback_and_return::advance_fallback_windows,
                    ApplyDeferred,
                )
                    .chain()
                    .in_set(ClerestoryUpdateSet::RecoveryWindow),
            )
            .add_systems(
                Last,
                (
                    registration::record_os_close_intent.after(close_when_requested),
                    application_controlled::emit_topology_notifications,
                    fallback_and_return::emit_pending_notifications,
                )
                    .chain()
                    .before(ExitSystems),
            );
    }
}
