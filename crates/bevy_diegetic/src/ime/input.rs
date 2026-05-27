//! Window IME and keyboard input routing for the active edit session.

use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use bevy::window::Ime;

use super::ActiveImeSession;
use super::ImeCancelCause;
use super::ImeCommitCause;
use super::ImeInputBlocker;
use super::ImeRequestCancel;
use super::ImeSessionId;
use super::ImeTarget;
use super::buffer::ImeEditCommand;
use super::buffer::ImeMovementDirection;
use super::buffer::ImeMovementUnit;
use super::buffer::ImeSelectionMode;
use super::events::ImeTextChanged;

/// Per-frame IME input routing state.
#[derive(Resource, Clone, Debug, Default)]
pub(super) struct ImeInputFrame {
    saw_platform_ime: bool,
}

/// Window toggled by the previous IME update pass.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub(super) struct ImeWindowState {
    active_window: Option<Entity>,
}

/// Synchronous app-owned keyboard routing hook.
#[derive(Resource, Default)]
pub struct ImeAppInputDispositionHook {
    handler: Option<Box<ImeAppInputHandler>>,
}

impl ImeAppInputDispositionHook {
    /// Installs the app-owned input hook.
    pub fn set(
        &mut self,
        handler: impl for<'a> FnMut(ImeAppInputContext<'a>) -> ImeAppInputDisposition
        + Send
        + Sync
        + 'static,
    ) {
        self.handler = Some(Box::new(handler));
    }

    /// Clears the app-owned input hook.
    pub fn clear(&mut self) { self.handler = None; }

    fn disposition(&mut self, context: ImeAppInputContext<'_>) -> ImeAppInputDisposition {
        let Some(handler) = self.handler.as_mut() else {
            return ImeAppInputDisposition::Edit;
        };
        handler(context)
    }
}

type ImeAppInputHandler =
    dyn for<'a> FnMut(ImeAppInputContext<'a>) -> ImeAppInputDisposition + Send + Sync + 'static;

/// Keyboard input context passed to the app-owned input hook.
pub struct ImeAppInputContext<'a> {
    /// Id of the active session.
    pub session_id: ImeSessionId,
    /// App-owned semantic target.
    pub target:     &'a ImeTarget,
    /// Current key button state.
    pub keys:       &'a ButtonInput<KeyCode>,
    /// Keyboard events observed this frame.
    pub events:     &'a [KeyboardInput],
}

/// App-owned input decision for a keyboard frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImeAppInputDisposition {
    /// Let the built-in editor consume the input.
    Edit,
    /// The app surface consumed the input; built-in editing should not run.
    Surface,
    /// Request commit for the app-owned session.
    Commit,
    /// Request cancellation for the app-owned session.
    Cancel,
}

pub(super) fn clear_frame_input(mut frame: ResMut<ImeInputFrame>) {
    frame.saw_platform_ime = false;
}

pub(super) fn update_window_ime(
    active_session: Res<ActiveImeSession>,
    mut window_state: ResMut<ImeWindowState>,
    mut windows: Query<&mut Window>,
) {
    let active_window = active_session.active_window();
    if let Some(previous) = window_state.active_window
        && Some(previous) != active_window
        && let Ok(mut window) = windows.get_mut(previous)
    {
        window.ime_enabled = false;
    }

    if let Some(entity) = active_window
        && let Ok(mut window) = windows.get_mut(entity)
    {
        window.ime_enabled = true;
    }

    window_state.active_window = active_window;
}

pub(super) fn handle_window_ime(
    mut ime_events: MessageReader<Ime>,
    mut active_session: ResMut<ActiveImeSession>,
    input_blocker: Res<ImeInputBlocker>,
    mut frame: ResMut<ImeInputFrame>,
    mut commands: Commands,
) {
    for event in ime_events.read() {
        let changed = match event {
            Ime::Preedit {
                window,
                value,
                cursor,
            } => active_session.apply_preedit(*window, value, *cursor, &input_blocker),
            Ime::Commit { window, value } => {
                active_session.apply_ime_commit(*window, value, &input_blocker)
            },
            Ime::Enabled { window } | Ime::Disabled { window } => {
                active_session.clear_preedit(*window, &input_blocker)
            },
        };
        if changed.is_some() {
            frame.saw_platform_ime = true;
        }
        trigger_text_changed(changed, &mut commands);
    }
}

