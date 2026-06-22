//! Per-frame camera input routing: which `OrbitCam` (if any) owns input this frame.
//!
//! Public surface (re-exported here from submodules):
//! - [`CameraInputRouting`], [`NoPositionFallback`], [`CameraInputRoutingConfig`] — caller-facing
//!   configuration (see [`config`]).
//! - [`ResolvedOrbitCamInputRoute`] — the per-frame result: routed camera, surface metrics, and
//!   per-camera blocker reasons.
//!
//! Internal surface (this file):
//! - [`OrbitCamRoutingPlugin`] — registers the resources, the `clear_latches_on_mode_replaced`
//!   observer, and the `resolve_camera_input_routing` system in
//!   [`OrbitCamInputInternalSet::Routing`].
//! - `resolve_camera_input_routing` — the system itself; collects window/camera snapshots, picks
//!   the routed camera, computes blockers, writes the resolved route and the per-camera gate
//!   components.
//! - `select_routed_camera` / `latched_camera` / `cursor_hit_camera` — selection helpers used by
//!   the system.
//!
//! Submodules:
//! - [`config`] — public config types.
//! - [`latches`] — per-source ownership state (`CameraInputSourceLatches`).
//! - [`blockers`] — per-camera gating components (`OrbitCamInputBlockers`,
//!   `OrbitCamInputContextGated`).
//! - [`snapshot`] — per-frame window/camera capture used internally by the system.

mod blockers;
mod config;
mod latches;
mod snapshot;

use std::collections::HashMap;

use bevy::prelude::*;
use blockers::ContextGate;
pub(crate) use blockers::OrbitCamInputBlockers;
pub(crate) use blockers::OrbitCamInputContextGated;
pub use config::CameraInputRouting;
pub use config::CameraInputRoutingConfig;
pub use config::NoPositionFallback;
pub(crate) use latches::CameraInputSourceLatches;
pub(crate) use latches::OrbitCamSlowModeLatches;
pub use latches::OrbitCamSlowModeState;
use latches::clear_latches_on_mode_replaced;
use snapshot::CameraRoutingSnapshot;
use snapshot::CameraRoutingSnapshotFlags;
use snapshot::collect_camera_snapshots;
use snapshot::collect_window_snapshots;

use super::CameraInputSurfaceMetrics;
use crate::system_sets::OrbitCamInputInternalSet;

pub(super) fn is_slow_mode_active(slow_latches: &OrbitCamSlowModeLatches, camera: Entity) -> bool {
    slow_latches.is_active(camera)
}

pub(super) fn toggle_slow_mode_latch(slow_latches: &mut OrbitCamSlowModeLatches, camera: Entity) {
    slow_latches.toggle(camera);
}

/// Result of the per-frame input routing pass: which `OrbitCam` (if any)
/// currently owns input, plus the cursor-surface metrics and blocker reasons
/// for every candidate camera.
///
/// Read this as a resource to discover the active camera — useful for
/// multi-camera HUDs, per-camera home-key bindings, and any feature that
/// needs to react to "which viewport is the cursor over."
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct ResolvedOrbitCamInputRoute {
    routed_camera: Option<Entity>,
    metrics:       HashMap<Entity, CameraInputSurfaceMetrics>,
    blockers:      HashMap<Entity, OrbitCamInputBlockers>,
}

impl ResolvedOrbitCamInputRoute {
    /// The `OrbitCam` entity currently receiving input, if any. Returns
    /// `None` when the cursor is not over any orbit-camera viewport.
    #[must_use]
    pub const fn routed_camera(&self) -> Option<Entity> { self.routed_camera }

    pub(crate) fn metrics_for(&self, camera: Entity) -> Option<CameraInputSurfaceMetrics> {
        self.metrics.get(&camera).copied()
    }

    pub(crate) fn blockers_for(&self, camera: Entity) -> Option<OrbitCamInputBlockers> {
        self.blockers.get(&camera).copied()
    }
}

pub(crate) struct OrbitCamRoutingPlugin;

impl Plugin for OrbitCamRoutingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraInputRoutingConfig>()
            .init_resource::<CameraInputSourceLatches>()
            .init_resource::<OrbitCamSlowModeLatches>()
            .init_resource::<ResolvedOrbitCamInputRoute>()
            .add_observer(clear_latches_on_mode_replaced)
            .add_systems(
                PreUpdate,
                resolve_camera_input_routing.in_set(OrbitCamInputInternalSet::Routing),
            );
    }
}

