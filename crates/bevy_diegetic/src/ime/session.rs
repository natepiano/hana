//! Active session resource and lifecycle observers.

use bevy::diagnostic::FrameCount;
use bevy::prelude::*;
use bevy::window::WindowClosed;
use bevy::window::WindowFocused;

use super::ImeAcceptCommit;
use super::ImeCancelCause;
use super::ImeCanceled;
use super::ImeCommitAttemptId;
use super::ImeCommitCause;
use super::ImeCommitRequested;
use super::ImeEditableFieldSpec;
use super::ImeInputBlocker;
use super::ImeRejectCommit;
use super::ImeSessionId;
use super::ImeStarted;
use super::ImeTarget;
use super::ImeValidationRejected;

/// Request event that opens an IME session for an already-resolved target.
#[derive(Event, Clone, Debug)]
pub struct ImeOpenSession {
    /// Semantic backing target for the session.
    pub target:       ImeTarget,
    /// Window that owns OS IME state for the session.
    pub window:       Entity,
    /// Initial committed buffer text.
    pub initial_text: String,
    /// Editable field contract for the session.
    pub field_spec:   ImeEditableFieldSpec,
}

/// Request event that asks the active session to commit.
#[derive(Event, Clone, Debug)]
pub struct ImeRequestCommit {
    /// Id of the session being committed.
    pub session_id: ImeSessionId,
    /// Cause assigned to the commit attempt.
    pub cause:      ImeCommitCause,
}

/// Request event that asks the active session to cancel.
#[derive(Event, Clone, Debug)]
pub struct ImeRequestCancel {
    /// Id of the session being canceled.
    pub session_id: ImeSessionId,
    /// Cause assigned to the cancellation.
    pub cause:      ImeCancelCause,
}

#[derive(Clone, Debug)]
struct ImeSession {
    session_id:      ImeSessionId,
    target:          ImeTarget,
    window:          Entity,
    field_spec:      ImeEditableFieldSpec,
    text:            String,
    pending_attempt: Option<ImeCommitAttemptId>,
}

/// Resource holding the single active IME session.
#[derive(Resource, Debug)]
pub(crate) struct ActiveImeSession {
    active:       Option<ImeSession>,
    next_session: u64,
    next_attempt: u64,
}

impl Default for ActiveImeSession {
    fn default() -> Self {
        Self {
            active:       None,
            next_session: 1,
            next_attempt: 1,
        }
    }
}

impl ActiveImeSession {
    fn next_session_id(&mut self) -> ImeSessionId {
        let session_id = ImeSessionId::new(self.next_session);
        self.next_session = self.next_session.wrapping_add(1).max(1);
        session_id
    }

    fn next_attempt_id(&mut self) -> ImeCommitAttemptId {
        let attempt_id = ImeCommitAttemptId::new(self.next_attempt);
        self.next_attempt = self.next_attempt.wrapping_add(1).max(1);
        attempt_id
    }
}

pub(super) fn open_session(
    request: On<ImeOpenSession>,
    mut active_session: ResMut<ActiveImeSession>,
    mut input_blocker: ResMut<ImeInputBlocker>,
    frame_count: Option<Res<FrameCount>>,
    mut commands: Commands,
) {
    let request = request.event();
    if let Some(previous) = active_session.active.take() {
        input_blocker.clear_session(previous.session_id);
        commands.trigger(ImeCanceled {
            session_id: previous.session_id,
            target:     previous.target,
            cause:      ImeCancelCause::Replaced,
        });
    }

    let session_id = active_session.next_session_id();
    let session = ImeSession {
        session_id,
        target: request.target.clone(),
        window: request.window,
        field_spec: request.field_spec.clone(),
        text: request.initial_text.clone(),
        pending_attempt: None,
    };
    active_session.active = Some(session);
    input_blocker.begin_session(session_id, request.window, frame_count.map(|count| count.0));

    commands.trigger(ImeStarted {
        session_id,
        target: request.target.clone(),
    });
}

pub(super) fn request_commit(
    request: On<ImeRequestCommit>,
    mut active_session: ResMut<ActiveImeSession>,
    mut commands: Commands,
) {
    let request = request.event();
    commit_matching_session(
        &mut active_session,
        request.session_id,
        request.cause,
        &mut commands,
    );
}

pub(super) fn request_cancel(
    request: On<ImeRequestCancel>,
    mut active_session: ResMut<ActiveImeSession>,
    mut input_blocker: ResMut<ImeInputBlocker>,
    mut commands: Commands,
) {
    let request = request.event();
    if cancel_matching_session(
        &mut active_session,
        request.session_id,
        request.cause,
        &mut commands,
    ) {
        input_blocker.clear_session(request.session_id);
    }
}

