//! Public API events for window restoration.

use bevy::prelude::*;
use bevy::window::WindowMode;

use super::WindowKey;
use super::monitors::MonitorId;
use super::monitors::MonitorInfo;

/// A registered window's verified target monitor is no longer installed.
#[derive(Event, Debug, Clone, Reflect)]
#[reflect(Event)]
#[type_path = "bevy_clerestory::recovery"]
pub struct WindowRecoveryPending {
    /// Canonical primary or managed persistence key.
    pub window_key: WindowKey,
    /// Process-local verified identity of the absent target monitor.
    pub monitor_id: MonitorId,
}

/// A registered window's exact verified target monitor is installed again.
#[derive(Event, Debug, Clone, Reflect)]
#[reflect(Event)]
#[type_path = "bevy_clerestory::recovery"]
pub struct WindowRecoveryAvailable {
    /// Canonical primary or managed persistence key.
    pub window_key: WindowKey,
    /// Current entity-free snapshot for the returned target monitor.
    pub monitor:    MonitorInfo,
}

/// Request restoration of one application-controlled window entity.
#[derive(EntityEvent, Debug, Clone, Reflect)]
#[reflect(Event)]
#[type_path = "bevy_clerestory::recovery"]
pub struct RestoreWindow {
    /// Existing or application-created canonical window entity.
    pub entity: Entity,
}

/// Cancel the current recovery generation for one canonical window key.
#[derive(Event, Debug, Clone, Reflect)]
#[reflect(Event)]
#[type_path = "bevy_clerestory::recovery"]
pub struct CancelWindowRecovery {
    /// Canonical primary or managed persistence key.
    pub window: WindowKey,
}

/// Event fired when a window restore completes and the window becomes visible.
///
/// This is an [`EntityEvent`] triggered on the window entity at the end of the restore
/// process, after position, size, and window mode have been applied. Dependent crates can
/// observe this event to know the final restored window state.
///
/// Use an observer to receive this event:
/// ```ignore
/// // For all windows
/// app.add_observer(|trigger: On<WindowRestored>| {
///     let event = trigger.event();
///     // Use `event.entity`, `event.physical_size`, `event.window_mode`, etc.
/// });
///
/// // For primary window only - check event.entity against PrimaryWindow query
/// fn on_window_restored(
///     trigger: On<WindowRestored>,
///     primary_window: Query<(), With<PrimaryWindow>>,
/// ) {
///     let event = trigger.event();
///     if primary_window.get(event.entity).is_ok() {
///         // Handle primary window only
///     }
/// }
/// ```
#[derive(EntityEvent, Debug, Clone, Reflect)]
#[reflect(Event)]
pub struct WindowRestored {
    /// The window entity this event targets.
    pub entity:            Entity,
    /// Identifier for this window (primary or managed name).
    pub window_key:        WindowKey,
    /// Target position in physical pixels (None on Wayland).
    pub physical_position: Option<IVec2>,
    /// Target position in logical pixels (pre-scale, from the saved state).
    /// None on Wayland or when the saved state had no position.
    pub logical_position:  Option<IVec2>,
    /// Target physical size that was applied (content area).
    pub physical_size:     UVec2,
    /// Target logical size that was applied (content area).
    pub logical_size:      UVec2,
    /// Window mode that was applied.
    pub window_mode:       WindowMode,
    /// Monitor index the window was restored to.
    pub monitor_index:     usize,
}

