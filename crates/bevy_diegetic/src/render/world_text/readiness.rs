use bevy::prelude::*;

/// Marker on a [`WorldText`](super::WorldText) entity whose glyphs are not yet fully
/// rasterized in the atlas. Removed automatically when all glyphs
/// become ready.
#[derive(Component)]
pub struct PendingGlyphs;

/// Internal marker: glyphs are ready and meshes are spawned, but we
/// wait for Bevy's transform propagation before firing [`WorldTextReady`].
#[derive(Component)]
pub struct AwaitingReady;

/// Fired on a [`WorldText`](super::WorldText) entity when all its glyphs are rasterized
/// and the text is fully rendered for the first time (or after a
/// text/style change).
///
/// Observe per-entity:
/// ```ignore
/// commands.spawn((WorldText::new("Hello"), ...))
///     .observe(|trigger: On<WorldTextReady>| {
///         info!("Text ready on {:?}", trigger.entity());
///     });
/// ```
#[derive(EntityEvent)]
pub struct WorldTextReady {
    /// The [`WorldText`](super::WorldText) entity that is now fully rendered.
    pub entity: Entity,
}

/// Fires [`WorldTextReady`] for entities whose meshes and transforms are
/// now fully propagated. Runs after `CalculateBounds` so that `Aabb` and
/// `GlobalTransform` are available on mesh children.
pub fn emit_world_text_ready(awaiting: Query<Entity, With<AwaitingReady>>, mut commands: Commands) {
    for entity in &awaiting {
        commands.entity(entity).remove::<AwaitingReady>();
        commands
            .entity(entity)
            .trigger(|current_entity| WorldTextReady {
                entity: current_entity,
            });
    }
}
