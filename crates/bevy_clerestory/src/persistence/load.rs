//! Window state loading and path resolution.

use std::collections::HashMap;
use std::env::current_exe;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use bevy::prelude::*;
use dirs::config_dir;

use super::CapturedWindowStates;
use super::constants::EXAMPLES_DIRECTORY_NAME;
use super::constants::RON_EXTENSION;
use super::format;
use super::format::WindowKey;
use super::window_state::PersistedWindowState;
#[cfg(test)]
use super::window_state::SavedWindowMode;
use crate::constants::STATE_FILE;
use crate::restore_window_config::RestoreWindowConfig;

/// Get the default state file path using the executable name.
///
/// When the executable lives in a Cargo `examples/` directory (the standard
/// layout for `cargo run --example`), state is stored as
/// `config_dir()/<crate>/<example>.ron` so that all examples for a crate are
/// grouped together. Regular binaries use `config_dir()/<executable_name>/windows.ron`.
pub(crate) fn get_default_state_path() -> Option<PathBuf> {
    let executable = current_exe().ok()?;
    let executable_name = executable.file_stem()?.to_str()?;
    let is_cargo_example =
        executable.parent().and_then(Path::file_name) == Some(EXAMPLES_DIRECTORY_NAME.as_ref());

    if is_cargo_example {
        config_dir().map(|config_dir| {
            config_dir
                .join(env!("CARGO_PKG_NAME"))
                .join(format!("{executable_name}{RON_EXTENSION}"))
        })
    } else {
        config_dir().map(|config_dir| config_dir.join(executable_name).join(STATE_FILE))
    }
}

/// Get the state file path for a given app name.
///
/// Returns `config_dir()/<app_name>/windows.ron`
pub(crate) fn get_state_path_for_app(app_name: &str) -> Option<PathBuf> {
    config_dir().map(|config_dir| config_dir.join(app_name).join(STATE_FILE))
}

/// Load all window states from the given path.
///
/// Supports migration from the old single-window format: if the file contains
/// a single `PersistedWindowState`, it is wrapped as `{"primary": state}`.
pub(super) fn load_all_states(path: &Path) -> Option<HashMap<WindowKey, PersistedWindowState>> {
    let contents = fs::read_to_string(path).ok()?;
    format::decode(&contents)
}

/// Seed [`CapturedWindowStates`] from the state file once during `PreStartup`.
pub(super) fn load_captured_window_states(
    config: Res<RestoreWindowConfig>,
    mut captured_window_states: ResMut<CapturedWindowStates>,
) {
    if captured_window_states.startup_was_read() {
        return;
    }
    #[cfg(test)]
    captured_window_states.record_file_read();
    let persisted = load_all_states(&config.path).unwrap_or_default();
    captured_window_states.seed(persisted);
}

#[cfg(test)]
#[allow(clippy::panic, reason = "tests should panic on unexpected values")]
mod tests {
    use std::collections::HashMap;
    use std::fs;

    use bevy::ecs::schedule::Schedule;
    use bevy::ecs::world::World;
    use tempfile::NamedTempFile;

    use super::PersistedWindowState;
    use super::SavedWindowMode;
    use super::WindowKey;
    use crate::constants::CURRENT_STATE_VERSION;
    use crate::constants::DEFAULT_SCALE_FACTOR;
    use crate::persistence::CapturedWindowStates;
    use crate::persistence::load;
    use crate::persistence::save;
    use crate::restore_window_config::RestoreWindowConfig;

    fn sample_state() -> PersistedWindowState {
        PersistedWindowState {
            logical_position:  Some((10, 20)),
            logical_width:     800,
            logical_height:    600,
            scale:             DEFAULT_SCALE_FACTOR,
            monitor:           0,
            saved_window_mode: SavedWindowMode::Windowed,
            app_name:          "test-app".to_string(),
        }
    }

    #[test]
    fn save_then_load_roundtrip_v2() {
        let file = match NamedTempFile::new() {
            Ok(file) => file,
            Err(error) => panic!("failed to create temp file: {error}"),
        };
        let path = file.path();

        let states = HashMap::from([
            (WindowKey::Primary, sample_state()),
            (WindowKey::Managed("primary".to_string()), sample_state()),
        ]);
        save::save_all_states(path, &states);

        let loaded = load::load_all_states(path);
        assert!(loaded.is_some(), "expected saved v1 state to load");
        let loaded = loaded.unwrap_or_default();
        assert!(loaded.contains_key(&WindowKey::Primary));
        assert!(loaded.contains_key(&WindowKey::Managed("primary".to_string())));
    }

    #[test]
    fn legacy_single_window_read_then_save_rewrites_as_v2() {
        let file = match NamedTempFile::new() {
            Ok(file) => file,
            Err(error) => panic!("failed to create temp file: {error}"),
        };
        let path = file.path();
        // Legacy format uses `width`/`height` field names (pre-multi-window era)
        let legacy_contents = "\
(
    position: Some((10, 20)),
    width: 800,
    height: 600,
    monitor_index: 0,
    mode: Windowed,
    app_name: \"test-app\",
)";

        if let Err(error) = fs::write(path, legacy_contents) {
            panic!("failed to write legacy content: {error}");
        }

        let states = load::load_all_states(path);
        assert!(states.is_some(), "expected legacy content to decode");
        let states = states.unwrap_or_default();
        save::save_all_states(path, &states);

        let contents = fs::read_to_string(path);
        assert!(contents.is_ok(), "expected rewritten file to be readable");
        let contents = contents.unwrap_or_default();
        assert!(
            contents.contains(&format!("version: {CURRENT_STATE_VERSION}")),
            "expected rewritten file to contain v2 version marker"
        );
        assert!(
            contents.contains("logical_width: 800"),
            "expected rewritten file to contain logical_width"
        );
    }

    #[test]
    fn startup_loader_reads_and_seeds_once() {
        let file = match NamedTempFile::new() {
            Ok(file) => file,
            Err(error) => panic!("failed to create temp file: {error}"),
        };
        save::save_all_states(
            file.path(),
            &HashMap::from([(WindowKey::Primary, sample_state())]),
        );

        let mut world = World::new();
        world.insert_resource(RestoreWindowConfig {
            path: file.path().to_path_buf(),
        });
        world.init_resource::<CapturedWindowStates>();
        let mut schedule = Schedule::default();
        schedule.add_systems(load::load_captured_window_states);

        schedule.run(&mut world);
        schedule.run(&mut world);

        let captured = world.resource::<CapturedWindowStates>();
        assert_eq!(captured.activity().file_reads, 1);
        assert!(captured.persisted(&WindowKey::Primary).is_some());
    }
}
