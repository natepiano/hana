//! Reads the generated companion files from a downstream keymap application.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use bevy::prelude::App;
use bevy::prelude::States;
use bevy::state::app::AppExtStates;
use bevy::state::app::StatesPlugin;
use hana_rubric::KeymapCommand;
use hana_rubric::KeymapPathAvailability;
use hana_rubric::KeymapPlugin;
use serde_json_lenient::Value;
use strum::AsRefStr;
use strum::EnumIter;
use strum::EnumMessage;
use strum::IntoEnumIterator;

const TEST_APP_NAME: &str = "hana-rubric-keymap-companion-test";
const XDG_CONFIG_HOME: &str = "XDG_CONFIG_HOME";
const HELD_COMMAND_DESCRIPTION_SUFFIX: &str =
    " A held command can be bound to one keystroke only, not a multi-key sequence.";

static ENVIRONMENT_LOCK: Mutex<()> = Mutex::new(());
static NEXT_TEMPORARY_DIRECTORY_ID: AtomicUsize = AtomicUsize::new(0);

mod commands {
    use bevy::prelude::Event;
    use bevy::prelude::Reflect;
    use bevy::prelude::ReflectEvent;
    use bevy_enhanced_input::prelude::InputAction;
    use hana_rubric::ReflectKeymapCommand;
    use hana_rubric::command;

    command! {
        action:      OpenPaletteAction,
        event:       OpenPalette,
        id:          "demo::open_palette",
        title:       "Open Command Palette",
        description: "Opens the command palette for the keymap demo.",
    }

    command! {
        action:      OpenRecentAction,
        event:       OpenRecent,
        id:          "demo::open_recent",
        title:       "Open Recent Item",
        description: "Opens the most recently used item.",
    }

    command! {
        held,
        action:      MoveSelectionAction,
        event:       MoveSelection,
        id:          "demo::move_selection",
        title:       "Move Selection",
        description: "Moves the current selection while the key is held.",
    }

    command! {
        action:      SubmitEditAction,
        event:       SubmitEdit,
        id:          "demo::submit_edit",
        title:       "Submit Edit",
        description: "Submits the active edit.",
    }
}

#[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, Hash, PartialEq, States)]
#[strum(serialize_all = "snake_case")]
enum DemoContext {
    #[strum(message = "While the keymap demo is browsing commands")]
    Browsing,
    #[strum(message = "While the keymap demo is editing text")]
    Editing,
}

#[test]
fn companion_files_cover_every_registered_command_and_condition() -> Result<(), String> {
    let environment_lock = ENVIRONMENT_LOCK
        .lock()
        .map_err(|error| format!("environment lock failed: {error}"))?;
    let temporary_directory = TemporaryDirectory::new("companion-files")
        .map_err(|error| format!("temporary directory creation failed: {error}"))?;
    let xdg_config_home = XdgConfigHome::set(temporary_directory.path());
    let paths = KeymapPathAvailability::for_app_name(TEST_APP_NAME)
        .into_resolved()
        .map_err(|keymap_path_failure| {
            format!("test keymap paths did not resolve: {keymap_path_failure:?}")
        })?;

    assert!(
        paths
            .config_directory()
            .starts_with(temporary_directory.path())
    );
    assert!(
        !paths.schema().exists(),
        "the fresh configuration directory already contains a schema"
    );
    assert!(
        !paths.default_keymap().exists(),
        "the fresh configuration directory already contains a default keymap"
    );

    let mut app = App::new();
    app.add_plugins(StatesPlugin)
        .insert_state(DemoContext::Browsing)
        .register_type::<commands::OpenPalette>()
        .register_type::<commands::OpenRecent>()
        .register_type::<commands::MoveSelection>()
        .register_type::<commands::SubmitEdit>()
        .add_plugins(
            KeymapPlugin::new()
                .with_app_name(TEST_APP_NAME)
                .with_defaults(include_str!("../examples/keymap_demo.jsonc"))
                .for_state_context::<DemoContext>(),
        );
    app.finish();
    drop(app);

    let schema_source = fs::read_to_string(paths.schema())
        .map_err(|error| format!("published schema could not be read: {error}"))?;
    let default_source = fs::read_to_string(paths.default_keymap())
        .map_err(|error| format!("published default keymap could not be read: {error}"))?;
    let schema: Value = serde_json_lenient::from_str(&schema_source)
        .map_err(|error| format!("published schema is not JSON: {error}"))?;

    assert_default_keymap(&default_source)?;
    assert_schema_command_descriptions(&schema)?;
    assert_schema_condition_descriptions(&schema)?;

    drop(xdg_config_home);
    drop(temporary_directory);
    drop(environment_lock);
    Ok(())
}