pub(super) fn accept_commit(
    response: On<ImeAcceptCommit>,
    mut active_session: ResMut<ActiveImeSession>,
    mut input_blocker: ResMut<ImeInputBlocker>,
    mut commands: Commands,
) {
    let response = response.event();
    let Some(session) = active_session.active.take() else {
        return;
    };
    if session.session_id != response.session_id
        || session.pending_attempt != Some(response.attempt_id)
    {
        active_session.active = Some(session);
        return;
    }

    input_blocker.clear_session(response.session_id);
    commands.trigger(super::ImeApplied {
        session_id: response.session_id,
        attempt_id: response.attempt_id,
        target:     session.target,
        result:     response.result.clone(),
    });
}

pub(super) fn reject_commit(
    response: On<ImeRejectCommit>,
    mut active_session: ResMut<ActiveImeSession>,
    mut commands: Commands,
) {
    let response = response.event();
    let Some(session) = active_session.active.as_mut() else {
        return;
    };
    if session.session_id != response.session_id
        || session.pending_attempt != Some(response.attempt_id)
    {
        return;
    }

    session.pending_attempt = None;
    commands.trigger(ImeValidationRejected {
        session_id: response.session_id,
        attempt_id: response.attempt_id,
        target:     session.target.clone(),
        reason:     response.reason.clone(),
    });
}

pub(super) fn lease_scoped_commands(
    keys: Res<ButtonInput<KeyCode>>,
    mut active_session: ResMut<ActiveImeSession>,
    mut input_blocker: ResMut<ImeInputBlocker>,
    mut commands: Commands,
) {
    let Some(session) = active_session.active.as_ref() else {
        return;
    };
    let session_id = session.session_id;
    if !input_blocker.matches_session_window(session_id, session.window) {
        return;
    }

    if keys.just_pressed(KeyCode::Enter) {
        commit_matching_session(
            &mut active_session,
            session_id,
            ImeCommitCause::Enter,
            &mut commands,
        );
    } else if keys.just_pressed(KeyCode::Escape)
        && cancel_matching_session(
            &mut active_session,
            session_id,
            ImeCancelCause::Escape,
            &mut commands,
        )
    {
        input_blocker.clear_session(session_id);
    }
}

pub(super) fn cleanup_stale_sessions(
    windows: Query<(), With<Window>>,
    entities: Query<(), ()>,
    mut focus_events: MessageReader<WindowFocused>,
    mut closed_events: MessageReader<WindowClosed>,
    mut active_session: ResMut<ActiveImeSession>,
    mut input_blocker: ResMut<ImeInputBlocker>,
    mut commands: Commands,
) {
    let Some(session) = active_session.active.as_ref() else {
        input_blocker.clear();
        focus_events.clear();
        closed_events.clear();
        return;
    };

    let window = session.window;
    let window_closed = closed_events.read().any(|event| event.window == window);
    let focus_lost = focus_events
        .read()
        .any(|event| event.window == window && !event.focused);

    let cause = if !input_blocker.matches_session_window(session.session_id, window) {
        Some(ImeCancelCause::LeaseLost)
    } else if window_closed || !windows.contains(window) {
        Some(ImeCancelCause::WindowClosed)
    } else if focus_lost {
        Some(ImeCancelCause::FocusLost)
    } else if !target_exists(&session.target, &entities) {
        Some(ImeCancelCause::TargetStale)
    } else {
        None
    };

    if let Some(cause) = cause {
        let session_id = session.session_id;
        if cancel_matching_session(&mut active_session, session_id, cause, &mut commands) {
            input_blocker.clear_session(session_id);
        }
    }
}

fn target_exists(target: &ImeTarget, entities: &Query<(), ()>) -> bool {
    let entity = match *target {
        ImeTarget::WorldPanelField { panel, .. } | ImeTarget::ScreenPanelField { panel, .. } => {
            panel
        },
        ImeTarget::AppOwned { owner, .. } => owner,
    };
    entities.contains(entity)
}

fn commit_matching_session(
    active_session: &mut ActiveImeSession,
    session_id: ImeSessionId,
    cause: ImeCommitCause,
    commands: &mut Commands,
) {
    let Some(active) = active_session.active.as_ref() else {
        return;
    };
    if active.session_id != session_id || active.pending_attempt.is_some() {
        return;
    }

    let attempt_id = active_session.next_attempt_id();
    let Some(active) = active_session.active.as_mut() else {
        return;
    };
    active.pending_attempt = Some(attempt_id);

    commands.trigger(ImeCommitRequested {
        session_id,
        attempt_id,
        target: active.target.clone(),
        cause,
        field_spec: active.field_spec.clone(),
        text: active.text.clone(),
    });
}

fn cancel_matching_session(
    active_session: &mut ActiveImeSession,
    session_id: ImeSessionId,
    cause: ImeCancelCause,
    commands: &mut Commands,
) -> bool {
    let Some(session) = active_session.active.take() else {
        return false;
    };
    if session.session_id != session_id {
        active_session.active = Some(session);
        return false;
    }

    commands.trigger(ImeCanceled {
        session_id,
        target: session.target,
        cause,
    });
    true
}
