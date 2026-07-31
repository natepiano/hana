//! Compile-time identity for lagrange camera families.

use bevy::prelude::*;

mod sealed {
    pub trait Sealed {}
}

use sealed::Sealed;

/// Type-family key for one complete lagrange camera kind.
///
/// Implementors are zero-sized marker types such as [`OrbitCamKind`] and
/// [`FreeCamKind`]. The trait is the compile-time checklist for behavior every
/// camera kind must supply: controller registration, `PlayAnimation` /
/// `CameraMoveList` support, `AnimateToFit` support, `ZoomToFit` support, and
/// `LookAt` / `LookAtAndZoomToFit` support.
/// Generic input code extends the same kind key with input-specific associated
/// types.
/// This trait is sealed; implementers are the crate-defined [`OrbitCamKind`]
/// and [`FreeCamKind`]. Camera kinds are defined by this crate.
///
/// [`FreeCamKind`]: crate::FreeCamKind
/// [`OrbitCamKind`]: crate::OrbitCamKind
pub trait CameraKind: Copy + Send + Sync + Sealed + 'static {
    /// The camera component for this camera family.
    type Camera: Component;

    /// Registers every required system for this camera kind.
    ///
    /// Camera plugins should call this default method instead of manually
    /// sequencing the individual registration methods, so the compile-time
    /// checklist stays centralized as new required camera behavior is added.
    fn add_camera_kind_systems(app: &mut App) {
        Self::add_controller_systems(app);
        Self::add_animation_systems(app);
        Self::add_animate_to_fit_systems(app);
        Self::add_zoom_to_fit_systems(app);
        Self::add_look_at_systems(app);
        Self::add_camera_kind_support_systems(app);
    }

    /// Registers the camera's controller and required per-kind runtime systems.
    fn add_controller_systems(app: &mut App);

    /// Registers this camera kind's `CameraMoveList` / `PlayAnimation`
    /// application path.
    fn add_animation_systems(app: &mut App);

    /// Registers this camera kind's `AnimateToFit` application path.
    fn add_animate_to_fit_systems(app: &mut App);

    /// Registers this camera kind's `ZoomToFit` application path.
    fn add_zoom_to_fit_systems(app: &mut App);

    /// Registers this camera kind's `LookAt` / `LookAtAndZoomToFit`
    /// application path.
    fn add_look_at_systems(app: &mut App);

    /// Registers optional shared support systems used by this camera kind.
    ///
    /// This hook exists for systems shared by several required behaviors on the
    /// same kind. It deliberately defaults to no-op; the behavior-specific
    /// methods above remain mandatory.
    fn add_camera_kind_support_systems(_: &mut App) {}
}

impl Sealed for crate::OrbitCamKind {}
impl Sealed for crate::FreeCamKind {}
