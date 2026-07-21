//! Shared restore preparation and retained restore origin.

use bevy::prelude::*;
use bevy::window::OnMonitor;
use bevy::window::PrimaryWindow;

use super::target_position;
use super::target_position::MonitorResolutionSource;
use super::target_position::PreparedWindowPosition;
use super::target_position::RestoreDiagnostics;
use super::target_position::TargetPosition;
use super::winit_info::WinitInfo;
use super::winit_info::X11FrameCompensated;
use crate::ManagedWindow;
use crate::Platform;
use crate::WindowKey;
use crate::monitors;
use crate::monitors::CurrentMonitor;
use crate::monitors::MonitorIdentity;
use crate::monitors::MonitorInfo;
use crate::monitors::Monitors;
use crate::persistence::CapturedPlacement;
use crate::persistence::CapturedWindowPlacement;
use crate::persistence::CapturedWindowStates;
use crate::persistence::PersistedWindowState;
use crate::persistence::RebasedCapturedPosition;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RestoreOrigin {
    Startup { window_key: WindowKey },
}

/// Marks that `init_winit_info` or an accepted `OnMonitor` association established
/// native-window readiness.
#[derive(Component)]
pub(crate) struct NativeWindowReady;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(crate) struct RestorePreparation {
    origin: RestoreOrigin,
}

impl RestorePreparation {
    #[must_use]
    pub(crate) const fn startup(window_key: WindowKey) -> Self {
        Self {
            origin: RestoreOrigin::Startup { window_key },
        }
    }

    #[must_use]
    pub(crate) const fn window_key(&self) -> &WindowKey {
        match &self.origin {
            RestoreOrigin::Startup { window_key } => window_key,
        }
    }
}

/// Accept or clear native readiness when Bevy changes a window's monitor association.
pub(crate) fn mark_native_window_ready(
    insert: On<Insert, OnMonitor>,
    windows: Query<(&Window, &OnMonitor), Or<(With<PrimaryWindow>, With<ManagedWindow>)>>,
    monitors: Res<Monitors>,
    mut commands: Commands,
) {
    let Ok((window, on_monitor)) = windows.get(insert.entity) else {
        return;
    };
    let mut entity = commands.entity(insert.entity);
    if let Some(current_monitor) =
        monitors::current_monitor_from_association(window, on_monitor, &monitors)
    {
        entity.insert((current_monitor, NativeWindowReady));
    } else {
        entity.remove::<(CurrentMonitor, NativeWindowReady)>();
    }
}

pub(crate) fn clear_native_window_ready(remove: On<Remove, OnMonitor>, mut commands: Commands) {
    commands
        .entity(remove.entity)
        .try_remove::<(CurrentMonitor, NativeWindowReady)>();
}

#[cfg(all(target_os = "linux", feature = "workaround-winit-4445"))]
type RestoreAttemptComponents = (
    RestorePreparation,
    TargetPosition,
    X11FrameCompensated,
    crate::x11_position_fix::X11FrameTop,
);

#[cfg(not(all(target_os = "linux", feature = "workaround-winit-4445")))]
type RestoreAttemptComponents = (RestorePreparation, TargetPosition, X11FrameCompensated);

pub(crate) fn cancel_restore(commands: &mut Commands, entity: Entity) {
    commands.entity(entity).queue(|mut entity: EntityWorldMut| {
        entity.remove::<RestoreAttemptComponents>();
        if let Some(mut window) = entity.get_mut::<Window>() {
            window.visible = true;
        }
    });
}

struct RestoreTargetBuilder<'a> {
    source:              &'a CapturedPlacement,
    monitors:            &'a Monitors,
    physical_decoration: UVec2,
    starting_scale:      f64,
    platform:            Platform,
}

struct PreparedRestore {
    target_position:           TargetPosition,
    monitor_resolution_source: MonitorResolutionSource,
}

