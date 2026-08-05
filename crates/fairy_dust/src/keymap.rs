//! Baseline capability: the one `hana_rubric` [`KeymapPlugin`] every Fairy Dust
//! app installs, and the document it is configured from.
//!
//! Fairy Dust declares its own capabilities with `command!`, so the command
//! registry — not a `bevy_enhanced_input` binding — is what turns `Ctrl+Shift+R`
//! into a restart. Every example therefore needs the keymap runtime, whether or
//! not it opens the command palette, which is why
//! [`SprinkleBuilder::run`](crate::SprinkleBuilder::run) installs the plugin
//! unconditionally.
//!
//! The install is deferred to `run` rather than done in the baseline plugin set
//! because a plugin's configuration is fixed at `add_plugins` time, while
//! [`SprinkleBuilder::with_command_palette_keymap`](crate::SprinkleBuilder::with_command_palette_keymap)
//! can name a different document at any point in the builder chain. Recording
//! the choice first and installing once at the end is what lets the two agree.

use bevy::prelude::App;
use bevy::prelude::Resource;
use hana_rubric::KeymapConfigurationDirectory;
use hana_rubric::KeymapPlugin;
use hana_rubric::Keystroke;

/// Fairy Dust's shipped default keymap, binding the palette and every command
/// Fairy Dust declares.
pub(crate) const FAIRY_DUST_DEFAULT_KEYMAP: &str = include_str!("../assets/keymap.default.jsonc");

/// The keymap document Fairy Dust's `KeymapPlugin` is installed with, and
/// whether that keymap reads and writes a configuration directory.
///
/// Configuring an application name is what lets a user keep their own
/// `keymap.jsonc` next to the published defaults. Without one, no disk worker
/// starts, nothing is written, and the palette reports the missing
/// configuration directory as a failure row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandPaletteKeymap {
    defaults:                &'static str,
    configuration_directory: KeymapConfigurationDirectory,
    protected_keystrokes:    Vec<Keystroke>,
}

impl CommandPaletteKeymap {
    /// Builds a keymap from a JSONC document, normally supplied with
    /// `include_str!`.
    ///
    /// The document replaces Fairy Dust's shipped defaults outright, so it binds
    /// `palette::open` and every `fairy_dust::` command as well as the
    /// application's own command ids. A document naming a command the registry
    /// does not declare is rejected whole, which leaves the palette unopenable
    /// and Fairy Dust's own chords dead.
    #[must_use]
    pub const fn new(defaults: &'static str) -> Self {
        Self {
            defaults,
            configuration_directory: KeymapConfigurationDirectory::Unconfigured,
            protected_keystrokes: Vec::new(),
        }
    }

    /// Reads and writes the user's own keymap under `app_name` in the platform
    /// configuration directory.
    ///
    /// Without this the palette reports the unavailable configuration directory
    /// as a failure row, because a keymap the user cannot edit is a real
    /// limitation rather than a normal state.
    #[must_use]
    pub fn for_application(mut self, app_name: &str) -> Self {
        self.configuration_directory =
            KeymapConfigurationDirectory::ForApplication(app_name.to_owned());
        self
    }

    /// Reserves `keystroke` from user-authored bindings, the way
    /// [`KeymapPlugin::with_protected_keystroke`] does.
    ///
    /// An application that installs its own `KeymapPlugin` through
    /// `for_context` and reserves a recovery chord names the same keystroke
    /// here, because `hana_rubric` refuses two plugin configurations that
    /// disagree on defaults, application name, or protected keystrokes.
    #[must_use]
    pub fn with_protected_keystroke(mut self, keystroke: Keystroke) -> Self {
        self.protected_keystrokes.push(keystroke);
        self
    }

    fn keymap_plugin(&self) -> KeymapPlugin {
        let keymap_plugin = KeymapPlugin::new().with_defaults(self.defaults);
        let mut keymap_plugin = match &self.configuration_directory {
            KeymapConfigurationDirectory::ForApplication(app_name) => {
                keymap_plugin.with_app_name(app_name)
            },
            KeymapConfigurationDirectory::Unconfigured => keymap_plugin,
        };
        for keystroke in &self.protected_keystrokes {
            keymap_plugin = keymap_plugin.with_protected_keystroke(*keystroke);
        }
        keymap_plugin
    }
}

