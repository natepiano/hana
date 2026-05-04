use std::marker::PhantomData;

use bevy::prelude::*;
use bevy_enhanced_input::prelude::*;

/// Non-consuming modifier action for `Cmd` (macOS) / `Ctrl` (other platforms).
#[derive(InputAction)]
#[action_output(bool)]
struct PrimaryShortcutsModifier;

/// Non-consuming modifier action for `Option` (macOS) / `Alt` (other platforms).
#[derive(InputAction)]
#[action_output(bool)]
struct AltModifier;

/// Non-consuming modifier action for `Ctrl` on macOS (distinct from `Cmd`).
#[derive(InputAction)]
#[action_output(bool)]
struct ControlModifier;

#[derive(Clone, Copy)]
enum PlatformShortcutMode {
    Command,
    Control,
}

impl PlatformShortcutMode {
    const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Command
        } else {
            Self::Control
        }
    }
}

const PLATFORM_SHORTCUT_MODE: PlatformShortcutMode = PlatformShortcutMode::current();

/// Modifier-aware keybinding builder with platform-specific `Cmd`/`Ctrl` handling.
///
/// Spawns modifier actions and provides methods to bind keys with automatic
/// `BlockBy` application, preventing single-key actions from firing when
/// any modifier is held.
///
/// # Platform behavior
///
/// **macOS:**
/// - `PrimaryShortcutsModifier` = `Cmd` (`Super`) for platform shortcuts
/// - `ControlModifier` = `Ctrl` as a separate physical key that blocks single keys
/// - `AltModifier` = `Option`, which blocks single keys
///
/// **Windows / Linux:**
/// - `PrimaryShortcutsModifier` = `Ctrl` for platform shortcuts and single-key blocking
/// - `AltModifier` = `Alt`, which blocks single keys
/// - `ControlModifier` is not spawned because `Ctrl` is already the primary modifier
///
/// # Type parameters
///
/// - `C: Component` is the input context component for the action spawner
///
/// # Examples
///
/// ```ignore
/// use bevy_kana::Keybindings;
///
/// fn setup_bindings(spawner: &mut ActionSpawner<MyContext>) {
///     let keybindings = Keybindings::new::<ShiftAction>(spawner, ActionSettings::default());
///     keybindings.spawn_key::<JumpAction>(spawner, KeyCode::Space);
///     keybindings.spawn_platform_key::<SaveAction>(spawner, KeyCode::KeyS);
///     keybindings.spawn_shift_key::<RunAction>(spawner, KeyCode::KeyR);
/// }
/// ```
pub struct Keybindings<C: Component> {
    all_modifier_entities:       Vec<Entity>,
    non_shift_modifier_entities: Vec<Entity>,
    action_settings:             ActionSettings,
    phantom_data:                PhantomData<C>,
}

impl<C: Component> Keybindings<C> {
    /// Spawns modifier actions and returns a `Keybindings` ready for use.
    ///
    /// The `S` type parameter is the `InputAction` for the shift modifier.
    /// This allows the caller to query `Action<S>` to check shift state
    /// (for example, for shift-click selection).
    pub fn new<S: InputAction>(
        spawner: &mut ActionSpawner<C>,
        action_settings: ActionSettings,
    ) -> Self {
        let non_consuming_modifier = ActionSettings {
            consume_input: false,
            require_reset: true,
            ..default()
        };
        let primary_modifier_bindings = match PLATFORM_SHORTCUT_MODE {
            PlatformShortcutMode::Command => bindings![KeyCode::SuperLeft, KeyCode::SuperRight],
            PlatformShortcutMode::Control => {
                bindings![KeyCode::ControlLeft, KeyCode::ControlRight]
            },
        };

        let shift_entity = spawner
            .spawn((
                Action::<S>::new(),
                non_consuming_modifier,
                bindings![KeyCode::ShiftLeft, KeyCode::ShiftRight],
            ))
            .id();
        let primary_entity = spawner
            .spawn((
                Action::<PrimaryShortcutsModifier>::new(),
                non_consuming_modifier,
                primary_modifier_bindings,
            ))
            .id();
        let alt_entity = spawner
            .spawn((
                Action::<AltModifier>::new(),
                non_consuming_modifier,
                bindings![KeyCode::AltLeft, KeyCode::AltRight],
            ))
            .id();

        let mut all_modifier_entities = vec![shift_entity, primary_entity, alt_entity];
        let mut non_shift_modifier_entities = vec![primary_entity, alt_entity];

        match PLATFORM_SHORTCUT_MODE {
            PlatformShortcutMode::Command => {
                // On macOS, `Ctrl` is a separate physical key from `Cmd`, so block it too.
                let control_entity = spawner
                    .spawn((
                        Action::<ControlModifier>::new(),
                        non_consuming_modifier,
                        bindings![KeyCode::ControlLeft, KeyCode::ControlRight],
                    ))
                    .id();
                all_modifier_entities.push(control_entity);
                non_shift_modifier_entities.push(control_entity);
            },
            PlatformShortcutMode::Control => {},
        }

        Self {
            all_modifier_entities,
            non_shift_modifier_entities,
            action_settings,
            phantom_data: PhantomData,
        }
    }

    /// Spawns an action bound to a single key, blocked by all modifiers.
    pub fn spawn_key<A: InputAction>(&self, spawner: &mut ActionSpawner<C>, key: KeyCode) {
        spawner.spawn((
            Action::<A>::new(),
            self.action_settings,
            BlockBy::new(self.all_modifier_entities.clone()),
            bindings![key],
        ));
    }

    /// Spawns an action bound to `Shift + key`, blocked by non-shift modifiers only.
    pub fn spawn_shift_key<A: InputAction>(&self, spawner: &mut ActionSpawner<C>, key: KeyCode) {
        spawner.spawn((
            Action::<A>::new(),
            self.action_settings,
            BlockBy::new(self.non_shift_modifier_entities.clone()),
            bindings![key.with_mod_keys(ModKeys::SHIFT)],
        ));
    }

    /// Spawns an action with arbitrary bindings, blocked by all modifiers.
    pub fn spawn_binding<A: InputAction, B: Bundle>(
        &self,
        spawner: &mut ActionSpawner<C>,
        bindings: B,
    ) {
        spawner.spawn((
            Action::<A>::new(),
            self.action_settings,
            BlockBy::new(self.all_modifier_entities.clone()),
            bindings,
        ));
    }

    /// Spawns an action with the platform `Cmd`/`Ctrl` modifier. No `BlockBy`
    /// is needed because the modifier key itself is the disambiguator.
    pub fn spawn_platform_key<A: InputAction>(&self, spawner: &mut ActionSpawner<C>, key: KeyCode) {
        let platform_bindings = match PLATFORM_SHORTCUT_MODE {
            PlatformShortcutMode::Command => bindings![key.with_mod_keys(ModKeys::SUPER)],
            PlatformShortcutMode::Control => bindings![key.with_mod_keys(ModKeys::CONTROL)],
        };
        spawner.spawn((Action::<A>::new(), self.action_settings, platform_bindings));
    }
}