impl RestoreTargetBuilder<'_> {
    fn build(&self) -> PreparedRestore {
        match self.source {
            CapturedPlacement::PersistedOnly(persisted_window_state) => {
                self.build_persisted(persisted_window_state)
            },
            CapturedPlacement::Captured(captured_window_placement) => {
                self.build_captured(captured_window_placement)
            },
        }
    }

    fn build_persisted(&self, persisted_window_state: &PersistedWindowState) -> PreparedRestore {
        let resolved_monitor = target_position::resolve_target_monitor_and_position(
            persisted_window_state.monitor,
            persisted_window_state.logical_position,
            self.monitors,
        );
        let prepared_window_position = match (
            resolved_monitor.monitor_resolution_source,
            resolved_monitor.logical_position,
        ) {
            (MonitorResolutionSource::FallbackToPrimary, _) => {
                PreparedWindowPosition::TargetUnavailable
            },
            (MonitorResolutionSource::Requested, Some((x, y))) => {
                PreparedWindowPosition::PersistedCoordinate(IVec2::new(x, y))
            },
            (MonitorResolutionSource::Requested, None) => {
                PreparedWindowPosition::PersistedWithoutCoordinate
            },
        };
        let target_position = target_position::compute_target_position(
            persisted_window_state,
            resolved_monitor.monitor_info,
            prepared_window_position,
            self.physical_decoration,
            self.starting_scale,
            self.platform,
        );
        PreparedRestore {
            target_position,
            monitor_resolution_source: resolved_monitor.monitor_resolution_source,
        }
    }

    fn build_captured(
        &self,
        captured_window_placement: &CapturedWindowPlacement,
    ) -> PreparedRestore {
        let persisted_window_state = captured_window_placement.project("");
        let (monitor_info, monitor_resolution_source) =
            self.resolve_captured_monitor(captured_window_placement, &persisted_window_state);
        let prepared_window_position = if matches!(
            monitor_resolution_source,
            MonitorResolutionSource::FallbackToPrimary
        ) {
            PreparedWindowPosition::TargetUnavailable
        } else {
            match captured_window_placement.rebased_position(monitor_info) {
                RebasedCapturedPosition::Restorable {
                    physical_position,
                    logical_position,
                } => PreparedWindowPosition::CapturedRestorable {
                    physical_position,
                    logical_position,
                },
                RebasedCapturedPosition::CompositorControlled => {
                    PreparedWindowPosition::CompositorControlled
                },
            }
        };
        let target_position = target_position::compute_target_position(
            &persisted_window_state,
            monitor_info,
            prepared_window_position,
            self.physical_decoration,
            self.starting_scale,
            self.platform,
        );
        PreparedRestore {
            target_position,
            monitor_resolution_source,
        }
    }

    fn resolve_captured_monitor<'a>(
        &'a self,
        captured_window_placement: &CapturedWindowPlacement,
        persisted_window_state: &PersistedWindowState,
    ) -> (&'a MonitorInfo, MonitorResolutionSource) {
        match captured_window_placement.monitor_snapshot.identity {
            MonitorIdentity::Verified(monitor_id) => self.monitors.by_id(monitor_id).map_or_else(
                || {
                    (
                        self.monitors.first(),
                        MonitorResolutionSource::FallbackToPrimary,
                    )
                },
                |monitor_info| (monitor_info, MonitorResolutionSource::Requested),
            ),
            MonitorIdentity::Unverified => {
                let resolved_monitor = target_position::resolve_target_monitor_and_position(
                    persisted_window_state.monitor,
                    persisted_window_state.logical_position,
                    self.monitors,
                );
                (
                    resolved_monitor.monitor_info,
                    resolved_monitor.monitor_resolution_source,
                )
            },
        }
    }
}

