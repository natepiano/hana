//! Window state loading and path resolution.

use std::collections::HashMap;
use std::env::current_exe;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use bevy::prelude::*;
use dirs::config_dir;

use super::CapturedWindowStates;
use super::constants::BACKUP_CORRUPT_LABEL;
use super::constants::BACKUP_MAX_ATTEMPTS;
use super::constants::BACKUP_SUFFIX;
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
///
/// A file that exists but cannot be decoded is copied aside first — see
/// [`back_up_unreadable_state_file`].
pub(super) fn load_all_states(path: &Path) -> Option<HashMap<WindowKey, PersistedWindowState>> {
    let contents = fs::read_to_string(path).ok()?;
    let decoded = format::decode(&contents);
    if decoded.is_none() {
        back_up_unreadable_state_file(path, &contents);
    }
    decoded
}

/// Copy a state file aside before the app starts writing over it.
///
/// `decode` returns `None` for an unsupported version, a parse error, a duplicate key and a
/// truncated file alike. The caller seeds an empty state from that `None`, and the first frame
/// that marks any window dirty writes the empty state over the file — so without this copy the
/// user's saved positions are gone with no way back and no message saying so.
///
/// Saving is deliberately **not** suppressed afterwards. The copy already preserves the data, and
/// refusing to write would silently stop remembering window positions for the rest of the session
/// to protect a file that is already safe.
///
/// The one case this does not fully cover: running a newer build, then an older one, then the
/// newer one again leaves the newer file only as a backup. It is still recoverable by hand.
fn back_up_unreadable_state_file(path: &Path, contents: &str) {
    let version_label = format::probe_version(contents).map_or_else(
        || BACKUP_CORRUPT_LABEL.to_string(),
        |version| format!("v{version}"),
    );
    let Some(backup_path) = unused_backup_path(path, &version_label) else {
        warn!(
            "[back_up_unreadable_state_file] Could not find an unused backup name for {path:?}; \
             leaving it in place. Saved window positions will be replaced on the next write."
        );
        return;
    };

    match fs::copy(path, &backup_path) {
        Ok(_) => warn!(
            "[back_up_unreadable_state_file] Could not read {path:?} ({version_label}). \
             Copied it to {backup_path:?} and starting from an empty state; window positions \
             will be remembered again from now on."
        ),
        Err(error) => warn!(
            "[back_up_unreadable_state_file] Could not read {path:?} ({version_label}) and \
             could not copy it to {backup_path:?}: {error}. Saved window positions will be \
             replaced on the next write."
        ),
    }
}