/// Event fired when the actual window state doesn't match what was requested.
///
/// After `try_apply_restore` completes, the library compares the intended restore
/// target against the live window state. If any field differs, this event fires
/// instead of [`WindowRestored`].
///
/// ## Sources
///
/// **Expected values** come from `TargetPosition`, which is computed
/// from the saved RON state file at startup. These represent what the restore *intended* to
/// achieve.
///
/// **Actual values** come from two live ECS sources, each chosen for accuracy:
///
/// - **`monitor_index`** → [`CurrentMonitor`](crate::CurrentMonitor) component, maintained by
///   `update_current_monitor`, which queries winit's `current_monitor()` and maps it to the
///   `Monitors` list. This updates quickly when the compositor moves the window.
///
/// - **`physical_position`, `logical_position`, `physical_size`, `logical_size`, `window_mode`,
///   `scale`** → the [`Window`](bevy::window::Window) component. Position and size reflect
///   `Window.position` / `Window.resolution`, and scale comes from
///   `Window.resolution.scale_factor()`. These lag behind the compositor because they only update
///   when winit fires corresponding events (`ScaleFactorChanged`, `Resized`, `Moved`). A common
///   mismatch is the scale factor still reflecting the launch monitor while `CurrentMonitor` has
///   already updated to the target monitor.
///
/// This intentional split means a mismatch signals that the window hasn't fully settled
/// — the compositor accepted the request but winit hasn't yet delivered all the
/// resulting state changes.
///
/// ## Field layout
///
/// The `expected_*` / `actual_*` pairs are deliberately flat rather than grouped into
/// nested comparison structs — the event is primarily consumed via reflection (BRP /
/// observers), where flat fields are easier to address than nested ones. The
/// `restore_window` example adapts this flat shape into nested `*Mismatch` types in
/// `examples/restore_window/events.rs`; any future reshape of the fields here must
/// update that adapter in tandem.
#[derive(EntityEvent, Debug, Clone, Reflect)]
#[reflect(Event)]
pub struct WindowRestoreMismatch {
    /// The window entity this event targets.
    pub entity:                     Entity,
    /// Identifier for this window (primary or managed name).
    pub window_key:                 WindowKey,
    /// Target physical position from `TargetPosition` (None on Wayland).
    pub expected_physical_position: Option<IVec2>,
    /// Actual physical position from `Window.position` (None on Wayland).
    pub actual_physical_position:   Option<IVec2>,
    /// Target logical position from the saved state (None on Wayland or when unsaved).
    pub expected_logical_position:  Option<IVec2>,
    /// Actual logical position, derived from `Window.position / actual_scale`.
    /// None on Wayland.
    pub actual_logical_position:    Option<IVec2>,
    /// Target physical size from `TargetPosition`.
    pub expected_physical_size:     UVec2,
    /// Actual physical size from `Window.resolution`.
    pub actual_physical_size:       UVec2,
    /// Expected logical size from `TargetPosition`.
    pub expected_logical_size:      UVec2,
    /// Actual logical size from `Window.resolution.width()`/`height()`.
    pub actual_logical_size:        UVec2,
    /// Target window mode from `TargetPosition`.
    pub expected_window_mode:       WindowMode,
    /// Actual window mode from `Window.mode`.
    pub actual_window_mode:         WindowMode,
    /// Target monitor index from `TargetPosition`.
    pub expected_monitor:           usize,
    /// Actual monitor index from `CurrentMonitor` (winit `current_monitor()`).
    pub actual_monitor:             usize,
    /// Target scale factor from `TargetPosition.target_scale`.
    pub expected_scale:             f64,
    /// Actual scale factor from `Window.resolution.scale_factor()`.
    /// Lags behind monitor changes; updates only on winit `ScaleFactorChanged`.
    pub actual_scale:               f64,
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use bevy::ecs::reflect::AppTypeRegistry;
    use bevy::ecs::reflect::ReflectEvent;
    use bevy::ecs::system::In;
    use bevy::reflect::TypePath;
    use bevy::reflect::TypeRegistry;
    use bevy_remote::builtin_methods::process_remote_observe_watching_request;
    use serde_json::Value;
    use serde_json::json;

    use super::*;
    use crate::MonitorConnected;
    use crate::MonitorDisconnected;
    use crate::MonitorIdentity;

