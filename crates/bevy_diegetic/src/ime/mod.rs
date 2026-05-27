//! Single-line IME session lifecycle.
//!
//! The module stays private; approved public API is re-exported from the crate
//! root with `Ime...` names so callers do not need a public module namespace.

mod activation;
mod events;
mod field;
mod ids;
mod lease;
mod session;
mod target;

use bevy::prelude::*;
pub use events::ImeAcceptCommit;
pub use events::ImeApplied;
pub use events::ImeAppliedResult;
pub use events::ImeCancelCause;
pub use events::ImeCanceled;
pub use events::ImeCommitCause;
pub use events::ImeCommitRequested;
pub use events::ImeRejectCommit;
pub use events::ImeRejection;
pub use events::ImeStarted;
pub use events::ImeValidationRejected;
pub use field::ImeAppOwnedFieldSpec;
pub use field::ImeBuiltInApplied;
pub use field::ImeBuiltInFieldKind;
pub use field::ImeBuiltInFieldSpec;
pub use field::ImeEditableFieldSpec;
pub use field::ImePanelField;
pub use ids::ImeCommitAttemptId;
pub use ids::ImeSessionId;
pub use ids::ImeValueRevision;
pub use ids::PanelFieldId;
pub use lease::ImeInputBlocker;
use session::ActiveImeSession;
pub use session::ImeOpenSession;
pub use session::ImeRequestCancel;
pub use session::ImeRequestCommit;
pub use target::ImeTarget;

/// Installs the single-line IME session lifecycle.
pub(crate) struct ImePlugin;

impl Plugin for ImePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveImeSession>()
            .init_resource::<ImeInputBlocker>()
            .add_observer(activation::observe_panel_clicks)
            .add_observer(session::open_session)
            .add_observer(session::request_commit)
            .add_observer(session::request_cancel)
            .add_observer(session::accept_commit)
            .add_observer(session::reject_commit)
            .add_systems(
                Update,
                (
                    session::lease_scoped_commands,
                    session::cleanup_stale_sessions,
                ),
            );
    }
}
