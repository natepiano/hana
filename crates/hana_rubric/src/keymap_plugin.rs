//! Keymap plugin configuration and system ordering.

use bevy::app::App;
use bevy::app::Plugin;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::SystemSet;
use bevy::input::InputSystems;
use bevy::prelude::PreUpdate;
use bevy::prelude::ResMut;
use bevy::prelude::Resource;
use bevy::prelude::States;
use bevy_enhanced_input::prelude::EnhancedInputSystems;

use crate::ActiveCondition;
use crate::CommandRegistry;
use crate::Diagnostic;
use crate::DiagnosticKind;
use crate::DiagnosticSeverity;
use crate::KeymapContext;
use crate::KeymapLoadFailures;
use crate::Keystroke;
use crate::condition::ConditionRegistry;
use crate::condition::ResourceContextPlugin;
use crate::condition::StateContextPlugin;
use crate::disk::DiskWorkerChannels;
use crate::disk::DiskWorkerMessage;
use crate::disk::KeymapPaths;
use crate::disk::start_disk_worker;
use crate::keymap;

const EMBEDDED_DEFAULTS_SOURCE: &str = "embedded defaults";

/// Registers the application's keymap context enum.
///
/// Add this plugin directly for global-only bindings, or construct it with [`Self::new`] and
/// choose a context source with [`Self::for_context`] or [`Self::for_state_context`].
pub struct KeymapPlugin {
    app_name:             Option<String>,
    defaults:             Option<&'static str>,
    protected_keystrokes: Vec<Keystroke>,
}

