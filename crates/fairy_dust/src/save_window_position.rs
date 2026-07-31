//! Capability: persist window position and size across runs via
//! `hana_clerestory::WindowManagerPlugin`.

use bevy::prelude::*;
use hana_clerestory::WindowManagerPlugin;

use crate::ensure_plugin;

pub(crate) fn install(app: &mut App) { ensure_plugin(app, WindowManagerPlugin); }
