//! Capability: orbit-camera input via `bevy_lagrange::LagrangePlugin`.

use bevy::prelude::*;
use bevy_lagrange::LagrangePlugin;

use crate::ensure_plugin;

pub(crate) fn install(app: &mut App) { ensure_plugin(app, LagrangePlugin); }