pub(super) fn handle_keyboard(
    keys: Res<ButtonInput<KeyCode>>,
    mut key_events: MessageReader<KeyboardInput>,
    mut active_session: ResMut<ActiveImeSession>,
    input_blocker: Res<ImeInputBlocker>,
    mut app_hook: ResMut<ImeAppInputDispositionHook>,
    frame: Res<ImeInputFrame>,
    mut commands: Commands,
) {
    if !active_session.is_leased(&input_blocker) {
        key_events.clear();
        return;
    }

    if active_session.is_composing() {
        if keys.just_pressed(KeyCode::Escape) {
            let changed = active_session.clear_active_preedit(&input_blocker);
            trigger_text_changed(changed, &mut commands);
        }
        key_events.clear();
        return;
    }

    let events: Vec<KeyboardInput> = key_events.read().cloned().collect();
    if let Some(disposition) =
        app_disposition(&mut app_hook, &active_session, &keys, events.as_slice())
        && apply_app_disposition(disposition, &active_session, &mut commands)
    {
        return;
    }

    if let Some(request) = request_from_keys(&keys, &active_session) {
        trigger_session_request(request, &mut commands);
        return;
    }

    if let Some(command) = command_from_keys(&keys) {
        let changed = active_session.apply_edit_command(command, &input_blocker);
        trigger_text_changed(changed, &mut commands);
    }

    if frame.saw_platform_ime || active_session.is_pending_commit() {
        key_events.clear();
        return;
    }

    for event in &events {
        if event.state != ButtonState::Pressed || command_modifier_pressed(&keys) {
            continue;
        }
        let Some(text) = event.text.as_deref() else {
            continue;
        };
        let changed = active_session.apply_keyboard_text(event.window, text, &input_blocker);
        trigger_text_changed(changed, &mut commands);
    }
}

fn app_disposition(
    app_hook: &mut ImeAppInputDispositionHook,
    active_session: &ActiveImeSession,
    keys: &ButtonInput<KeyCode>,
    events: &[KeyboardInput],
) -> Option<ImeAppInputDisposition> {
    let session_id = active_session.active_session_id()?;
    let target = active_session.active_target()?;
    if !matches!(target, ImeTarget::AppOwned { .. }) {
        return None;
    }
    Some(app_hook.disposition(ImeAppInputContext {
        session_id,
        target,
        keys,
        events,
    }))
}

fn apply_app_disposition(
    disposition: ImeAppInputDisposition,
    active_session: &ActiveImeSession,
    commands: &mut Commands,
) -> bool {
    let Some(session_id) = active_session.active_session_id() else {
        return true;
    };
    match disposition {
        ImeAppInputDisposition::Edit => false,
        ImeAppInputDisposition::Surface => true,
        ImeAppInputDisposition::Commit => {
            commands.trigger(super::ImeRequestCommit {
                session_id,
                cause: ImeCommitCause::Request,
            });
            true
        },
        ImeAppInputDisposition::Cancel => {
            commands.trigger(ImeRequestCancel {
                session_id,
                cause: ImeCancelCause::Request,
            });
            true
        },
    }
}

fn request_from_keys(
    keys: &ButtonInput<KeyCode>,
    active_session: &ActiveImeSession,
) -> Option<ImeSessionRequest> {
    let session_id = active_session.active_session_id()?;
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) {
        return Some(ImeSessionRequest::Commit(super::ImeRequestCommit {
            session_id,
            cause: ImeCommitCause::Enter,
        }));
    }

    keys.just_pressed(KeyCode::Escape)
        .then_some(ImeSessionRequest::Cancel(ImeRequestCancel {
            session_id,
            cause: ImeCancelCause::Escape,
        }))
}

