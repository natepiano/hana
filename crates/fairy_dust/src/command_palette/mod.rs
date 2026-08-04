//! Capability: a searchable box listing every declared keymap command, with the
//! keymap failures that broke the bindings rendered above its query field.
//!
//! [`SprinkleBuilder::with_command_palette`](crate::SprinkleBuilder::with_command_palette)
//! installs `KeymapPlugin` with Fairy Dust's embedded defaults. The query field
//! is an editable panel field driven by `hana_diegetic`'s IME session, so the
//! platform candidate window is positioned at the caret and Fairy Dust's own
//! keyboard shortcuts stand down while the session holds the window's input
//! lease.

mod constants;
mod failure_row;
mod panel;
mod query;

use std::path::Path;
use std::process::Command;

use bevy::prelude::App;
use bevy::prelude::Commands;
use bevy::prelude::Component;
use bevy::prelude::Entity;
use bevy::prelude::Event;
use bevy::prelude::IntoScheduleConfigs;
use bevy::prelude::Mut;
use bevy::prelude::On;
use bevy::prelude::Plugin;
use bevy::prelude::PreUpdate;
use bevy::prelude::Query;
use bevy::prelude::Reflect;
use bevy::prelude::ReflectEvent;
use bevy::prelude::Res;
use bevy::prelude::ResMut;
use bevy::prelude::Resource;
use bevy::prelude::Transform;
use bevy::prelude::Window;
use bevy::prelude::With;
use bevy::prelude::World;
use bevy::prelude::error;
use bevy::window::PrimaryWindow;
use bevy_enhanced_input::prelude::EnhancedInputPlugin;
use bevy_enhanced_input::prelude::InputAction;
use hana_diegetic::Anchor;
use hana_diegetic::ButtonClicked;
use hana_diegetic::DiegeticPanel;
use hana_diegetic::ImeAcceptCommit;
use hana_diegetic::ImeAppOwnedFieldSpec;
use hana_diegetic::ImeAppliedResult;
use hana_diegetic::ImeCanceled;
use hana_diegetic::ImeCommitAuthority;
use hana_diegetic::ImeCommitRequested;
use hana_diegetic::ImeEditableFieldSpec;
use hana_diegetic::ImeInputBlocker;
use hana_diegetic::ImeOpenSession;
use hana_diegetic::ImeRejectCommit;
use hana_diegetic::ImeRejection;
use hana_diegetic::ImeReplacePanelTree;
use hana_diegetic::ImeTarget;
use hana_diegetic::ImeTextChanged;
use hana_diegetic::PanelPicking;
use hana_diegetic::Px;
use hana_diegetic::Sizing;
use hana_rubric::CommandId;
use hana_rubric::CommandInvocationOutcome;
use hana_rubric::CommandRegistry;
use hana_rubric::KeymapBindings;
use hana_rubric::KeymapLoadFailures;
use hana_rubric::KeymapPathAvailability;
use hana_rubric::KeymapPlugin;
use hana_rubric::KeymapSystems;
use hana_rubric::KeystrokeRouting;
use hana_rubric::ReflectKeymapCommand;
use hana_rubric::command;

use self::constants::FIELD_ID;
use self::failure_row::KeymapFailureAction;
use self::failure_row::keymap_failure_rows;
use self::panel::FailureActionRow;
use self::panel::PaletteView;
use self::panel::failure_action_row_index;
use self::panel::palette_panel_origin;
use self::panel::palette_panel_width;
use self::panel::palette_tree;
use self::query::PaletteSelection;
use self::query::command_rows;
use self::query::resolve_selection;
use crate::ensure_plugin;

/// Fairy Dust's shipped default keymap, which binds only the palette's own
/// command.
pub(crate) const FAIRY_DUST_DEFAULT_KEYMAP: &str =
    include_str!("../../assets/keymap.default.jsonc");

command! {
    action:      OpenCommandPalette,
    event:       OpenCommandPaletteEvent,
    id:          "palette::open",
    title:       "Open Command Palette",
    description: "Search every declared command and run the selected one.",
}

