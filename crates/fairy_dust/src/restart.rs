//! Capability: Ctrl+Shift+R re-launches the example binary.
//!
//! Wires the keybinding through `bevy_enhanced_input` using the `bevy_kana`
//! macros (`action!`, `event!`, `bind_action_system!`). The bound system
//! sets a process-wide flag and triggers `AppExit::Success`. After the Bevy
//! event loop returns, [`crate::SprinkleBuilder::run`] spawns a *trampoline*
//! copy of the same binary (marked via env var) and exits. The trampoline
//! sleeps long enough for the parent to be fully reaped — releasing the BRP
//! TCP socket, GPU device handle, Cocoa runloop, etc. — then `exec`s itself
//! without the env var, becoming a clean new instance.

use std::path::Path;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering;

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy_enhanced_input::prelude::*;
use bevy_kana::action;
use bevy_kana::bind_action_system;
use bevy_kana::event;

use crate::constants::TRAMPOLINE_ENV;
use crate::constants::TRAMPOLINE_SLEEP;
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

/// Spawn a trampoline copy of the current binary if a `Ctrl+Shift+R` press
/// requested it.
pub(crate) fn perform_restart_if_requested() {
    if RESTART_STATE.load(Ordering::SeqCst) != RestartState::Requested.to_u8() {
        return;
    }
    do_restart();
}

/// Called from [`crate::sprinkle_example`] before any Bevy state is built.
/// If this process was spawned as a restart trampoline, sleep so the parent
/// is fully reaped, then `exec` ourselves without the trampoline marker.
pub(crate) fn handle_trampoline_if_active() {
    if std::env::var(TRAMPOLINE_ENV).is_err() {
        return;
    }
    std::thread::sleep(TRAMPOLINE_SLEEP);
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            eprintln!("fairy_dust: current_exe failed: {err}");
            std::process::exit(1);
        },
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    trampoline_relaunch(&exe, &args);
}

#[cfg(unix)]
fn do_restart() {
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            eprintln!("fairy_dust: current_exe failed: {err}");
            std::process::exit(1);
        },
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    match std::process::Command::new(&exe)
        .args(&args)
        .env(TRAMPOLINE_ENV, "1")
        .spawn()
    {
        Ok(_) => {},
        Err(err) => {
            eprintln!("fairy_dust: failed to spawn trampoline: {err}");
            std::process::exit(1);
        },
    }
    std::process::exit(0);
}

#[cfg(unix)]
fn trampoline_relaunch(exe: &Path, args: &[String]) -> ! {
    match std::process::Command::new(exe)
        .args(args)
        .env_remove(TRAMPOLINE_ENV)
        .spawn()
    {
        Ok(_) => {},
        Err(err) => eprintln!("fairy_dust: trampoline relaunch failed: {err}"),
    }
    std::process::exit(0);
}

#[cfg(windows)]
fn do_restart() {
    spawn_trampoline();
    std::process::exit(0);
}

#[cfg(windows)]
fn spawn_trampoline() {
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            eprintln!("fairy_dust: current_exe failed: {err}");
            return;
        },
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    match std::process::Command::new(&exe)
        .args(&args)
        .env(TRAMPOLINE_ENV, "1")
        .spawn()
    {
        Ok(_) => {},
        Err(err) => eprintln!("fairy_dust: failed to spawn trampoline: {err}"),
    }
}

#[cfg(windows)]
fn trampoline_relaunch(exe: &Path, args: &[String]) -> ! {
    match std::process::Command::new(exe)
        .args(args)
        .env_remove(TRAMPOLINE_ENV)
        .spawn()
    {
        Ok(_) => {},
        Err(err) => eprintln!("fairy_dust: trampoline relaunch failed: {err}"),
    }
    std::process::exit(0);
}

#[cfg(not(any(unix, windows)))]
fn do_restart() {
    eprintln!("fairy_dust: restart not supported on this platform");
}

#[cfg(not(any(unix, windows)))]
fn trampoline_relaunch(_: &Path, _: &[String]) -> ! {
    eprintln!("fairy_dust: restart not supported on this platform");
    std::process::exit(1);
}