impl Default for CommandPaletteKeymap {
    fn default() -> Self { Self::new(FAIRY_DUST_DEFAULT_KEYMAP) }
}

/// The keymap the application chose, recorded before the plugin is built so a
/// second request that disagrees is refused rather than silently ignored.
#[derive(Resource)]
struct ChosenKeymap(CommandPaletteKeymap);

/// Records `keymap` as the document Fairy Dust's `KeymapPlugin` will be built
/// from.
///
/// A second request carrying a different keymap is refused: silently keeping the
/// first document would leave an application running bindings it never asked
/// for.
pub(crate) fn configure(app: &mut App, keymap: CommandPaletteKeymap) {
    if let Some(chosen_keymap) = app.world().get_resource::<ChosenKeymap>() {
        assert!(
            chosen_keymap.0 == keymap,
            "fairy_dust: the keymap is already configured with a different document. Call \
             `with_command_palette` or `with_command_palette_keymap` once."
        );
        return;
    }
    app.insert_resource(ChosenKeymap(keymap));
}

/// Whether a builder call has named the keymap Fairy Dust's `KeymapPlugin` is
/// built from.
#[cfg(test)]
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum KeymapChoice {
    /// A builder call recorded this document.
    Named(CommandPaletteKeymap),
    /// No builder call named one, so `run` installs Fairy Dust's shipped
    /// defaults.
    Unnamed,
}

/// The keymap recorded so far.
#[cfg(test)]
pub(crate) fn chosen(app: &App) -> KeymapChoice {
    app.world()
        .get_resource::<ChosenKeymap>()
        .map_or(KeymapChoice::Unnamed, |chosen_keymap| {
            KeymapChoice::Named(chosen_keymap.0.clone())
        })
}