fn resolve_camera_input_routing(world: &mut World) {
    let config = *world.resource::<CameraInputRoutingConfig>();
    let windows = collect_window_snapshots(world);
    let snapshots = collect_camera_snapshots(world, &windows);
    let available_cameras = snapshots
        .iter()
        .filter(|snapshot| {
            snapshot.has(CameraRoutingSnapshotFlags::ACTIVE)
                && !snapshot.has(CameraRoutingSnapshotFlags::DISABLED)
        })
        .map(|snapshot| snapshot.entity)
        .collect::<Vec<_>>();

    let latches = {
        let mut latches = world.resource_mut::<CameraInputSourceLatches>();
        latches.recover_unavailable_latches(&available_cameras);
        latches.clone()
    };
    let slow_latches = {
        let mut slow_latches = world.resource_mut::<OrbitCamSlowModeLatches>();
        slow_latches.recover_unavailable_latches(&available_cameras);
        slow_latches.clone()
    };

    let routed_camera = select_routed_camera(&config, &snapshots, &available_cameras, &latches);
    let mut resolved = ResolvedOrbitCamInputRoute {
        routed_camera,
        metrics: HashMap::new(),
        blockers: HashMap::new(),
    };

    for snapshot in snapshots {
        let blockers = OrbitCamInputBlockers::from_snapshot(&snapshot, routed_camera);

        world.entity_mut(snapshot.entity).insert((
            blockers,
            OrbitCamInputContextGated {
                context_gate: ContextGate::from(!blockers.is_blocked()),
            },
            OrbitCamSlowModeState::from_active(slow_latches.is_active(snapshot.entity)),
        ));
        resolved.metrics.insert(snapshot.entity, snapshot.metrics);
        resolved.blockers.insert(snapshot.entity, blockers);
    }

    world.insert_resource(resolved);
}

fn select_routed_camera(
    config: &CameraInputRoutingConfig,
    snapshots: &[CameraRoutingSnapshot],
    available_cameras: &[Entity],
    latches: &CameraInputSourceLatches,
) -> Option<Entity> {
    match config.mode {
        CameraInputRouting::Explicit => config
            .explicit_camera
            .filter(|camera| available_cameras.contains(camera)),
        CameraInputRouting::CursorHitTest => latched_camera(latches, available_cameras)
            .or_else(|| cursor_hit_camera(snapshots))
            .or_else(|| {
                if config.no_position_fallback == NoPositionFallback::OnlyEligibleCamera
                    && available_cameras.len() == 1
                {
                    available_cameras.first().copied()
                } else {
                    None
                }
            }),
    }
}

fn latched_camera(
    latches: &CameraInputSourceLatches,
    available_cameras: &[Entity],
) -> Option<Entity> {
    let mut camera = None;
    for latch in [latches.mouse_latch(), latches.keyboard_latch()]
        .into_iter()
        .flatten()
    {
        let latched_camera = latch.camera();
        if !available_cameras.contains(&latched_camera) {
            continue;
        }
        match camera {
            Some(existing) if existing != latched_camera => return None,
            Some(_) => {},
            None => camera = Some(latched_camera),
        }
    }
    camera
}

fn cursor_hit_camera(snapshots: &[CameraRoutingSnapshot]) -> Option<Entity> {
    snapshots
        .iter()
        .filter(|snapshot| {
            snapshot.has(CameraRoutingSnapshotFlags::ACTIVE)
                && !snapshot.has(CameraRoutingSnapshotFlags::DISABLED)
                && snapshot.has(CameraRoutingSnapshotFlags::CURSOR_HIT)
        })
        .max_by_key(|snapshot| snapshot.order)
        .map(|snapshot| snapshot.entity)
}

#[cfg(test)]
mod tests {
    use bevy::camera::RenderTarget;
    use bevy::camera::RenderTargetInfo;
    use bevy::prelude::*;
    use bevy::window::PrimaryWindow;
    use bevy::window::WindowRef;