/// First unused `<file>.bak.<label>` name, adding `.1`, `.2`, … so an earlier backup of the same
/// version is never clobbered by a later failure.
fn unused_backup_path(path: &Path, version_label: &str) -> Option<PathBuf> {
    let file_name = path.file_name()?.to_str()?;
    let directory = path.parent()?;
    let base = directory.join(format!("{file_name}.{BACKUP_SUFFIX}.{version_label}"));
    if !base.exists() {
        return Some(base);
    }
    (1..BACKUP_MAX_ATTEMPTS)
        .map(|attempt| {
            directory.join(format!(
                "{file_name}.{BACKUP_SUFFIX}.{version_label}.{attempt}"
            ))
        })
        .find(|candidate| !candidate.exists())
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
    use bevy::math::IVec2;
    use tempfile::NamedTempFile;

    use super::PersistedWindowState;
    use super::SavedWindowMode;
    use super::WindowKey;
    use crate::constants::CURRENT_STATE_VERSION;
    use crate::monitors::PanelIdentity;
    use crate::persistence::CapturedWindowStates;
    use crate::persistence::PersistedPosition;
    use crate::persistence::load;
    use crate::persistence::save;
    use crate::restore_window_config::RestoreWindowConfig;

    fn sample_state() -> PersistedWindowState {
        PersistedWindowState {
            monitor_panel:     PanelIdentity::Anonymous,
            position:          PersistedPosition::MonitorOffset(IVec2::new(10, 20)),
            logical_width:     800,
            logical_height:    600,
            monitor:           0,
            saved_window_mode: SavedWindowMode::Windowed,
            app_name:          "test-app".to_string(),
        }
    }

    #[test]
    fn save_then_load_roundtrip() {
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
        assert!(loaded.is_some(), "expected saved state to load");
        let loaded = loaded.unwrap_or_default();
        assert!(loaded.contains_key(&WindowKey::Primary));
        assert!(loaded.contains_key(&WindowKey::Managed("primary".to_string())));
    }

    #[test]
    fn legacy_single_window_read_then_save_rewrites_in_the_current_format() {
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

    /// An unreadable file must survive the launch that replaces it.
    ///
    /// The destructive path is reached going forward, not only on a downgrade: any decode failure
    /// seeds an empty state, and the first dirty frame writes that empty state over the file.
    #[test]
    fn an_undecodable_state_file_is_copied_aside_before_it_is_replaced() {
        let file = match NamedTempFile::new() {
            Ok(file) => file,
            Err(error) => panic!("failed to create temp file: {error}"),
        };
        let path = file.path();
        let future_contents = "(version: 250, entries: [])";
        if let Err(error) = fs::write(path, future_contents) {
            panic!("failed to write future-version content: {error}");
        }

        assert!(
            load::load_all_states(path).is_none(),
            "an unsupported version must not decode"
        );

        let backup_path = path.with_file_name(format!(
            "{}.bak.v250",
            match path.file_name().and_then(|name| name.to_str()) {
                Some(name) => name,
                None => panic!("temp file should have a name"),
            }
        ));
        let backup = fs::read_to_string(&backup_path);
        assert!(
            backup.is_ok(),
            "expected a backup at {backup_path:?} naming the version it could not read"
        );
        assert_eq!(backup.unwrap_or_default(), future_contents);
        let _ = fs::remove_file(&backup_path);
    }

    /// A file too damaged to probe still gets a backup, under a name that says so.
    #[test]
    fn an_unparseable_state_file_is_copied_aside_under_a_corrupt_label() {
        let file = match NamedTempFile::new() {
            Ok(file) => file,
            Err(error) => panic!("failed to create temp file: {error}"),
        };
        let path = file.path();
        if let Err(error) = fs::write(path, "(version: 3, entries: [") {
            panic!("failed to write truncated content: {error}");
        }

        assert!(load::load_all_states(path).is_none());

        let backup_path = path.with_file_name(format!(
            "{}.bak.corrupt",
            match path.file_name().and_then(|name| name.to_str()) {
                Some(name) => name,
                None => panic!("temp file should have a name"),
            }
        ));
        assert!(
            backup_path.exists(),
            "expected a corrupt-labelled backup at {backup_path:?}"
        );
        let _ = fs::remove_file(&backup_path);
    }

    /// A second failure of the same version must not clobber the first backup.
    #[test]
    fn a_second_backup_of_the_same_version_gets_its_own_name() {
        let file = match NamedTempFile::new() {
            Ok(file) => file,
            Err(error) => panic!("failed to create temp file: {error}"),
        };
        let path = file.path();
        let file_name = match path.file_name().and_then(|name| name.to_str()) {
            Some(name) => name.to_string(),
            None => panic!("temp file should have a name"),
        };

        if let Err(error) = fs::write(path, "(version: 250, entries: [])") {
            panic!("failed to write first content: {error}");
        }
        let _ = load::load_all_states(path);
        if let Err(error) = fs::write(path, "(version: 250, entries: [], extra: 1)") {
            panic!("failed to write second content: {error}");
        }
        let _ = load::load_all_states(path);

        let first = path.with_file_name(format!("{file_name}.bak.v250"));
        let second = path.with_file_name(format!("{file_name}.bak.v250.1"));
        assert!(first.exists(), "first backup missing");
        assert!(second.exists(), "second backup overwrote the first");
        let _ = fs::remove_file(&first);
        let _ = fs::remove_file(&second);
    }

    /// A readable file is never backed up — the copy is a failure path, not a save hook.
    #[test]
    fn a_decodable_state_file_is_not_copied_aside() {
        let file = match NamedTempFile::new() {
            Ok(file) => file,
            Err(error) => panic!("failed to create temp file: {error}"),
        };
        let path = file.path();
        let states = HashMap::from([(WindowKey::Primary, sample_state())]);
        save::save_all_states(path, &states);

        assert!(load::load_all_states(path).is_some());

        let file_name = match path.file_name().and_then(|name| name.to_str()) {
            Some(name) => name.to_string(),
            None => panic!("temp file should have a name"),
        };
        for label in ["v3", "corrupt"] {
            let backup = path.with_file_name(format!("{file_name}.bak.{label}"));
            assert!(!backup.exists(), "unexpected backup at {backup:?}");
        }
    }

    /// The write lands via a temporary file, and that temporary is gone once it succeeds.
    #[test]
    fn a_successful_save_leaves_no_temporary_file_behind() {
        let file = match NamedTempFile::new() {
            Ok(file) => file,
            Err(error) => panic!("failed to create temp file: {error}"),
        };
        let path = file.path();
        let states = HashMap::from([(WindowKey::Primary, sample_state())]);

        assert_eq!(
            save::save_all_states(path, &states),
            save::StateFileWrite::Written
        );

        assert!(!path.with_extension("ron.tmp").exists());
        assert!(load::load_all_states(path).is_some());
    }
}
