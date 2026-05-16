use std::collections::HashMap;

use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::window::WindowRef;

use super::CameraInputDisabled;
use super::CameraInputSurfaceMetrics;
use super::CameraInteractionSources;
use super::OrbitCamInputModeReplaced;
use super::OrbitCamManual;
use crate::CameraInputInterruptBehavior;
use crate::CameraMoveList;
use crate::OrbitCam;
#[cfg(feature = "bevy_egui")]
use crate::egui::BlockOnEguiFocus;
#[cfg(feature = "bevy_egui")]
use crate::egui::EguiWantsFocus;
#[cfg(feature = "bevy_egui")]
use crate::egui::FocusFrame;
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
}

impl CameraInputSourceLatches {
    pub(crate) const fn acquire_sources(
        &mut self,
        camera: Entity,
        sources: CameraInteractionSources,
    ) {
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

    pub(crate) fn release_sources(&mut self, camera: Entity, sources: CameraInteractionSources) {
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

    pub(crate) fn clear_camera(&mut self, camera: Entity) {
        if self.mouse.is_some_and(|latch| latch.camera() == camera) {
            self.mouse = None;
        }
        if self.keyboard.is_some_and(|latch| latch.camera() == camera) {
            self.keyboard = None;
        }
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
    pub(crate) context_gate: ContextGate,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ContextGate {
    Allowed,
    #[default]
    Blocked,
}

impl ContextGate {
    pub(crate) const fn is_allowed(self) -> bool { matches!(self, Self::Allowed) }
}

impl From<bool> for ContextGate {
    fn from(allowed: bool) -> Self {
        if allowed {
            Self::Allowed
        } else {
            Self::Blocked
        }
    }
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

    let latches = {
        let mut latches = world.resource_mut::<CameraInputSourceLatches>();
        latches.recover_unavailable_latches(&available_cameras);
        latches.clone()
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EguiBlockState {
    Blocked,
    Open,
}

impl From<bool> for EguiBlockState {
    fn from(blocked: bool) -> Self { if blocked { Self::Blocked } else { Self::Open } }
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
    for latch in [latches.mouse, latches.keyboard].into_iter().flatten() {
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

    fn spawn_non_orbit_camera(world: &mut World, order: isize) -> Entity {
        world
            .spawn((
                Camera { order, ..default() },
                RenderTarget::Window(WindowRef::Primary),
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
    fn source_latch_routes_without_cursor_hit() {
        let mut app = test_app();
        let camera = spawn_camera(app.world_mut(), OrbitCamPreset::SimpleMouse);
        app.world_mut()
            .resource_mut::<CameraInputSourceLatches>()
            .keyboard = Some(OrbitCamInputOwnerLatch(camera));

        app.update();

        assert_eq!(
            app.world()
                .resource::<ResolvedOrbitCamInputRoute>()
                .routed_camera,
            Some(camera)
        );
    }

    #[test]
    fn non_orbit_cameras_do_not_count_as_routing_targets() {
        let mut app = test_app();
        let camera = spawn_camera(app.world_mut(), OrbitCamPreset::SimpleMouse);
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
        let routed = spawn_camera(app.world_mut(), OrbitCamPreset::SimpleMouse);
        let manual = spawn_camera(app.world_mut(), OrbitCamManual);
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
        let camera = spawn_camera(app.world_mut(), (OrbitCamPreset::SimpleMouse, metrics));
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
                .map(|gated| gated.context_gate),
            Some(ContextGate::Blocked)
        );
    }
}