    use super::blockers::OrbitCamInputBlockerBits;
    use super::*;
    use crate::OrbitCam;
    use crate::input::CameraInputDisabled;
    use crate::input::CameraInteractionSources;
    use crate::input::OrbitCamInputMode;
    use crate::input::OrbitCamInputModeReplaced;
    use crate::input::OrbitCamPreset;
    use crate::system_sets::LagrangeSystemSetsPlugin;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            LagrangeSystemSetsPlugin,
            OrbitCamRoutingPlugin,
        ));
        app
    }

    fn spawn_camera(world: &mut World, components: impl Bundle) -> Entity {
        world
            .spawn((
                OrbitCam::default(),
                Camera::default(),
                RenderTarget::Window(WindowRef::Primary),
                components,
            ))
            .id()
    }

    fn spawn_non_orbit_camera(world: &mut World, order: isize) -> Entity {
        world
            .spawn((
                Camera { order, ..default() },
                RenderTarget::Window(WindowRef::Primary),
            ))
            .id()
    }

    fn test_window(focused: bool) -> Window {
        let mut window = Window {
            focused,
            ..default()
        };
        window.set_cursor_position(Some(Vec2::new(100.0, 100.0)));
        window
    }

    fn test_camera(target: RenderTarget) -> (OrbitCam, Camera, RenderTarget, OrbitCamInputMode) {
        let mut camera = Camera::default();
        camera.computed.target_info = Some(RenderTargetInfo {
            physical_size: UVec2::new(1280, 720),
            scale_factor:  1.0,
        });
        (
            OrbitCam::default(),
            camera,
            target,
            OrbitCamInputMode::with_preset(OrbitCamPreset::simple_mouse()),
        )
    }

    #[test]
    fn explicit_routing_config_applies_in_preinput() {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::with_preset(OrbitCamPreset::simple_mouse()),
        );
        app.insert_resource(CameraInputRoutingConfig::explicit(camera));

        app.update();

        assert_eq!(
            app.world()
                .resource::<ResolvedOrbitCamInputRoute>()
                .routed_camera,
            Some(camera)
        );
    }

    #[test]
    fn mode_replacement_clears_matching_latches() {
        let mut app = test_app();
        let camera = spawn_camera(app.world_mut(), OrbitCamInputMode::Manual);
        app.world_mut()
            .resource_mut::<CameraInputSourceLatches>()
            .acquire_sources(camera, CameraInteractionSources::MOUSE);
        app.world_mut()
            .entity_mut(camera)
            .trigger(|camera| OrbitCamInputModeReplaced { camera });

        assert!(
            app.world()
                .resource::<CameraInputSourceLatches>()
                .mouse_latch()
                .is_none()
        );
    }

    #[test]
    fn mode_replacement_clears_slow_latch_without_slow_mode() {
        let mut app = test_app();
        let camera = spawn_camera(app.world_mut(), OrbitCamInputMode::Manual);
        app.world_mut()
            .resource_mut::<OrbitCamSlowModeLatches>()
            .toggle(camera);

        app.world_mut()
            .entity_mut(camera)
            .trigger(|camera| OrbitCamInputModeReplaced { camera });

        assert!(
            !app.world()
                .resource::<OrbitCamSlowModeLatches>()
                .is_active(camera)
        );
    }

    #[test]
    fn stale_slow_latches_are_recovered_during_routing() {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::with_preset(OrbitCamPreset::simple_mouse()),
        );
        app.world_mut()
            .resource_mut::<OrbitCamSlowModeLatches>()
            .toggle(camera);
        let _ = app.world_mut().despawn(camera);

        app.update();

        assert!(
            !app.world()
                .resource::<OrbitCamSlowModeLatches>()
                .is_active(camera)
        );
    }

    #[test]
    fn stale_latches_are_recovered_during_routing() {
        let mut app = test_app();
        let stale_camera = Entity::PLACEHOLDER;
        app.world_mut()
            .resource_mut::<CameraInputSourceLatches>()
            .acquire_sources(stale_camera, CameraInteractionSources::KEYBOARD);

        app.update();

        assert!(
            app.world()
                .resource::<CameraInputSourceLatches>()
                .keyboard_latch()
                .is_none()
        );
    }

    #[test]
    fn source_latch_routes_without_cursor_hit() {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::with_preset(OrbitCamPreset::simple_mouse()),
        );
        app.world_mut()
            .resource_mut::<CameraInputSourceLatches>()
            .acquire_sources(camera, CameraInteractionSources::KEYBOARD);

        app.update();

        assert_eq!(
            app.world()
                .resource::<ResolvedOrbitCamInputRoute>()
                .routed_camera,
            Some(camera)
        );
    }

    #[test]
    fn focused_window_cursor_selects_camera_when_window_cursors_overlap() {
        let mut app = test_app();
        let primary_window = app
            .world_mut()
            .spawn((test_window(true), PrimaryWindow))
            .id();
        let second_window = app.world_mut().spawn(test_window(false)).id();
        let primary_camera = app
            .world_mut()
            .spawn(test_camera(RenderTarget::Window(WindowRef::Primary)))
            .id();
        let second_camera = app
            .world_mut()
            .spawn(test_camera(RenderTarget::Window(WindowRef::Entity(
                second_window,
            ))))
            .id();

        app.update();

        assert_eq!(
            app.world()
                .resource::<ResolvedOrbitCamInputRoute>()
                .routed_camera,
            Some(primary_camera)
        );

        if let Some(mut window) = app.world_mut().get_mut::<Window>(primary_window) {
            window.focused = false;
        }
        if let Some(mut window) = app.world_mut().get_mut::<Window>(second_window) {
            window.focused = true;
        }

        app.update();

        assert_eq!(
            app.world()
                .resource::<ResolvedOrbitCamInputRoute>()
                .routed_camera,
            Some(second_camera)
        );
    }

    #[test]
    fn non_orbit_cameras_do_not_count_as_routing_targets() {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::with_preset(OrbitCamPreset::simple_mouse()),
        );
        let overlay = spawn_non_orbit_camera(app.world_mut(), 1);
        app.insert_resource(
            CameraInputRoutingConfig::cursor_hit_test()
                .with_no_position_fallback(NoPositionFallback::OnlyEligibleCamera),
        );

        app.update();

        let resolved = app.world().resource::<ResolvedOrbitCamInputRoute>();
        assert_eq!(resolved.routed_camera, Some(camera));
        assert!(!resolved.metrics.contains_key(&overlay));
        assert!(
            app.world()
                .get::<OrbitCamInputContextGated>(overlay)
                .is_none()
        );
    }

    #[test]
    fn metrics_are_recorded_for_non_routed_manual_cameras() {
        let mut app = test_app();
        let routed = spawn_camera(
            app.world_mut(),
            OrbitCamInputMode::with_preset(OrbitCamPreset::simple_mouse()),
        );
        let manual = spawn_camera(app.world_mut(), OrbitCamInputMode::Manual);
        app.insert_resource(CameraInputRoutingConfig::explicit(routed));

        app.update();

        let resolved = app.world().resource::<ResolvedOrbitCamInputRoute>();
        assert!(resolved.metrics.contains_key(&manual));
        assert_eq!(resolved.routed_camera, Some(routed));
    }

    #[test]
    fn explicit_surface_metrics_are_recorded_in_route_resource() {
        let mut app = test_app();
        let metrics = CameraInputSurfaceMetrics::camera_view_and_input_surface(
            Vec2::new(120.0, 80.0),
            Vec2::new(240.0, 160.0),
        );
        let camera = spawn_camera(
            app.world_mut(),
            (
                OrbitCamInputMode::with_preset(OrbitCamPreset::simple_mouse()),
                metrics,
            ),
        );
        app.insert_resource(CameraInputRoutingConfig::explicit(camera));

        app.update();

        assert_eq!(
            app.world()
                .resource::<ResolvedOrbitCamInputRoute>()
                .metrics_for(camera),
            Some(metrics)
        );
    }

    #[test]
    fn disabled_camera_sets_blocker_and_gates_context() {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            (
                OrbitCamInputMode::with_preset(OrbitCamPreset::simple_mouse()),
                CameraInputDisabled,
            ),
        );
        app.insert_resource(CameraInputRoutingConfig::explicit(camera));

        app.update();

        assert_eq!(
            app.world()
                .resource::<ResolvedOrbitCamInputRoute>()
                .blockers
                .get(&camera)
                .map(|blockers| blockers.bits.contains(OrbitCamInputBlockerBits::DISABLED)),
            Some(true)
        );
        assert_eq!(
            app.world()
                .resource::<ResolvedOrbitCamInputRoute>()
                .blockers
                .get(&camera)
                .copied()
                .map(OrbitCamInputBlockers::is_blocked),
            Some(true)
        );
        assert_eq!(
            app.world()
                .get::<OrbitCamInputContextGated>(camera)
                .map(|gated| gated.context_gate),
            Some(ContextGate::Blocked)
        );
    }
}
