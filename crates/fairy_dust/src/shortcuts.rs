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
//! Bare keys Fairy Dust already binds (`H` home, `P` cube spin or fold play)
//! register into [`ReservedKeys`]. A second capability is rejected immediately,
//! while [`assert_no_reserved_collisions`] rejects an example shortcut that
//! reuses one at startup.

use std::any::TypeId;

use bevy::ecs::system::SystemId;
use bevy::prelude::*;

use crate::constants::MODIFIER_KEYS;

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
    key:        KeyCode,
    owner:      TypeId,
    owner_name: &'static str,
    label:      &'static str,
}

/// Bare keys already bound by Fairy Dust capabilities. Populated and checked at
/// capability install, then read by [`assert_no_reserved_collisions`].
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

/// Records a Fairy Dust bare-key binding. Repeated reservations by `O` are
/// idempotent; another owner reserving `key` is rejected immediately, and
/// [`assert_no_reserved_collisions`] rejects example shortcuts at startup.
pub(crate) fn reserve_key<O: 'static>(app: &mut App, key: KeyCode, label: &'static str) {
    app.init_resource::<ReservedKeys>();
    let owner = TypeId::of::<O>();
    let owner_name = std::any::type_name::<O>();
    let mut reserved = app.world_mut().resource_mut::<ReservedKeys>();
    if let Some(existing) = reserved.0.iter().find(|reserved| reserved.key == key) {
        assert!(
            existing.owner == owner,
            "fairy_dust reserved key {:?} for `{}` ({}) collides with `{}` ({}); use only one capability for a bare key",
            key,
            label,
            owner_name,
            existing.label,
            existing.owner_name,
        );
        return;
    }
    reserved.0.push(ReservedKey {
        key,
        owner,
        owner_name,
        label,
    });
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

#[cfg(test)]
mod tests {
    use std::panic::AssertUnwindSafe;

    use bevy::prelude::*;

    use super::ReservedKeys;
    use super::reserve_key;

    struct FirstCapability;
    struct SecondCapability;

    #[test]
    fn same_capability_key_reservation_is_idempotent() {
        let mut app = App::new();

        reserve_key::<FirstCapability>(&mut app, KeyCode::KeyP, "first");
        reserve_key::<FirstCapability>(&mut app, KeyCode::KeyP, "first");

        assert_eq!(app.world().resource::<ReservedKeys>().0.len(), 1);
    }

    #[test]
    fn different_capability_key_reservation_is_rejected() {
        let mut app = App::new();
        reserve_key::<FirstCapability>(&mut app, KeyCode::KeyP, "first");

        let collision = std::panic::catch_unwind(AssertUnwindSafe(|| {
            reserve_key::<SecondCapability>(&mut app, KeyCode::KeyP, "second");
        }));

        assert!(collision.is_err());
    }
}
