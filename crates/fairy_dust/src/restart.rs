//! Capability: Ctrl+Shift+R rebuilds and re-launches the example via cargo.
//!
//! Wires the keybinding through `bevy_enhanced_input` using the `bevy_kana`
//! macros (`action!`, `event!`, `bind_action_system!`). The bound system
//! spawns `cargo run --example <name>` from the workspace root, then exits
//! the current process directly with `std::process::exit(0)`.
//!
//! Bypassing `AppExit` is deliberate: on macOS the winit run loop can fail
//! to honor `AppExit::Success` cleanly, leaving the old window stuck. A
//! direct exit always works and lets cargo handle the incremental rebuild
//! and re-launch.
//!
//! The example name and workspace root are derived from `current_exe()`:
//! cargo writes example binaries to `<workspace>/target/<profile>/examples/<name>`.

use std::path::Path;
use std::path::PathBuf;

use bevy::prelude::*;
use bevy_enhanced_input::prelude::*;
use bevy_kana::action;
use bevy_kana::bind_action_system;
use bevy_kana::event;
use bevy_lagrange::OrbitCam;

use crate::constants::CARGO_BIN;
use crate::constants::CARGO_EXAMPLE_FLAG;
use crate::constants::CARGO_EXAMPLES_DIR;
use crate::constants::CARGO_RUN_SUBCOMMAND;
use crate::constants::CARGO_TARGET_DIR;
use crate::ensure_plugin;
use crate::orbit_cam::FairyDustOrbitCam;
use crate::restart_camera;
use crate::restart_camera::RestartCameraRestore;

#[derive(Component)]
struct FairyDustRestartContext;

action!(Restart);
event!(RestartEvent);

pub(crate) fn install(app: &mut App) {
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

fn request_restart(
    cameras: Query<&OrbitCam, With<FairyDustOrbitCam>>,
    restore_state: Option<Res<RestartCameraRestore>>,
) {
    info!("fairy_dust restart: Ctrl+Shift+R pressed, invoking cargo and exiting");
    let encoded_pose = restart_camera::encode_child_pose(&cameras, restore_state.as_deref());
    do_restart(encoded_pose);
}

/// No-op now that restart exits the process directly from the input handler.
/// Retained so [`crate::SprinkleBuilder::run`] doesn't need a cfg branch.
pub(crate) const fn perform_restart_if_requested() {}

#[cfg(any(unix, windows))]
fn do_restart(encoded_pose: Option<String>) {
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
    info!(
        "fairy_dust restart: spawning `cargo run --example {}` in {}",
        example_name,
        workspace_root.display(),
    );
    let mut command = std::process::Command::new(CARGO_BIN);
    command
        .args([CARGO_RUN_SUBCOMMAND, CARGO_EXAMPLE_FLAG, &example_name])
        .current_dir(&workspace_root);
    restart_camera::apply_child_env(&mut command, encoded_pose);
    match command.spawn() {
        Ok(child) => {
            info!("fairy_dust restart: cargo spawned as pid {}", child.id());
            std::process::exit(0);
        },
        Err(err) => {
            eprintln!("fairy_dust: failed to spawn `cargo run`: {err}");
            std::process::exit(1);
        },
    }
}

#[cfg(not(any(unix, windows)))]
fn do_restart(_: Option<String>) {
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
    if examples_dir.file_name()?.to_str()? != CARGO_EXAMPLES_DIR {
        return None;
    }
    let profile_dir = examples_dir.parent()?;
    let target_dir = profile_dir.parent()?;
    if target_dir.file_name()?.to_str()? != CARGO_TARGET_DIR {
        return None;
    }
    Some((name, target_dir.parent()?.to_path_buf()))
}