fn assert_default_keymap(default_source: &str) -> Result<(), String> {
    let embedded_defaults = include_str!("../examples/keymap_demo.jsonc");
    let header = default_source
        .strip_suffix(embedded_defaults)
        .ok_or_else(|| String::from("published default does not end with the embedded keymap"))?;

    for context in DemoContext::iter() {
        let context_name = context.as_ref();
        let context_description = context
            .get_message()
            .ok_or_else(|| format!("`{context_name}` has no context description"))?;

        assert!(
            header.contains(context_name),
            "published default header does not name `{context_name}`"
        );
        assert!(
            header.contains(context_description),
            "published default header does not describe `{context_name}`"
        );
    }

    Ok(())
}

fn assert_schema_command_descriptions(schema: &Value) -> Result<(), String> {
    let alternatives =
        schema["properties"]["bindings"]["items"]["properties"]["bindings"]["additionalProperties"]
            ["anyOf"]
            .as_array()
            .ok_or_else(|| String::from("schema does not contain binding alternatives"))?;

    for (command_id, expected_description) in [
        (
            commands::OpenPalette::ID,
            <commands::OpenPalette as KeymapCommand>::DESCRIPTION,
        ),
        (
            commands::OpenRecent::ID,
            <commands::OpenRecent as KeymapCommand>::DESCRIPTION,
        ),
        (
            commands::MoveSelection::ID,
            <commands::MoveSelection as KeymapCommand>::DESCRIPTION,
        ),
        (
            commands::SubmitEdit::ID,
            <commands::SubmitEdit as KeymapCommand>::DESCRIPTION,
        ),
    ] {
        let command = alternatives
            .iter()
            .find(|alternative| {
                alternative.get("const").and_then(Value::as_str) == Some(command_id)
            })
            .ok_or_else(|| format!("schema does not contain `{command_id}`"))?;
        let description = command
            .get("description")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("schema does not describe `{command_id}`"))?;

        let expected_description = if command_id == commands::MoveSelection::ID {
            format!("{expected_description}{HELD_COMMAND_DESCRIPTION_SUFFIX}")
        } else {
            expected_description.to_owned()
        };
        assert_eq!(
            description, expected_description,
            "schema description for `{command_id}`"
        );
    }

    Ok(())
}

fn assert_schema_condition_descriptions(schema: &Value) -> Result<(), String> {
    let alternatives = schema["properties"]["bindings"]["items"]["properties"]["context"]["anyOf"]
        .as_array()
        .ok_or_else(|| String::from("schema does not contain context alternatives"))?;

    for context in DemoContext::iter() {
        let condition_name = context.as_ref();
        let condition_description = context
            .get_message()
            .ok_or_else(|| format!("`{condition_name}` has no context description"))?;
        let condition = alternatives
            .iter()
            .find(|alternative| {
                alternative.get("const").and_then(Value::as_str) == Some(condition_name)
            })
            .ok_or_else(|| format!("schema does not contain `{condition_name}`"))?;

        assert_eq!(
            condition.get("description").and_then(Value::as_str),
            Some(condition_description)
        );
    }

    Ok(())
}

struct TemporaryDirectory {
    path: PathBuf,
}

impl TemporaryDirectory {
    fn new(label: &str) -> io::Result<Self> {
        let directory_id = NEXT_TEMPORARY_DIRECTORY_ID.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!(
            "hana-rubric-{label}-{}-{directory_id}",
            std::process::id()
        ));

        if path.exists() {
            fs::remove_dir_all(&path)?;
        }
        fs::create_dir(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path { &self.path }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.path); }
}

struct XdgConfigHome {
    previous: Option<OsString>,
}

impl XdgConfigHome {
    fn set(path: &Path) -> Self {
        let previous = env::var_os(XDG_CONFIG_HOME);

        // SAFETY: ENVIRONMENT_LOCK serializes this test's process-wide environment mutation.
        unsafe { env::set_var(XDG_CONFIG_HOME, path) };

        Self { previous }
    }
}

impl Drop for XdgConfigHome {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(previous) => {
                // SAFETY: ENVIRONMENT_LOCK serializes this test's process-wide environment
                // mutation.
                unsafe { env::set_var(XDG_CONFIG_HOME, previous) };
            },
            None => {
                // SAFETY: ENVIRONMENT_LOCK serializes this test's process-wide environment
                // mutation.
                unsafe { env::remove_var(XDG_CONFIG_HOME) };
            },
        }
    }
}
