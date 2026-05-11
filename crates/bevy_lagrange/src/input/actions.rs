use bevy::prelude::*;
use bevy_enhanced_input::prelude::InputAction;

mod sealed {
    pub trait Sealed {}
}

/// Marker trait for semantic Lagrange camera actions.
///
/// This trait is sealed; downstream crates cannot implement it for their own
/// action types.
pub trait CameraSemanticAction: InputAction + sealed::Sealed {}

/// Marker trait for held Lagrange camera actions.
pub trait HeldCameraAction: CameraSemanticAction {}

/// Marker trait for impulse Lagrange camera actions.
pub trait ImpulseCameraAction: CameraSemanticAction {}

/// Enhanced-input action for orbit intent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, InputAction, Reflect)]
#[action_output(Vec2)]
pub struct OrbitCamOrbitAction;

/// Enhanced-input action for pan intent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, InputAction, Reflect)]
#[action_output(Vec2)]
pub struct OrbitCamPanAction;

/// Enhanced-input action for coarse zoom intent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, InputAction, Reflect)]
#[action_output(f32)]
pub struct OrbitCamZoomCoarseAction;

/// Enhanced-input action for smooth zoom intent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, InputAction, Reflect)]
#[action_output(f32)]
pub struct OrbitCamZoomSmoothAction;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, InputAction)]
#[action_output(bool)]
pub(super) struct OrbitCamOrbitEngagedAction;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, InputAction)]
#[action_output(bool)]
pub(super) struct OrbitCamPanEngagedAction;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, InputAction)]
#[action_output(bool)]
pub(super) struct OrbitCamZoomEngagedAction;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, InputAction)]
#[action_output(Vec2)]
pub(super) struct OrbitCamAdapterOrbitAction;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, InputAction)]
#[action_output(Vec2)]
pub(super) struct OrbitCamAdapterPanAction;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, InputAction)]
#[action_output(f32)]
pub(super) struct OrbitCamAdapterZoomCoarseAction;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, InputAction)]
#[action_output(f32)]
pub(super) struct OrbitCamAdapterZoomSmoothAction;

macro_rules! impl_camera_action {
    ($action:ty) => {
        impl sealed::Sealed for $action {}
        impl CameraSemanticAction for $action {}
    };
}

impl_camera_action!(OrbitCamOrbitAction);
impl_camera_action!(OrbitCamPanAction);
impl_camera_action!(OrbitCamZoomCoarseAction);
impl_camera_action!(OrbitCamZoomSmoothAction);
impl_camera_action!(OrbitCamOrbitEngagedAction);
impl_camera_action!(OrbitCamPanEngagedAction);
impl_camera_action!(OrbitCamZoomEngagedAction);
impl_camera_action!(OrbitCamAdapterOrbitAction);
impl_camera_action!(OrbitCamAdapterPanAction);
impl_camera_action!(OrbitCamAdapterZoomCoarseAction);
impl_camera_action!(OrbitCamAdapterZoomSmoothAction);

impl HeldCameraAction for OrbitCamOrbitAction {}
impl HeldCameraAction for OrbitCamPanAction {}
impl HeldCameraAction for OrbitCamZoomSmoothAction {}

impl ImpulseCameraAction for OrbitCamZoomCoarseAction {}
