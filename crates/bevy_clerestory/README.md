# bevy_clerestory

[![License](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](https://github.com/natepiano/hana/tree/main/crates/bevy_clerestory#license)
[![Crates.io](https://img.shields.io/crates/v/bevy_clerestory.svg)](https://crates.io/crates/bevy_clerestory)
[![Downloads](https://img.shields.io/crates/d/bevy_clerestory.svg)](https://crates.io/crates/bevy_clerestory)
[![CI](https://github.com/natepiano/hana/actions/workflows/ci.yml/badge.svg)](https://github.com/natepiano/hana/actions/workflows/ci.yml)


A Bevy plugin that saves and restores window placement, handles mixed-scale
monitors, and can recover windows after a monitor reconnects.

## Motivation

Originally created as a mechanism to restore the `PrimaryWindow` to its last known position when launching - the way you expect an app to work. I quickly discovered that on my `MacBook` Pro with Retina display (scale factor 2.0) and my external monitor (scale factor 1.0), there were numerous issues with saving/restoring positions across differently-scaled monitors. 

The first discovered issue is that winit uses the scale factor of the focused window from which you launch the application. And if the target monitor for the app has a different scale factor, then that will get factored into the size and position calculations resulting in something you definitely don't want.

`bevy_clerestory` plugin works around this issue by using winit directly to capture actual monitor position/size/scale and comparing it to the target position/size for the window and does the conversions correctly.

Windows has similar scale factor issues, plus additional quirks like invisible window borders that prevent precise placement. Linux X11 has its own quirks with window manager keyboard shortcuts not firing position events. This plugin now supports macOS, Windows, and Linux (X11 and Wayland) with workarounds for platform-specific issues (see [Platform Support](#platform-support) for details).

Clerestory can also track monitor connections while an application is running and help recover windows after their monitor is disconnected and reconnected.

## Usage

```rust,no_run
use bevy::prelude::*;
use bevy_clerestory::WindowManagerPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(WindowManagerPlugin)
        .run();
}
```

For a complete interactive example with fullscreen mode switching, run:

```bash
cargo run --example restore_window
```

## API

This crate exposes several types for working with monitors and windows beyond the plugin itself. See [docs.rs](https://docs.rs/bevy_clerestory) for full API documentation.

### `Monitors` Resource

`Monitors` contains the displays currently known to Bevy. Its order comes from winit and can change
when displays are disconnected or reconnected, so an index is useful for the current inventory but
is not a permanent monitor identity.

- `monitors.at(physical_x, physical_y)` – Find the monitor containing a position (physical pixels)
- `monitors.by_index(index)` – Find the monitor currently reporting `index`
- `monitors.first()` – Get the first monitor in winit's current order; this is not necessarily the primary monitor
- `monitors.closest_to(physical_x, physical_y)` – Find the closest monitor to a position (physical pixels)
- `monitors.by_id(id)` – Find the one current monitor with a verified `MonitorId`
- `monitors.iter()` – Iterate over each current monitor entity and its `MonitorInfo`

### `MonitorInfo`

Information about one monitor: `identity`, `index`, `scale`, `physical_position`, and
`physical_size`. It is a copy of the monitor's data and does not contain its Bevy entity.

`identity` is `MonitorIdentity::Verified(MonitorId)` when the operating system provides enough
information to distinguish that physical monitor from the others. Otherwise it is
`MonitorIdentity::Unverified`, and Clerestory will not assume that a monitor with the same position,
connector, or index is the same physical device.

### `CurrentMonitor` Component

Automatically maintained on the primary window and every `ManagedWindow`. Query it to get monitor
information and the window's effective mode:

```rust
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_clerestory::CurrentMonitor;

fn my_system(q: Query<(&Window, &CurrentMonitor), With<PrimaryWindow>>) {
    let Ok((window, monitor)) = q.single() else {
        return;
    };
    println!("Monitor {}, scale {}", monitor.index, monitor.scale);
    println!("Effective mode: {:?}", monitor.effective_window_mode);
}
```

- `monitor.index`, `monitor.scale`, `monitor.physical_position`, `monitor.physical_size` – Monitor info (via `Deref<Target = MonitorInfo>`)
- `monitor.effective_window_mode` – The actual window mode, even when `window.mode` is stale (e.g., macOS green button fullscreen reports `Windowed` but the window is actually fullscreen)

### Plugin Configuration

- `WindowManagerPlugin` – Uses executable name for config directory
- `WindowManagerPlugin::with_app_name("name")` – Custom app name
- `WindowManagerPlugin::with_path(path)` – Full control over state file path
- `WindowManagerPlugin::with_persistence(mode)` – Set persistence behavior for managed windows

### Multi-Window Support

Add `ManagedWindow` to any secondary window to opt it into save/restore:

```rust
use bevy::prelude::*;
use bevy_clerestory::ManagedWindow;

# fn spawn_inspector(mut commands: Commands) {
commands.spawn((
    Window {
        title: "Inspector".into(),
        ..default()
    },
    ManagedWindow {
        name: "inspector".to_string(),
    },
));
# }
```

Each managed window gets the same restore treatment as the primary window — scale factor compensation, position clamping, and platform workarounds.

Control what happens when windows are closed with `ManagedWindowPersistence`:

- `RememberAll` (default) — closed windows keep their saved state for next launch
- `ActiveOnly` — only currently open windows are persisted

```rust
use bevy::prelude::App;
use bevy_clerestory::ManagedWindowPersistence;
use bevy_clerestory::WindowManagerPlugin;

# fn configure(app: &mut App) {
app.add_plugins(WindowManagerPlugin::with_persistence(ManagedWindowPersistence::ActiveOnly));
# }
```

See `examples/restore_window.rs` for a complete interactive example.

### Recover windows after a monitor reconnects

When a monitor is unplugged, the operating system may move its windows to another display. In other
cases, Bevy may delete a window entity because it was linked to the monitor entity that disappeared.
If the application remains running, Clerestory can remember which physical monitor a window belonged
to and help return that window when the same monitor reconnects.

Add `WindowRecovery` once to the initial `PrimaryWindow` or to a `ManagedWindow`. Clerestory accepts
the registration only after it can associate the window with one verified physical monitor. A
verified monitor is one the operating system has provided enough information to distinguish from
other monitors. Clerestory does not guess from its position, connector, or current index.

Choose how much of the recovery the application wants Clerestory to perform:

| Policy | Behavior |
| --- | --- |
| `Disabled` | The window does not participate in reconnect recovery. |
| `ApplicationControlled` | Clerestory reports when the target monitor disappears and returns. The application creates or selects the window, prepares its content, and sends `RestoreWindow`. |
| `FallbackAndReturn` | Clerestory tracks a surviving window that the operating system moved to another display, or creates one replacement `Window` when Bevy deleted it. It returns that window automatically when the target monitor comes back. |

For `FallbackAndReturn`, the window behaves as follows:

1. If the operating system moves the existing window to another display, Clerestory leaves it there
   while the target monitor is absent.
2. If Bevy deletes the window entity and another monitor is available, Clerestory creates one new
   `Window` using the saved window settings. That replacement starts on the first monitor in winit's
   current order, which is not necessarily the primary monitor.
3. When the same verified monitor returns, Clerestory moves the surviving or replacement window back
   to its saved placement.

Clerestory restores the Bevy `Window` and its settings. It does not clone application-owned cameras,
UI, or other content. The application must attach that content when the replacement gains its
`PrimaryWindow` or `ManagedWindow` component. Reconstructed windows may also return in a different
front-to-back order because Clerestory does not preserve window stacking order.

```rust
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_clerestory::ManagedWindow;
use bevy_clerestory::WindowRecovery;

fn register_windows(
    mut commands: Commands,
    primary: Single<Entity, With<PrimaryWindow>>,
) {
    commands
        .entity(*primary)
        .insert(WindowRecovery::FallbackAndReturn);

    commands.spawn((
        Window {
            title: "Inspector".into(),
            ..default()
        },
        ManagedWindow {
            name: "inspector".into(),
        },
        WindowRecovery::ApplicationControlled,
    ));
}
```

#### Change or cancel recovery

Once Clerestory starts tracking the window, changing or removing its `WindowRecovery` component does
not change the active policy or target monitor. To choose another policy, first send
`CancelWindowRecovery`, then remove the component and add the new policy.

Recovery is tracked by a stable `WindowKey`, not only by the current Bevy entity. The primary window
uses `WindowKey::Primary`; a managed window uses `WindowKey::Managed(name)`. This lets cancellation
and application-controlled recovery continue after the original entity has been deleted.

`CancelWindowRecovery` stops recovery for that key. It does not close a surviving window that was
moved to another display; the window stays where it is. With
`ManagedWindowPersistence::RememberAll`, the saved state for an absent cancelled window remains
available on the next launch. With `ActiveOnly`, that saved entry is removed.

#### Application-controlled recovery

Two events tell the application what happened:

- `WindowRecoveryPending` means the registered monitor has disappeared. It contains the
  `WindowKey` and the missing monitor's `MonitorId`.
- For `ApplicationControlled` recovery, `WindowRecoveryAvailable` means that monitor has returned.
  The application uses this event to create or choose a window before sending `RestoreWindow`.
  `FallbackAndReturn` handles the return itself and does not send this event.

A `MonitorId` is meaningful only within the current application process and is never written to the
state file.

For `ApplicationControlled`, the application creates the window and owns its cameras, UI, and other
content. The following observer creates the managed window after its target returns, then asks
Clerestory to restore it. This small example has no application-owned content; when a real window has
content, prepare it before sending `RestoreWindow`. The replacement does not receive another
`WindowRecovery` component.

```rust
use bevy::prelude::*;
use bevy_clerestory::ManagedWindow;
use bevy_clerestory::RestoreWindow;
use bevy_clerestory::WindowKey;
use bevy_clerestory::WindowRecoveryAvailable;

fn restore_inspector(
    available: On<WindowRecoveryAvailable>,
    mut commands: Commands,
) {
    if !matches!(
        &available.window_key,
        WindowKey::Managed(name) if name == "inspector"
    ) {
        return;
    }

    let entity = commands
        .spawn((
            Window {
                title: "Inspector".into(),
                ..default()
            },
            ManagedWindow {
                name: "inspector".into(),
            },
        ))
        .id();
    commands.trigger(RestoreWindow { entity });
}
```

`RestoreWindow` names the Bevy entity to restore. That entity must be the current primary window or a
managed window with the registered name. Clerestory rejects unrelated or ambiguous entities. The
request does not need a `WindowKey` because Clerestory derives it from the entity's components.

#### Reattach content to a replacement window

When Clerestory creates a replacement, its Bevy entity is new. Cameras, UI, and other
application-owned objects that referred to the old entity must be updated as soon as the replacement
receives `PrimaryWindow` or `ManagedWindow`. This should happen when the role component is added,
because the replacement may exist on a fallback monitor before its final restore finishes. The
following system updates an application-owned content root when the registered inspector appears:

```rust
use bevy::prelude::*;
use bevy_clerestory::ManagedWindow;

#[derive(Component)]
struct InspectorContentRoot {
    window: Entity,
}

fn rebind_inspector_content(
    inspectors: Query<(Entity, &ManagedWindow), Added<ManagedWindow>>,
    mut content_roots: Query<&mut InspectorContentRoot>,
) {
    for (window, managed) in &inspectors {
        if managed.name != "inspector" {
            continue;
        }
        for mut content_root in &mut content_roots {
            content_root.window = window;
        }
    }
}
```

#### Platform limits

Automatic return requires both a verified monitor and a window placement that the operating system
lets Clerestory restore. macOS, Windows, and X11 allow applications to choose a windowed position.
Wayland compositors choose windowed positions, so a windowed Wayland window cannot use automatic
return. A verified borderless-fullscreen output may still support automatic return on Wayland.
`ApplicationControlled` can report that a verified Wayland target returned, but a requested windowed
restore cannot promise a position and may report `WindowRestoreMismatch`.

#### Keep the application alive with no windows

Bevy normally exits when its last window disappears. If the application must stay alive long enough
to recover deleted windows, configure `WindowPlugin` with `ExitCondition::DontExit`. The application
must then decide what should exit it. This example sends `AppExit` when the operating system requests
that any window close:

```rust,no_run
use bevy::prelude::*;
use bevy::window::ExitCondition;
use bevy::window::WindowCloseRequested;
use bevy::window::WindowPlugin;
use bevy_clerestory::WindowManagerPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            exit_condition: ExitCondition::DontExit,
            ..default()
        }))
        .add_plugins(WindowManagerPlugin)
        .add_systems(Update, exit_when_close_is_requested)
        .run();
}

fn exit_when_close_is_requested(
    mut close_requests: MessageReader<WindowCloseRequested>,
    mut exit: MessageWriter<AppExit>,
) {
    if close_requests.read().next().is_some() {
        exit.write(AppExit::Success);
    }
}
```

#### Remote control through BRP

Bevy Remote Protocol (BRP) clients can observe recovery notifications, monitor connection changes,
and restore results with `world.observe+watch`. They can send `RestoreWindow` and
`CancelWindowRecovery` with `world.trigger_event`. These events carry Bevy `ReflectEvent` data and
keep their reflected paths in `bevy_clerestory::recovery`, `bevy_clerestory::monitors`, and
`bevy_clerestory::events`.

See the [restore-after-reconnect example](examples/restore_after_reconnect/README.md) for a complete
application using both policies, an ordered diagnostic log, automated checks, and a two-cycle manual
monitor-disconnect script.

#### What the automated tests cover

Automated Bevy tests simulate monitor disconnection and reconnection without unplugging a physical
display. They verify registration, windows moved to another display, replacement of deleted windows,
return to the same verified monitor after entity and index changes, cancellation, operation with no
displays or no windows, and Wayland's capability limits.

Real hardware is still required to verify what the operating system does when a cable, dock, or lid
changes: whether it moves or deletes a window, how it reports the returning display, and how real
scale-factor changes behave. Those results are recorded separately in the
[restore-after-reconnect README](examples/restore_after_reconnect/README.md). It contains one earlier
macOS disconnect/reconnect record; the complete two-cycle macOS, Windows, X11, and Wayland results
have not yet been recorded.

### State File Format

The state file uses a versioned v2 schema:

- `version: 2`
- `entries: [{ key, state }, ...]`

All spatial values (position, size) are stored in **logical pixels**, making them independent
of monitor scale factor. On restore, values are converted to physical pixels using the target
monitor's live scale factor.

`key` is typed (`Primary` or `Managed("<name>")`), so the primary window and a managed
window named `"primary"` are distinct and unambiguous.

Legacy state files (unversioned and v1) are still accepted on read and migrated to v2 on save.

## Version Compatibility

| bevy_clerestory | Bevy |
|---------------------|------|
| 0.1 – 0.2           | 0.19 |

## Platform Support

This table records physical testing of window placement save/restore. Physical reconnect test
results are tracked separately (see
[What the automated tests cover](#what-the-automated-tests-cover)).

| Platform | Status | Notes |
|----------|--------|-------|
| macOS    | ✅ Tested | Native hardware with multiple monitors at different scales |
| Windows  | ✅ Tested | `VMware` VM with multi-monitor, different scale factors |
| Linux X11 | ✅ Tested | Position and size restoration with keyboard snap workaround |
| Linux Wayland | ✅ Tested | Size + fullscreen only (Wayland cannot query/set position) |


**Note on Windows testing**: Windows support has been tested in a `VMware` virtual machine with multiple monitors at different scale factors. Native Windows installations may behave differently - if you encounter issues, please open an issue with details about your monitor configuration.

**Note on Linux support**: Linux support has been tested on KDE Plasma (Asahi Linux on Fedora). X11 includes a workaround for keyboard snap shortcuts (Meta+Arrow) that don't fire position events ([winit #4443](https://github.com/rust-windowing/winit/issues/4443)). Wayland has an inherent limitation: clients cannot query or set window position, so only size and fullscreen state can be restored. If you encounter issues, please open an issue with details about your distribution, desktop environment, and monitor configuration.

## Feature Flags (Platform Workarounds)

This plugin includes workarounds for known issues in winit and Bevy. Each workaround is behind a feature flag, and **all are enabled by default**.

This design allows:
- **Easy testing of upstream fixes** - disable a workaround to verify an upstream fix works
- **Opt-out flexibility** - if a workaround doesn't suit your setup, you can exclude it
- **Minimal code when not needed** - platform-specific workarounds are compiled out on other platforms

### Available Feature Flags

| Feature | Platform | Issue | Description |
|---------|----------|-------|-------------|
| `workaround-winit-4341` | Windows | [winit #4041](https://github.com/rust-windowing/winit/issues/4041) | DPI drag bounce fix |
| `workaround-winit-3124` | Windows | [winit #3124](https://github.com/rust-windowing/winit/issues/3124) | DX12/DXGI fullscreen crash fix |
| `workaround-winit-4443` | Linux X11 | [winit #4443](https://github.com/rust-windowing/winit/issues/4443) | Keyboard snap position fix |
| `workaround-winit-4440` | Windows, macOS, Linux X11 | [winit #4440](https://github.com/rust-windowing/winit/issues/4440) | Multi-monitor scale factor compensation |

### Disabling Workarounds

To test without a specific workaround (e.g., to verify an upstream fix):

```bash
# Disable all workarounds
cargo run --example restore_window --no-default-features
```

In your `Cargo.toml`, you can selectively enable features:

```toml
[dependencies]
bevy_clerestory = { version = "0.1", default-features = false, features = ["workaround-winit-4341"] }
```

## License

`bevy_clerestory` is free, open source and permissively licensed!
Except where noted (below and/or in individual files), all code in this repository is dual-licensed under either:

* MIT License ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))
* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))

at your option.

### Your contributions

Unless you explicitly state otherwise,
any contribution intentionally submitted for inclusion in the work by you,
as defined in the Apache-2.0 license,
shall be dual licensed as above,
without any additional terms or conditions.