/// The keymap document the command palette's `KeymapPlugin` is installed with,
/// and whether that keymap reads and writes a configuration directory.
///
/// Configuring an application name is what lets a user keep their own
/// `keymap.jsonc` next to the published defaults. Without one, no disk worker
/// starts, nothing is written, and the palette reports the missing
/// configuration directory as a failure row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandPaletteKeymap {
    defaults:                &'static str,
    configuration_directory: KeymapConfigurationDirectory,
}

/// Where the palette's keymap keeps the user's own document.
#[derive(Clone, Debug, Eq, PartialEq)]
enum KeymapConfigurationDirectory {
    /// Read and write the user's keymap under this application name.
    ForApplication(&'static str),
    /// Run from the embedded defaults alone, touching no configuration
    /// directory.
    Unconfigured,
}

impl CommandPaletteKeymap {
    /// Builds a palette keymap from a JSONC document, normally supplied with
    /// `include_str!`.
    ///
    /// The document replaces Fairy Dust's shipped defaults outright, so it binds
    /// `palette::open` as well as the application's own command ids. A document
    /// naming a command the registry does not declare is rejected whole, which
    /// leaves the palette unopenable.
    #[must_use]
    pub const fn new(defaults: &'static str) -> Self {
        Self {
            defaults,
            configuration_directory: KeymapConfigurationDirectory::Unconfigured,
        }
    }

    /// Reads and writes the user's own keymap under `app_name` in the platform
    /// configuration directory.
    ///
    /// Without this the palette reports the unavailable configuration directory
    /// as a failure row, because a keymap the user cannot edit is a real
    /// limitation rather than a normal state.
    #[must_use]
    pub const fn for_application(mut self, app_name: &'static str) -> Self {
        self.configuration_directory = KeymapConfigurationDirectory::ForApplication(app_name);
        self
    }

