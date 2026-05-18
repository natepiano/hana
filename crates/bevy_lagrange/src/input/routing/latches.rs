//! Per-source ownership latches that keep camera input flowing across frames.
//!
//! Types:
//! - [`OrbitCamInputOwnerLatch`] — handle naming a single owning camera entity.
//! - [`CameraInputSourceLatches`] — resource holding the active mouse and keyboard owners. While a
//!   latch is set, that source routes to the latched camera even when the cursor leaves its
//!   viewport.
//!
//! Also contains the [`clear_latches_on_mode_replaced`] observer that drops a camera's
//! latches when its input mode is replaced.

use bevy::prelude::*;

use crate::input::CameraInteractionSources;
use crate::input::OrbitCamInputModeReplaced;

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

pub(super) fn clear_latches_on_mode_replaced(
    replaced: On<OrbitCamInputModeReplaced>,
    mut latches: ResMut<CameraInputSourceLatches>,
) {
    latches.clear_camera(replaced.camera);
}
