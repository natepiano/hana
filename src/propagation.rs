use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;

use super::NoOutline;
use super::Outline;

/// When `Outline` is added to an entity, propagate it to all descendant `Mesh3d` entities.
/// Skips entities with `NoOutline`. Sets `group_source` for `Grouped` overlap mode.
pub(crate) fn propagate_outline_to_descendants(
    added: On<Add, Outline>,
    outline_query: Query<&Outline>,
    mesh_query: Query<(), (With<Mesh3d>, Without<NoOutline>)>,
    children_query: Query<&Children>,
    mut commands: Commands,
) {
    let source = added.entity;
    let Ok(outline) = outline_query.get(source) else {
        return;
    };

    // Don't re-propagate from entities that received their outline via propagation
    if outline.group_source.is_some() {
        return;
    }

    let mut propagated = outline.clone();
    propagated.group_source = Some(source);

    for descendant in children_query.iter_descendants(source) {
        if mesh_query.contains(descendant) {
            commands.entity(descendant).insert(propagated.clone());
        }
    }
}

/// When a new child is added to the hierarchy, check if any ancestor has `Outline`
/// and propagate it. Handles glTF scene loading where children spawn after the parent.
pub(crate) fn propagate_outline_on_child_added(
    added: On<Add, ChildOf>,
    child_mesh_query: Query<(), (With<Mesh3d>, Without<NoOutline>)>,
    outline_query: Query<&Outline>,
    parent_query: Query<&ChildOf>,
    mut commands: Commands,
) {
    let child = added.entity;
    if !child_mesh_query.contains(child) {
        return;
    }

    // Follow `parent_query` through `ChildOf::parent` until a source `Outline` is found.
    let mut current = child;
    while let Ok(child_of) = parent_query.get(current) {
        let parent = child_of.parent();
        if let Ok(outline) = outline_query.get(parent) {
            // Preserve `Outline::group_source` when the parent outline is already propagated.
            let source = outline.group_source.unwrap_or(parent);
            let mut propagated = outline.clone();
            propagated.group_source = Some(source);
            commands.entity(child).insert(propagated);
            return;
        }
        current = parent;
    }
}

/// When `Mesh3d` is added to an entity, check if any ancestor has `Outline` and propagate it.
/// Handles glTF scene loading where `Mesh3d` may be added after `ChildOf`.
pub(crate) fn propagate_outline_on_mesh_added(
    added: On<Add, Mesh3d>,
    no_outline_query: Query<(), With<NoOutline>>,
    outline_query: Query<&Outline>,
    parent_query: Query<&ChildOf>,
    existing_outline: Query<(), With<Outline>>,
    mut commands: Commands,
) {
    let child = added.entity;
    if no_outline_query.contains(child) {
        return;
    }
    if existing_outline.contains(child) {
        return;
    }

    // Follow `parent_query` through `ChildOf::parent` until an `Outline` is found.
    let mut current = child;
    while let Ok(child_of) = parent_query.get(current) {
        let parent = child_of.parent();
        if let Ok(outline) = outline_query.get(parent) {
            let source = outline.group_source.unwrap_or(parent);
            let mut propagated = outline.clone();
            propagated.group_source = Some(source);
            commands.entity(child).insert(propagated);
            return;
        }
        current = parent;
    }
}

/// When a `WorldInstanceReady` fires on an entity with `Outline`, propagate to
/// all descendant meshes. This handles the `WorldAssetRoot` case where the world instance
/// entity may not have a `ChildOf` back to the entity with `Outline`.
pub(crate) fn propagate_outline_on_scene_ready(
    ready: On<WorldInstanceReady>,
    outline_query: Query<&Outline>,
    mesh_query: Query<(), (With<Mesh3d>, Without<NoOutline>)>,
    children_query: Query<&Children>,
    mut commands: Commands,
) {
    let source = ready.entity;
    let Ok(outline) = outline_query.get(source) else {
        return;
    };
    if outline.group_source.is_some() {
        return;
    }

    let mut propagated = outline.clone();
    propagated.group_source = Some(source);

    for descendant in children_query.iter_descendants(source) {
        if mesh_query.contains(descendant) {
            commands.entity(descendant).insert(propagated.clone());
        }
    }
}

/// When `Outline` is removed from a source entity, remove it from all descendants.
/// Only acts on source outlines (not propagated copies) to avoid cascading removals.
pub(crate) fn remove_outline_from_descendants(
    removed: On<Remove, Outline>,
    outline_query: Query<&Outline>,
    mesh_query: Query<(), With<Mesh3d>>,
    children_query: Query<&Children>,
    mut commands: Commands,
) {
    let source = removed.entity;

    // Check if any descendant has a propagated outline from this source.
    // If descendants have outlines with a different source (or no source), leave them alone.
    for descendant in children_query.iter_descendants(source) {
        if !mesh_query.contains(descendant) {
            continue;
        }
        if let Ok(desc_outline) = outline_query.get(descendant)
            && desc_outline.group_source == Some(source)
        {
            commands.entity(descendant).try_remove::<Outline>();
        }
    }
}

/// When a source `Outline` changes, update all descendant copies.
pub(crate) fn sync_propagated_outlines(
    changed_outlines: Query<(Entity, &Outline, &Children), Changed<Outline>>,
    mesh_query: Query<(), (With<Mesh3d>, Without<NoOutline>)>,
    children_query: Query<&Children>,
    mut outline_mut: Query<&mut Outline, Without<Children>>,
) {
    for (source, outline, _) in &changed_outlines {
        // Only sync outlines that are sources (no group_source means this is the original)
        if outline.group_source.is_some() {
            continue;
        }

        let mut propagated = outline.clone();
        propagated.group_source = Some(source);

        for descendant in children_query.iter_descendants(source) {
            if mesh_query.contains(descendant)
                && let Ok(mut desc_outline) = outline_mut.get_mut(descendant)
            {
                *desc_outline = propagated.clone();
            }
        }
    }
}
