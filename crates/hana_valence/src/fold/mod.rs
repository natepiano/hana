//! Authored fold sequences and Hana-owned playback state.

mod author;
mod hinge;
mod playback;
mod sequence;

pub use author::FoldAuthorError;
pub use author::FoldFromArrangement;
pub use author::FoldSequenceBuilder;
pub use author::FoldSnapshotDiagnostic;
pub use author::FoldSnapshotDiagnostics;
pub use author::FoldSnapshotInvalidReason;
use bevy_app::App;
use bevy_app::Plugin;
use bevy_app::PostUpdate;
use bevy_ecs::schedule::ApplyDeferred;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::schedule::SystemSet;
pub use hinge::FoldAngleDiagnostic;
pub use hinge::FoldAngleDiagnostics;
pub use hinge::FoldAngleInvalidReason;
pub use hinge::FoldAngles;
pub use hinge::actuate_fold_hinges;
pub use playback::FoldCommand;
pub use playback::FoldCommandEvent;
pub use playback::FoldDirection;
pub use playback::FoldMotion;
pub use sequence::FoldDiagnostic;
pub use sequence::FoldDiagnostics;
pub use sequence::FoldEndpoint;
pub use sequence::FoldInvalidReason;
pub use sequence::FoldMember;
pub use sequence::FoldMembers;
pub use sequence::FoldSequence;
pub use sequence::FoldSequenceState;
pub use sequence::FoldStage;

use crate::AnchorSystems;

/// Installs authored fold-sequence validation and playback state.
///
/// `FoldPlugin` does not install anchor geometry providers, arrangement
/// drivers, hinge-to-pose conversion, anchor resolution, or transform
/// propagation. Consumers continue to own those systems.
pub struct FoldPlugin;

impl Plugin for FoldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FoldAngleDiagnostics>()
            .init_resource::<FoldDiagnostics>()
            .init_resource::<FoldSnapshotDiagnostics>()
            .add_observer(author::on_fold_from_arrangement_inserted)
            .add_observer(playback::on_fold_command)
            .add_observer(sequence::on_fold_member_inserted)
            .add_observer(sequence::on_fold_member_discarded)
            .add_observer(sequence::on_fold_sequence_inserted)
            .add_observer(sequence::on_fold_sequence_removed)
            .configure_sets(
                PostUpdate,
                (
                    (FoldSystems::Advance, FoldSystems::Actuate)
                        .chain()
                        .in_set(AnchorSystems::AnimatePose),
                    FoldSystems::Actuate.before(crate::hinge_to_pose),
                ),
            )
            .add_systems(
                PostUpdate,
                (
                    (
                        author::snapshot_fold_arrangements,
                        ApplyDeferred,
                        sequence::validate_fold_sequences,
                    )
                        .chain()
                        .in_set(AnchorSystems::AnimatePose)
                        .before(FoldSystems::Advance),
                    playback::advance_fold_sequences.in_set(FoldSystems::Advance),
                    hinge::actuate_fold_hinges.in_set(FoldSystems::Actuate),
                ),
            );
    }
}

/// Ordered system sets for fold playback and physical actuation.
#[derive(SystemSet, Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum FoldSystems {
    /// Advances continuous fold playback after sequence validation.
    Advance,
    /// Applies the current fold position to authored physical adapters.
    Actuate,
}
