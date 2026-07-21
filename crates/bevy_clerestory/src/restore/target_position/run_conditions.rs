use bevy::prelude::*;

use super::target::TargetPosition;

/// Run condition: returns true if any entity has a `TargetPosition` component.
pub(crate) fn has_restoring_windows(query: Query<(), With<TargetPosition>>) -> bool {
    !query.is_empty()
}
