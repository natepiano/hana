//! Per-source ownership latches that keep camera input flowing across frames.
//!
//! Types:
//! - [`OrbitCamInputOwnerLatch`] — handle naming a single owning camera entity.
//! - [`CameraInputSourceLatches`] — resource holding the active mouse and keyboard owners. While a
//!   latch is set, that source routes to the latched camera even when the cursor leaves its
//!   viewport.
//! - [`OrbitCamSlowModeLatches`] — resource storing per-camera slow-mode state.
//!
//! Also contains the [`clear_latches_on_mode_replaced`] observer that drops a camera's
//! latches when its input mode is replaced.

use std::collections::HashSet;

use bevy::prelude::*;

use crate::input::CameraInteractionSources;
use crate::input::OrbitCamInputModeReplaced;
use crate::input::OrbitCamResolvedBindings;
use crate::input::control_summary;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OrbitCamInputOwnerLatch(Entity);

impl OrbitCamInputOwnerLatch {
    pub(super) const fn camera(self) -> Entity { self.0 }
}

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CameraInputSourceLatches {
    mouse:    Option<OrbitCamInputOwnerLatch>,
    keyboard: Option<OrbitCamInputOwnerLatch>,
}

impl CameraInputSourceLatches {
    pub const fn acquire_sources(&mut self, camera: Entity, sources: CameraInteractionSources) {
        if sources.contains(CameraInteractionSources::MOUSE)
            || sources.contains(CameraInteractionSources::WHEEL)
            || sources.contains(CameraInteractionSources::SMOOTH_SCROLL)
        {
            self.mouse = Some(OrbitCamInputOwnerLatch(camera));
        }
        if sources.contains(CameraInteractionSources::KEYBOARD) {
            self.keyboard = Some(OrbitCamInputOwnerLatch(camera));
        }
    }

    pub fn release_sources(&mut self, camera: Entity, sources: CameraInteractionSources) {
        if (sources.contains(CameraInteractionSources::MOUSE)
            || sources.contains(CameraInteractionSources::WHEEL)
            || sources.contains(CameraInteractionSources::SMOOTH_SCROLL))
            && self.mouse.is_some_and(|latch| latch.camera() == camera)
        {
            self.mouse = None;
        }
        if sources.contains(CameraInteractionSources::KEYBOARD)
            && self.keyboard.is_some_and(|latch| latch.camera() == camera)
        {
            self.keyboard = None;
        }
    }

    fn clear_camera(&mut self, camera: Entity) {
        if self.mouse.is_some_and(|latch| latch.camera() == camera) {
            self.mouse = None;
        }
        if self.keyboard.is_some_and(|latch| latch.camera() == camera) {
            self.keyboard = None;
        }
    }

    pub(super) fn recover_unavailable_latches(&mut self, available_cameras: &[Entity]) {
        let is_available = |camera| available_cameras.contains(&camera);
        if self
            .mouse
            .is_some_and(|latch| !is_available(latch.camera()))
        {
            debug!("cleared stale mouse OrbitCam input latch");
            self.mouse = None;
        }
        if self
            .keyboard
            .is_some_and(|latch| !is_available(latch.camera()))
        {
            debug!("cleared stale keyboard OrbitCam input latch");
            self.keyboard = None;
        }
    }

    pub(super) const fn mouse_latch(&self) -> Option<OrbitCamInputOwnerLatch> { self.mouse }

    pub(super) const fn keyboard_latch(&self) -> Option<OrbitCamInputOwnerLatch> { self.keyboard }
}

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct OrbitCamSlowModeLatches {
    active_cameras: HashSet<Entity>,
}

/// Read-only state describing whether an `OrbitCam`'s toggled slow mode is
/// active.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
pub struct OrbitCamSlowModeState {
    active: bool,
}

impl OrbitCamSlowModeState {
    /// Returns `true` while the camera's toggled slow mode is active.
    #[must_use]
    pub const fn is_active(self) -> bool { self.active }

    /// Creates a component state from an active flag.
    #[must_use]
    pub const fn from_active(active: bool) -> Self { Self { active } }

    /// Updates whether the camera's toggled slow mode is active.
    pub const fn set_active(&mut self, active: bool) { self.active = active; }
}

impl OrbitCamSlowModeLatches {
    pub(super) fn is_active(&self, camera: Entity) -> bool { self.active_cameras.contains(&camera) }

    pub(super) fn toggle(&mut self, camera: Entity) {
        if !self.active_cameras.insert(camera) {
            self.active_cameras.remove(&camera);
        }
    }

    fn clear_camera(&mut self, camera: Entity) { self.active_cameras.remove(&camera); }

    pub(super) fn recover_unavailable_latches(&mut self, available_cameras: &[Entity]) {
        self.active_cameras.retain(|camera| {
            let retain = available_cameras.contains(camera);
            if !retain {
                debug!("cleared stale OrbitCam slow-mode latch");
            }
            retain
        });
    }
}

pub(super) fn clear_latches_on_mode_replaced(
    replaced: On<OrbitCamInputModeReplaced>,
    mut latches: ResMut<CameraInputSourceLatches>,
    mut slow_latches: ResMut<OrbitCamSlowModeLatches>,
    bindings: Query<&OrbitCamResolvedBindings>,
) {
    latches.clear_camera(replaced.camera);
    let has_effective_slow_mode = bindings
        .get(replaced.camera)
        .is_ok_and(|bindings| control_summary::effective_slow_mode(&bindings.0).is_some());
    if !has_effective_slow_mode {
        slow_latches.clear_camera(replaced.camera);
    }
}
