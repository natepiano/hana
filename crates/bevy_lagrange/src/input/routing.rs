use std::collections::HashMap;

use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::window::WindowRef;

use crate::CameraInputInterruptBehavior;
use crate::CameraMoveList;
#[cfg(feature = "bevy_egui")]
use crate::egui::BlockOnEguiFocus;
#[cfg(feature = "bevy_egui")]
use crate::egui::EguiWantsFocus;
use crate::input::CameraInputDisabled;
use crate::input::CameraInputSurfaceMetrics;
use crate::input::OrbitCamInputModeReplaced;
use crate::input::OrbitCamManual;
use crate::system_sets::OrbitCamInputInternalSet;

/// Camera input routing mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[non_exhaustive]
pub enum CameraInputRouting {
    /// Choose the active camera by cursor/touch hit testing.
    #[default]
    CursorHitTest,
    /// Use the configured explicit camera entity.
    Explicit,
}

/// Fallback policy for input without pointer position metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[non_exhaustive]
pub enum NoPositionFallback {
    /// Drop input unless a latch, explicit route, or unambiguous hit test identifies a camera.
    #[default]
    NoInput,
    /// Route to the only eligible `OrbitCam` when exactly one exists.
    OnlyEligibleCamera,
}

/// Public routing preference for preset/custom camera input.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Resource, Default)]
pub struct CameraInputRoutingConfig {
    /// Routing mode.
    pub mode:                 CameraInputRouting,
    /// Explicit target camera used when `mode` is [`CameraInputRouting::Explicit`].
    pub explicit_camera:      Option<Entity>,
    /// Fallback policy for keyboard/gamepad style input without pointer position.
    pub no_position_fallback: NoPositionFallback,
}

impl CameraInputRoutingConfig {
    /// Creates cursor-hit-test routing with default no-position fallback.
    #[must_use]
    pub const fn cursor_hit_test() -> Self {
        Self {
            mode:                 CameraInputRouting::CursorHitTest,
            explicit_camera:      None,
            no_position_fallback: NoPositionFallback::NoInput,
        }
    }

    /// Creates explicit routing to `camera`.
    #[must_use]
    pub const fn explicit(camera: Entity) -> Self {
        Self {
            mode:                 CameraInputRouting::Explicit,
            explicit_camera:      Some(camera),
            no_position_fallback: NoPositionFallback::NoInput,
        }
    }