    fn keymap_plugin(&self) -> KeymapPlugin {
        let keymap_plugin = KeymapPlugin::new().with_defaults(self.defaults);
        match self.configuration_directory {
            KeymapConfigurationDirectory::ForApplication(app_name) => {
                keymap_plugin.with_app_name(app_name)
            },
            KeymapConfigurationDirectory::Unconfigured => keymap_plugin,
        }
    }
}

/// Installs the command palette, its keymap runtime, and the embedded defaults
/// the registry is built from.
struct CommandPalettePlugin {
    keymap: CommandPaletteKeymap,
}

impl Plugin for CommandPalettePlugin {
    fn build(&self, app: &mut App) {
        ensure_plugin(app, EnhancedInputPlugin);
        assert!(
            !app.is_plugin_added::<KeymapPlugin>(),
            "fairy_dust: the command palette installs `KeymapPlugin` itself, and the application \
             already installed one. Pass the application's defaults to \
             `with_command_palette_keymap` and install the plugin once."
        );
        app.add_plugins(self.keymap.keymap_plugin());
        app.add_systems(
            PreUpdate,
            hand_keyboard_to_query_field.before(KeymapSystems::Route),
        );
        app.add_observer(toggle_command_palette)
            .add_observer(update_query)
            .add_observer(dispatch_selected_command)
            .add_observer(close_on_cancel)
            .add_observer(run_failure_action);
    }
}

/// The keymap the palette was installed with, so a second install request that
/// disagrees is refused rather than silently ignored.
#[derive(Resource)]
struct InstalledCommandPaletteKeymap(CommandPaletteKeymap);

/// Marks the spawned palette box so the observers can rebuild or despawn it.
#[derive(Component)]
struct CommandPalettePanel;

/// The repair actions the palette's failure rows currently offer, in row order.
///
/// The rows are rebuilt on every keystroke, so the click observer resolves a
/// clicked action against the rows that were rendered rather than recomputing
/// the diagnostics.
#[derive(Component)]
struct PaletteFailureActions(Vec<KeymapFailureAction>);

/// Adds the palette to `app` once.
///
/// A second request carrying a different keymap is refused: silently keeping the
/// first document would leave an application running bindings it never asked
/// for.
pub(crate) fn install(app: &mut App, keymap: CommandPaletteKeymap) {
    if let Some(installed) = app.world().get_resource::<InstalledCommandPaletteKeymap>() {
        assert!(
            installed.0 == keymap,
            "fairy_dust: the command palette is already installed with a different keymap. Call \
             `with_command_palette` or `with_command_palette_keymap` once."
        );
        return;
    }

    app.insert_resource(InstalledCommandPaletteKeymap(keymap.clone()));
    app.add_plugins(CommandPalettePlugin { keymap });
}

/// Hands the keyboard to the query field for as long as its IME session holds
/// the window's input lease, and hands it back when the session ends.
///
/// The keymap keeps routing throughout, so it never loses a release edge; it
/// simply dispatches nothing but `palette::open`, which is what lets the same
/// keystroke that opened the palette close it again.
fn hand_keyboard_to_query_field(
    ime_input_blocker: Option<Res<ImeInputBlocker>>,
    windows: Query<Entity, With<PrimaryWindow>>,
    mut keystroke_routing: ResMut<KeystrokeRouting>,
) {
    let query_field_owns_the_keyboard = match (ime_input_blocker, windows.single()) {
        (Some(ime_input_blocker), Ok(window)) => ime_input_blocker.blocks_window(window),
        (None, _) | (_, Err(_)) => false,
    };

    match (query_field_owns_the_keyboard, keystroke_routing.as_ref()) {
        (true, KeystrokeRouting::EveryBinding) => {
            *keystroke_routing =
                KeystrokeRouting::text_entry([CommandId::declared::<OpenCommandPaletteEvent>()]);
        },
        (false, KeystrokeRouting::TextEntry { .. }) => {
            *keystroke_routing = KeystrokeRouting::EveryBinding;
        },
        (true, KeystrokeRouting::TextEntry { .. }) | (false, KeystrokeRouting::EveryBinding) => {},
    }
}

/// Opens the palette, or closes it when the same command fires while it is open.
fn toggle_command_palette(
    _open: On<OpenCommandPaletteEvent>,
    windows: Query<(Entity, &Window), With<PrimaryWindow>>,
    panels: Query<Entity, With<CommandPalettePanel>>,
    keymap_load_failures: Res<KeymapLoadFailures>,
    keymap_path_availability: Res<KeymapPathAvailability>,
    keymap_bindings: Res<KeymapBindings>,
    command_registry: Option<Res<CommandRegistry>>,
    mut commands: Commands,
) {
    if let Ok(panel) = panels.single() {
        commands.entity(panel).despawn();
        return;
    }
    let Ok((window_entity, window)) = windows.single() else {
        return;
    };
    let Some(command_registry) = command_registry else {
        error!("fairy_dust: the command palette has no command registry to list");
        return;
    };

    let keymap_failures = keymap_failure_rows(&keymap_load_failures, &keymap_path_availability);
    let command_rows = command_rows(&command_registry, &keymap_bindings, "");
    let origin = palette_panel_origin(window);
    let panel_width = palette_panel_width(window);
    let panel = DiegeticPanel::screen()
        .size(Sizing::fixed(Px(panel_width)), Sizing::FIT)
        .anchor(Anchor::TopLeft)
        .screen_position(origin.x, origin.y)
        .picking(PanelPicking::INTERACTIVE)
        .with_tree(palette_tree(&PaletteView {
            query: "",
            selection: &PaletteSelection::Rejected(self::query::PaletteRejection::EmptyQuery),
            commands: &command_rows,
            keymap_failures: &keymap_failures,
            panel_width,
        }))
        .build();
    let panel = match panel {
        Ok(panel) => panel,
        Err(error) => {
            error!("fairy_dust: failed to build the command palette: {error}");
            return;
        },
    };

    let panel = commands
        .spawn((
            CommandPalettePanel,
            PaletteFailureActions(
                keymap_failures
                    .iter()
                    .map(|keymap_failure| keymap_failure.action.clone())
                    .collect(),
            ),
            panel,
            Transform::default(),
        ))
        .id();
    commands.trigger(ImeOpenSession {
        target:       ImeTarget::ScreenPanelField {
            panel,
            field_id: FIELD_ID.into(),
        },
        window:       window_entity,
        initial_text: String::new(),
        field_spec:   ImeEditableFieldSpec::AppOwned(ImeAppOwnedFieldSpec::new(FIELD_ID)),
        anchor:       None,
    });
}

/// Re-renders the palette for each keystroke the IME session commits to its buffer.
fn update_query(
    changed: On<ImeTextChanged>,
    windows: Query<&Window, With<PrimaryWindow>>,
    panels: Query<Entity, With<CommandPalettePanel>>,
    keymap_load_failures: Res<KeymapLoadFailures>,
    keymap_path_availability: Res<KeymapPathAvailability>,
    keymap_bindings: Res<KeymapBindings>,
    command_registry: Option<Res<CommandRegistry>>,
    mut commands: Commands,
) {
    if !targets_palette(&changed.target) {
        return;
    }
    rebuild_palette(
        &windows,
        &panels,
        &keymap_load_failures,
        &keymap_path_availability,
        &keymap_bindings,
        command_registry.as_deref(),
        &changed.snapshot.committed_text,
        &mut commands,
    );
}

/// Runs the selected command when the IME session commits, and reports why it
/// could not when the query resolved to no runnable command.
fn dispatch_selected_command(
    commit: On<ImeCommitRequested>,
    authority: Res<ImeCommitAuthority>,
    panels: Query<Entity, With<CommandPalettePanel>>,
    command_registry: Option<Res<CommandRegistry>>,
    mut commands: Commands,
) {
    if !targets_palette(&commit.target)
        || !authority.is_current(commit.session_id, commit.attempt_id)
    {
        return;
    }
    let Some(command_registry) = command_registry else {
        return;
    };

    let selection = resolve_selection(&command_registry, &commit.text);
    let command_id = match selection {
        PaletteSelection::Selected(command_id) => command_id,
        PaletteSelection::Rejected(rejection) => {
            commands.trigger(ImeRejectCommit {
                session_id: commit.session_id,
                attempt_id: commit.attempt_id,
                reason:     ImeRejection::AppOwned(rejection.reason().to_owned()),
            });
            return;
        },
    };

    commands.trigger(ImeAcceptCommit {
        session_id: commit.session_id,
        attempt_id: commit.attempt_id,
        result:     ImeAppliedResult::AppOwned {
            display_text:   None,
            value_revision: None,
        },
    });
    if let Ok(panel) = panels.single() {
        commands.entity(panel).despawn();
    }
    commands.queue(move |world: &mut World| invoke_command(&command_id, world));
}

/// Closes the palette when the IME session is canceled, which is what Escape does.
fn close_on_cancel(
    canceled: On<ImeCanceled>,
    panels: Query<Entity, With<CommandPalettePanel>>,
    mut commands: Commands,
) {
    if !targets_palette(&canceled.target) {
        return;
    }
    if let Ok(panel) = panels.single() {
        commands.entity(panel).despawn();
    }
}

/// Opens the file or reveals the directory the clicked failure row names.
fn run_failure_action(
    clicked: On<ButtonClicked>,
    palette_failure_actions: Query<&PaletteFailureActions, With<CommandPalettePanel>>,
) {
    let FailureActionRow::Row(row_index) = failure_action_row_index(&clicked.id.to_string()) else {
        return;
    };
    let Ok(palette_failure_actions) = palette_failure_actions.single() else {
        return;
    };
    match palette_failure_actions.0.get(row_index) {
        Some(KeymapFailureAction::OpenFile(path)) => open_path(path, RevealInParent::No),
        Some(KeymapFailureAction::RevealDirectory(path)) => open_path(path, RevealInParent::Yes),
        Some(KeymapFailureAction::NoAction) | None => {},
    }
}

/// Whether the platform opener selects the path inside its parent rather than
/// opening the path itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RevealInParent {
    Yes,
    No,
}