impl KeymapPlugin {
    /// Starts a keymap plugin configuration.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            app_name:             None,
            defaults:             None,
            protected_keystrokes: Vec::new(),
        }
    }

    /// Sets the application name used to resolve the keymap configuration directory.
    #[must_use]
    pub fn with_app_name(mut self, app_name: &str) -> Self {
        self.app_name = Some(app_name.to_owned());
        self
    }

    /// Sets the application-embedded default JSONC keymap.
    #[must_use]
    pub const fn with_defaults(mut self, defaults: &'static str) -> Self {
        self.defaults = Some(defaults);
        self
    }

    /// Reserves a recovery keystroke from user-authored bindings.
    #[must_use]
    pub fn with_protected_keystroke(mut self, keystroke: Keystroke) -> Self {
        self.protected_keystrokes.push(keystroke);
        self
    }

    /// Registers a resource-backed context enum and synchronizes its active condition.
    #[must_use]
    pub fn for_context<C>(self) -> impl Plugin
    where
        C: KeymapContext + Resource,
    {
        ResourceContextPlugin::<C>::new(self)
    }

    /// Registers a state-backed context enum and synchronizes its active condition.
    #[must_use]
    pub fn for_state_context<C>(self) -> impl Plugin
    where
        C: KeymapContext + States,
    {
        StateContextPlugin::<C>::new(self)
    }

    pub(crate) fn install(&self, app: &mut App) {
        Self::install_runtime(app);
        if app.world().contains_resource::<KeymapPluginConfiguration>() || !self.has_configuration()
        {
            return;
        }

        app.insert_resource(KeymapPluginConfiguration::from(self));
    }

    pub(crate) fn install_runtime(app: &mut App) {
        if app.world().contains_resource::<KeymapRuntimeInstalled>() {
            return;
        }

        app.init_resource::<KeymapRuntimeInstalled>()
            .init_resource::<ConditionRegistry>()
            .init_resource::<ActiveCondition>()
            .init_resource::<KeymapLoadFailures>()
            .init_resource::<keymap::PendingReload>()
            .init_resource::<keymap::runtime::KeymapRuntime>()
            .configure_sets(
                PreUpdate,
                (KeymapSystems::UpdateActiveCondition, KeymapSystems::Route).chain(),
            )
            .add_systems(
                PreUpdate,
                (collect_disk_reload, keymap::commit_reload)
                    .chain()
                    .before(KeymapSystems::Route),
            )
            .add_systems(
                PreUpdate,
                keymap::route_input
                    .in_set(KeymapSystems::Route)
                    .after(InputSystems)
                    .before(EnhancedInputSystems::Update),
            );
        app.world_mut()
            .resource_mut::<ActiveCondition>()
            .enable_global();
    }

    pub(crate) fn finish_assembly(app: &mut App) {
        #[cfg(test)]
        increment_finish_attempts(app);

        if app.world().contains_resource::<KeymapAssemblyFinished>() {
            return;
        }
        let Some(keymap_plugin_configuration) = app
            .world()
            .get_resource::<KeymapPluginConfiguration>()
            .cloned()
        else {
            return;
        };
        let Some(defaults) = keymap_plugin_configuration.defaults else {
            return;
        };

        app.insert_resource(KeymapAssemblyFinished);
        #[cfg(test)]
        increment_assembly_runs(app);
        let command_registry = match CommandRegistry::initialize(app.world_mut()) {
            Ok(command_registry) => command_registry,
            Err(diagnostics) => {
                app.insert_resource(RegistryValidationFailed);
                record_startup_diagnostics(app, &diagnostics);
                CommandRegistry::empty()
            },
        };
        app.world_mut().insert_resource(command_registry);

        let published_defaults = {
            let world = app.world();
            let condition_registry = world.resource::<ConditionRegistry>();
            String::from_utf8_lossy(&keymap::reference_default_bytes(
                defaults,
                condition_registry,
                &keymap_plugin_configuration.protected_keystrokes,
            ))
            .into_owned()
        };
        let allow_default_failures = app.world().contains_resource::<RegistryValidationFailed>();
        app.world_mut()
            .insert_resource(keymap::ReloadConfiguration::new(
                EMBEDDED_DEFAULTS_SOURCE.to_owned(),
                published_defaults.clone(),
                keymap_plugin_configuration.protected_keystrokes,
                allow_default_failures,
            ));

        if !keymap::commit_defaults(app.world_mut()) {
            return;
        }

        let Some(app_name) = keymap_plugin_configuration.app_name.as_deref() else {
            record_startup_diagnostics(
                app,
                &[startup_diagnostic(
                    EMBEDDED_DEFAULTS_SOURCE,
                    DiagnosticKind::Disk,
                    "Could not resolve a configuration directory because no keymap application name was configured.",
                )],
            );
            return;
        };
        let Some(paths) = KeymapPaths::new(app_name) else {
            record_startup_diagnostics(
                app,
                &[startup_diagnostic(
                    app_name,
                    DiagnosticKind::Disk,
                    "Could not resolve a configuration directory for the keymap application.",
                )],
            );
            return;
        };
        let schema_result = {
            let world = app.world();
            keymap::schema_bytes(
                world.resource::<CommandRegistry>(),
                world.resource::<ConditionRegistry>(),
            )
        };
        let schema = match schema_result {
            Ok(schema) => Some(schema),
            Err(error) => {
                record_startup_diagnostics(
                    app,
                    &[startup_diagnostic(
                        &paths.schema().display().to_string(),
                        DiagnosticKind::Companion,
                        &format!("Could not generate the keymap schema: {error}"),
                    )],
                );
                None
            },
        };
        app.world_mut().insert_resource(KeymapDiskWorker {
            disk_worker_channels: start_disk_worker(
                &paths,
                published_defaults.into_bytes(),
                schema,
            ),
        });
    }

    const fn has_configuration(&self) -> bool {
        self.app_name.is_some() || self.defaults.is_some() || !self.protected_keystrokes.is_empty()
    }
}

impl Default for KeymapPlugin {
    fn default() -> Self { Self::new() }
}

impl Plugin for KeymapPlugin {
    fn build(&self, app: &mut App) { self.install(app); }

    fn finish(&self, app: &mut App) { Self::finish_assembly(app); }
}

#[derive(Clone, Resource)]
struct KeymapPluginConfiguration {
    app_name:             Option<String>,
    defaults:             Option<&'static str>,
    protected_keystrokes: Vec<Keystroke>,
}

impl From<&KeymapPlugin> for KeymapPluginConfiguration {
    fn from(keymap_plugin: &KeymapPlugin) -> Self {
        Self {
            app_name:             keymap_plugin.app_name.clone(),
            defaults:             keymap_plugin.defaults,
            protected_keystrokes: keymap_plugin.protected_keystrokes.clone(),
        }
    }
}

