// components
pub(super) const COMPONENT_HAS_WINDOWS: &str = "HasWindows";
pub(super) const COMPONENT_MANAGED_WINDOW: &str = "ManagedWindow";
pub(super) const COMPONENT_MONITOR: &str = "Monitor";
pub(super) const COMPONENT_ON_MONITOR: &str = "OnMonitor";
pub(super) const COMPONENT_WINDOW: &str = "Window";

// fields
pub(super) const FIELD_ACCEPTED_RECORDS: &str = "accepted_records";
pub(super) const FIELD_ACTUAL: &str = "actual";
pub(super) const FIELD_ARMING_STATE: &str = "arming_state";
pub(super) const FIELD_COMPONENT: &str = "component";
pub(super) const FIELD_CURRENT_MONITOR: &str = "current_monitor";
pub(super) const FIELD_ENTITY: &str = "entity";
pub(super) const FIELD_EXPECTED: &str = "expected";
pub(super) const FIELD_HAS_WINDOWS: &str = "has_windows";
pub(super) const FIELD_MONITOR: &str = "monitor";
pub(super) const FIELD_MONITOR_ENTITY: &str = "monitor_entity";
pub(super) const FIELD_NATIVE_CURRENT_MONITOR_STATE: &str = "native_current_monitor_state";
pub(super) const FIELD_NATIVE_MATCHED_ENTITY: &str = "native_matched_entity";
pub(super) const FIELD_NATIVE_MATCHED_IDENTITY: &str = "native_matched_identity";
pub(super) const FIELD_PHASE: &str = "phase";
pub(super) const FIELD_PLACEMENT_CAPABILITY: &str = "placement_capability";
pub(super) const FIELD_PLATFORM: &str = "platform";
pub(super) const FIELD_RECOVERY_CYCLE: &str = "recovery_cycle";
pub(super) const FIELD_RECOVERY_POLICY: &str = "recovery_policy";
pub(super) const FIELD_RECOVERY_REASON: &str = "recovery_reason";
pub(super) const FIELD_SELECTED_MONITOR_INDEX: &str = "selected_monitor_index";
pub(super) const FIELD_TOPOLOGY_REVISION: &str = "topology_revision";
pub(super) const FIELD_TRANSITION: &str = "transition";
pub(super) const FIELD_WINDOW: &str = "window";
pub(super) const FIELD_WINDOW_KEY: &str = "window_key";
pub(super) const FIELD_WINDOW_MODE: &str = "window_mode";
pub(super) const FIELD_WINDOW_POSITION: &str = "window_position";
pub(super) const FIELD_WINDOW_SIZE: &str = "window_size";
pub(super) const FIELD_WINDOW_TITLE: &str = "window_title";

// kinds
pub(super) const KIND_CLOSE_INTENT: &str = "close-intent";
pub(super) const KIND_COMPONENT_LIFECYCLE: &str = "component-lifecycle";
pub(super) const KIND_CONTENT_ATTACHED: &str = "content-attached";
pub(super) const KIND_CONTROL_ASSOCIATION_CONFIRMED: &str = "control-association-confirmed";
pub(super) const KIND_ENTITY_REMOVAL: &str = "entity-removal";
pub(super) const KIND_MONITOR_CONNECTED: &str = "monitor-connected";
pub(super) const KIND_MONITOR_DISCONNECTED: &str = "monitor-disconnected";
pub(super) const KIND_MONITOR_TOPOLOGY: &str = "monitor-topology";
pub(super) const KIND_PRE_UNPLUG_ASSOCIATION: &str = "pre-unplug-association";
pub(super) const KIND_PROBE_SESSION: &str = "probe-session";
pub(super) const KIND_RECOVERY_ACCEPTED: &str = "recovery-accepted";
pub(super) const KIND_RECOVERY_AVAILABLE: &str = "recovery-available";
pub(super) const KIND_RECOVERY_CANCELLATION_REQUESTED: &str = "recovery-cancellation-requested";
pub(super) const KIND_RECOVERY_MISMATCH: &str = "recovery-mismatch";
pub(super) const KIND_RECOVERY_PENDING: &str = "recovery-pending";
pub(super) const KIND_RECOVERY_READY: &str = "recovery-ready";
pub(super) const KIND_RECOVERY_RESTORE_REQUESTED: &str = "recovery-restore-requested";
pub(super) const KIND_RECOVERY_RESTORED: &str = "recovery-restored";
pub(super) const KIND_RECOVERY_UNARMED: &str = "recovery-unarmed";
pub(super) const KIND_WINDOW_COMPONENT_CHANGED: &str = "window-component-changed";
pub(super) const KIND_WINDOW_CLOSED: &str = "window-closed";
pub(super) const KIND_WINDOW_CLOSING: &str = "window-closing";
pub(super) const KIND_WINDOW_CREATED: &str = "window-created";
pub(super) const KIND_WINDOW_DESTROYED: &str = "window-destroyed";
pub(super) const KIND_WINDOW_MODE_CHANGED: &str = "window-mode-changed";
pub(super) const KIND_WINDOW_MOVED: &str = "window-moved";
pub(super) const KIND_WINDOW_RESIZED: &str = "window-resized";

