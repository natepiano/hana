//! Compiled-keymap replacement from disk-worker source snapshots.

use std::path::PathBuf;
use std::str;
use std::str::Utf8Error;

use bevy::ecs::world::World;
use bevy::prelude::Resource;

use super::CompiledKeymap;
use super::Generation;
use super::KeymapBindings;
use super::MergedKeymap;
use super::merged::UserKeymap;
use crate::CommandRegistry;
use crate::Diagnostic;
use crate::DiagnosticKind;
use crate::DiagnosticOrigin;
use crate::DiagnosticSeverity;
use crate::KeymapLoadFailures;
use crate::Keystroke;
use crate::condition::ConditionRegistry;
use crate::disk::MAX_RETAINED_DIAGNOSTICS;

/// Sources shared by every keymap replacement transaction.
#[derive(Clone, Resource)]
pub(crate) struct ReloadConfiguration {
    defaults_diagnostic_origin: DiagnosticOrigin,
    published_defaults:         String,
    protected_keystrokes:       Vec<Keystroke>,
    allow_default_failures:     bool,
}

impl ReloadConfiguration {
    pub(crate) const fn new(
        defaults_diagnostic_origin: DiagnosticOrigin,
        published_defaults: String,
        protected_keystrokes: Vec<Keystroke>,
        allow_default_failures: bool,
    ) -> Self {
        Self {
            defaults_diagnostic_origin,
            published_defaults,
            protected_keystrokes,
            allow_default_failures,
        }
    }

    #[cfg(test)]
    pub(crate) fn published_defaults(&self) -> &str { &self.published_defaults }
}

/// One source update waiting for the next reload transaction.
#[derive(Default, Resource)]
pub(crate) struct PendingReload {
    request: Option<ReloadRequest>,
}

impl PendingReload {
    pub(crate) fn replace(&mut self, request: ReloadRequest) { self.request = Some(request); }

    const fn take(&mut self) -> Option<ReloadRequest> { self.request.take() }
}

/// Whether the disk worker read bytes for the user keymap file.
pub(crate) enum UserKeymapContents {
    Read(Vec<u8>),
    Absent,
}

/// Transport-neutral user-keymap input for the reload transaction.
pub(crate) enum ReloadRequest {
    Defaults,
    DiskDiagnostics(Vec<Diagnostic>),
    UserSnapshot {
        source_path: PathBuf,
        contents:    UserKeymapContents,
        diagnostics: Vec<Diagnostic>,
    },
}

/// Commits the startup defaults synchronously before the disk worker starts.
pub(crate) fn commit_defaults(world: &mut World) -> bool {
    matches!(
        commit_request(world, ReloadRequest::Defaults),
        CommitOutcome::Committed
    )
}

/// Replaces the compiled keymap for the next available disk-worker request.
pub(crate) fn commit_reload(world: &mut World) {
    let Some(request) = world
        .get_resource_mut::<PendingReload>()
        .and_then(|mut pending_reload| pending_reload.take())
    else {
        return;
    };

    let _ = commit_request(world, request);
}

