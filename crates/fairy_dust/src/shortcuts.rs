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
use bevy::window::PrimaryWindow;
use hana_diegetic::ImeInputBlocker;
use hana_rubric::CommandKeystroke;
use hana_rubric::CommandRegistry;
use hana_rubric::KeymapBindings;
use hana_rubric::Modifiers;
use hana_rubric::PrimaryTrigger;

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
    app.add_systems(Update, run_shortcuts.run_if(no_text_entry_in_progress));
}

/// Whether the window is free of a text editor, which is the condition every
/// registered shortcut runs under.
///
/// While the command palette's query field holds the window's IME lease, its
/// keystrokes are text. Running them as shortcuts too would home the camera
/// every time the reader typed `h`.
pub(crate) fn no_text_entry_in_progress(
    ime_input_blocker: Option<Res<ImeInputBlocker>>,
    windows: Query<Entity, With<PrimaryWindow>>,
) -> bool {
    let (Some(ime_input_blocker), Ok(window)) = (ime_input_blocker, windows.single()) else {
        return true;
    };
    !ime_input_blocker.blocks_window(window)
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
///
/// Two registries can claim a bare key. [`ReservedKeys`] holds the ones a
/// capability wires straight to a [`KeyCode`], and the keymap holds the ones a
/// document binds to a command — including a user document that moved a Fairy
/// Dust chord onto a bare letter. This runs in `Startup`, after `finish` has
/// committed the first keymap generation, so [`KeymapBindings`] is the live
/// table rather than an empty one.
fn assert_no_reserved_collisions(
    registrations: Res<ShortcutRegistrations>,
    reserved: Res<ReservedKeys>,
    command_registry: Option<Res<CommandRegistry>>,
    keymap_bindings: Option<Res<KeymapBindings>>,
) {
    for registration in &registrations.0 {
        let collision = reserved
            .0
            .iter()
            .find(|reserved| reserved.key == registration.key)
            .map(|reserved| reserved.label.to_owned())
            .or_else(|| {
                bound_command_on_bare_key(
                    command_registry.as_deref(),
                    keymap_bindings.as_deref(),
                    registration.key,
                )
            });
        // `panic!` is denied workspace-wide; `assert!` is the allowed hard-fail.
        assert!(
            collision.is_none(),
            "fairy_dust example shortcut key {:?} collides with the reserved `{}` binding; \
             use the matching Fairy Dust capability or pick a different key",
            registration.key,
            collision.unwrap_or_default(),
        );
    }
}

/// The command the live keymap runs from `key` alone, if any.
///
/// Only a one-keystroke, modifier-free binding can double-fire with an example
/// shortcut: [`run_shortcuts`] stands down while any modifier is held.
fn bound_command_on_bare_key(
    command_registry: Option<&CommandRegistry>,
    keymap_bindings: Option<&KeymapBindings>,
    key: KeyCode,
) -> Option<String> {
    let (command_registry, keymap_bindings) = (command_registry?, keymap_bindings?);
    command_registry
        .iter()
        .find(|command_info| keystroke_on_bare_key(keymap_bindings.keystroke(command_info.id), key))
        .map(|command_info| command_info.id.to_string())
}

fn keystroke_on_bare_key(command_keystroke: CommandKeystroke<'_>, key: KeyCode) -> bool {
    let CommandKeystroke::BoundTo(keystroke_sequence) = command_keystroke else {
        return false;
    };
    let [keystroke] = keystroke_sequence.as_slice() else {
        return false;
    };
    keystroke.modifiers() == Modifiers::none()
        && matches!(
            keystroke.primary_trigger(),
            PrimaryTrigger::OrdinaryKey(ordinary_key) if ordinary_key.key_code() == key
        )
}

#[cfg(test)]
mod tests {
    use std::panic::AssertUnwindSafe;

    use bevy::prelude::*;
    use bevy::window::PrimaryWindow;
    use hana_diegetic::DiegeticTextMeasurer;
    use hana_diegetic::HeadlessDiegeticUiPlugin;
    use hana_diegetic::ImeAppOwnedFieldSpec;
    use hana_diegetic::ImeEditableFieldSpec;
    use hana_diegetic::ImeOpenSession;
    use hana_diegetic::ImeTarget;

    use super::ReservedKeys;
    use super::ShortcutRegistrations;
    use super::assert_no_reserved_collisions;
    use super::install;
    use super::register_press;
    use super::reserve_key;
    use crate::CommandPaletteKeymap;

    struct FirstCapability;
    struct SecondCapability;

    #[derive(Default, Resource)]
    struct ShortcutRuns(usize);

    /// A key bound as an example shortcut is text while the palette's query
    /// field holds the window's IME lease, so the shortcut must not run.
    #[test]
    fn a_bound_key_does_not_fire_while_an_editor_holds_the_lease() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(DiegeticTextMeasurer::default())
            .add_plugins(HeadlessDiegeticUiPlugin);
        app.init_resource::<ShortcutRuns>();
        let window = app.world_mut().spawn(PrimaryWindow).id();
        let system_id = app.register_system(|mut runs: ResMut<ShortcutRuns>| runs.0 += 1);
        install(&mut app);
        register_press(&mut app, KeyCode::KeyH, system_id);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyH);

        app.update();
        let ran_before_the_lease = app.world().resource::<ShortcutRuns>().0;

        app.world_mut().trigger(ImeOpenSession {
            target: ImeTarget::AppOwned {
                owner:    window,
                field_id: "query".into(),
            },
            window,
            initial_text: String::new(),
            field_spec: ImeEditableFieldSpec::AppOwned(ImeAppOwnedFieldSpec::new("query")),
            anchor: None,
        });
        app.update();

        assert_eq!(ran_before_the_lease, 1);
        assert_eq!(app.world().resource::<ShortcutRuns>().0, 1);
    }

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

    /// A user keymap can bind a Fairy Dust command to a bare letter, which the
    /// hardcoded reservation list knows nothing about. The example shortcut on
    /// that letter would then double-fire, so the startup check reads the live
    /// keymap as well.
    #[test]
    fn an_example_shortcut_on_a_bare_key_the_keymap_binds_is_rejected_at_startup() {
        let mut app = App::new();
        app.init_resource::<ShortcutRegistrations>();
        app.init_resource::<ReservedKeys>();
        crate::keymap::configure(
            &mut app,
            CommandPaletteKeymap::new(
                r#"{ "bindings": [{ "bindings": { "h": "fairy_dust::show_help" } }] }"#,
            ),
        );
        crate::keymap::install(&mut app);
        app.finish();
        let system_id = app.world_mut().register_system(|| {});
        register_press(&mut app, KeyCode::KeyH, system_id);

        let collision = std::panic::catch_unwind(AssertUnwindSafe(|| {
            app.add_systems(Startup, assert_no_reserved_collisions);
            app.update();
        }));

        assert!(collision.is_err());
    }

    /// A bare key reserved by a capability is refused to an example shortcut,
    /// which would otherwise double-fire against the reserving capability.
    #[test]
    fn an_example_shortcut_on_a_reserved_key_is_rejected_at_startup() {
        let mut app = App::new();
        app.init_resource::<ShortcutRegistrations>();
        reserve_key::<FirstCapability>(&mut app, KeyCode::KeyP, "first");
        let system_id = app.world_mut().register_system(|| {});
        register_press(&mut app, KeyCode::KeyP, system_id);

        let collision = std::panic::catch_unwind(AssertUnwindSafe(|| {
            app.add_systems(Startup, assert_no_reserved_collisions);
            app.update();
        }));

        assert!(collision.is_err());
    }
}
