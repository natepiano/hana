use bevy::prelude::*;

use super::AwaitingOverlayReady;
use super::OverlayContainer;
use super::TypographyOverlay;
use super::TypographyOverlayReady;

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

/// Fires [`TypographyOverlayReady`] for each entity awaiting it, naming the
/// overlay bounds entity (`AwaitingOverlayReady::target`) as the fit target.
///
/// The overlay's geometry — bounds rectangle, metric lines, labels — is built
/// synchronously by the same `build_typography_overlay` pass that inserts
/// [`AwaitingOverlayReady`], so the bounds entity is measurable as soon as this
/// marker appears; no per-label readiness gate is needed.
pub fn emit_typography_overlay_ready(
    awaiting: Query<(Entity, &AwaitingOverlayReady)>,
    mut commands: Commands,
) {
    for (entity, awaiting) in &awaiting {
        commands.entity(entity).remove::<AwaitingOverlayReady>();
        let target = awaiting.target;
        commands.entity(target).trigger(|e| TypographyOverlayReady {
            entity: e,
            owner:  entity,
        });
    }
}
