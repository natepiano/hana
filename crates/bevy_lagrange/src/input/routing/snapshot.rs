//! Per-frame snapshots of windows and cameras used by the routing system.
//!
//! Types (all submodule-internal):
//! - [`WindowSnapshot`] — captured window size and cursor position.
//! - [`CameraRoutingSnapshot`] — captured per-camera routing inputs (entity, draw order, surface
//!   metrics, and the bit flags below).
//! - [`CameraRoutingSnapshotFlags`] — `ACTIVE`/`MANUAL`/`DISABLED`/`EGUI_BLOCKED`/
//!   `ANIMATION_IGNORE`/`CURSOR_HIT` bitset.
//! - [`EguiBlockState`] — present-frame egui focus status (only meaningful under the `bevy_egui`
//!   feature; always `Open` otherwise).
//!
//! [`collect_window_snapshots`] and [`collect_camera_snapshots`] are the entry points the
//! parent `resolve_camera_input_routing` system calls each frame.

use std::collections::HashMap;

use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::window::WindowRef;

use crate::CameraInputInterruptBehavior;
use crate::CameraMoveList;
use crate::OrbitCam;
#[cfg(feature = "bevy_egui")]
use crate::egui::BlockOnEguiFocus;
#[cfg(feature = "bevy_egui")]
use crate::egui::EguiWantsFocus;
#[cfg(feature = "bevy_egui")]
use crate::egui::FocusFrame;
use crate::input::CameraInputDisabled;
use crate::input::CameraInputSurfaceMetrics;
use crate::input::OrbitCamManual;

#[derive(Clone, Copy)]
pub(super) struct WindowSnapshot {
    size:   Vec2,
    cursor: Option<Vec2>,
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(super) struct CameraRoutingSnapshotFlags: u8 {
        const ACTIVE = 1 << 0;
        const MANUAL = 1 << 1;
        const DISABLED = 1 << 2;
        const EGUI_BLOCKED = 1 << 3;
        const ANIMATION_IGNORE = 1 << 4;
        const CURSOR_HIT = 1 << 5;
    }
}

pub(super) struct CameraRoutingSnapshot {
    pub(super) entity:  Entity,
    pub(super) order:   isize,
    pub(super) flags:   CameraRoutingSnapshotFlags,
    pub(super) metrics: CameraInputSurfaceMetrics,
}