fn commit_request(world: &mut World, request: ReloadRequest) -> CommitOutcome {
    let (user_keymap, disk_diagnostics, validates_defaults) = match request {
        ReloadRequest::Defaults => (UserKeymap::DefaultsOnly, Vec::new(), true),
        ReloadRequest::DiskDiagnostics(diagnostics) => {
            record_transport_diagnostics(world, diagnostics);
            return CommitOutcome::NoChange;
        },
        ReloadRequest::UserSnapshot {
            source_path,
            contents,
            diagnostics,
        } => match contents {
            UserKeymapContents::Absent => (UserKeymap::DefaultsOnly, diagnostics, false),
            UserKeymapContents::Read(contents) => {
                let user_keymap = match str::from_utf8(&contents) {
                    Ok(source) => UserKeymap::Layered {
                        origin:   DiagnosticOrigin::KeymapFile(source_path),
                        contents: source.to_owned(),
                    },
                    Err(error) => {
                        let diagnostic = utf8_diagnostic(source_path, error);
                        record_load_diagnostics(world, vec![diagnostic]);
                        record_transport_diagnostics(world, diagnostics);
                        return CommitOutcome::NoChange;
                    },
                };

                (user_keymap, diagnostics, false)
            },
        },
    };

    if !disk_diagnostics.is_empty() {
        record_transport_diagnostics(world, disk_diagnostics);
    }

    let reload_configuration = world.resource::<ReloadConfiguration>().clone();
    let merged_keymap = world.resource_scope::<CommandRegistry, _>(|world, command_registry| {
        let condition_registry = world.resource::<ConditionRegistry>();
        MergedKeymap::from_sources(
            &reload_configuration.defaults_diagnostic_origin,
            &reload_configuration.published_defaults,
            &user_keymap,
            &command_registry,
            condition_registry,
            &reload_configuration.protected_keystrokes,
        )
    });

    let (merged_keymap, diagnostics) = match merged_keymap {
        Ok(result) => result,
        Err(diagnostics) => {
            record_load_diagnostics(world, diagnostics);
            return CommitOutcome::NoChange;
        },
    };

    let defaults_failed = validates_defaults
        && diagnostics.iter().any(|diagnostic| {
            diagnostic.origin == reload_configuration.defaults_diagnostic_origin
                && diagnostic.severity == DiagnosticSeverity::Failure
        });
    if defaults_failed && !reload_configuration.allow_default_failures {
        record_load_diagnostics(world, diagnostics);
        return CommitOutcome::NoChange;
    }

    let generation = next_generation(world);
    let compiled_keymap = world.resource_scope::<CommandRegistry, _>(|_, command_registry| {
        merged_keymap.compile(generation, &command_registry)
    });
    world.insert_resource(compiled_keymap);
    world.insert_resource(KeymapBindings::from_bindings(merged_keymap.bindings()));
    record_load_diagnostics(world, diagnostics);

    CommitOutcome::Committed
}

fn next_generation(world: &World) -> Generation {
    world
        .get_resource::<CompiledKeymap>()
        .map_or(Generation(0), |compiled_keymap| {
            Generation(compiled_keymap.generation.0 + 1)
        })
}

fn utf8_diagnostic(source_path: PathBuf, error: Utf8Error) -> Diagnostic {
    Diagnostic {
        origin:             DiagnosticOrigin::KeymapFile(source_path),
        byte_range:         0..0,
        line:               0,
        column:             0,
        block_index:        0,
        context:            String::new(),
        original_keystroke: String::new(),
        command_id:         String::new(),
        kind:               DiagnosticKind::Syntax,
        severity:           DiagnosticSeverity::Failure,
        message:            format!("The user keymap is not valid UTF-8: {error}"),
        suggestions:        Vec::new(),
    }
}

fn record_load_diagnostics(world: &mut World, diagnostics: Vec<Diagnostic>) {
    log_diagnostics(&diagnostics);
    let mut keymap_load_failures = world.resource_mut::<KeymapLoadFailures>();
    keymap_load_failures.diagnostics.retain(|diagnostic| {
        diagnostic.severity != DiagnosticSeverity::Failure || !is_reload_diagnostic(diagnostic)
    });
    keymap_load_failures.diagnostics.extend(diagnostics);
    retain_recent_diagnostics(&mut keymap_load_failures.diagnostics);
}

fn record_transport_diagnostics(world: &mut World, diagnostics: Vec<Diagnostic>) {
    log_diagnostics(&diagnostics);
    let mut keymap_load_failures = world.resource_mut::<KeymapLoadFailures>();
    keymap_load_failures.diagnostics.extend(diagnostics);
    retain_recent_diagnostics(&mut keymap_load_failures.diagnostics);
}

const fn is_reload_diagnostic(diagnostic: &Diagnostic) -> bool {
    matches!(
        diagnostic.kind,
        DiagnosticKind::Syntax
            | DiagnosticKind::Keystroke
            | DiagnosticKind::Command
            | DiagnosticKind::Context
    )
}

fn retain_recent_diagnostics(diagnostics: &mut Vec<Diagnostic>) {
    let discarded = diagnostics.len().saturating_sub(MAX_RETAINED_DIAGNOSTICS);
    if discarded > 0 {
        diagnostics.drain(0..discarded);
    }
}

