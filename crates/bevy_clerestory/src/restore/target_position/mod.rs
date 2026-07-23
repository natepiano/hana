//! Restore target planning, state transitions, and restore application.

mod application;
mod monitor;
mod run_conditions;
mod strategy;
mod target;

#[cfg(test)]
pub(crate) use application::InjectedWinitWindows;
pub(crate) use application::ObservedScaleInputs;
pub(crate) use application::capture_scale_inputs;
pub(crate) use application::restore_windows;
pub(crate) use monitor::MonitorResolutionSource;
pub(crate) use monitor::resolve_target_monitor_and_position;
pub(crate) use run_conditions::has_restoring_windows;
pub(crate) use strategy::FullscreenRestoreState;
pub(crate) use strategy::MonitorScaleStrategy;
pub(crate) use strategy::NativeFullscreenState;
pub(crate) use strategy::WindowRestoreState;
pub(crate) use target::PreparedWindowPosition;
pub(crate) use target::RestoreDiagnostics;
pub(crate) use target::TargetPosition;
pub(crate) use target::compute_target_position;
