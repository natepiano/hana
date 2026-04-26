use bevy::prelude::*;

use super::AwaitingOverlayReady;
use super::OverlayContainer;
use super::TypographyOverlay;
use super::TypographyOverlayReady;
use crate::render::PendingGlyphs;

/// Observer: spawns an [`OverlayContainer`] child when
/// [`TypographyOverlay`] is added to an entity.
pub fn on_overlay_added(trigger: On<Add, TypographyOverlay>, mut commands: Commands) {
    commands.entity(trigger.entity).with_child((
        OverlayContainer,
        Transform::IDENTITY,
        Visibility::Inherited,
    ));
}

/// Observer: despawns the [`OverlayContainer`] child (and all its
/// descendants) when [`TypographyOverlay`] is removed from an entity.
pub fn on_overlay_removed(
    trigger: On<Remove, TypographyOverlay>,
    containers: Query<(Entity, &ChildOf), With<OverlayContainer>>,
    mut commands: Commands,
) {
    let parent = trigger.entity;
    for (container_entity, child_of) in &containers {
        if child_of.parent() == parent {
            commands.entity(container_entity).despawn();
        }
    }
}

/// Checks overlay label readiness and fires [`TypographyOverlayReady`]
/// once all descendant text labels have no [`PendingGlyphs`].
pub fn emit_typography_overlay_ready(
    awaiting: Query<(Entity, &AwaitingOverlayReady)>,
    pending: Query<(), With<PendingGlyphs>>,
    children_query: Query<&Children>,
    mut commands: Commands,
) {
    for (entity, awaiting) in &awaiting {
        let any_pending = children_query
            .iter_descendants(entity)
            .any(|d| pending.get(d).is_ok());
        if any_pending {
            continue;
        }
        commands.entity(entity).remove::<AwaitingOverlayReady>();
        let ready_target = awaiting.ready_target;
        commands
            .entity(ready_target)
            .trigger(|e| TypographyOverlayReady {
                entity: e,
                owner:  entity,
            });
    }
}