fn log_diagnostics(diagnostics: &[Diagnostic]) {
    for diagnostic in diagnostics {
        match diagnostic.severity {
            DiagnosticSeverity::Failure => {
                bevy::log::error!("{}", diagnostic.message);
            },
            DiagnosticSeverity::Advisory => {
                bevy::log::warn!("{}", diagnostic.message);
            },
        }
    }
}

enum CommitOutcome {
    Committed,
    NoChange,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;
    use std::time::Instant;

    use bevy::input::ButtonInput;
    use bevy::input::keyboard::KeyCode;
    use bevy::prelude::App;
    use bevy::prelude::Event;
    use bevy::prelude::On;
    use bevy::prelude::Reflect;
    use bevy::prelude::ReflectEvent;
    use bevy::prelude::ResMut;
    use bevy::prelude::Resource;
    use bevy::reflect::TypeRegistry;
    use bevy_enhanced_input::prelude::ActionValue;
    use bevy_enhanced_input::prelude::CustomInput;
    use bevy_enhanced_input::prelude::CustomInputs;

    use super::PendingReload;
    use super::ReloadConfiguration;
    use super::ReloadRequest;
    use super::UserKeymapContents;
    use super::commit_defaults;
    use super::commit_reload;
    use crate::Capability;
    use crate::CommandId;
    use crate::CommandRegistry;
    use crate::DiagnosticOrigin;
    use crate::HeldCommandLookupOutcome;
    use crate::HoldPhase;
    use crate::KeymapCommand;
    use crate::KeymapLoadFailures;
    use crate::KeystrokeSequence;
    use crate::MatchOutcome;
    use crate::ReflectKeymapCommand;
    use crate::condition::ConditionRegistry;
    use crate::keymap::CompiledKeymap;
    use crate::keymap::runtime;
    use crate::keymap::runtime::KeymapRuntime;

    const DEFAULTS_PATH: &str = "published-defaults.jsonc";
    const USER_KEYMAP_FIXTURE: &str = "user-keymap.jsonc";
    const MATCH_TIMEOUT: Duration = Duration::from_secs(1);

    #[derive(Default, Event, Reflect)]
    #[reflect(Event, KeymapCommand)]
    struct ReloadFirst;

    impl KeymapCommand for ReloadFirst {
        const ID: &'static str = "reload::first";
        const TITLE: &'static str = "Reload First";
        const DESCRIPTION: &'static str = "First command used by reload transaction tests.";
        const CAPABILITY: Capability = Capability::OneShot;

        fn build() -> Self { Self }

        fn hold_phase(&self) -> Option<HoldPhase> { None }
    }

    #[derive(Default, Event, Reflect)]
    #[reflect(Event, KeymapCommand)]
    struct ReloadSecond;

    impl KeymapCommand for ReloadSecond {
        const ID: &'static str = "reload::second";
        const TITLE: &'static str = "Reload Second";
        const DESCRIPTION: &'static str = "Second command used by reload transaction tests.";
        const CAPABILITY: Capability = Capability::OneShot;

        fn build() -> Self { Self }

        fn hold_phase(&self) -> Option<HoldPhase> { None }
    }

    #[derive(Default, Event, Reflect)]
    #[reflect(Event, KeymapCommand)]
    struct ReloadHeld;

    impl KeymapCommand for ReloadHeld {
        const ID: &'static str = "reload::held";
        const TITLE: &'static str = "Reload Held";
        const DESCRIPTION: &'static str = "Held command used by reload transaction tests.";
        const CAPABILITY: Capability = Capability::Held;

        fn build() -> Self { Self }

        fn hold_phase(&self) -> Option<HoldPhase> { Some(HoldPhase::Begin) }
    }

    #[derive(Default, Resource)]
    struct ReloadDispatchCount(usize);