fn command_from_keys(keys: &ButtonInput<KeyCode>) -> Option<ImeEditCommand> {
    let selection = if shift_pressed(keys) {
        ImeSelectionMode::Extend
    } else {
        ImeSelectionMode::Move
    };

    if primary_modifier_pressed(keys) && keys.just_pressed(KeyCode::KeyA) {
        return Some(ImeEditCommand::SelectAll);
    }

    if keys.just_pressed(KeyCode::ArrowLeft) {
        return Some(ImeEditCommand::Move {
            direction: ImeMovementDirection::Backward,
            unit: movement_unit(keys),
            selection,
        });
    }

    if keys.just_pressed(KeyCode::ArrowRight) {
        return Some(ImeEditCommand::Move {
            direction: ImeMovementDirection::Forward,
            unit: movement_unit(keys),
            selection,
        });
    }

    if keys.just_pressed(KeyCode::Home) {
        return Some(ImeEditCommand::Move {
            direction: ImeMovementDirection::Backward,
            unit: ImeMovementUnit::Line,
            selection,
        });
    }

    if keys.just_pressed(KeyCode::End) {
        return Some(ImeEditCommand::Move {
            direction: ImeMovementDirection::Forward,
            unit: ImeMovementUnit::Line,
            selection,
        });
    }

    if keys.just_pressed(KeyCode::Backspace) {
        return Some(ImeEditCommand::DeleteBackward(delete_unit(keys)));
    }

    keys.just_pressed(KeyCode::Delete)
        .then_some(ImeEditCommand::DeleteForward(delete_unit(keys)))
}

fn movement_unit(keys: &ButtonInput<KeyCode>) -> ImeMovementUnit {
    if word_modifier_pressed(keys) {
        ImeMovementUnit::Word
    } else if super_pressed(keys) {
        ImeMovementUnit::Line
    } else {
        ImeMovementUnit::Character
    }
}

fn delete_unit(keys: &ButtonInput<KeyCode>) -> ImeMovementUnit {
    if word_modifier_pressed(keys) {
        ImeMovementUnit::Word
    } else {
        ImeMovementUnit::Character
    }
}

fn trigger_text_changed(changed: Option<ImeTextChanged>, commands: &mut Commands) {
    if let Some(changed) = changed {
        commands.trigger(changed);
    }
}

fn shift_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight)
}

fn primary_modifier_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    control_pressed(keys) || super_pressed(keys)
}

fn command_modifier_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    control_pressed(keys) || super_pressed(keys)
}

fn word_modifier_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    control_pressed(keys) || alt_pressed(keys)
}

fn control_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight)
}

fn super_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::SuperLeft) || keys.pressed(KeyCode::SuperRight)
}

fn alt_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight)
}

enum ImeSessionRequest {
    Commit(super::ImeRequestCommit),
    Cancel(ImeRequestCancel),
}

fn trigger_session_request(request: ImeSessionRequest, commands: &mut Commands) {
    match request {
        ImeSessionRequest::Commit(request) => commands.trigger(request),
        ImeSessionRequest::Cancel(request) => commands.trigger(request),
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::ButtonInput;
    use bevy::prelude::KeyCode;

    use super::command_from_keys;
    use crate::ime::buffer::ImeEditCommand;
    use crate::ime::buffer::ImeMovementDirection;
    use crate::ime::buffer::ImeMovementUnit;
    use crate::ime::buffer::ImeSelectionMode;

    fn keys(pressed: &[KeyCode]) -> ButtonInput<KeyCode> {
        let mut keys = ButtonInput::default();
        for key in pressed {
            keys.press(*key);
        }
        keys
    }

    #[test]
    fn primary_a_maps_to_select_all() {
        let keys = keys(&[KeyCode::SuperLeft, KeyCode::KeyA]);

        assert_eq!(command_from_keys(&keys), Some(ImeEditCommand::SelectAll));
    }

    #[test]
    fn alt_arrow_left_maps_to_word_movement() {
        let keys = keys(&[KeyCode::AltLeft, KeyCode::ArrowLeft]);

        assert_eq!(
            command_from_keys(&keys),
            Some(ImeEditCommand::Move {
                direction: ImeMovementDirection::Backward,
                unit:      ImeMovementUnit::Word,
                selection: ImeSelectionMode::Move,
            })
        );
    }

    #[test]
    fn shift_end_extends_to_line_end() {
        let keys = keys(&[KeyCode::ShiftLeft, KeyCode::End]);

        assert_eq!(
            command_from_keys(&keys),
            Some(ImeEditCommand::Move {
                direction: ImeMovementDirection::Forward,
                unit:      ImeMovementUnit::Line,
                selection: ImeSelectionMode::Extend,
            })
        );
    }
}