    /// Sets the no-position fallback policy.
    #[must_use]
    pub const fn with_no_position_fallback(mut self, fallback: NoPositionFallback) -> Self {
        self.no_position_fallback = fallback;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OrbitCamInputOwnerLatch(Entity);

impl OrbitCamInputOwnerLatch {
    pub(crate) const fn camera(self) -> Entity { self.0 }
}

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CameraInputSourceLatches {
    mouse:    Option<OrbitCamInputOwnerLatch>,
    keyboard: Option<OrbitCamInputOwnerLatch>,
    gamepads: HashMap<Entity, OrbitCamInputOwnerLatch>,
    touches:  HashMap<u64, OrbitCamInputOwnerLatch>,
}

impl CameraInputSourceLatches {
    pub(crate) fn clear_camera(&mut self, camera: Entity) {
        if self.mouse.is_some_and(|latch| latch.camera() == camera) {
            self.mouse = None;
        }
        if self.keyboard.is_some_and(|latch| latch.camera() == camera) {
            self.keyboard = None;
        }
        self.gamepads.retain(|_, latch| latch.camera() != camera);
        self.touches.retain(|_, latch| latch.camera() != camera);
    }

    fn recover_unavailable_latches(&mut self, available_cameras: &[Entity]) {
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
        self.gamepads
            .retain(|_, latch| is_available(latch.camera()));
        self.touches.retain(|_, latch| is_available(latch.camera()));
    }
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    struct OrbitCamInputBlockerBits: u8 {
        const DISABLED = 1 << 0;
        const INACTIVE_CAMERA = 1 << 1;
        const EGUI_FOCUS = 1 << 2;
        const ANIMATION_IGNORE = 1 << 3;
        const UNAVAILABLE_OWNER = 1 << 4;
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct OrbitCamInputBlockers {
    bits: OrbitCamInputBlockerBits,
}

impl OrbitCamInputBlockers {
    pub(crate) const fn is_blocked(self) -> bool { !self.bits.is_empty() }

    fn from_snapshot(snapshot: &CameraRoutingSnapshot, routed_camera: Option<Entity>) -> Self {
        let mut bits = OrbitCamInputBlockerBits::empty();
        if snapshot.has(CameraRoutingSnapshotFlags::DISABLED) {
            bits.insert(OrbitCamInputBlockerBits::DISABLED);
        }
        if !snapshot.has(CameraRoutingSnapshotFlags::MANUAL)
            && routed_camera != Some(snapshot.entity)
        {
            bits.insert(OrbitCamInputBlockerBits::INACTIVE_CAMERA);
        }
        if snapshot.has(CameraRoutingSnapshotFlags::EGUI_BLOCKED) {
            bits.insert(OrbitCamInputBlockerBits::EGUI_FOCUS);
        }
        if snapshot.has(CameraRoutingSnapshotFlags::ANIMATION_IGNORE) {
            bits.insert(OrbitCamInputBlockerBits::ANIMATION_IGNORE);
        }
        Self { bits }
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct OrbitCamInputContextGated {
    pub(crate) allowed: bool,
}

#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub(crate) struct ResolvedOrbitCamInputRoute {
    routed_camera: Option<Entity>,
    metrics:       HashMap<Entity, CameraInputSurfaceMetrics>,
    blockers:      HashMap<Entity, OrbitCamInputBlockers>,
}

impl ResolvedOrbitCamInputRoute {
    pub(crate) const fn routed_camera(&self) -> Option<Entity> { self.routed_camera }

    pub(crate) fn metrics_for(&self, camera: Entity) -> Option<CameraInputSurfaceMetrics> {
        self.metrics.get(&camera).copied()
    }

    pub(crate) fn blockers_for(&self, camera: Entity) -> Option<OrbitCamInputBlockers> {
        self.blockers.get(&camera).copied()
    }
}

#[derive(Clone, Copy)]
struct WindowSnapshot {
    size:   Vec2,
    cursor: Option<Vec2>,
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    struct CameraRoutingSnapshotFlags: u8 {
        const ACTIVE = 1 << 0;
        const MANUAL = 1 << 1;
        const DISABLED = 1 << 2;
        const EGUI_BLOCKED = 1 << 3;
        const ANIMATION_IGNORE = 1 << 4;
        const CURSOR_HIT = 1 << 5;
    }
}

struct CameraRoutingSnapshot {
    entity:  Entity,
    order:   isize,
    flags:   CameraRoutingSnapshotFlags,
    metrics: CameraInputSurfaceMetrics,
}

impl CameraRoutingSnapshot {
    const fn has(&self, flag: CameraRoutingSnapshotFlags) -> bool { self.flags.contains(flag) }
}

pub(crate) struct OrbitCamRoutingPlugin;

impl Plugin for OrbitCamRoutingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraInputRoutingConfig>()
            .init_resource::<CameraInputSourceLatches>()
            .init_resource::<ResolvedOrbitCamInputRoute>()
            .add_observer(clear_latches_on_mode_replaced)
            .add_systems(
                PreUpdate,
                resolve_camera_input_routing.in_set(OrbitCamInputInternalSet::Routing),
            );
    }
}

fn clear_latches_on_mode_replaced(
    replaced: On<OrbitCamInputModeReplaced>,
    mut latches: ResMut<CameraInputSourceLatches>,
) {
    latches.clear_camera(replaced.camera);
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

    world
        .resource_mut::<CameraInputSourceLatches>()
        .recover_unavailable_latches(&available_cameras);

    let routed_camera = select_routed_camera(&config, &snapshots, &available_cameras);
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
                allowed: !blockers.is_blocked(),
            },
            snapshot.metrics,
        ));
        resolved.metrics.insert(snapshot.entity, snapshot.metrics);
        resolved.blockers.insert(snapshot.entity, blockers);
    }

    world.insert_resource(resolved);
}