#[derive(Resource)]
struct KeymapDiskWorker {
    disk_worker_channels: DiskWorkerChannels,
}

#[derive(Resource)]
struct KeymapAssemblyFinished;

#[derive(Resource)]
pub(crate) struct RegistryValidationFailed;

#[derive(Default, Resource)]
struct KeymapRuntimeInstalled;

fn collect_disk_reload(
    keymap_disk_worker: Option<ResMut<KeymapDiskWorker>>,
    mut pending_reload: ResMut<keymap::PendingReload>,
) {
    let Some(keymap_disk_worker) = keymap_disk_worker else {
        return;
    };
    let Some(DiskWorkerMessage {
        snapshot,
        diagnostics,
        ..
    }) = keymap_disk_worker.disk_worker_channels.take_message()
    else {
        return;
    };

    let request = match snapshot {
        None => keymap::ReloadRequest::DiskDiagnostics(diagnostics),
        Some(snapshot) => keymap::ReloadRequest::UserSnapshot {
            source_path: snapshot.source_path.display().to_string(),
            contents: snapshot.contents.map(|contents| contents.as_ref().to_vec()),
            diagnostics,
        },
    };
    pending_reload.replace(request);
}

fn record_startup_diagnostics(app: &mut App, diagnostics: &[Diagnostic]) {
    for diagnostic in diagnostics {
        bevy::log::error!("{}", diagnostic.message);
    }
    app.world_mut()
        .resource_mut::<KeymapLoadFailures>()
        .retained_diagnostics
        .extend(diagnostics.iter().cloned());
}

fn startup_diagnostic(source_path: &str, kind: DiagnosticKind, message: &str) -> Diagnostic {
    Diagnostic {
        source_path: source_path.to_owned(),
        byte_range: 0..0,
        line: 0,
        column: 0,
        block_index: 0,
        context: String::new(),
        original_keystroke: String::new(),
        command_id: String::new(),
        kind,
        severity: DiagnosticSeverity::Failure,
        message: message.to_owned(),
        suggestions: Vec::new(),
    }
}

#[cfg(test)]
#[derive(Default, Resource)]
struct FinishAttempts(usize);

#[cfg(test)]
#[derive(Default, Resource)]
struct AssemblyRuns(usize);

#[cfg(test)]
fn increment_finish_attempts(app: &mut App) {
    app.init_resource::<FinishAttempts>();
    app.world_mut().resource_mut::<FinishAttempts>().0 += 1;
}

#[cfg(test)]
fn increment_assembly_runs(app: &mut App) {
    app.init_resource::<AssemblyRuns>();
    app.world_mut().resource_mut::<AssemblyRuns>().0 += 1;
}

/// Keymap-system ordering points exposed to application context derivation systems.
#[derive(Clone, Debug, Eq, Hash, PartialEq, SystemSet)]
pub enum KeymapSystems {
    /// Synchronizes the application's changed context into [`crate::ActiveCondition`].
    UpdateActiveCondition,
    /// Routes input through the compiled keymap.
    Route,
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests stop when isolated plugin setup cannot continue"
)]
#[allow(
    dead_code,
    reason = "test command declarations are registered through reflection"
)]
mod tests {
    use std::time::Duration;
    use std::time::Instant;

    use bevy::input::ButtonInput;
    use bevy::input::keyboard::KeyCode;
    use bevy::prelude::App;
    use bevy::prelude::AppTypeRegistry;
    use bevy::prelude::Event;
    use bevy::prelude::On;
    use bevy::prelude::Reflect;
    use bevy::prelude::ReflectEvent;
    use bevy::prelude::ResMut;
    use bevy::prelude::Resource;
    use bevy::prelude::States;
    use bevy_enhanced_input::prelude::InputAction;
    use strum::AsRefStr;
    use strum::EnumIter;
    use strum::EnumMessage;