/// Adds the one `KeymapPlugin` this app gets, built from the recorded keymap or
/// Fairy Dust's shipped defaults.
///
/// An application that added `KeymapPlugin` itself keeps it and Fairy Dust adds
/// nothing. A context source is a different plugin type — `for_context` and its
/// siblings return their own plugin — so `is_plugin_added` reports no
/// `KeymapPlugin` and Fairy Dust adds one alongside it. What decides then is
/// `hana_rubric`'s own guard: it compares the two configurations' defaults,
/// application name, and protected keystrokes and panics when they disagree, so
/// such an application names all three through
/// [`CommandPaletteKeymap`].
pub(crate) fn install(app: &mut App) {
    if app.is_plugin_added::<KeymapPlugin>() {
        return;
    }
    let keymap = app
        .world()
        .get_resource::<ChosenKeymap>()
        .map_or_else(CommandPaletteKeymap::default, |chosen| chosen.0.clone());
    app.add_plugins(keymap.keymap_plugin());
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::str::FromStr;

    use bevy::prelude::App;
    use bevy::prelude::Resource;
    use hana_rubric::CommandId;
    use hana_rubric::CommandKeystroke;
    use hana_rubric::CommandLookup;
    use hana_rubric::CommandRegistry;
    use hana_rubric::KeymapBindings;
    use hana_rubric::KeymapPlugin;
    use hana_rubric::Keystroke;
    use strum::AsRefStr;
    use strum::EnumIter;
    use strum::EnumMessage;

    use super::CommandPaletteKeymap;
    use super::FAIRY_DUST_DEFAULT_KEYMAP;
    use super::configure;
    use super::install;

    const OTHER_KEYMAP: &str = r#"{ "bindings": [] }"#;
    const RECOVERY_KEYSTROKE: &str = "ctrl-alt-r";

    /// Stands in for an application that installs its own `KeymapPlugin` through
    /// `for_context`, which is a different plugin type from `KeymapPlugin`.
    #[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, PartialEq, Resource)]
    #[strum(serialize_all = "snake_case")]
    enum RecoveryContext {
        #[strum(message = "While the recovery test context is active")]
        Active,
    }

    fn recovery_keystroke() -> Keystroke {
        RECOVERY_KEYSTROKE
            .parse()
            .expect("the recovery chord is a valid keystroke")
    }

    /// Every capability Fairy Dust declares, and the palette that lists them.
    const FAIRY_DUST_COMMAND_IDS: [&str; 7] = [
        "palette::open",
        "fairy_dust::restart",
        "fairy_dust::toggle_screen_space_panels",
        "fairy_dust::toggle_home_aabb_gizmo",
        "fairy_dust::show_help",
        "fairy_dust::toggle_free_cam_look_pitch",
        "fairy_dust::cycle_camera_preset",
    ];

    fn command_id(text: &str) -> CommandId {
        CommandId::from_str(text).expect("fairy dust command ids are valid")
    }

    #[test]
    fn an_unconfigured_app_installs_the_shipped_defaults() {
        let mut app = App::new();

        install(&mut app);

        assert!(app.is_plugin_added::<KeymapPlugin>());
        assert_eq!(
            CommandPaletteKeymap::default(),
            CommandPaletteKeymap::new(FAIRY_DUST_DEFAULT_KEYMAP)
        );
    }

    #[test]
    fn repeating_the_same_choice_is_accepted() {
        let mut app = App::new();

        configure(&mut app, CommandPaletteKeymap::new(OTHER_KEYMAP));
        configure(&mut app, CommandPaletteKeymap::new(OTHER_KEYMAP));
        install(&mut app);

        assert!(app.is_plugin_added::<KeymapPlugin>());
    }

    /// An application that installs its own context-source plugin and reserves a
    /// recovery chord names the same chord on its `CommandPaletteKeymap`, so the
    /// two plugin configurations `hana_rubric` compares agree.
    #[test]
    fn a_reserved_recovery_chord_agrees_with_a_context_plugin() {
        let mut app = App::new();
        app.add_plugins(
            KeymapPlugin::new()
                .with_defaults(FAIRY_DUST_DEFAULT_KEYMAP)
                .with_protected_keystroke(recovery_keystroke())
                .for_context::<RecoveryContext>(),
        );

        configure(
            &mut app,
            CommandPaletteKeymap::default().with_protected_keystroke(recovery_keystroke()),
        );
        install(&mut app);

        assert!(app.is_plugin_added::<KeymapPlugin>());
    }

    /// The mirror: leaving the chord off the palette keymap is the disagreement
    /// `hana_rubric` refuses, which is what made a reserved chord unreachable
    /// before `CommandPaletteKeymap` could carry one.
    #[test]
    #[should_panic(expected = "already installed with different defaults")]
    fn a_palette_keymap_missing_the_reserved_chord_is_refused() {
        let mut app = App::new();
        app.add_plugins(
            KeymapPlugin::new()
                .with_defaults(FAIRY_DUST_DEFAULT_KEYMAP)
                .with_protected_keystroke(recovery_keystroke())
                .for_context::<RecoveryContext>(),
        );

        configure(&mut app, CommandPaletteKeymap::default());
        install(&mut app);
    }

    #[test]
    fn a_second_install_leaves_the_first_plugin_in_place() {
        let mut app = App::new();

        install(&mut app);
        install(&mut app);

        assert!(app.is_plugin_added::<KeymapPlugin>());
    }

    /// The whole point of the baseline install: an application that never asks
    /// for the command palette still reaches every Fairy Dust capability by its
    /// keystroke. A missing binding here is a dead hotkey in every example.
    #[test]
    fn the_baseline_install_binds_every_fairy_dust_command() {
        let mut app = App::new();
        install(&mut app);
        app.finish();

        let keymap_bindings = app.world().resource::<KeymapBindings>();
        for command_id in FAIRY_DUST_COMMAND_IDS.map(command_id) {
            assert!(
                matches!(
                    keymap_bindings.keystroke(&command_id),
                    CommandKeystroke::BoundTo(_)
                ),
                "the shipped defaults leave `{command_id}` unbound"
            );
        }
    }

    /// The registry is what the palette lists, so a command missing here — or
    /// carrying an unauthored title — is one a user cannot find by name.
    #[test]
    fn every_fairy_dust_command_is_listed_with_an_authored_title() {
        let mut app = App::new();
        install(&mut app);
        app.finish();

        let command_registry = app.world().resource::<CommandRegistry>();
        for command_id in FAIRY_DUST_COMMAND_IDS.map(command_id) {
            let CommandLookup::Found(command_info) = command_registry.lookup(&command_id) else {
                panic!("`{command_id}` is bound by the shipped defaults but never declared");
            };
            assert!(
                !command_info.title.is_empty(),
                "`{command_id}` carries no title"
            );
            assert!(
                command_info.capability.is_palette_invocable(),
                "`{command_id}` is declared but the palette will not list it"
            );
        }
    }
}