impl CameraRoutingSnapshot {
    pub(super) const fn has(&self, flag: CameraRoutingSnapshotFlags) -> bool {
        self.flags.contains(flag)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EguiBlockState {
    Blocked,
    Open,
}

impl From<bool> for EguiBlockState {
    fn from(blocked: bool) -> Self { if blocked { Self::Blocked } else { Self::Open } }
}

pub(super) fn collect_window_snapshots(
    world: &mut World,
) -> HashMap<Option<Entity>, WindowSnapshot> {
    let mut windows = HashMap::new();

    let mut primary_query = world.query_filtered::<&Window, With<PrimaryWindow>>();
    if let Ok(window) = primary_query.single(world) {
        windows.insert(
            None,
            WindowSnapshot {
                size:   Vec2::new(window.width(), window.height()),
                cursor: window.cursor_position(),
            },
        );
    }

    let mut other_query = world.query_filtered::<(Entity, &Window), Without<PrimaryWindow>>();
    for (entity, window) in other_query.iter(world) {
        windows.insert(
            Some(entity),
            WindowSnapshot {
                size:   Vec2::new(window.width(), window.height()),
                cursor: window.cursor_position(),
            },
        );
    }

    windows
}

pub(super) fn collect_camera_snapshots(
    world: &mut World,
    windows: &HashMap<Option<Entity>, WindowSnapshot>,
) -> Vec<CameraRoutingSnapshot> {
    collect_camera_snapshots_impl(world, windows)
}

#[cfg(not(feature = "bevy_egui"))]
fn collect_camera_snapshots_impl(
    world: &mut World,
    windows: &HashMap<Option<Entity>, WindowSnapshot>,
) -> Vec<CameraRoutingSnapshot> {
    let mut query = world.query_filtered::<(
        Entity,
        &Camera,
        &RenderTarget,
        Option<&OrbitCamManual>,
        Option<&CameraInputDisabled>,
        Option<&CameraMoveList>,
        Option<&CameraInputInterruptBehavior>,
        Option<&CameraInputSurfaceMetrics>,
    ), With<OrbitCam>>();

    query
        .iter(world)
        .map(
            |(entity, camera, target, manual, disabled, move_list, interrupt, explicit_metrics)| {
                camera_snapshot(
                    entity,
                    camera,
                    target,
                    manual,
                    disabled,
                    move_list,
                    interrupt,
                    explicit_metrics,
                    EguiBlockState::Open,
                    windows,
                )
            },
        )
        .collect()
}

#[cfg(feature = "bevy_egui")]
fn collect_camera_snapshots_impl(
    world: &mut World,
    windows: &HashMap<Option<Entity>, WindowSnapshot>,
) -> Vec<CameraRoutingSnapshot> {
    let egui_blocks_all = world.get_resource::<EguiWantsFocus>().is_some_and(|focus| {
        matches!(focus.previous, FocusFrame::Wants) || matches!(focus.current, FocusFrame::Wants)
    });
    let mut query = world.query_filtered::<(
        Entity,
        &Camera,
        &RenderTarget,
        Option<&OrbitCamManual>,
        Option<&CameraInputDisabled>,
        Option<&CameraMoveList>,
        Option<&CameraInputInterruptBehavior>,
        Option<&CameraInputSurfaceMetrics>,
        Option<&BlockOnEguiFocus>,
    ), With<OrbitCam>>();

    query
        .iter(world)
        .map(
            |(
                entity,
                camera,
                target,
                manual,
                disabled,
                move_list,
                interrupt,
                explicit_metrics,
                block_on_egui,
            )| {
                camera_snapshot(
                    entity,
                    camera,
                    target,
                    manual,
                    disabled,
                    move_list,
                    interrupt,
                    explicit_metrics,
                    EguiBlockState::from(egui_blocks_all && block_on_egui.is_some()),
                    windows,
                )
            },
        )
        .collect()
}

fn camera_snapshot(
    entity: Entity,
    camera: &Camera,
    target: &RenderTarget,
    manual: Option<&OrbitCamManual>,
    disabled: Option<&CameraInputDisabled>,
    move_list: Option<&CameraMoveList>,
    interrupt: Option<&CameraInputInterruptBehavior>,
    explicit_metrics: Option<&CameraInputSurfaceMetrics>,
    egui_block_state: EguiBlockState,
    windows: &HashMap<Option<Entity>, WindowSnapshot>,
) -> CameraRoutingSnapshot {
    let window = window_snapshot(target, windows);
    let metrics = camera_input_surface_metrics(camera, window, explicit_metrics.copied());
    let cursor_hit = window
        .and_then(|window| window.cursor)
        .is_some_and(|cursor| cursor_hits_camera(cursor, camera));
    let animation = move_list.is_some()
        && interrupt.copied().unwrap_or_default() == CameraInputInterruptBehavior::Ignore;
    let mut flags = CameraRoutingSnapshotFlags::empty();
    flags.set(CameraRoutingSnapshotFlags::ACTIVE, camera.is_active);
    flags.set(CameraRoutingSnapshotFlags::MANUAL, manual.is_some());
    flags.set(CameraRoutingSnapshotFlags::DISABLED, disabled.is_some());
    flags.set(
        CameraRoutingSnapshotFlags::EGUI_BLOCKED,
        matches!(egui_block_state, EguiBlockState::Blocked),
    );
    flags.set(CameraRoutingSnapshotFlags::ANIMATION_IGNORE, animation);
    flags.set(CameraRoutingSnapshotFlags::CURSOR_HIT, cursor_hit);

    CameraRoutingSnapshot {
        entity,
        order: camera.order,
        flags,
        metrics,
    }
}

fn camera_input_surface_metrics(
    camera: &Camera,
    window: Option<&WindowSnapshot>,
    explicit: Option<CameraInputSurfaceMetrics>,
) -> CameraInputSurfaceMetrics {
    let mut metrics = CameraInputSurfaceMetrics {
        camera_view_size:   camera.logical_viewport_size(),
        input_surface_size: window
            .map(|window| window.size)
            .or_else(|| camera.logical_viewport_size()),
    };

    if let Some(explicit) = explicit {
        if explicit.camera_view_size.is_some() {
            metrics.camera_view_size = explicit.camera_view_size;
        }
        if explicit.input_surface_size.is_some() {
            metrics.input_surface_size = explicit.input_surface_size;
        }
    }

    metrics
}

fn window_snapshot<'a>(
    target: &RenderTarget,
    windows: &'a HashMap<Option<Entity>, WindowSnapshot>,
) -> Option<&'a WindowSnapshot> {
    let RenderTarget::Window(window_ref) = target else {
        return None;
    };

    match window_ref {
        WindowRef::Primary => windows.get(&None),
        WindowRef::Entity(entity) => windows.get(&Some(*entity)),
    }
}

fn cursor_hits_camera(cursor: Vec2, camera: &Camera) -> bool {
    camera
        .logical_viewport_rect()
        .is_some_and(|Rect { min, max }| {
            cursor.x > min.x && cursor.x < max.x && cursor.y > min.y && cursor.y < max.y
        })
}