    fn transaction_app(defaults: &str) -> Result<App, String> {
        let mut type_registry = TypeRegistry::default();
        type_registry.register::<ReloadFirst>();
        type_registry.register::<ReloadSecond>();
        type_registry.register::<ReloadHeld>();
        let mut custom_inputs = CustomInputs::default();
        let command_registry = CommandRegistry::build(&type_registry, &mut custom_inputs)
            .map_err(|diagnostics| format!("reload registry diagnostics: {diagnostics:?}"))?;
        let mut app = App::new();

        app.insert_resource(command_registry)
            .insert_resource(custom_inputs)
            .insert_resource(ConditionRegistry::default())
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<KeymapLoadFailures>()
            .init_resource::<KeymapRuntime>()
            .init_resource::<PendingReload>()
            .init_resource::<ReloadDispatchCount>()
            .insert_resource(ReloadConfiguration::new(
                DiagnosticOrigin::KeymapFile(PathBuf::from(DEFAULTS_PATH)),
                defaults.to_owned(),
                Vec::new(),
                false,
            ));
        app.world_mut().add_observer(
            |_: On<ReloadFirst>, mut reload_dispatch_count: ResMut<ReloadDispatchCount>| {
                reload_dispatch_count.0 += 1;
            },
        );
        Ok(app)
    }

    fn queue_user_snapshot(app: &mut App, contents: &[u8]) {
        app.world_mut()
            .resource_mut::<PendingReload>()
            .replace(ReloadRequest::UserSnapshot {
                source_path: PathBuf::from(USER_KEYMAP_FIXTURE),
                contents:    UserKeymapContents::Read(contents.to_vec()),
                diagnostics: Vec::new(),
            });
    }

    fn global_matches(app: &mut App, keystroke: &str) -> Result<bool, String> {
        let sequence = keystroke
            .parse::<KeystrokeSequence>()
            .map_err(|error| format!("invalid test keystroke: {error}"))?;
        let match_outcome = app
            .world_mut()
            .resource_mut::<CompiledKeymap>()
            .global
            .match_keystroke(sequence.first(), Instant::now(), MATCH_TIMEOUT);

        Ok(matches!(match_outcome, MatchOutcome::Matched(_)))
    }

    fn held_custom_input(app: &App) -> Result<CustomInput, String> {
        let command_id = CommandId::try_from(ReloadHeld::ID)
            .map_err(|error| format!("invalid held command ID: {error}"))?;

        match app
            .world()
            .resource::<CommandRegistry>()
            .held_command_lookup(&command_id)
        {
            HeldCommandLookupOutcome::RegisteredHeldInput(custom_input) => Ok(custom_input),
            HeldCommandLookupOutcome::KnownNonHeld => {
                Err(String::from("reload held command is not held"))
            },
            HeldCommandLookupOutcome::UnknownCommand => {
                Err(String::from("reload held command is not registered"))
            },
        }
    }

    fn press(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        runtime::route_input(app.world_mut());
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear_just_pressed(key);
    }

    fn release(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(key);
        runtime::route_input(app.world_mut());
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear_just_released(key);
    }

    #[test]
    fn whole_file_failure_keeps_generation_and_pending_sequence_untouched() -> Result<(), String> {
        let defaults = r#"{
            "bindings": [{ "bindings": {
                "a": "reload::held",
                "g h": "reload::first"
            } }]
        }"#;
        let mut app = transaction_app(defaults)?;

