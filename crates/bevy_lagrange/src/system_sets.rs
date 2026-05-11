use bevy::input::InputSystems;
use bevy::prelude::*;
use bevy_enhanced_input::prelude::EnhancedInputSystems;

/// Public schedule phases for camera input processing.
///
/// App-authored manual camera input writers should run in
/// `OrbitCamInputPhase::WriteManual`.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub enum OrbitCamInputPhase {
    /// Library-owned preparation before enhanced-input context evaluation.
    PreInput,
    /// App-authored manual camera intent writers.
    WriteManual,
    /// Library-owned finalization before the camera controller reads input.
    Finalize,
}

pub(crate) struct LagrangeSystemSetsPlugin;

impl Plugin for LagrangeSystemSetsPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            PreUpdate,
            (
                OrbitCamInputPhase::PreInput
                    .after(InputSystems)
                    .before(EnhancedInputSystems::Update),
                OrbitCamInputPhase::WriteManual.after(EnhancedInputSystems::Apply),
                OrbitCamInputPhase::Finalize.after(OrbitCamInputPhase::WriteManual),
            ),
        );
    }
}