    fn has_reflect_event<T: 'static>(registry: &TypeRegistry) -> bool {
        registry
            .get(TypeId::of::<T>())
            .and_then(|registration| registration.data::<ReflectEvent>())
            .is_some()
    }

    fn observe_event<T>(app: &mut App, event: T) -> Value
    where
        T: Event + Reflect + TypePath,
        for<'a> T::Trigger<'a>: Default,
    {
        let observe_params = json!({
            "event": T::type_path(),
            "entity": null,
        });
        assert_eq!(
            process_remote_observe_watching_request(
                In(Some(observe_params.clone())),
                app.world_mut(),
            ),
            Ok(None)
        );

        app.world_mut().trigger(event);

        let observed =
            process_remote_observe_watching_request(In(Some(observe_params)), app.world_mut());
        assert!(matches!(observed, Ok(Some(_))), "observed: {observed:?}");
        match observed {
            Ok(Some(observed)) => {
                let events = observed.as_array();
                assert_eq!(events.map(Vec::len), Some(1));
                events
                    .and_then(|events| events.first())
                    .cloned()
                    .unwrap_or(Value::Null)
            },
            Ok(None) | Err(_) => Value::Null,
        }
    }

    fn assert_public_fields(event: &Value, expected: &[&str]) {
        let fields = event.as_object();
        assert_eq!(fields.map(serde_json::Map::len), Some(expected.len()));
        assert!(
            expected
                .iter()
                .all(|field| fields.is_some_and(|fields| fields.contains_key(*field)))
        );
    }

    fn assert_monitor_fields(event: &Value) {
        assert_public_fields(
            event.get("monitor").unwrap_or(&Value::Null),
            &[
                "identity",
                "index",
                "scale",
                "physical_position",
                "physical_size",
            ],
        );
    }

    fn monitor_info(id: u64, index: usize) -> MonitorInfo {
        MonitorInfo {
            identity: MonitorIdentity::Verified(MonitorId::from_test_raw(id)),
            index,
            scale: 2.0,
            physical_position: IVec2::new(1_920, 120),
            physical_size: UVec2::new(2_560, 1_440),
        }
    }

    #[test]
    fn restore_result_type_paths_preserve_events_namespace() {
        assert_eq!(
            [
                <WindowRestored as TypePath>::type_path(),
                <WindowRestoreMismatch as TypePath>::type_path(),
            ],
            [
                "bevy_clerestory::events::WindowRestored",
                "bevy_clerestory::events::WindowRestoreMismatch",
            ]
        );
    }

    #[test]
    fn public_window_events_auto_register_reflected_event_type_data() {
        let app = App::new();
        let registry = app.world().resource::<AppTypeRegistry>().read();

        assert!(has_reflect_event::<WindowRecoveryPending>(&registry));
        assert!(has_reflect_event::<WindowRecoveryAvailable>(&registry));
        assert!(has_reflect_event::<RestoreWindow>(&registry));
        assert!(has_reflect_event::<CancelWindowRecovery>(&registry));
        assert!(has_reflect_event::<WindowRestored>(&registry));
        assert!(has_reflect_event::<WindowRestoreMismatch>(&registry));
    }

    fn assert_recovery_event_observations(app: &mut App) {
        let pending = observe_event(
            app,
            WindowRecoveryPending {
                window_key: WindowKey::Primary,
                monitor_id: MonitorId::from_test_raw(17),
            },
        );
        assert_public_fields(&pending, &["window_key", "monitor_id"]);
        assert_eq!(pending.get("window_key"), Some(&json!("Primary")));
        assert_eq!(pending.get("monitor_id"), Some(&json!(17)));

        let available = observe_event(
            app,
            WindowRecoveryAvailable {
                window_key: WindowKey::Managed("inspector".to_string()),
                monitor:    monitor_info(19, 3),
            },
        );
        assert_public_fields(&available, &["window_key", "monitor"]);
        assert_monitor_fields(&available);
        assert_eq!(available.pointer("/monitor/index"), Some(&json!(3)));
    }

    fn assert_restore_event_observations(app: &mut App) {
        let restored_entity = app.world_mut().spawn_empty().id();
        let restored = observe_event(
            app,
            WindowRestored {
                entity:            restored_entity,
                window_key:        WindowKey::Managed("inspector".to_string()),
                physical_position: Some(IVec2::new(20, 40)),
                logical_position:  Some(IVec2::new(10, 20)),
                physical_size:     UVec2::new(1_600, 1_200),
                logical_size:      UVec2::new(800, 600),
                window_mode:       WindowMode::Windowed,
                monitor_index:     2,
            },
        );
        assert_public_fields(
            &restored,
            &[
                "entity",
                "window_key",
                "physical_position",
                "logical_position",
                "physical_size",
                "logical_size",
                "window_mode",
                "monitor_index",
            ],
        );
        assert_eq!(
            restored.get("entity"),
            Some(&json!(restored_entity.to_bits()))
        );
        assert_eq!(restored.get("monitor_index"), Some(&json!(2)));

        let mismatch_entity = app.world_mut().spawn_empty().id();
        let mismatch = observe_event(
            app,
            WindowRestoreMismatch {
                entity:                     mismatch_entity,
                window_key:                 WindowKey::Managed("dashboard".to_string()),
                expected_physical_position: Some(IVec2::new(200, 400)),
                actual_physical_position:   Some(IVec2::new(220, 440)),
                expected_logical_position:  Some(IVec2::new(100, 200)),
                actual_logical_position:    Some(IVec2::new(110, 220)),
                expected_physical_size:     UVec2::new(1_600, 1_200),
                actual_physical_size:       UVec2::new(1_920, 1_080),
                expected_logical_size:      UVec2::new(800, 600),
                actual_logical_size:        UVec2::new(960, 540),
                expected_window_mode:       WindowMode::Windowed,
                actual_window_mode:         WindowMode::Windowed,
                expected_monitor:           2,
                actual_monitor:             4,
                expected_scale:             2.0,
                actual_scale:               1.5,
            },
        );
        assert_public_fields(
            &mismatch,
            &[
                "entity",
                "window_key",
                "expected_physical_position",
                "actual_physical_position",
                "expected_logical_position",
                "actual_logical_position",
                "expected_physical_size",
                "actual_physical_size",
                "expected_logical_size",
                "actual_logical_size",
                "expected_window_mode",
                "actual_window_mode",
                "expected_monitor",
                "actual_monitor",
                "expected_scale",
                "actual_scale",
            ],
        );
        assert_eq!(mismatch.get("actual_monitor"), Some(&json!(4)));
    }

    fn assert_monitor_event_observations(app: &mut App) {
        let connected_entity = app.world_mut().spawn_empty().id();
        let connected = observe_event(
            app,
            MonitorConnected {
                entity:  connected_entity,
                monitor: monitor_info(23, 1),
            },
        );
        assert_public_fields(&connected, &["entity", "monitor"]);
        assert_monitor_fields(&connected);
        assert_eq!(
            connected.get("entity"),
            Some(&json!(connected_entity.to_bits()))
        );

        let former_entity = app.world_mut().spawn_empty().id();
        let disconnected = observe_event(
            app,
            MonitorDisconnected {
                former_entity,
                monitor: monitor_info(29, 5),
            },
        );
        assert_public_fields(&disconnected, &["former_entity", "monitor"]);
        assert_monitor_fields(&disconnected);
        assert_eq!(
            disconnected.get("former_entity"),
            Some(&json!(former_entity.to_bits()))
        );
        assert_eq!(disconnected.pointer("/monitor/index"), Some(&json!(5)));
    }

    #[test]
    fn remote_observation_serializes_recovery_restore_and_monitor_events() {
        let mut app = App::new();

        assert_recovery_event_observations(&mut app);
        assert_restore_event_observations(&mut app);
        assert_monitor_event_observations(&mut app);
    }
}
