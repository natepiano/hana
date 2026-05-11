use bevy::prelude::*;

/// Enhanced-input context component installed on cameras controlled by `OrbitCam`.
#[derive(Component, Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component, Default)]
pub struct OrbitCamInputContext;
