//! Capability: persist window position and size across runs via
//! `bevy_window_manager::WindowManagerPlugin`.

use bevy::prelude::*;
use bevy_window_manager::WindowManagerPlugin;

use crate::ensure_plugin;

pub(crate) fn install(app: &mut App) { ensure_plugin(app, WindowManagerPlugin); }
