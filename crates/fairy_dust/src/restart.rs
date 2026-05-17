//! Capability: Ctrl+Shift+R rebuilds and re-launches the example via cargo.
//!
//! Wires the keybinding through `bevy_enhanced_input` using the `bevy_kana`
//! macros (`action!`, `event!`, `bind_action_system!`). The bound system
//! sets a process-wide flag and triggers `AppExit::Success`. After the Bevy
//! event loop returns, [`crate::SprinkleBuilder::run`] spawns
//! `cargo run --example <name>` from the workspace root, then exits. Cargo
//! handles incremental rebuild and launches the fresh binary.
//!
//! The example name and workspace root are derived from `current_exe()`:
//! cargo writes example binaries to `<workspace>/target/<profile>/examples/<name>`.

use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering;

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy_enhanced_input::prelude::*;
use bevy_kana::action;
use bevy_kana::bind_action_system;
use bevy_kana::event;

use crate::ensure_plugin;

#[derive(Component)]
struct FairyDustRestartContext;

action!(Restart);
event!(RestartEvent);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RestartState {
    Idle,
    Requested,
}

impl RestartState {
    const fn to_u8(self) -> u8 {
        match self {
            Self::Idle => 0,
            Self::Requested => 1,
        }
    }
}

static RESTART_STATE: AtomicU8 = AtomicU8::new(RestartState::Idle.to_u8());

pub(crate) fn install(app: &mut App) {
    RESTART_STATE.store(RestartState::Idle.to_u8(), Ordering::SeqCst);
    ensure_plugin(app, EnhancedInputPlugin);
    app.add_input_context::<FairyDustRestartContext>();
    app.add_systems(Startup, spawn_restart_action);
    bind_action_system!(app, Restart, RestartEvent, request_restart);
}

fn spawn_restart_action(mut commands: Commands) {
    commands.spawn((
        FairyDustRestartContext,
        actions!(FairyDustRestartContext[
            (
                Action::<Restart>::new(),
                bindings![KeyCode::KeyR.with_mod_keys(ModKeys::SHIFT | ModKeys::CONTROL)],
            ),
        ]),
    ));
}

fn request_restart(mut exit: MessageWriter<AppExit>) {
    RESTART_STATE.store(RestartState::Requested.to_u8(), Ordering::SeqCst);
    exit.write(AppExit::Success);
}

/// Spawn `cargo run --example <name>` if a `Ctrl+Shift+R` press requested it.
pub(crate) fn perform_restart_if_requested() {
    if RESTART_STATE.load(Ordering::SeqCst) != RestartState::Requested.to_u8() {
        return;
    }
    do_restart();
}

#[cfg(any(unix, windows))]
fn do_restart() {
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            eprintln!("fairy_dust: current_exe failed: {err}");
            std::process::exit(1);
        },
    };
    let Some((example_name, workspace_root)) = derive_cargo_args(&exe) else {
        eprintln!(
            "fairy_dust: could not derive cargo example context from {}; \
             restart requires the binary to live at \
             <workspace>/target/<profile>/examples/<name>",
            exe.display(),
        );
        std::process::exit(1);
    };
    match std::process::Command::new("cargo")
        .args(["run", "--example", &example_name])
        .current_dir(&workspace_root)
        .spawn()
    {
        Ok(_) => std::process::exit(0),
        Err(err) => {
            eprintln!("fairy_dust: failed to spawn `cargo run`: {err}");
            std::process::exit(1);
        },
    }
}

#[cfg(not(any(unix, windows)))]
fn do_restart() {
    eprintln!("fairy_dust: restart not supported on this platform");
}

/// Recover the example name and workspace root from the running binary's path.
///
/// Expects the path layout cargo produces for examples:
/// `<workspace>/target/<profile>/examples/<name>`.
#[cfg(any(unix, windows))]
fn derive_cargo_args(exe: &Path) -> Option<(String, PathBuf)> {
    let name = exe.file_name()?.to_str()?.to_string();
    let examples_dir = exe.parent()?;
    if examples_dir.file_name()?.to_str()? != "examples" {
        return None;
    }
    let profile_dir = examples_dir.parent()?;
    let target_dir = profile_dir.parent()?;
    if target_dir.file_name()?.to_str()? != "target" {
        return None;
    }
    Some((name, target_dir.parent()?.to_path_buf()))
}
