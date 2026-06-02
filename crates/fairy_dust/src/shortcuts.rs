//! Capability: example-owned keyboard shortcuts that never collide with Fairy
//! Dust's own chords.
//!
//! Examples register a key with
//! [`SprinkleBuilder::with_shortcut`](crate::SprinkleBuilder::with_shortcut)
//! (runs once per press) or
//! [`SprinkleBuilder::with_held_shortcut`](crate::SprinkleBuilder::with_held_shortcut)
//! (runs every frame while held). Each registers a `(key, system)` pair; the
//! example never names an input type, so its only imports stay `bevy` and
//! `fairy_dust`.
//!
//! [`run_shortcuts`] runs a registered system only when its key fires **and no
//! modifier is held**. Fairy Dust's own chords (`Ctrl+Shift+L` and friends)
//! fire only *with* their modifiers, so a bare example key and a Fairy Dust
//! chord on the same letter never both fire — the modifier guard is what the
//! original raw-input examples were missing.
//!
//! Bare keys Fairy Dust already binds (`H` home, `P` cube spin) register into
//! [`ReservedKeys`]; [`assert_no_reserved_collisions`] rejects an example
//! shortcut that reuses one at startup, instead of letting it double-fire.

use bevy::ecs::system::SystemId;
use bevy::prelude::*;

/// Keys whose press, while held, suppresses every bare example shortcut. Bare
/// shortcuts fire only when none of these is down, mirroring the `BlockBy` that
/// guards Fairy Dust's own bei chords.
const MODIFIER_KEYS: [KeyCode; 8] = [
    KeyCode::ControlLeft,
    KeyCode::ControlRight,
    KeyCode::ShiftLeft,
    KeyCode::ShiftRight,
    KeyCode::AltLeft,
    KeyCode::AltRight,
    KeyCode::SuperLeft,
    KeyCode::SuperRight,
];

/// When a registered shortcut's system runs relative to the key press.
#[derive(Clone, Copy)]
enum ShortcutTiming {
    /// Once, on the frame the key goes down.
    Press,
    /// Every frame the key is held.
    Held,
}

/// Marks that the shortcuts capability has been installed, so repeated
/// `with_shortcut` calls add the `Startup`/`Update` systems only once.
#[derive(Resource)]
struct ShortcutsInstalled;

/// A Fairy Dust bare-key binding that example shortcuts must not reuse.
struct ReservedKey {
    key:   KeyCode,
    label: &'static str,
}

/// Bare keys already bound by Fairy Dust capabilities. Populated at capability
/// install; read once by [`assert_no_reserved_collisions`].
#[derive(Resource, Default)]
struct ReservedKeys(Vec<ReservedKey>);

struct ShortcutRegistration {
    key:       KeyCode,
    timing:    ShortcutTiming,
    system_id: SystemId,
}

/// Example shortcuts recorded during builder construction, run by
/// [`run_shortcuts`].
#[derive(Resource, Default)]
struct ShortcutRegistrations(Vec<ShortcutRegistration>);

/// Adds the shortcut registry, reserved-key check, and runner exactly once.
/// Idempotent — called by every `with_shortcut` / `with_held_shortcut`.
pub(crate) fn install(app: &mut App) {
    app.init_resource::<ShortcutRegistrations>();
    app.init_resource::<ReservedKeys>();
    if app.world().contains_resource::<ShortcutsInstalled>() {
        return;
    }
    app.insert_resource(ShortcutsInstalled);
    app.add_systems(Startup, assert_no_reserved_collisions);
    app.add_systems(Update, run_shortcuts);
}

/// Records `key` to run `system_id` once each time it is pressed.
pub(crate) fn register_press(app: &mut App, key: KeyCode, system_id: SystemId) {
    push(app, key, ShortcutTiming::Press, system_id);
}

/// Records `key` to run `system_id` every frame it is held.
pub(crate) fn register_held(app: &mut App, key: KeyCode, system_id: SystemId) {
    push(app, key, ShortcutTiming::Held, system_id);
}

fn push(app: &mut App, key: KeyCode, timing: ShortcutTiming, system_id: SystemId) {
    app.world_mut()
        .resource_mut::<ShortcutRegistrations>()
        .0
        .push(ShortcutRegistration {
            key,
            timing,
            system_id,
        });
}

/// Records a Fairy Dust bare-key binding so [`assert_no_reserved_collisions`]
/// can reject an example shortcut that reuses it.
pub(crate) fn reserve_key(app: &mut App, key: KeyCode, label: &'static str) {
    app.init_resource::<ReservedKeys>();
    app.world_mut()
        .resource_mut::<ReservedKeys>()
        .0
        .push(ReservedKey { key, label });
}

/// Runs each registered shortcut whose key fires this frame, skipping all of
/// them while any modifier is held so bare keys never shadow Fairy Dust chords.
fn run_shortcuts(
    keys: Res<ButtonInput<KeyCode>>,
    registrations: Res<ShortcutRegistrations>,
    mut commands: Commands,
) {
    if keys.any_pressed(MODIFIER_KEYS) {
        return;
    }
    for registration in &registrations.0 {
        let fired = match registration.timing {
            ShortcutTiming::Press => keys.just_pressed(registration.key),
            ShortcutTiming::Held => keys.pressed(registration.key),
        };
        if fired {
            commands.run_system(registration.system_id);
        }
    }
}

/// Fails the run at startup if an example shortcut reuses a key Fairy Dust
/// already binds bare, turning a silent double-fire into a clear error.
fn assert_no_reserved_collisions(
    registrations: Res<ShortcutRegistrations>,
    reserved: Res<ReservedKeys>,
) {
    for registration in &registrations.0 {
        let collision = reserved
            .0
            .iter()
            .find(|reserved| reserved.key == registration.key);
        // `panic!` is denied workspace-wide; `assert!` is the allowed hard-fail.
        assert!(
            collision.is_none(),
            "fairy_dust example shortcut key {:?} collides with the reserved `{}` binding; \
             use the matching Fairy Dust capability or pick a different key",
            registration.key,
            collision.map_or("", |reserved| reserved.label),
        );
    }
}