fn collect_window_snapshots(world: &mut World) -> HashMap<Option<Entity>, WindowSnapshot> {
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

fn collect_camera_snapshots(
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
    let mut query = world.query::<(
        Entity,
        &Camera,
        &RenderTarget,
        Option<&OrbitCamManual>,
        Option<&CameraInputDisabled>,
        Option<&CameraMoveList>,
        Option<&CameraInputInterruptBehavior>,
    )>();

    query
        .iter(world)
        .map(
            |(entity, camera, target, manual, disabled, move_list, interrupt)| {
                camera_snapshot(
                    entity, camera, target, manual, disabled, move_list, interrupt, false, windows,
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
    let egui_blocks_all = world
        .get_resource::<EguiWantsFocus>()
        .is_some_and(|focus| focus.prev || focus.curr);
    let mut query = world.query::<(
        Entity,
        &Camera,
        &RenderTarget,
        Option<&OrbitCamManual>,
        Option<&CameraInputDisabled>,
        Option<&CameraMoveList>,
        Option<&CameraInputInterruptBehavior>,
        Option<&BlockOnEguiFocus>,
    )>();

    query
        .iter(world)
        .map(
            |(entity, camera, target, manual, disabled, move_list, interrupt, block_on_egui)| {
                camera_snapshot(
                    entity,
                    camera,
                    target,
                    manual,
                    disabled,
                    move_list,
                    interrupt,
                    egui_blocks_all && block_on_egui.is_some(),
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
    egui_blocked: bool,
    windows: &HashMap<Option<Entity>, WindowSnapshot>,
) -> CameraRoutingSnapshot {
    let window = window_snapshot(target, windows);
    let metrics = CameraInputSurfaceMetrics {
        camera_view_size:   camera.logical_viewport_size(),
        input_surface_size: window
            .map(|window| window.size)
            .or_else(|| camera.logical_viewport_size()),
    };
    let cursor_hit = window
        .and_then(|window| window.cursor)
        .is_some_and(|cursor| cursor_hits_camera(cursor, camera));
    let animation = move_list.is_some()
        && interrupt.copied().unwrap_or_default() == CameraInputInterruptBehavior::Ignore;
    let mut flags = CameraRoutingSnapshotFlags::empty();
    flags.set(CameraRoutingSnapshotFlags::ACTIVE, camera.is_active);
    flags.set(CameraRoutingSnapshotFlags::MANUAL, manual.is_some());
    flags.set(CameraRoutingSnapshotFlags::DISABLED, disabled.is_some());
    flags.set(CameraRoutingSnapshotFlags::EGUI_BLOCKED, egui_blocked);
    flags.set(CameraRoutingSnapshotFlags::ANIMATION_IGNORE, animation);
    flags.set(CameraRoutingSnapshotFlags::CURSOR_HIT, cursor_hit);

    CameraRoutingSnapshot {
        entity,
        order: camera.order,
        flags,
        metrics,
    }
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

fn select_routed_camera(
    config: &CameraInputRoutingConfig,
    snapshots: &[CameraRoutingSnapshot],
    available_cameras: &[Entity],
) -> Option<Entity> {
    match config.mode {
        CameraInputRouting::Explicit => config
            .explicit_camera
            .filter(|camera| available_cameras.contains(camera)),
        CameraInputRouting::CursorHitTest => cursor_hit_camera(snapshots).or_else(|| {
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
    use bevy::prelude::*;
    use bevy::window::WindowRef;

    use super::*;
    use crate::OrbitCam;
    use crate::input::OrbitCamInputModeReplaced;
    use crate::input::OrbitCamManual;
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

    #[test]
    fn explicit_routing_config_applies_in_preinput() {
        let mut app = test_app();
        let camera = spawn_camera(app.world_mut(), OrbitCamPreset::SimpleMouse);
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
        let camera = spawn_camera(app.world_mut(), OrbitCamManual);
        app.world_mut()
            .resource_mut::<CameraInputSourceLatches>()
            .mouse = Some(OrbitCamInputOwnerLatch(camera));
        app.world_mut()
            .entity_mut(camera)
            .trigger(|camera| OrbitCamInputModeReplaced { camera });

        assert!(
            app.world()
                .resource::<CameraInputSourceLatches>()
                .mouse
                .is_none()
        );
    }

    #[test]
    fn stale_latches_are_recovered_during_routing() {
        let mut app = test_app();
        let stale_camera = Entity::PLACEHOLDER;
        app.world_mut()
            .resource_mut::<CameraInputSourceLatches>()
            .keyboard = Some(OrbitCamInputOwnerLatch(stale_camera));

        app.update();

        assert!(
            app.world()
                .resource::<CameraInputSourceLatches>()
                .keyboard
                .is_none()
        );
    }

    #[test]
    fn metrics_are_recorded_for_non_routed_manual_cameras() {
        let mut app = test_app();
        let routed = spawn_camera(app.world_mut(), OrbitCamPreset::SimpleMouse);
        let manual = spawn_camera(app.world_mut(), OrbitCamManual);
        app.insert_resource(CameraInputRoutingConfig::explicit(routed));

        app.update();

        let resolved = app.world().resource::<ResolvedOrbitCamInputRoute>();
        assert!(resolved.metrics.contains_key(&manual));
        assert_eq!(resolved.routed_camera, Some(routed));
    }

    #[test]
    fn disabled_camera_sets_blocker_and_gates_context() {
        let mut app = test_app();
        let camera = spawn_camera(
            app.world_mut(),
            (OrbitCamPreset::SimpleMouse, CameraInputDisabled),
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
                .map(|gated| gated.allowed),
            Some(false)
        );
    }
}
