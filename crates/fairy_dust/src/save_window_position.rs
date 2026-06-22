//! Capability: persist window position and size across runs via
//! `bevy_clerestory::WindowManagerPlugin`.

use bevy::prelude::*;
use bevy_clerestory::WindowManagerPlugin;

use crate::ensure_plugin;

pub(crate) fn install(app: &mut App) { ensure_plugin(app, WindowManagerPlugin); }