/// Hands `path` to the platform's file handler.
fn open_path(path: &Path, reveal_in_parent: RevealInParent) {
    #[cfg(target_os = "macos")]
    let mut opener = {
        let mut opener = Command::new("open");
        if reveal_in_parent == RevealInParent::Yes {
            opener.arg("-R");
        }
        opener
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut opener = {
        let _ = reveal_in_parent;
        Command::new("xdg-open")
    };
    #[cfg(windows)]
    let mut opener = {
        let _ = reveal_in_parent;
        Command::new("explorer")
    };

    if let Err(error) = opener.arg(path).spawn() {
        error!(
            "fairy_dust: the command palette could not open {}: {error}",
            path.display()
        );
    }
}

fn rebuild_palette(
    windows: &Query<&Window, With<PrimaryWindow>>,
    panels: &Query<Entity, With<CommandPalettePanel>>,
    keymap_load_failures: &KeymapLoadFailures,
    keymap_path_availability: &KeymapPathAvailability,
    keymap_bindings: &KeymapBindings,
    command_registry: Option<&CommandRegistry>,
    palette_query: &str,
    commands: &mut Commands,
) {
    let (Ok(panel), Ok(window), Some(command_registry)) =
        (panels.single(), windows.single(), command_registry)
    else {
        return;
    };
    let keymap_failures = keymap_failure_rows(keymap_load_failures, keymap_path_availability);
    let command_rows = command_rows(command_registry, keymap_bindings, palette_query);
    let origin = palette_panel_origin(window);
    let panel_width = palette_panel_width(window);
    let tree = palette_tree(&PaletteView {
        query: palette_query,
        selection: &resolve_selection(command_registry, palette_query),
        commands: &command_rows,
        keymap_failures: &keymap_failures,
        panel_width,
    });
    commands.entity(panel).insert(PaletteFailureActions(
        keymap_failures
            .iter()
            .map(|keymap_failure| keymap_failure.action.clone())
            .collect(),
    ));
    commands
        .entity(panel)
        .entry::<DiegeticPanel>()
        .and_modify(move |mut palette_panel| {
            let _ = palette_panel.set_screen_size(Sizing::fixed(Px(panel_width)), Sizing::FIT);
            let _ = palette_panel.set_screen_position(origin);
        });
    commands.trigger(ImeReplacePanelTree { panel, tree });
}

fn invoke_command(command_id: &CommandId, world: &mut World) {
    let outcome = world.resource_scope(|world, command_registry: Mut<CommandRegistry>| {
        command_registry.invoke(command_id, world)
    });
    match outcome {
        CommandInvocationOutcome::Invoked => {},
        CommandInvocationOutcome::UnknownCommand => {
            error!("fairy_dust: the command palette selected an unknown command {command_id}");
        },
        CommandInvocationOutcome::HeldCommandRequiresPhase => {
            error!("fairy_dust: the command palette selected the hold-to-act command {command_id}");
        },
    }
}

fn targets_palette(target: &ImeTarget) -> bool {
    matches!(target, ImeTarget::ScreenPanelField { field_id, .. }
        if field_id.to_string() == FIELD_ID)
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
pub(crate) mod test_support {
    use std::str::FromStr;

    use bevy::prelude::App;
    use bevy::prelude::Event;
    use bevy::prelude::Reflect;
    use bevy::prelude::ReflectEvent;
    use bevy_enhanced_input::prelude::InputAction;
    use hana_rubric::CommandId;
    use hana_rubric::CommandRegistry;
    use hana_rubric::KeymapPlugin;
    use hana_rubric::ReflectKeymapCommand;
    use hana_rubric::command;

    use super::FAIRY_DUST_DEFAULT_KEYMAP;

    pub(crate) const DISPATCH_COMMAND_ID: &str = "palette_test::dispatch";
    pub(crate) const DISPATCH_COMMAND_TITLE: &str = "Palette Test Dispatch";
    pub(crate) const HELD_COMMAND_ID: &str = "palette_test::hold";
    pub(crate) const OPEN_COMMAND_ID: &str = "palette::open";
    pub(crate) const UNREMAPPABLE_COMMAND_ID: &str = "palette_test::recover";

    command! {
        action:      PaletteTestDispatch,
        event:       PaletteTestDispatchEvent,
        id:          "palette_test::dispatch",
        title:       "Palette Test Dispatch",
        description: "Dispatched by the palette selection test.",
    }

    command! {
        held,
        action:      PaletteTestHold,
        event:       PaletteTestHoldEvent,
        id:          "palette_test::hold",
        title:       "Palette Test Hold",
        description: "Hold-to-act, so the palette must not list it.",
    }

    command! {
        action:      PaletteTestRecover,
        event:       PaletteTestRecoverEvent,
        id:          "palette_test::recover",
        title:       "Palette Test Recover",
        description: "Permanently bound, and still reachable from the palette.",
        capability:  Unremappable,
    }

    /// Builds a finished app whose `CommandRegistry` holds every command this
    /// crate declares, from the document Fairy Dust actually ships. No
    /// application name is configured, so no disk worker starts and no
    /// configuration directory is touched.
    pub(crate) fn palette_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(KeymapPlugin::new().with_defaults(FAIRY_DUST_DEFAULT_KEYMAP));
        app.finish();
        app
    }

    pub(crate) fn registry(app: &App) -> &CommandRegistry {
        app.world().resource::<CommandRegistry>()
    }

    pub(crate) fn command_id(text: &str) -> CommandId {
        CommandId::from_str(text).expect("test command ids are valid")
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::ResMut;
    use bevy::prelude::Resource;
    use hana_rubric::CommandKeystroke;

    use self::test_support::DISPATCH_COMMAND_ID;
    use self::test_support::DISPATCH_COMMAND_TITLE;
    use self::test_support::OPEN_COMMAND_ID;
    use self::test_support::PaletteTestDispatchEvent;
    use self::test_support::command_id;
    use self::test_support::palette_test_app;
    use super::*;

    #[derive(Default, Resource)]
    struct DispatchedCommands(usize);

    /// The palette dispatches through its own selection and invocation path,
    /// not by reaching into the registry, so the test drives the same code the
    /// Enter key does.
    #[test]
    fn selecting_a_row_dispatches_through_the_palette() {
        let mut app = palette_test_app();
        app.init_resource::<DispatchedCommands>().add_observer(
            |_dispatched: On<PaletteTestDispatchEvent>, mut counted: ResMut<DispatchedCommands>| {
                counted.0 += 1;
            },
        );

        let selection = resolve_selection(
            app.world().resource::<CommandRegistry>(),
            DISPATCH_COMMAND_TITLE,
        );
        let selected = command_id(DISPATCH_COMMAND_ID);
        assert_eq!(selection, PaletteSelection::Selected(selected.clone()));

        invoke_command(&selected, app.world_mut());

        assert_eq!(app.world().resource::<DispatchedCommands>().0, 1);
    }

    /// The shipped document binds only Fairy Dust's own command, so installing
    /// the palette in an application that declares nothing else still compiles
    /// a keymap and leaves `palette::open` bound.
    #[test]
    fn the_shipped_defaults_bind_the_palette_open_command() {
        let app = palette_test_app();

        let keymap_bindings = app.world().resource::<KeymapBindings>();

        assert!(matches!(
            keymap_bindings.keystroke(&command_id(OPEN_COMMAND_ID)),
            CommandKeystroke::BoundTo(_)
        ));
    }

    /// A row for a command the keymap binds nothing to prints no keystroke, so
    /// the column stays empty rather than falling back to the command id.
    #[test]
    fn an_unbound_command_row_carries_no_keystroke() {
        let app = palette_test_app();
        let rows = command_rows(
            app.world().resource::<CommandRegistry>(),
            app.world().resource::<KeymapBindings>(),
            DISPATCH_COMMAND_TITLE,
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].keystroke, self::query::RowKeystroke::Unbound);
    }

    #[test]
    fn installing_the_palette_twice_with_the_same_keymap_is_one_install() {
        let mut app = App::new();
        install(
            &mut app,
            CommandPaletteKeymap::new(FAIRY_DUST_DEFAULT_KEYMAP),
        );
        install(
            &mut app,
            CommandPaletteKeymap::new(FAIRY_DUST_DEFAULT_KEYMAP),
        );

        assert!(
            app.world()
                .contains_resource::<InstalledCommandPaletteKeymap>()
        );
    }

    #[test]
    #[should_panic(expected = "already installed with a different keymap")]
    fn installing_the_palette_twice_with_different_keymaps_is_refused() {
        let mut app = App::new();
        install(
            &mut app,
            CommandPaletteKeymap::new(FAIRY_DUST_DEFAULT_KEYMAP),
        );
        install(&mut app, CommandPaletteKeymap::new(r#"{ "bindings": [] }"#));
    }
}