// phases
pub(super) const PHASE_ADD: &str = "add";
pub(super) const PHASE_DISCARD: &str = "discard";
pub(super) const PHASE_DESPAWN: &str = "despawn";
pub(super) const PHASE_INSERT: &str = "insert";
pub(super) const PHASE_REMOVE: &str = "remove";

// probe configuration
pub(super) const DEFAULT_EXTERNAL_MONITOR_INDEX: usize = 1;
pub(super) const EXIT_AFTER_FRAME_ENVIRONMENT_VARIABLE: &str = "CLERESTORY_PROBE_EXIT_AFTER_FRAME";
pub(super) const MONITOR_INDEX_ENVIRONMENT_VARIABLE: &str = "CLERESTORY_PROBE_MONITOR_INDEX";
pub(super) const MONITOR_PROBE_TARGET: &str = "bevy_clerestory::monitor_probe";
pub(super) const PERSISTENCE_FILE_PREFIX: &str = "bevy-clerestory-hotplug-probe";
pub(super) const RECOVERY_PROBE_TARGET: &str = "bevy_clerestory::recovery_probe";
pub(super) const SECOND_RECOVERY_CYCLE: usize = 2;

// producers
pub(super) const PRODUCER_MONITOR_CONNECTED: &str = "observer::MonitorConnected";
pub(super) const PRODUCER_MONITOR_DISCONNECTED: &str = "observer::MonitorDisconnected";
pub(super) const PRODUCER_POST_UPDATE_WINDOWS: &str = "PostUpdate::trace_window_component_changes";
pub(super) const PRODUCER_CONTROL_ASSOCIATION: &str =
    "Update::place_and_confirm_unregistered_control";
pub(super) const PRODUCER_RECOVERY_AVAILABLE: &str = "observer::WindowRecoveryAvailable";
pub(super) const PRODUCER_APPLICATION_RECOVERY_CANCELLATION_REQUESTED: &str =
    "observer::WindowRecoveryPending::application_consumer";
pub(super) const PRODUCER_AUTOMATIC_RECOVERY_CANCELLATION_REQUESTED: &str =
    "Update::cancel_automatic_window_recovery";
pub(super) const PRODUCER_RECOVERY_MISMATCH: &str = "observer::WindowRestoreMismatch";
pub(super) const PRODUCER_RECOVERY_PENDING: &str = "observer::WindowRecoveryPending";
pub(super) const PRODUCER_RECOVERY_READY: &str = "Update::record_recovery_readiness";
pub(super) const PRODUCER_RECOVERY_RESTORE_REQUESTED: &str =
    "PreUpdate::request_application_window_restore";
pub(super) const PRODUCER_RECOVERY_RESTORED: &str = "observer::WindowRestored";
pub(super) const PRODUCER_SETUP_CONTENT: &str = "PreUpdate::attach_window_content";
pub(super) const PRODUCER_STARTUP_SESSION: &str = "Startup::trace_probe_session";
pub(super) const PRODUCER_UPDATE_INTERNAL_WINDOW_MESSAGES: &str =
    "Update::trace_internal_window_messages";
pub(super) const PRODUCER_UPDATE_OS_WINDOW_EVENTS: &str = "Update::trace_os_window_events";

// trace fields
pub(super) const TRACE_FIELD_FRAME_COUNT: &str = "frame_count";
pub(super) const TRACE_FIELD_PRODUCER_SCHEDULE: &str = "producer_schedule";

// transitions
pub(super) const TRANSITION_CREATED: &str = "created";
pub(super) const TRANSITION_REMOVED: &str = "removed";

// values
pub(super) const VALUE_ARMED: &str = "armed";
pub(super) const VALUE_CURRENT_MONITOR_HANDLE_RETURNED: &str = "current-monitor-handle-returned";
pub(super) const VALUE_CURRENT_MONITOR_NO_HANDLE: &str = "current-monitor-no-handle";
pub(super) const VALUE_NATIVE_WINDOW_UNAVAILABLE: &str = "native-window-unavailable";
pub(super) const VALUE_UNARMED: &str = "unarmed";
pub(super) const VALUE_UNARMED_UNVERIFIED: &str = "unverified-monitor-identity";
pub(super) const VALUE_UNARMED_WAYLAND_WINDOWED: &str = "wayland-windowed-placement";
pub(super) const VALUE_UNRESOLVED: &str = "unresolved";
pub(super) const VALUE_UNBOUND: &str = "unbound";

// windows
pub(super) const APPLICATION_WINDOW_KEY: &str = "hotplug-application";
pub(super) const APPLICATION_WINDOW_TITLE: &str =
    "Clerestory Reconnect Consumer - Application Controlled";
pub(super) const AUTOMATIC_WINDOW_KEY: &str = "hotplug-automatic";
pub(super) const AUTOMATIC_WINDOW_TITLE: &str = "Clerestory Reconnect Consumer - Managed Automatic";
pub(super) const CONTENT_FONT_SIZE: f32 = 28.0;
pub(super) const CONTENT_MARGIN: f32 = 24.0;
pub(super) const CONTROL_WINDOW_TITLE: &str =
    "Clerestory Reconnect Consumer - Unregistered Control";
pub(super) const PRIMARY_WINDOW_TITLE: &str = "Clerestory Reconnect Consumer - Primary Automatic";
pub(super) const PROBE_WINDOW_HEIGHT: u32 = 540;
pub(super) const PROBE_WINDOW_WIDTH: u32 = 800;