pub(crate) fn prepare_restore_targets(
    mut commands: Commands,
    preparations: Query<
        (Entity, &RestorePreparation, &CurrentMonitor),
        (
            With<Window>,
            With<NativeWindowReady>,
            Without<TargetPosition>,
        ),
    >,
    monitors: Res<Monitors>,
    winit_info: Option<Res<WinitInfo>>,
    captured_window_states: Res<CapturedWindowStates>,
    platform: Res<Platform>,
) {
    let Some(winit_info) = winit_info else {
        return;
    };
    if monitors.is_empty() {
        return;
    }

    for (entity, restore_preparation, current_monitor) in &preparations {
        let window_key = restore_preparation.window_key();
        let Some(prepared_restore) = captured_window_states.placement(window_key).map(|source| {
            RestoreTargetBuilder {
                source,
                monitors: &monitors,
                physical_decoration: winit_info.physical_decoration(),
                starting_scale: current_monitor.scale,
                platform: *platform,
            }
            .build()
        }) else {
            commands
                .entity(entity)
                .remove::<RestorePreparation>()
                .queue(move |mut entity: EntityWorldMut| {
                    if let Some(mut window) = entity.get_mut::<Window>() {
                        window.visible = true;
                    }
                });
            continue;
        };

        if matches!(
            prepared_restore.monitor_resolution_source,
            MonitorResolutionSource::FallbackToPrimary
        ) {
            warn!(
                "[prepare_restore_targets] [{window_key}] target monitor unavailable, falling back to the primary monitor without a retained coordinate"
            );
        }

        let target_position = prepared_restore.target_position;
        let is_fullscreen = target_position.saved_window_mode.is_fullscreen();
        let restore_diagnostics = RestoreDiagnostics {
            starting_monitor_index: current_monitor.index,
            starting_scale:         current_monitor.scale,
            target_scale:           target_position.target_scale,
            monitor_scale_strategy: target_position.monitor_scale_strategy,
        };
        commands
            .entity(entity)
            .insert((target_position, restore_diagnostics));

        if is_fullscreen || !platform.needs_frame_compensation() {
            commands.entity(entity).insert(X11FrameCompensated);
        }

        #[cfg(all(target_os = "windows", feature = "workaround-winit-3124"))]
        if is_fullscreen {
            commands.queue(move |world: &mut World| {
                if let Some(mut window) = world.get_mut::<Window>(entity) {
                    window.visible = true;
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bevy::window::PrimaryWindow;
    use bevy::window::WindowMode;
    use bevy::window::WindowPosition;
    use bevy::window::WindowScaleFactorChanged;

    use super::*;
    use crate::CancelWindowRecovery;
    use crate::ManagedWindowPersistence;
    use crate::WindowRecovery;
    use crate::WindowRestoreMismatch;
    use crate::WindowRestored;
    use crate::managed::ManagedWindowRegistry;
    use crate::monitors::InjectedCurrentMonitorSource;
    use crate::monitors::MonitorId;
    use crate::monitors::MonitorTopologyRevision;
    use crate::monitors::NativeQueryActivity;
    use crate::persistence::CapturedWindowPosition;
    use crate::persistence::SavedWindowMode;
    use crate::recovery::RecoveryPlugin;
    use crate::restore::RestorePlugin;
    use crate::restore::check_restore_settling;
    use crate::restore::has_restoring_windows;
    use crate::restore::restore_windows;
    use crate::restore::settle_state::SettleState;
    use crate::restore::target_position::MonitorScaleStrategy;

    const CAPTURED_OFFSET: IVec2 = IVec2::new(100, 50);
    const TARGET_ID: MonitorId = MonitorId::from_test_raw(7);

    #[derive(Clone, Copy)]
    enum StartupReadiness {
        Waiting,
        Associated,
    }

    #[derive(Debug, PartialEq)]
    struct TargetSnapshot {
        physical_position:      Option<IVec2>,
        logical_position:       Option<IVec2>,
        physical_size:          UVec2,
        logical_size:           UVec2,
        target_scale:           f64,
        starting_scale:         f64,
        monitor_scale_strategy: MonitorScaleStrategy,
        saved_window_mode:      SavedWindowMode,
        monitor_index:          usize,
    }

    #[derive(Default, Resource)]
    struct RestoreOutcomeCounts {
        restored:   usize,
        mismatched: usize,
    }

    fn record_restored(_: On<WindowRestored>, mut outcomes: ResMut<RestoreOutcomeCounts>) {
        outcomes.restored += 1;
    }

    fn record_mismatch(_: On<WindowRestoreMismatch>, mut outcomes: ResMut<RestoreOutcomeCounts>) {
        outcomes.mismatched += 1;
    }

    impl From<&TargetPosition> for TargetSnapshot {
        fn from(target_position: &TargetPosition) -> Self {
            Self {
                physical_position:      target_position.physical_position,
                logical_position:       target_position.logical_position,
                physical_size:          target_position.physical_size,
                logical_size:           target_position.logical_size,
                target_scale:           target_position.target_scale,
                starting_scale:         target_position.starting_scale,
                monitor_scale_strategy: target_position.monitor_scale_strategy,
                saved_window_mode:      target_position.saved_window_mode.clone(),
                monitor_index:          target_position.monitor_index,
            }
        }
    }

    fn monitor(
        identity: MonitorIdentity,
        index: usize,
        scale: f64,
        physical_position: IVec2,
    ) -> MonitorInfo {
        MonitorInfo {
            identity,
            index,
            scale,
            physical_position,
            physical_size: UVec2::new(1_920, 1_080),
        }
    }

    fn monitors_with_returned_target() -> Monitors {
        monitors_with_returned_target_entities(Entity::from_bits(1), Entity::from_bits(2))
    }

    fn monitors_with_returned_target_entities(
        starting_monitor_entity: Entity,
        target_monitor_entity: Entity,
    ) -> Monitors {
        Monitors::from_test_monitors([
            (
                starting_monitor_entity,
                monitor(MonitorIdentity::Unverified, 0, 1.0, IVec2::ZERO),
            ),
            (
                target_monitor_entity,
                monitor(
                    MonitorIdentity::Verified(TARGET_ID),
                    2,
                    2.0,
                    IVec2::new(2_000, -200),
                ),
            ),
        ])
    }

    fn captured_placement(
        monitor_identity: MonitorIdentity,
        monitor_index: usize,
        position: CapturedWindowPosition,
        saved_window_mode: SavedWindowMode,
    ) -> CapturedWindowPlacement {
        CapturedWindowPlacement {
            monitor_snapshot: monitor(monitor_identity, monitor_index, 1.0, IVec2::new(-1_920, 0)),
            position,
            logical_size: UVec2::new(800, 600),
            saved_window_mode,
            captured_scale: 1.0,
        }
    }

    fn build_target_for_synthetic_runtime(
        captured_window_placement: &CapturedWindowPlacement,
        monitors: &Monitors,
    ) -> TargetPosition {
        RestoreTargetBuilder {
            source: &CapturedPlacement::Captured(captured_window_placement.clone()),
            monitors,
            physical_decoration: UVec2::ZERO,
            starting_scale: 1.0,
            platform: Platform::Windows,
        }
        .build()
        .target_position
    }

    fn captured_startup_app(
        captured_window_placement: CapturedWindowPlacement,
        startup_readiness: StartupReadiness,
    ) -> (App, Entity, Entity) {
        let mut app = App::new();
        let readiness_monitor_entity = app.world_mut().spawn_empty().id();
        let target_monitor_entity = app.world_mut().spawn_empty().id();
        let monitors =
            monitors_with_returned_target_entities(readiness_monitor_entity, target_monitor_entity);
        app.insert_resource(monitors)
            .insert_resource(WinitInfo::default())
            .insert_resource(Platform::Windows)
            .init_resource::<CapturedWindowStates>()
            .init_resource::<InjectedCurrentMonitorSource>()
            .add_observer(monitors::install_current_monitor_from_association)
            .add_observer(mark_native_window_ready)
            .add_observer(clear_native_window_ready)
            .add_systems(
                Update,
                (monitors::update_current_monitor, prepare_restore_targets).chain(),
            );
        let entity = app
            .world_mut()
            .spawn((
                Window::default(),
                PrimaryWindow,
                RestorePreparation::startup(WindowKey::Primary),
            ))
            .id();
        app.world_mut()
            .resource_mut::<CapturedWindowStates>()
            .promote(WindowKey::Primary, entity, captured_window_placement);
        app.world_mut()
            .resource_mut::<CapturedWindowStates>()
            .bind_and_freeze(&WindowKey::Primary, entity);
        if matches!(startup_readiness, StartupReadiness::Associated) {
            app.world_mut()
                .entity_mut(entity)
                .insert(OnMonitor(readiness_monitor_entity));
            app.world_mut().flush();
        }
        (app, entity, readiness_monitor_entity)
    }

    #[test]
    fn startup_and_synthetic_runtime_use_equivalent_target_computation() {
        let captured_window_placement = captured_placement(
            MonitorIdentity::Verified(TARGET_ID),
            1,
            CapturedWindowPosition::Restorable {
                logical_offset: CAPTURED_OFFSET,
            },
            SavedWindowMode::Windowed,
        );
        let synthetic_runtime_target = build_target_for_synthetic_runtime(
            &captured_window_placement,
            &monitors_with_returned_target(),
        );
        let (mut app, entity, readiness_monitor_entity) =
            captured_startup_app(captured_window_placement, StartupReadiness::Associated);

        app.update();

        assert_eq!(
            app.world()
                .get::<OnMonitor>(entity)
                .map(|on_monitor| on_monitor.0),
            Some(readiness_monitor_entity)
        );
        let startup_target = app.world().get::<TargetPosition>(entity);
        assert_eq!(
            startup_target.map(TargetSnapshot::from),
            Some(TargetSnapshot::from(&synthetic_runtime_target))
        );
        assert_eq!(
            startup_target.and_then(|target_position| target_position.physical_position),
            Some(IVec2::new(2_200, -100))
        );
        let captured_window_states = app.world().resource::<CapturedWindowStates>();
        assert!(matches!(
            captured_window_states.placement(&WindowKey::Primary),
            Some(CapturedPlacement::Captured(_))
        ));
        assert_eq!(captured_window_states.activity().file_reads, 0);
        assert_eq!(captured_window_states.activity().projections, 0);
        assert_eq!(captured_window_states.activity().writes, 0);
    }

    #[test]
    fn preparation_waits_for_native_window_readiness() {
        let captured_window_placement = captured_placement(
            MonitorIdentity::Verified(TARGET_ID),
            1,
            CapturedWindowPosition::Restorable {
                logical_offset: CAPTURED_OFFSET,
            },
            SavedWindowMode::Windowed,
        );
        let (mut app, entity, readiness_monitor_entity) =
            captured_startup_app(captured_window_placement, StartupReadiness::Waiting);

        app.update();
        app.update();

        assert!(app.world().get::<TargetPosition>(entity).is_none());
        assert!(app.world().get::<NativeWindowReady>(entity).is_none());
        assert_eq!(
            app.world()
                .resource::<InjectedCurrentMonitorSource>()
                .activity(),
            NativeQueryActivity {
                window_map:       0,
                monitor_metadata: 0,
            }
        );

        app.world_mut()
            .entity_mut(entity)
            .insert(OnMonitor(readiness_monitor_entity));
        app.world_mut().flush();

        assert!(app.world().get::<NativeWindowReady>(entity).is_some());
        assert!(app.world().get::<CurrentMonitor>(entity).is_some());
        app.update();

        assert!(app.world().get::<TargetPosition>(entity).is_some());
        assert_eq!(
            app.world()
                .resource::<InjectedCurrentMonitorSource>()
                .activity(),
            NativeQueryActivity {
                window_map:       0,
                monitor_metadata: 0,
            }
        );
    }

    #[test]
    fn recovery_cancellation_finishes_a_hidden_live_window_and_removes_the_restore_attempt() {
        let captured_window_placement = captured_placement(
            MonitorIdentity::Verified(TARGET_ID),
            1,
            CapturedWindowPosition::Restorable {
                logical_offset: CAPTURED_OFFSET,
            },
            SavedWindowMode::Windowed,
        );
        let monitors = monitors_with_returned_target();
        let mut target_position =
            build_target_for_synthetic_runtime(&captured_window_placement, &monitors);
        target_position.settle_state = Some(SettleState::new());
        let restore_diagnostics = RestoreDiagnostics {
            starting_monitor_index: 0,
            starting_scale:         target_position.starting_scale,
            target_scale:           target_position.target_scale,
            monitor_scale_strategy: target_position.monitor_scale_strategy,
        };
        let mut app = App::new();
        app.insert_resource(monitors)
            .insert_resource(MonitorTopologyRevision::default())
            .insert_resource(Platform::Windows)
            .insert_resource(ManagedWindowPersistence::RememberAll)
            .init_resource::<ManagedWindowRegistry>()
            .init_resource::<CapturedWindowStates>()
            .init_resource::<RestoreOutcomeCounts>()
            .init_resource::<Time>()
            .add_message::<WindowScaleFactorChanged>()
            .add_plugins(RecoveryPlugin)
            .add_observer(record_restored)
            .add_observer(record_mismatch)
            .add_systems(
                Update,
                (restore_windows, check_restore_settling)
                    .chain()
                    .run_if(has_restoring_windows),
            );
        let entity = app
            .world_mut()
            .spawn((
                Window {
                    visible: false,
                    ..default()
                },
                PrimaryWindow,
                WindowRecovery::ApplicationControlled,
                RestorePreparation::startup(WindowKey::Primary),
                target_position,
                X11FrameCompensated,
                restore_diagnostics,
            ))
            .id();
        app.world_mut().flush();

        app.world_mut().trigger(CancelWindowRecovery {
            window: WindowKey::Primary,
        });
        app.world_mut().flush();

        assert!(app.world().get_entity(entity).is_ok());
        assert_eq!(
            app.world()
                .get::<Window>(entity)
                .map(|window| window.visible),
            Some(true)
        );
        assert!(app.world().get::<RestorePreparation>(entity).is_none());
        assert!(app.world().get::<TargetPosition>(entity).is_none());
        assert!(app.world().get::<X11FrameCompensated>(entity).is_none());
        assert!(app.world().get::<RestoreDiagnostics>(entity).is_some());

        app.update();
        app.update();
        app.update();

        assert!(app.world().get::<RestorePreparation>(entity).is_none());
        assert!(app.world().get::<TargetPosition>(entity).is_none());
        assert!(app.world().get::<X11FrameCompensated>(entity).is_none());
        let outcomes = app.world().resource::<RestoreOutcomeCounts>();
        assert_eq!(outcomes.restored, 0);
        assert_eq!(outcomes.mismatched, 0);
    }

    #[test]
    fn rejected_monitor_association_clears_stale_readiness() {
        let captured_window_placement = captured_placement(
            MonitorIdentity::Verified(TARGET_ID),
            1,
            CapturedWindowPosition::Restorable {
                logical_offset: CAPTURED_OFFSET,
            },
            SavedWindowMode::Windowed,
        );
        let (mut app, entity, _) =
            captured_startup_app(captured_window_placement, StartupReadiness::Associated);
        assert!(app.world().get::<NativeWindowReady>(entity).is_some());
        assert!(app.world().get::<CurrentMonitor>(entity).is_some());

        let unresolved_monitor_entity = app.world_mut().spawn_empty().id();
        app.world_mut()
            .entity_mut(entity)
            .insert(OnMonitor(unresolved_monitor_entity));
        app.world_mut().flush();

        assert!(app.world().get::<NativeWindowReady>(entity).is_none());
        assert!(app.world().get::<CurrentMonitor>(entity).is_none());
        app.update();
        assert!(app.world().get::<TargetPosition>(entity).is_none());
        assert_eq!(
            app.world()
                .resource::<InjectedCurrentMonitorSource>()
                .activity(),
            NativeQueryActivity {
                window_map:       0,
                monitor_metadata: 0,
            }
        );
    }

    #[test]
    fn removed_monitor_association_clears_stale_readiness() {
        let captured_window_placement = captured_placement(
            MonitorIdentity::Verified(TARGET_ID),
            1,
            CapturedWindowPosition::Restorable {
                logical_offset: CAPTURED_OFFSET,
            },
            SavedWindowMode::Windowed,
        );
        let (mut app, entity, _) =
            captured_startup_app(captured_window_placement, StartupReadiness::Associated);
        assert!(app.world().get::<NativeWindowReady>(entity).is_some());
        assert!(app.world().get::<CurrentMonitor>(entity).is_some());

        app.world_mut().entity_mut(entity).remove::<OnMonitor>();
        app.world_mut().flush();

        assert!(app.world().get::<NativeWindowReady>(entity).is_none());
        assert!(app.world().get::<CurrentMonitor>(entity).is_none());
    }

    #[test]
    fn compositor_controlled_capture_never_prepares_a_coordinate() {
        let captured_window_placement = captured_placement(
            MonitorIdentity::Verified(TARGET_ID),
            1,
            CapturedWindowPosition::CompositorControlled,
            SavedWindowMode::Windowed,
        );
        let target_position = build_target_for_synthetic_runtime(
            &captured_window_placement,
            &monitors_with_returned_target(),
        );

        assert_eq!(target_position.monitor_index, 2);
        assert_eq!(target_position.physical_position, None);
        assert_eq!(target_position.logical_position, None);
    }

    #[test]
    fn x11_fullscreen_prestartup_flushes_target_before_monitor_move() {
        let monitors = monitors_with_returned_target();
        let starting_monitor = *monitors.first();
        let captured_window_placement = captured_placement(
            MonitorIdentity::Verified(TARGET_ID),
            1,
            CapturedWindowPosition::Restorable {
                logical_offset: CAPTURED_OFFSET,
            },
            SavedWindowMode::BorderlessFullscreen,
        );
        let mut app = App::new();
        app.insert_resource(monitors)
            .insert_resource(WinitInfo::default())
            .insert_resource(Platform::X11)
            .init_resource::<CapturedWindowStates>()
            .add_plugins(RestorePlugin);
        let entity = app
            .world_mut()
            .spawn((
                Window::default(),
                PrimaryWindow,
                CurrentMonitor {
                    monitor_info:          starting_monitor,
                    effective_window_mode: WindowMode::Windowed,
                },
                NativeWindowReady,
            ))
            .id();
        app.world_mut()
            .resource_mut::<CapturedWindowStates>()
            .promote(WindowKey::Primary, entity, captured_window_placement);

        app.world_mut().run_schedule(PreStartup);

        let target_position = app.world().get::<TargetPosition>(entity);
        assert_eq!(
            target_position.and_then(|target_position| target_position.physical_position),
            Some(IVec2::new(2_200, -100))
        );
        assert_eq!(
            app.world()
                .get::<Window>(entity)
                .map(|window| window.position),
            Some(WindowPosition::At(IVec2::new(2_200, -100)))
        );
        assert!(app.world().get::<X11FrameCompensated>(entity).is_some());
    }

    #[test]
    fn persisted_adapter_fallback_is_coordinate_free() {
        let persisted_window_state = PersistedWindowState {
            logical_position:  Some((1_000, 200)),
            logical_width:     800,
            logical_height:    600,
            scale:             1.0,
            monitor:           4,
            saved_window_mode: SavedWindowMode::Windowed,
            app_name:          "test".to_string(),
        };
        let monitors = Monitors::from_test_monitors([(
            Entity::from_bits(1),
            monitor(MonitorIdentity::Unverified, 0, 2.0, IVec2::ZERO),
        )]);
        let source = CapturedPlacement::PersistedOnly(persisted_window_state);
        let prepared_restore = RestoreTargetBuilder {
            source:              &source,
            monitors:            &monitors,
            physical_decoration: UVec2::ZERO,
            starting_scale:      1.0,
            platform:            Platform::Windows,
        }
        .build();

        assert_eq!(
            prepared_restore.monitor_resolution_source,
            MonitorResolutionSource::FallbackToPrimary
        );
        assert_eq!(prepared_restore.target_position.monitor_index, 0);
        assert_eq!(prepared_restore.target_position.physical_position, None);
    }

    #[test]
    fn primary_preparation_reads_seeded_state_without_another_file_read() {
        let mut captured_window_states = CapturedWindowStates::default();
        captured_window_states.seed(HashMap::from([(
            WindowKey::Primary,
            PersistedWindowState {
                logical_position:  Some((10, 20)),
                logical_width:     800,
                logical_height:    600,
                scale:             1.0,
                monitor:           0,
                saved_window_mode: SavedWindowMode::Windowed,
                app_name:          "test".to_string(),
            },
        )]));

        assert!(matches!(
            captured_window_states.placement(&WindowKey::Primary),
            Some(CapturedPlacement::PersistedOnly(_))
        ));
        assert_eq!(captured_window_states.activity().file_reads, 0);
    }
}