    use super::AssemblyRuns;
    use super::FinishAttempts;
    use super::KeymapAssemblyFinished;
    use super::KeymapDiskWorker;
    use super::KeymapPlugin;
    use super::KeymapPluginConfiguration;
    use crate::ActiveCondition;
    use crate::Capability;
    use crate::DiagnosticKind;
    use crate::HoldPhase;
    use crate::KeymapCommand;
    use crate::KeymapLoadFailures;
    use crate::KeystrokeSequence;
    use crate::MatchOutcome;
    use crate::ReflectKeymapCommand;
    use crate::condition::ConditionRegistry;
    use crate::disk::ENVIRONMENT_LOCK;
    use crate::disk::KeymapPaths;
    use crate::disk::TestDirectory;
    use crate::disk::XdgConfigHome;
    use crate::keymap;
    use crate::keymap::CompiledKeymap;

    const DEFAULTS: &str = r#"{ "bindings": [] }"#;
    const TEST_APP_NAME: &str = "hana-rubric-plugin-test";

    crate::command! {
        action:      PluginDispatchAction,
        event:       PluginDispatch,
        id:          "plugin::dispatch",
        title:       "Plugin Dispatch",
        description: "Dispatches through the plugin integration tests.",
    }

    #[derive(Default, Event, Reflect)]
    #[reflect(Event, KeymapCommand)]
    struct EmptyDescription;

    impl KeymapCommand for EmptyDescription {
        const ID: &'static str = "plugin::empty_description";
        const TITLE: &'static str = "Empty Description";
        const DESCRIPTION: &'static str = "";
        const CAPABILITY: Capability = Capability::OneShot;

        fn build() -> Self { Self }

        fn hold_phase(&self) -> Option<HoldPhase> { None }
    }

    #[derive(Default, Event, Reflect)]
    #[reflect(Event, KeymapCommand)]
    struct PluginDuplicateFirst;

    impl KeymapCommand for PluginDuplicateFirst {
        const ID: &'static str = "plugin::duplicate";
        const TITLE: &'static str = "Plugin Duplicate First";
        const DESCRIPTION: &'static str = "The first duplicate command declaration.";
        const CAPABILITY: Capability = Capability::OneShot;

        fn build() -> Self { Self }

        fn hold_phase(&self) -> Option<HoldPhase> { None }
    }

    #[derive(Default, Event, Reflect)]
    #[reflect(Event, KeymapCommand)]
    struct PluginDuplicateSecond;

    impl KeymapCommand for PluginDuplicateSecond {
        const ID: &'static str = "plugin::duplicate";
        const TITLE: &'static str = "Plugin Duplicate Second";
        const DESCRIPTION: &'static str = "The second duplicate command declaration.";
        const CAPABILITY: Capability = Capability::OneShot;

        fn build() -> Self { Self }

        fn hold_phase(&self) -> Option<HoldPhase> { None }
    }

    #[derive(Default, Resource)]
    struct DispatchCount(usize);

    #[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, PartialEq, Resource)]
    #[strum(serialize_all = "snake_case")]
    enum PluginContext {
        #[strum(message = "While the plugin test context is active")]
        Active,
    }

    #[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, PartialEq, Resource)]
    #[strum(serialize_all = "snake_case")]
    enum MissingMessageContext {
        Active,
    }

    #[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, PartialEq, Resource)]
    enum DuplicateNameContext {
        #[strum(serialize = "same", message = "The first duplicate condition")]
        First,
        #[strum(serialize = "same", message = "The second duplicate condition")]
        Second,
    }

    #[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, Hash, PartialEq, States)]
    #[strum(serialize_all = "snake_case")]
    enum PluginStateContext {
        #[strum(message = "While the state test context is active")]
        Active,
    }

    fn register_plugin_command(app: &mut App) {
        app.world_mut().insert_resource(AppTypeRegistry::default());
        let app_type_registry = app.world().resource::<AppTypeRegistry>().clone();
        app_type_registry.write().register::<PluginDispatch>();
    }

    fn register_empty_registry(app: &mut App) {
        app.world_mut().insert_resource(AppTypeRegistry::default());
    }

    fn register_empty_description_command(app: &mut App) {
        register_empty_registry(app);
        let app_type_registry = app.world().resource::<AppTypeRegistry>().clone();
        app_type_registry.write().register::<EmptyDescription>();
    }

    fn register_duplicate_command_ids(app: &mut App) {
        register_empty_registry(app);
        let app_type_registry = app.world().resource::<AppTypeRegistry>().clone();
        let mut type_registry = app_type_registry.write();
        type_registry.register::<PluginDuplicateFirst>();
        type_registry.register::<PluginDuplicateSecond>();
    }