        assert!(commit_defaults(app.world_mut()));
        let custom_input = held_custom_input(&app)?;
        press(&mut app, KeyCode::KeyA);
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );
        let generation = app.world().resource::<CompiledKeymap>().generation;
        let sequence = "g h"
            .parse::<KeystrokeSequence>()
            .map_err(|error| format!("invalid pending test sequence: {error}"))?;
        let first_match = app
            .world_mut()
            .resource_mut::<CompiledKeymap>()
            .global
            .match_keystroke(sequence.first(), Instant::now(), MATCH_TIMEOUT);

        assert!(matches!(first_match, MatchOutcome::Pending));
        queue_user_snapshot(&mut app, br#"{ "bindings": ["#);
        commit_reload(app.world_mut());

        let compiled_keymap = app.world().resource::<CompiledKeymap>();
        assert_eq!(compiled_keymap.generation, generation);
        assert!(compiled_keymap.global.is_pending());
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );
        assert!(
            app.world()
                .resource::<KeymapLoadFailures>()
                .diagnostics
                .iter()
                .any(|diagnostic| {
                    diagnostic.origin
                        == DiagnosticOrigin::KeymapFile(PathBuf::from(USER_KEYMAP_FIXTURE))
                })
        );
        Ok(())
    }

    #[test]
    fn reload_while_a_key_is_held_releases_its_previous_physical_source() -> Result<(), String> {
        let defaults = r#"{ "bindings": [{ "bindings": { "a": "reload::held" } }] }"#;
        let mut app = transaction_app(defaults)?;

        assert!(commit_defaults(app.world_mut()));
        let custom_input = held_custom_input(&app)?;
        press(&mut app, KeyCode::KeyA);
        queue_user_snapshot(&mut app, defaults.as_bytes());
        commit_reload(app.world_mut());
        runtime::route_input(app.world_mut());

        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(false))
        );
        Ok(())
    }

    #[test]
    fn reloading_a_held_binding_moves_its_physical_source() -> Result<(), String> {
        let defaults = r#"{ "bindings": [{ "bindings": { "a": "reload::held" } }] }"#;
        let mut app = transaction_app(defaults)?;

        assert!(commit_defaults(app.world_mut()));
        let custom_input = held_custom_input(&app)?;
        press(&mut app, KeyCode::KeyA);
        queue_user_snapshot(
            &mut app,
            br#"{
                "bindings": [{ "bindings": {
                    "a": null,
                    "b": "reload::held"
                } }]
            }"#,
        );
        commit_reload(app.world_mut());
        runtime::route_input(app.world_mut());
        release(&mut app, KeyCode::KeyA);
        press(&mut app, KeyCode::KeyA);

        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(false))
        );
        release(&mut app, KeyCode::KeyA);
        press(&mut app, KeyCode::KeyB);
        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(true))
        );
        Ok(())
    }

    #[test]
    fn generation_change_discards_a_pending_sequence() -> Result<(), String> {
        let defaults = r#"{ "bindings": [{ "bindings": { "g h": "reload::first" } }] }"#;
        let mut app = transaction_app(defaults)?;

        assert!(commit_defaults(app.world_mut()));
        let generation = app.world().resource::<CompiledKeymap>().generation;
        press(&mut app, KeyCode::KeyG);
        release(&mut app, KeyCode::KeyG);
        assert!(app.world().resource::<CompiledKeymap>().global.is_pending());
        queue_user_snapshot(
            &mut app,
            br#"{ "bindings": [{ "bindings": { "j": "reload::second" } }] }"#,
        );
        commit_reload(app.world_mut());
        press(&mut app, KeyCode::KeyH);

        assert_ne!(
            app.world().resource::<CompiledKeymap>().generation,
            generation
        );
        assert!(!app.world().resource::<CompiledKeymap>().global.is_pending());
        assert_eq!(app.world().resource::<ReloadDispatchCount>().0, 0);
        Ok(())
    }

    #[test]
    fn reloading_multiple_held_aliases_releases_the_shared_action() -> Result<(), String> {
        let defaults = r#"{
            "bindings": [{ "bindings": {
                "a": "reload::held",
                "b": "reload::held"
            } }]
        }"#;
        let mut app = transaction_app(defaults)?;

        assert!(commit_defaults(app.world_mut()));
        let custom_input = held_custom_input(&app)?;
        press(&mut app, KeyCode::KeyA);
        press(&mut app, KeyCode::KeyB);
        queue_user_snapshot(&mut app, defaults.as_bytes());
        commit_reload(app.world_mut());
        runtime::route_input(app.world_mut());

        assert_eq!(
            app.world().resource::<CustomInputs>().get(&custom_input),
            Some(&ActionValue::Bool(false))
        );
        Ok(())
    }

    #[test]
    fn partial_failure_commits_defaults_and_every_valid_user_edit() -> Result<(), String> {
        let defaults = r#"{ "bindings": [{ "bindings": { "g": "reload::first" } }] }"#;
        let mut app = transaction_app(defaults)?;

        assert!(commit_defaults(app.world_mut()));
        queue_user_snapshot(
            &mut app,
            br#"{
                "bindings": [{
                    "bindings": {
                        "h": "reload::second",
                        "j": "unknown::command"
                    }
                }]
            }"#,
        );
        commit_reload(app.world_mut());

        assert!(global_matches(&mut app, "g")?);
        assert!(global_matches(&mut app, "h")?);
        assert!(!global_matches(&mut app, "j")?);
        assert!(
            app.world()
                .resource::<KeymapLoadFailures>()
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.command_id == "unknown::command")
        );
        Ok(())
    }
}