    fn assert_isolated_paths(temporary_directory: &TestDirectory) {
        let paths = KeymapPaths::new(TEST_APP_NAME).expect("test keymap paths resolve");

        assert!(
            paths
                .config_directory()
                .starts_with(temporary_directory.path())
        );
    }

    fn assert_compiled_global_binding(app: &mut App) {
        let sequence = "g"
            .parse::<KeystrokeSequence>()
            .expect("plugin global test sequence is valid");
        let match_outcome = app
            .world_mut()
            .resource_mut::<CompiledKeymap>()
            .match_global(sequence.first(), Instant::now(), Duration::from_secs(1));

        assert!(matches!(match_outcome, MatchOutcome::Matched(_)));
    }

    #[test]
    fn terminal_context_plugin_keeps_the_builder_app_name() {
        let mut app = App::new();
        app.add_plugins(
            KeymapPlugin::new()
                .with_app_name(TEST_APP_NAME)
                .for_context::<PluginContext>(),
        );

        assert_eq!(
            app.world()
                .resource::<KeymapPluginConfiguration>()
                .app_name
                .as_deref(),
            Some(TEST_APP_NAME)
        );
    }

    #[test]
    fn two_context_sources_are_rejected_before_the_second_sync_system_is_added() {
        let mut app = App::new();
        app.add_plugins(KeymapPlugin::new().for_context::<PluginContext>())
            .add_plugins(KeymapPlugin::new().for_state_context::<PluginStateContext>());

        assert_eq!(
            app.world().resource::<ConditionRegistry>().iter().count(),
            1
        );
        assert!(
            app.world()
                .resource::<KeymapLoadFailures>()
                .retained_diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("exactly one context source"))
        );
    }

    #[test]
    fn clean_user_reload_retains_startup_context_registration_diagnostic() {
        let environment_lock = ENVIRONMENT_LOCK
            .lock()
            .expect("environment lock is available");
        let temporary_directory =
            TestDirectory::new("retained-context-diagnostic").expect("temporary directory exists");
        let xdg_config_home = XdgConfigHome::set(temporary_directory.path());
        let mut app = App::new();
        register_plugin_command(&mut app);
        app.add_plugins(
            KeymapPlugin::new()
                .with_app_name(TEST_APP_NAME)
                .with_defaults(DEFAULTS)
                .for_context::<PluginContext>(),
        )
        .add_plugins(KeymapPlugin::new().for_state_context::<PluginStateContext>());
        assert_isolated_paths(&temporary_directory);
        assert!(
            app.world()
                .resource::<KeymapLoadFailures>()
                .retained_diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("exactly one context source"))
        );
        app.finish();

        let generation = app.world().resource::<CompiledKeymap>().generation().0;
        app.world_mut()
            .resource_mut::<keymap::PendingReload>()
            .replace(keymap::ReloadRequest::UserSnapshot {
                source_path: String::from("user-keymap.jsonc"),
                contents:    Some(
                    br#"{ "bindings": [{ "bindings": { "g": "plugin::dispatch" } }] }"#.to_vec(),
                ),
                diagnostics: Vec::new(),
            });
        keymap::commit_reload(app.world_mut());

        assert_eq!(
            app.world().resource::<CompiledKeymap>().generation().0,
            generation + 1
        );
        let keymap_load_failures = app.world().resource::<KeymapLoadFailures>();
        assert!(keymap_load_failures.diagnostics.is_empty());
        assert!(
            keymap_load_failures
                .retained_diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("exactly one context source"))
        );

        drop(app);
        drop(xdg_config_home);
        drop(temporary_directory);
        drop(environment_lock);
    }

    #[test]
    fn finish_is_idempotent_for_both_plugin_add_orders() {
        for context_first in [false, true] {
            let environment_lock = ENVIRONMENT_LOCK
                .lock()
                .expect("environment lock is available");
            let temporary_directory =
                TestDirectory::new("finish-idempotence").expect("temporary directory exists");
            let xdg_config_home = XdgConfigHome::set(temporary_directory.path());
            let mut app = App::new();
            register_empty_registry(&mut app);
            let context_plugin = KeymapPlugin::new()
                .with_app_name(TEST_APP_NAME)
                .with_defaults(DEFAULTS)
                .for_context::<PluginContext>();

            if context_first {
                app.add_plugins(context_plugin)
                    .add_plugins(KeymapPlugin::new());
            } else {
                app.add_plugins(KeymapPlugin::new())
                    .add_plugins(context_plugin);
            }
            assert_isolated_paths(&temporary_directory);
            app.finish();

            assert_eq!(app.world().resource::<FinishAttempts>().0, 2);
            assert_eq!(app.world().resource::<AssemblyRuns>().0, 1);
            assert!(app.world().contains_resource::<KeymapDiskWorker>());
            assert_eq!(app.world().resource::<CompiledKeymap>().generation().0, 0);
            assert!(app.world().contains_resource::<KeymapAssemblyFinished>());
            if context_first {
                assert!(!app.world().resource::<ActiveCondition>().is_initialized());
            }

            drop(app);
            drop(xdg_config_home);
            drop(temporary_directory);
            drop(environment_lock);
        }
    }

    #[test]
    fn chained_protected_keystrokes_reject_each_user_binding() -> Result<(), String> {
        let environment_lock = ENVIRONMENT_LOCK
            .lock()
            .expect("environment lock is available");
        let temporary_directory =
            TestDirectory::new("protected-keys").expect("temporary directory exists");
        let xdg_config_home = XdgConfigHome::set(temporary_directory.path());
        let first = "ctrl-g"
            .parse()
            .map_err(|error| format!("invalid first protected keystroke: {error}"))?;
        let second = "ctrl-h"
            .parse()
            .map_err(|error| format!("invalid second protected keystroke: {error}"))?;
        let mut app = App::new();
        register_plugin_command(&mut app);
        app.add_plugins(
            KeymapPlugin::new()
                .with_app_name(TEST_APP_NAME)
                .with_defaults(DEFAULTS)
                .with_protected_keystroke(first)
                .with_protected_keystroke(second),
        );
        assert_isolated_paths(&temporary_directory);
        app.finish();
        app.world_mut()
            .resource_mut::<keymap::PendingReload>()
            .replace(keymap::ReloadRequest::UserSnapshot {
                source_path: String::from("user-keymap.jsonc"),
                contents:    Some(
                    br#"{
                        "bindings": [{
                            "bindings": {
                                "ctrl-g": "plugin::dispatch",
                                "ctrl-h": "plugin::dispatch"
                            }
                        }]
                    }"#
                    .to_vec(),
                ),
                diagnostics: Vec::new(),
            });
        keymap::commit_reload(app.world_mut());

        let reserved = app
            .world()
            .resource::<KeymapLoadFailures>()
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.kind == DiagnosticKind::ReservedKeystroke)
            .count();
        assert_eq!(reserved, 2);

        drop(app);
        drop(xdg_config_home);
        drop(temporary_directory);
        drop(environment_lock);
        Ok(())
    }

    #[test]
    fn defaults_without_an_application_name_commit_and_route() {
        let environment_lock = ENVIRONMENT_LOCK
            .lock()
            .expect("environment lock is available");
        let temporary_directory =
            TestDirectory::new("defaults-without-app-name").expect("temporary directory exists");
        let xdg_config_home = XdgConfigHome::set(temporary_directory.path());
        let mut app = App::new();
        register_plugin_command(&mut app);
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<DispatchCount>()
            .add_plugins(
                KeymapPlugin::new().with_defaults(
                    r#"{ "bindings": [{ "bindings": { "g": "plugin::dispatch" } }] }"#,
                ),
            );
        app.world_mut().add_observer(
            |_: On<PluginDispatch>, mut dispatch_count: ResMut<DispatchCount>| {
                dispatch_count.0 += 1;
            },
        );
        app.finish();

        assert!(app.world().contains_resource::<CompiledKeymap>());
        assert_compiled_global_binding(&mut app);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyG);
        app.update();
        assert_eq!(app.world().resource::<DispatchCount>().0, 1);

        drop(app);
        drop(xdg_config_home);
        drop(temporary_directory);
        drop(environment_lock);
    }

    #[test]
    fn invalid_context_registration_starts_but_routes_no_global_binding() {
        let environment_lock = ENVIRONMENT_LOCK
            .lock()
            .expect("environment lock is available");
        let temporary_directory =
            TestDirectory::new("missing-context-message").expect("temporary directory exists");
        let xdg_config_home = XdgConfigHome::set(temporary_directory.path());
        let mut app = App::new();
        register_plugin_command(&mut app);
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<DispatchCount>()
            .add_plugins(
                KeymapPlugin::new()
                    .with_app_name(TEST_APP_NAME)
                    .with_defaults(
                        r#"{ "bindings": [{ "bindings": { "g": "plugin::dispatch" } }] }"#,
                    )
                    .for_context::<MissingMessageContext>(),
            );
        app.world_mut().add_observer(
            |_: On<PluginDispatch>, mut dispatch_count: ResMut<DispatchCount>| {
                dispatch_count.0 += 1;
            },
        );
        assert_isolated_paths(&temporary_directory);
        app.finish();
        assert!(app.world().contains_resource::<CompiledKeymap>());
        assert_compiled_global_binding(&mut app);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyG);
        app.update();

        assert_eq!(app.world().resource::<DispatchCount>().0, 0);
        assert!(
            app.world()
                .resource::<KeymapLoadFailures>()
                .diagnostics
                .is_empty()
        );
        assert!(
            app.world()
                .resource::<KeymapLoadFailures>()
                .retained_diagnostics
                .iter()
                .any(|diagnostic| diagnostic.context == "active")
        );

        drop(app);
        drop(xdg_config_home);
        drop(temporary_directory);
        drop(environment_lock);
    }

    #[test]
    fn duplicate_context_names_start_but_route_no_global_binding() {
        let environment_lock = ENVIRONMENT_LOCK
            .lock()
            .expect("environment lock is available");
        let temporary_directory =
            TestDirectory::new("duplicate-context-name").expect("temporary directory exists");
        let xdg_config_home = XdgConfigHome::set(temporary_directory.path());
        let mut app = App::new();
        register_plugin_command(&mut app);
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<DispatchCount>()
            .add_plugins(
                KeymapPlugin::new()
                    .with_app_name(TEST_APP_NAME)
                    .with_defaults(
                        r#"{ "bindings": [{ "bindings": { "g": "plugin::dispatch" } }] }"#,
                    )
                    .for_context::<DuplicateNameContext>(),
            );
        app.world_mut().add_observer(
            |_: On<PluginDispatch>, mut dispatch_count: ResMut<DispatchCount>| {
                dispatch_count.0 += 1;
            },
        );
        assert_isolated_paths(&temporary_directory);
        app.finish();
        assert!(app.world().contains_resource::<CompiledKeymap>());
        assert_compiled_global_binding(&mut app);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyG);
        app.update();

        assert_eq!(app.world().resource::<DispatchCount>().0, 0);
        assert!(
            app.world()
                .resource::<KeymapLoadFailures>()
                .retained_diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("registered more than once"))
        );

        drop(app);
        drop(xdg_config_home);
        drop(temporary_directory);
        drop(environment_lock);
    }

    #[test]
    fn invalid_command_declaration_starts_with_an_empty_registry() {
        let environment_lock = ENVIRONMENT_LOCK
            .lock()
            .expect("environment lock is available");
        let temporary_directory =
            TestDirectory::new("invalid-command").expect("temporary directory exists");
        let xdg_config_home = XdgConfigHome::set(temporary_directory.path());
        let mut app = App::new();
        register_empty_description_command(&mut app);
        app.add_plugins(
            KeymapPlugin::new()
                .with_app_name(TEST_APP_NAME)
                .with_defaults(
                    r#"{ "bindings": [{ "bindings": { "g": "plugin::empty_description" } }] }"#,
                ),
        );
        assert_isolated_paths(&temporary_directory);
        app.finish();

        assert_eq!(
            app.world()
                .resource::<crate::CommandRegistry>()
                .iter()
                .count(),
            0
        );
        assert!(
            app.world()
                .resource::<KeymapLoadFailures>()
                .retained_diagnostics
                .iter()
                .any(|diagnostic| diagnostic.kind == DiagnosticKind::MissingCommandDescription)
        );
        assert!(app.world().contains_resource::<CompiledKeymap>());

        drop(app);
        drop(xdg_config_home);
        drop(temporary_directory);
        drop(environment_lock);
    }

    #[test]
    fn finish_rejects_duplicate_command_ids() {
        let environment_lock = ENVIRONMENT_LOCK
            .lock()
            .expect("environment lock is available");
        let temporary_directory =
            TestDirectory::new("duplicate-command-id").expect("temporary directory exists");
        let xdg_config_home = XdgConfigHome::set(temporary_directory.path());
        let mut app = App::new();
        register_duplicate_command_ids(&mut app);
        app.add_plugins(
            KeymapPlugin::new()
                .with_app_name(TEST_APP_NAME)
                .with_defaults(DEFAULTS),
        );
        assert_isolated_paths(&temporary_directory);
        app.finish();

        assert_eq!(
            app.world()
                .resource::<crate::CommandRegistry>()
                .iter()
                .count(),
            0
        );
        assert!(
            app.world()
                .resource::<KeymapLoadFailures>()
                .retained_diagnostics
                .iter()
                .any(|diagnostic| diagnostic.kind == DiagnosticKind::DuplicateCommandId)
        );
        assert!(app.world().contains_resource::<CompiledKeymap>());

        drop(app);
        drop(xdg_config_home);
        drop(temporary_directory);
        drop(environment_lock);
    }

    #[test]
    fn retained_disk_diagnostic_does_not_allow_an_unresolvable_shipped_default() {
        let environment_lock = ENVIRONMENT_LOCK
            .lock()
            .expect("environment lock is available");
        let temporary_directory =
            TestDirectory::new("retained-disk-diagnostic").expect("temporary directory exists");
        let xdg_config_home = XdgConfigHome::set(temporary_directory.path());
        let mut app = App::new();
        register_empty_registry(&mut app);
        app.add_plugins(
            KeymapPlugin::new()
                .with_app_name(TEST_APP_NAME)
                .with_defaults(r#"{ "bindings": [{ "bindings": { "g": "missing::command" } }] }"#),
        );
        super::record_startup_diagnostics(
            &mut app,
            &[super::startup_diagnostic(
                "keymap directory",
                DiagnosticKind::Disk,
                "The keymap directory could not be created.",
            )],
        );
        assert_isolated_paths(&temporary_directory);
        app.finish();

        assert!(
            app.world()
                .resource::<KeymapLoadFailures>()
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.command_id == "missing::command")
        );
        assert!(!app.world().contains_resource::<CompiledKeymap>());

        drop(app);
        drop(xdg_config_home);
        drop(temporary_directory);
        drop(environment_lock);
    }

    #[test]
    fn unresolvable_shipped_default_does_not_commit_a_generation() {
        let environment_lock = ENVIRONMENT_LOCK
            .lock()
            .expect("environment lock is available");
        let temporary_directory =
            TestDirectory::new("unresolvable-default").expect("temporary directory exists");
        let xdg_config_home = XdgConfigHome::set(temporary_directory.path());
        let mut app = App::new();
        register_empty_registry(&mut app);
        app.add_plugins(
            KeymapPlugin::new()
                .with_app_name(TEST_APP_NAME)
                .with_defaults(
                    "{\n  \"bindings\": [{ \"bindings\": { \"g\": \"missing::command\" } }]\n}\n",
                ),
        );
        assert_isolated_paths(&temporary_directory);
        app.finish();

        let diagnostic = app
            .world()
            .resource::<KeymapLoadFailures>()
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.command_id == "missing::command")
            .expect("unresolvable shipped default is diagnosed");
        let published_defaults = app
            .world()
            .resource::<keymap::ReloadConfiguration>()
            .published_defaults();
        let reported_line = published_defaults
            .lines()
            .nth(diagnostic.line.saturating_sub(1))
            .expect("reported default line exists");

        assert!(reported_line.contains("missing::command"));
        assert!(!app.world().contains_resource::<CompiledKeymap>());
        assert!(!app.world().contains_resource::<KeymapDiskWorker>());

        drop(app);
        drop(xdg_config_home);
        drop(temporary_directory);
        drop(environment_lock);
    }
}
