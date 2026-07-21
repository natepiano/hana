use std::marker::PhantomData;

use bevy::camera::visibility::RenderLayers;
use bevy::ecs::change_detection::Ref;
use bevy::ecs::change_detection::Tick;
use bevy::ecs::world::DeferredWorld;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_kana::resolve_entity_cascade;
use hana_valence::AnchoredHere;
use hana_valence::AnchoredTo;
use hana_valence::Member;
use hana_valence::ResolvedAnchorGeometry;
use hana_valence::ResolvedAnchorOffset;

use super::ComputedDiegeticPanel;
use super::DiegeticPanel;
use super::DiegeticPanelChangeClassification;
use super::LastPanelDimensions;
use super::PanelAnchorOffset;
use super::PanelAttachmentAuthored;
use super::PanelPrecomposeCache;
use super::PanelScreenHandoff;
use super::PanelSpace;
use super::ResolvedScreenPanelPosition;
use super::SavedPanelScreenState;
use super::SavedPanelWorldState;
use super::anchoring::AnchoredWorldPanelPose;
use super::diegetic_panel::PreparedPanelScreenConversion;
use super::diegetic_panel::ScaledLayoutTreeCache;
use crate::cascade::Cascade;
use crate::cascade::CascadeAttribute;
use crate::cascade::CascadeDefault;
use crate::cascade::FontUnit;
use crate::cascade::HdrTextCoverageBias;
use crate::cascade::Resolved;
use crate::cascade::SdfMaterial;
use crate::cascade::ShapeMaterial;
use crate::cascade::TextAlpha;
use crate::cascade::TextMaterial;
use crate::layout::GlyphShadowMode;
use crate::layout::Lighting;
use crate::layout::ShadowCasting;
use crate::layout::Sidedness;
use crate::render;
use crate::render::AntiAlias;
use crate::render::HairlineFade;
use crate::render::PanelTextRuns;
use crate::render::ResolvedSdfSurfaceRegistry;
use crate::screen_space;
use crate::screen_space::ScreenSpaceCamera;
use crate::screen_space::ScreenSpaceLight;
use crate::widgets;
use crate::widgets::PanelWidget;
use crate::widgets::PanelWidgetIndex;
use crate::widgets::PanelWidgets;
use crate::widgets::ScreenWidgetAnchorProxy;
use crate::widgets::ScreenWidgetAnchoredHere;
use crate::widgets::ScreenWidgetAnchoredTo;
use crate::widgets::WidgetFocusAuthority;
use crate::widgets::WidgetInteractivity;

/// Identifies the panel role that created and owns one runtime entity.
#[derive(Clone, Copy, Component)]
pub(crate) struct PanelOwned {
    owner: Entity,
}

impl PanelOwned {
    pub(crate) const fn owner(self) -> Entity { self.owner }
}

impl From<Entity> for PanelOwned {
    fn from(owner: Entity) -> Self { Self { owner } }
}

#[derive(Clone, Copy)]
enum PanelRemoval {
    RoleOnly,
    Entity,
}

/// Records that Hana inserted `T` and the change tick of Hana's latest write.
///
/// A different change tick means application code replaced or mutated `T`.
/// Hana then relinquishes ownership and leaves that component untouched.
#[derive(Component)]
pub(crate) struct PanelComponentOwnership<T: Component> {
    owner:        Entity,
    written_tick: Tick,
    marker:       PhantomData<T>,
}

impl<T: Component> PanelComponentOwnership<T> {
    const fn new(owner: Entity, written_tick: Tick) -> Self {
        Self {
            owner,
            written_tick,
            marker: PhantomData,
        }
    }

    pub(crate) fn owns(&self, owner: Entity, current: Tick) -> bool {
        self.owner == owner && self.written_tick == current
    }

    pub(crate) const fn owner(&self) -> Entity { self.owner }
}

#[derive(Clone, Component)]
pub(super) struct PanelCascadeOwnership<A: CascadeAttribute> {
    owner:                 Entity,
    resolved_restore:      Option<Resolved<A>>,
    resolved_written_tick: Option<Tick>,
}

#[derive(Component)]
pub(super) struct PreservedResolved<A: CascadeAttribute>(Resolved<A>);

#[derive(Clone)]
enum RenderLayersWrite {
    Present {
        render_layers: RenderLayers,
        changed_tick:  Tick,
    },
    Absent,
}

impl RenderLayersWrite {
    fn current(world: &World, entity: Entity) -> Self {
        world
            .get_entity(entity)
            .ok()
            .and_then(|entity_ref| entity_ref.get_ref::<RenderLayers>())
            .map_or(Self::Absent, |render_layers| Self::Present {
                render_layers: (*render_layers).clone(),
                changed_tick:  render_layers.last_changed(),
            })
    }

    fn matches_current(&self, world: &World, entity: Entity) -> bool {
        match (self, Self::current(world, entity)) {
            (
                Self::Present {
                    render_layers: written,
                    changed_tick: written_tick,
                },
                Self::Present {
                    render_layers: current,
                    changed_tick: current_tick,
                },
            ) => *written_tick == current_tick && *written == current,
            (Self::Absent, Self::Absent) => true,
            (Self::Present { .. }, Self::Absent) | (Self::Absent, Self::Present { .. }) => false,
        }
    }

    fn matches_ref(&self, current: Option<&Ref<'_, RenderLayers>>) -> bool {
        match (self, current) {
            (
                Self::Present {
                    render_layers: written,
                    changed_tick: written_tick,
                },
                Some(current),
            ) => *written_tick == current.last_changed() && *written == **current,
            (Self::Absent, None) => true,
            (Self::Present { .. }, None) | (Self::Absent, Some(_)) => false,
        }
    }
}

/// Records a `RenderLayers` value written by one panel role and the value that
/// must be restored when that role ends.
#[derive(Clone, Component)]
pub(crate) struct PanelRenderLayersOwnership {
    owner:               Entity,
    restore:             Option<RenderLayers>,
    render_layers_write: RenderLayersWrite,
}

impl PanelRenderLayersOwnership {
    pub(crate) fn is_owned_by(&self, panel: Entity) -> bool { self.owner == panel }

    pub(crate) fn matches_current(
        &self,
        panel: Entity,
        current: Option<&Ref<'_, RenderLayers>>,
    ) -> bool {
        self.owner == panel && self.render_layers_write.matches_ref(current)
    }
}

pub(super) fn teardown_panel_role(
    trigger: On<Remove, DiegeticPanel>,
    panels: Query<(Entity, &DiegeticPanel)>,
    owned_entities: Query<(Entity, &PanelOwned)>,
    world_widget_demands: Query<&AnchoredHere, With<PanelWidget>>,
    screen_widget_demands: Query<&ScreenWidgetAnchoredHere, With<PanelWidget>>,
    authored_attachments: Query<&PanelAttachmentAuthored>,
    world_attachments: Query<&AnchoredTo>,
    screen_attachments: Query<&ScreenWidgetAnchoredTo>,
    parents: Query<&ChildOf>,
    cameras: Query<(Entity, &ScreenSpaceCamera)>,
    lights: Query<(Entity, &ScreenSpaceLight)>,
    primary: Query<Entity, With<PrimaryWindow>>,
    mut resolved_surfaces: Option<ResMut<ResolvedSdfSurfaceRegistry>>,
    mut focus_authority: Option<ResMut<WidgetFocusAuthority>>,
    mut commands: Commands,
) {
    let entity = trigger.entity;
    let Ok((_, panel)) = panels.get(entity) else {
        return;
    };
    screen_space::cleanup_screen_space_view(
        entity,
        panel,
        &panels,
        &cameras,
        &lights,
        &primary,
        &mut commands,
    );
    if let Some(resolved_surfaces) = resolved_surfaces.as_deref_mut() {
        resolved_surfaces.remove_panel(entity);
    }

    if let Some(focus_authority) = focus_authority.as_deref_mut() {
        widgets::finalize_panel_focus(entity, focus_authority, &mut commands);
    }

    finalize_widget_anchor_state(
        entity,
        &owned_entities,
        &world_widget_demands,
        &screen_widget_demands,
        &authored_attachments,
        &world_attachments,
        &screen_attachments,
        &mut commands,
    );

    let panel_removal = trigger
        .trigger()
        .new_archetype
        .map_or(PanelRemoval::Entity, |_| PanelRemoval::RoleOnly);
    for owned_entity in panel_owned_despawn_roots(entity, panel_removal, &owned_entities, &parents)
    {
        commands.entity(owned_entity).despawn();
    }
    commands.queue(move |world: &mut World| teardown_owned_shared_state(world, entity));

    if matches!(panel_removal, PanelRemoval::Entity) {
        return;
    }

    let mut panel_entity = commands.entity(entity);
    panel_entity.remove::<(
        ComputedDiegeticPanel,
        DiegeticPanelChangeClassification,
        LastPanelDimensions,
        PanelPrecomposeCache,
        PanelWidgetIndex,
        ResolvedScreenPanelPosition,
        ScaledLayoutTreeCache,
        PanelSpace,
        PreparedPanelScreenConversion,
    )>();
    panel_entity.remove::<(
        PanelAttachmentAuthored,
        PanelAnchorOffset,
        Member,
        AnchoredWorldPanelPose,
        PanelScreenHandoff,
        SavedPanelScreenState,
        SavedPanelWorldState,
        PanelWidgets,
        PanelTextRuns,
        ScreenWidgetAnchoredTo,
    )>();
    render::remove_panel_shape_relationship(&mut panel_entity);
}

pub(super) fn finalize_panel_focus_before_despawn(
    trigger: On<Despawn, DiegeticPanel>,
    mut focus_authority: ResMut<WidgetFocusAuthority>,
    mut commands: Commands,
) {
    widgets::finalize_panel_focus(trigger.entity, &mut focus_authority, &mut commands);
}

fn finalize_widget_anchor_state(
    panel: Entity,
    owned_entities: &Query<(Entity, &PanelOwned)>,
    world_widget_demands: &Query<&AnchoredHere, With<PanelWidget>>,
    screen_widget_demands: &Query<&ScreenWidgetAnchoredHere, With<PanelWidget>>,
    authored_attachments: &Query<&PanelAttachmentAuthored>,
    world_attachments: &Query<&AnchoredTo>,
    screen_attachments: &Query<&ScreenWidgetAnchoredTo>,
    commands: &mut Commands<'_, '_>,
) {
    for (widget, ownership) in owned_entities {
        if ownership.owner() != panel {
            continue;
        }
        let mut dependents = world_widget_demands
            .get(widget)
            .ok()
            .into_iter()
            .flat_map(AnchoredHere::iter)
            .collect::<HashSet<_>>();
        dependents.extend(
            screen_widget_demands
                .get(widget)
                .ok()
                .into_iter()
                .flat_map(ScreenWidgetAnchoredHere::iter),
        );
        for dependent in dependents {
            let authored_targets_widget = authored_attachments
                .get(dependent)
                .is_ok_and(|attachment| attachment.target() == widget);
            let world_targets_widget = world_attachments
                .get(dependent)
                .is_ok_and(|attachment| attachment.target() == widget);
            let screen_targets_widget = screen_attachments
                .get(dependent)
                .is_ok_and(|attachment| attachment.target() == widget);
            if authored_targets_widget {
                commands
                    .entity(dependent)
                    .remove::<(PanelAttachmentAuthored, PanelAnchorOffset)>();
            } else {
                if world_targets_widget {
                    commands.entity(dependent).remove::<AnchoredTo>();
                }
                if screen_targets_widget {
                    commands
                        .entity(dependent)
                        .remove::<ScreenWidgetAnchoredTo>();
                }
            }
        }
        remove_owned_component::<AnchoredTo>(commands, panel, widget);
        remove_owned_component::<ResolvedAnchorGeometry>(commands, panel, widget);
        remove_owned_component::<ScreenWidgetAnchorProxy>(commands, panel, widget);
    }
}

/// Despawns a deferred runtime spawn when its recorded panel role no longer
/// exists by the time `PanelOwned` is inserted.
pub(super) fn finalize_orphaned_panel_owned(
    trigger: On<Add, PanelOwned>,
    ownership: Query<&PanelOwned>,
    panels: Query<(), With<DiegeticPanel>>,
    mut commands: Commands,
) {
    let entity = trigger.entity;
    let Ok(ownership) = ownership.get(entity) else {
        return;
    };
    if panels.get(ownership.owner()).is_err() {
        commands.entity(entity).despawn();
    }
}

fn panel_owned_despawn_roots(
    panel: Entity,
    panel_removal: PanelRemoval,
    owned_entities: &Query<(Entity, &PanelOwned)>,
    parents: &Query<&ChildOf>,
) -> Vec<Entity> {
    let owned = owned_entities
        .iter()
        .filter(|(_, ownership)| ownership.owner() == panel)
        .map(|(entity, _)| entity)
        .collect::<HashSet<_>>();

    owned
        .iter()
        .copied()
        .filter(|entity| {
            matches!(panel_removal, PanelRemoval::RoleOnly)
                || !has_ancestor(*entity, panel, parents)
        })
        .filter(|entity| !has_owned_ancestor(*entity, &owned, parents))
        .collect()
}

fn has_ancestor(entity: Entity, ancestor: Entity, parents: &Query<&ChildOf>) -> bool {
    let mut current = entity;
    let mut visited = HashSet::new();
    while visited.insert(current) {
        let Ok(parent) = parents.get(current) else {
            return false;
        };
        current = parent.parent();
        if current == ancestor {
            return true;
        }
    }
    false
}

fn has_owned_ancestor(entity: Entity, owned: &HashSet<Entity>, parents: &Query<&ChildOf>) -> bool {
    let mut current = entity;
    let mut visited = HashSet::new();
    while visited.insert(current) {
        let Ok(parent) = parents.get(current) else {
            return false;
        };
        current = parent.parent();
        if owned.contains(&current) {
            return true;
        }
    }
    false
}

/// Inserts `value` only when `T` is absent and records that Hana created it.
pub(super) fn seed_owned_component<T: Component>(
    world: &mut World,
    owner: Entity,
    entity: Entity,
    value: T,
) {
    let Ok(entity_ref) = world.get_entity(entity) else {
        return;
    };
    if entity_ref.contains::<T>() {
        return;
    }
    insert_owned_component(world, owner, entity, value);
}

/// Records each derived cache write while the authored cascade remains owned
/// by the panel role.
pub(super) fn record_resolved_ownership<A: CascadeAttribute>(
    trigger: On<Insert, Resolved<A>>,
    mut world: DeferredWorld,
) {
    let entity = trigger.entity;
    let Some((owner, authored_written)) = world
        .get::<PanelComponentOwnership<Cascade<A>>>(entity)
        .map(|ownership| (ownership.owner, ownership.written_tick))
    else {
        return;
    };
    let authored_is_owned = world
        .get_entity(entity)
        .ok()
        .and_then(|entity_ref| entity_ref.get_ref::<Cascade<A>>())
        .is_some_and(|authored| authored.last_changed() == authored_written);
    let resolved = world
        .get_entity(entity)
        .ok()
        .and_then(|entity_ref| entity_ref.get_ref::<Resolved<A>>())
        .map(|resolved| (resolved.0.clone(), resolved.last_changed()));
    let cascade_value = resolve_entity_cascade::<A>(&world, entity);
    let Some(mut cascade_ownership) = world.get_mut::<PanelCascadeOwnership<A>>(entity) else {
        return;
    };
    if cascade_ownership.owner != owner {
        return;
    }
    cascade_ownership.resolved_written_tick = resolved.and_then(|(resolved, written)| {
        (authored_is_owned && cascade_value.as_ref() == Some(&resolved)).then_some(written)
    });
}

/// Restores a preserved application cache after the cascade engine removes
/// the cache at the authored-component removal boundary.
pub(super) fn restore_preserved_resolved<A: CascadeAttribute>(
    trigger: On<Remove, Resolved<A>>,
    preserved: Query<&PreservedResolved<A>>,
    mut commands: Commands,
) {
    let entity = trigger.entity;
    let Ok(preserved) = preserved.get(entity) else {
        return;
    };
    commands
        .entity(entity)
        .insert(preserved.0.clone())
        .remove::<PreservedResolved<A>>();
}

/// Writes a Hana-owned component without replacing application-owned state.
pub(crate) fn write_owned_component<T: Component>(
    commands: &mut Commands<'_, '_>,
    owner: Entity,
    entity: Entity,
    value: T,
) {
    commands.queue(move |world: &mut World| {
        let ownership = world
            .get::<PanelComponentOwnership<T>>(entity)
            .map(|ownership| (ownership.owner, ownership.written_tick));
        match ownership {
            Some((recorded_owner, written)) if recorded_owner == owner => {
                let still_owned = world
                    .get_entity(entity)
                    .ok()
                    .and_then(|entity_ref| entity_ref.get_ref::<T>())
                    .is_some_and(|component| component.last_changed() == written);
                if still_owned {
                    insert_owned_component(world, owner, entity, value);
                } else if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
                    entity_mut.remove::<PanelComponentOwnership<T>>();
                }
            },
            Some(_) => {},
            None => seed_owned_component(world, owner, entity, value),
        }
    });
}

/// Updates a panel-seeded authored cascade while its ownership tick matches.
pub(super) fn write_owned_cascade<A: CascadeAttribute>(
    commands: &mut Commands<'_, '_>,
    owner: Entity,
    entity: Entity,
    value: Cascade<A>,
) {
    commands.queue(move |world: &mut World| {
        let ownership = world
            .get::<PanelComponentOwnership<Cascade<A>>>(entity)
            .map(|ownership| (ownership.owner, ownership.written_tick));
        match ownership {
            Some((recorded_owner, written)) if recorded_owner == owner => {
                let still_owned = world
                    .get_entity(entity)
                    .ok()
                    .and_then(|entity_ref| entity_ref.get_ref::<Cascade<A>>())
                    .is_some_and(|component| component.last_changed() == written);
                if still_owned {
                    insert_owned_cascade(world, owner, entity, value);
                } else if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
                    entity_mut.remove::<(
                        PanelComponentOwnership<Cascade<A>>,
                        PanelCascadeOwnership<A>,
                    )>();
                }
            },
            Some(_) => {},
            None => seed_owned_cascade(world, owner, entity, value),
        }
    });
}

/// Inserts an authored cascade only when absent and records its pre-existing
/// derived cache for role teardown.
pub(super) fn seed_owned_cascade<A: CascadeAttribute>(
    world: &mut World,
    owner: Entity,
    entity: Entity,
    value: Cascade<A>,
) {
    let Ok(entity_ref) = world.get_entity(entity) else {
        return;
    };
    if entity_ref.contains::<Cascade<A>>() {
        return;
    }
    let resolved_restore = entity_ref.get::<Resolved<A>>().cloned();
    let Ok(mut entity_mut) = world.get_entity_mut(entity) else {
        return;
    };
    entity_mut.insert(PanelCascadeOwnership::<A> {
        owner,
        resolved_restore,
        resolved_written_tick: None,
    });
    insert_owned_cascade(world, owner, entity, value);
}

/// Removes `T` only while its ownership record still matches Hana's write.
pub(crate) fn remove_owned_component<T: Component>(
    commands: &mut Commands<'_, '_>,
    owner: Entity,
    entity: Entity,
) {
    commands.queue(move |world: &mut World| {
        remove_owned_component_now::<T>(world, owner, entity);
    });
}

/// Writes or removes `RenderLayers` while retaining the application value that
/// preceded Hana's first write.
pub(crate) fn write_owned_render_layers(
    commands: &mut Commands<'_, '_>,
    owner: Entity,
    entity: Entity,
    value: Option<RenderLayers>,
) {
    commands.queue(move |world: &mut World| {
        let current = world.get::<RenderLayers>(entity).cloned();
        let existing = world.get::<PanelRenderLayersOwnership>(entity).cloned();
        let restore = match existing {
            Some(ownership) if ownership.owner == owner => {
                if !ownership.render_layers_write.matches_current(world, entity) {
                    if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
                        entity_mut.remove::<PanelRenderLayersOwnership>();
                    }
                    return;
                }
                ownership.restore
            },
            Some(_) => return,
            None => current,
        };
        {
            let Ok(mut entity_mut) = world.get_entity_mut(entity) else {
                return;
            };
            if let Some(value) = value {
                entity_mut.insert(value);
            } else {
                entity_mut.remove::<RenderLayers>();
            }
        }
        let written = RenderLayersWrite::current(world, entity);
        world.entity_mut(entity).insert(PanelRenderLayersOwnership {
            owner,
            restore,
            render_layers_write: written,
        });
    });
}

fn insert_owned_component<T: Component>(
    world: &mut World,
    owner: Entity,
    entity: Entity,
    value: T,
) {
    let Ok(mut entity_mut) = world.get_entity_mut(entity) else {
        return;
    };
    entity_mut.insert(value);
    let Some(written) = entity_mut
        .get_ref::<T>()
        .map(|component| component.last_changed())
    else {
        return;
    };
    entity_mut.insert(PanelComponentOwnership::<T>::new(owner, written));
}

fn insert_owned_cascade<A: CascadeAttribute>(
    world: &mut World,
    owner: Entity,
    entity: Entity,
    value: Cascade<A>,
) {
    let written = world.change_tick();
    let Ok(mut entity_mut) = world.get_entity_mut(entity) else {
        return;
    };
    entity_mut.insert(PanelComponentOwnership::<Cascade<A>>::new(owner, written));
    entity_mut.insert(value);
}

enum OwnershipDisposition {
    Preserved,
    Removed,
}

fn remove_owned_component_now<T: Component>(
    world: &mut World,
    owner: Entity,
    entity: Entity,
) -> OwnershipDisposition {
    let ownership = world
        .get::<PanelComponentOwnership<T>>(entity)
        .map(|ownership| (ownership.owner, ownership.written_tick));
    let Some((recorded_owner, written)) = ownership else {
        return OwnershipDisposition::Preserved;
    };
    if recorded_owner != owner {
        return OwnershipDisposition::Preserved;
    }
    let still_owned = world
        .get_entity(entity)
        .ok()
        .and_then(|entity_ref| entity_ref.get_ref::<T>())
        .is_some_and(|component| component.last_changed() == written);
    let Ok(mut entity_mut) = world.get_entity_mut(entity) else {
        return OwnershipDisposition::Preserved;
    };
    entity_mut.remove::<PanelComponentOwnership<T>>();
    if still_owned {
        entity_mut.remove::<T>();
        OwnershipDisposition::Removed
    } else {
        OwnershipDisposition::Preserved
    }
}

fn teardown_owned_shared_state(world: &mut World, panel: Entity) {
    restore_render_layers(world, panel);
    remove_seeded_cascade::<WidgetInteractivity>(world, panel);
    remove_seeded_cascade::<TextAlpha>(world, panel);
    remove_seeded_cascade::<FontUnit>(world, panel);
    remove_seeded_cascade::<HdrTextCoverageBias>(world, panel);
    remove_seeded_cascade::<Lighting>(world, panel);
    remove_seeded_cascade::<ShadowCasting>(world, panel);
    remove_seeded_cascade::<Sidedness>(world, panel);
    remove_seeded_cascade::<AntiAlias>(world, panel);
    remove_seeded_cascade::<HairlineFade>(world, panel);
    remove_seeded_cascade::<GlyphShadowMode>(world, panel);
    remove_seeded_cascade::<SdfMaterial>(world, panel);
    remove_seeded_cascade::<TextMaterial>(world, panel);
    remove_seeded_cascade::<ShapeMaterial>(world, panel);
    remove_owned_component_now::<AnchoredTo>(world, panel, panel);
    remove_owned_component_now::<ResolvedAnchorOffset>(world, panel, panel);
    remove_owned_component_now::<ResolvedAnchorGeometry>(world, panel, panel);
}

fn remove_seeded_cascade<A: CascadeAttribute>(world: &mut World, panel: Entity) {
    let Some(ledger) = world.get::<PanelCascadeOwnership<A>>(panel).cloned() else {
        return;
    };
    let current = world
        .get_entity(panel)
        .ok()
        .and_then(|entity_ref| entity_ref.get_ref::<Resolved<A>>())
        .map(|resolved| (Resolved(resolved.0.clone()), resolved.last_changed()));
    let current_is_derived = ledger
        .resolved_written_tick
        .zip(current.as_ref().map(|(_, written)| *written))
        .is_some_and(|(derived, current)| derived == current);
    let preserved = if current_is_derived {
        ledger.resolved_restore
    } else {
        current.map(|(resolved, _)| resolved)
    };
    let cascade_engine_installed = world.contains_resource::<CascadeDefault<A>>();
    let ownership_disposition = remove_owned_component_now::<Cascade<A>>(world, panel, panel);
    let Ok(mut entity_mut) = world.get_entity_mut(panel) else {
        return;
    };
    entity_mut.remove::<PanelCascadeOwnership<A>>();
    if matches!(ownership_disposition, OwnershipDisposition::Preserved) {
        return;
    }
    if cascade_engine_installed && let Some(preserved) = preserved {
        entity_mut.insert(PreservedResolved::<A>(preserved));
    }
}

fn restore_render_layers(world: &mut World, panel: Entity) {
    let mut query = world.query::<(Entity, &PanelRenderLayersOwnership)>();
    let owned = query
        .iter(world)
        .filter(|(_, ownership)| ownership.owner == panel)
        .map(|(entity, ownership)| (entity, ownership.clone()))
        .collect::<Vec<_>>();
    for (entity, ownership) in owned {
        let still_owned = ownership.render_layers_write.matches_current(world, entity);
        let Ok(mut entity_mut) = world.get_entity_mut(entity) else {
            continue;
        };
        if still_owned {
            if let Some(restore) = ownership.restore {
                entity_mut.insert(restore);
            } else {
                entity_mut.remove::<RenderLayers>();
            }
        }
        entity_mut.remove::<PanelRenderLayersOwnership>();
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use bevy::asset::AssetPlugin;
    use bevy::camera::visibility::RenderLayers;
    use bevy::camera::visibility::VisibilityPlugin;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::mesh::MeshPlugin;
    use bevy::picking::InteractionPlugin;
    use bevy::picking::Pickable;
    use bevy::picking::PickingPlugin;
    use bevy::picking::mesh_picking::MeshPickingPlugin;
    use bevy::prelude::*;
    use bevy::render::storage::ShaderBuffer;
    use bevy::transform::TransformPlugin;
    use bevy::window::PrimaryWindow;
    use hana_valence::AnchorId;
    use hana_valence::AnchorPose;
    use hana_valence::AnchoredTo;
    use hana_valence::ArrangementMembers;
    use hana_valence::Hinge;
    use hana_valence::Member;
    use hana_valence::MemberIndex;
    use hana_valence::PendingMemberPlacement;
    use hana_valence::QuadTiling;
    use hana_valence::ResolvedAnchorGeometry;
    use hana_valence::Strip;

    use super::PanelCascadeOwnership;
    use super::PanelComponentOwnership;
    use super::PanelRenderLayersOwnership;
    use super::PreservedResolved;
    use super::write_owned_cascade;
    use super::write_owned_render_layers;
    use crate::ArrangedPanel;
    use crate::Button;
    use crate::DiegeticPanel;
    use crate::DiegeticPanelCommands as _;
    use crate::El;
    use crate::HeadlessLayoutPlugin;
    use crate::LayoutBuilder;
    use crate::LayoutTree;
    use crate::Mm;
    use crate::PanelAttachment;
    use crate::PanelDraw;
    use crate::PanelElementId;
    use crate::PanelEntityReader;
    use crate::PanelLine;
    use crate::PanelPoint;
    use crate::PanelWidget;
    use crate::PanelWidgetReader;
    use crate::Px;
    use crate::TextStyle;
    use crate::Unit;
    use crate::WidgetOf;
    use crate::cascade::Cascade;
    use crate::cascade::CascadeDefault;
    use crate::cascade::CascadeFrom;
    use crate::cascade::FontUnit;
    use crate::cascade::Resolved;
    use crate::cascade::TextAlpha;
    use crate::layout::Anchor;
    use crate::panel::PanelAttachmentAuthored;
    use crate::panel::PanelOwned;
    use crate::panel::arrangement::PanelArrangementRuntime;
    use crate::render::AntiAlias;
    use crate::render::HairlineFade;
    use crate::render::PanelInteractionMesh;
    use crate::render::PanelTextRuns;
    use crate::render::RenderPlugin;
    use crate::render::ResolvedSdfSurfaceRegistry;
    use crate::render::TextRunOf;
    use crate::screen_space::ScreenSpacePlugin;
    use crate::text::DiegeticTextMeasurer;
    use crate::text::TextPlugin;
    use crate::widgets::PanelWidgetIndex;
    use crate::widgets::PanelWidgets;
    use crate::widgets::ScreenWidgetAnchorProxy;
    use crate::widgets::ScreenWidgetAnchoredHere;
    use crate::widgets::ScreenWidgetAnchoredTo;
    use crate::widgets::WidgetInteractivity;
    use crate::widgets::WidgetsPlugin;

    #[derive(Component)]
    struct ApplicationState;

    struct OwnershipFixture {
        panel:                   Entity,
        layerless_child:         Entity,
        application_layer_child: Entity,
        runtime_child:           Entity,
        cascade_source:          Entity,
        anchor_target:           Entity,
        interactivity:           Cascade<WidgetInteractivity>,
        pose:                    AnchorPose,
        hairline:                HairlineFade,
        font:                    FontUnit,
    }

    fn ownership_teardown_fixture() -> (App, OwnershipFixture) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(DiegeticTextMeasurer::default())
            .add_plugins((HeadlessLayoutPlugin, WidgetsPlugin));

        let cascade_source = app.world_mut().spawn_empty().id();
        let anchor_target = app.world_mut().spawn_empty().id();
        let interactivity = Cascade::Override(WidgetInteractivity::Disabled);
        let pose = AnchorPose {
            rotation:    Quat::from_rotation_y(0.25),
            translation: Vec3::new(1.0, 2.0, 3.0),
        };
        let anchor = AnchoredTo::new(anchor_target, AnchorId::Center, AnchorId::Center);
        let hairline = HairlineFade::Fade { exponent: 1.25 };
        let panel_bundle = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .with_tree(LayoutBuilder::new(100.0, 50.0).build())
            .build()
            .expect("panel should build");
        let panel = app
            .world_mut()
            .spawn((
                panel_bundle,
                ApplicationState,
                interactivity,
                CascadeFrom::new(cascade_source),
                pose,
                anchor,
                Resolved(hairline),
                RenderLayers::layer(7),
            ))
            .id();
        let layerless_child = app
            .world_mut()
            .spawn((ApplicationState, ChildOf(panel)))
            .id();
        let application_layer_child = app
            .world_mut()
            .spawn((ApplicationState, ChildOf(panel)))
            .id();
        app.update();

        assert!(
            app.world()
                .get::<PanelComponentOwnership<Cascade<FontUnit>>>(panel)
                .is_some(),
            "the construction seed should record the cascade it inserted",
        );
        let runtime_child = app
            .world_mut()
            .spawn((PanelOwned::from(panel), ChildOf(panel)))
            .id();
        app.world_mut()
            .run_system_once(move |mut commands: Commands| {
                for entity in [panel, layerless_child, application_layer_child] {
                    write_owned_render_layers(
                        &mut commands,
                        panel,
                        entity,
                        Some(RenderLayers::layer(3)),
                    );
                }
            })
            .expect("layer ownership writes should run");
        app.world_mut()
            .entity_mut(application_layer_child)
            .insert(RenderLayers::layer(9));
        let font = FontUnit(Unit::Pixels);
        app.world_mut().entity_mut(panel).insert(Resolved(font));
        app.world_mut().entity_mut(panel).remove::<DiegeticPanel>();
        app.update();

        (
            app,
            OwnershipFixture {
                panel,
                layerless_child,
                application_layer_child,
                runtime_child,
                cascade_source,
                anchor_target,
                interactivity,
                pose,
                hairline,
                font,
            },
        )
    }

    #[test]
    fn teardown_restores_only_state_owned_by_the_panel_role() {
        let (app, fixture) = ownership_teardown_fixture();
        let world = app.world();
        assert!(world.get_entity(fixture.panel).is_ok());
        assert!(world.get::<ApplicationState>(fixture.panel).is_some());
        assert!(world.get_entity(fixture.layerless_child).is_ok());
        assert!(world.get_entity(fixture.application_layer_child).is_ok());
        assert!(world.get_entity(fixture.runtime_child).is_err());
        assert_eq!(
            world.get::<Cascade<WidgetInteractivity>>(fixture.panel),
            Some(&fixture.interactivity),
        );
        assert_eq!(
            world
                .get::<CascadeFrom>(fixture.panel)
                .map(CascadeFrom::target),
            Some(fixture.cascade_source),
        );
        assert_eq!(world.get::<AnchorPose>(fixture.panel), Some(&fixture.pose));
        assert_eq!(
            world
                .get::<AnchoredTo>(fixture.panel)
                .map(AnchoredTo::target),
            Some(fixture.anchor_target),
        );
        assert!(world.get::<Cascade<FontUnit>>(fixture.panel).is_none());
        assert_eq!(
            world
                .get::<Resolved<FontUnit>>(fixture.panel)
                .map(|resolved| resolved.0),
            Some(fixture.font),
        );
        assert!(world.get::<Cascade<AntiAlias>>(fixture.panel).is_none());
        assert!(world.get::<Resolved<AntiAlias>>(fixture.panel).is_none());
        assert!(world.get::<Cascade<HairlineFade>>(fixture.panel).is_none());
        assert_eq!(
            world
                .get::<Resolved<HairlineFade>>(fixture.panel)
                .map(|resolved| resolved.0),
            Some(fixture.hairline),
        );
        assert_eq!(
            world.get::<RenderLayers>(fixture.panel),
            Some(&RenderLayers::layer(7))
        );
        assert!(world.get::<RenderLayers>(fixture.layerless_child).is_none());
        assert_eq!(
            world.get::<RenderLayers>(fixture.application_layer_child),
            Some(&RenderLayers::layer(9)),
        );
        assert!(world.get::<PanelWidgetIndex>(fixture.panel).is_none());
        assert!(world.get::<PanelWidgets>(fixture.panel).is_none());
        assert!(
            world
                .get::<PanelComponentOwnership<Cascade<FontUnit>>>(fixture.panel)
                .is_none(),
        );
        assert!(
            world
                .get::<PanelCascadeOwnership<FontUnit>>(fixture.panel)
                .is_none()
        );
        assert!(
            world
                .get::<PanelRenderLayersOwnership>(fixture.panel)
                .is_none()
        );
        assert!(
            world
                .get::<PanelRenderLayersOwnership>(fixture.layerless_child)
                .is_none(),
        );
        assert!(
            world
                .get::<PanelRenderLayersOwnership>(fixture.application_layer_child)
                .is_none(),
        );
    }

    #[test]
    fn screen_propagation_restores_layers_and_preserves_same_value_and_aba_writes() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(DiegeticTextMeasurer::default())
            .add_plugins(HeadlessLayoutPlugin)
            .add_plugins(ScreenSpacePlugin);
        app.world_mut().spawn((Window::default(), PrimaryWindow));

        let panel_bundle = DiegeticPanel::screen()
            .size(crate::Px(100.0), crate::Px(50.0))
            .render_layers(RenderLayers::layer(3))
            .layout(|_| {})
            .build()
            .expect("screen panel should build");
        let panel = app
            .world_mut()
            .spawn((panel_bundle, RenderLayers::layer(7)))
            .id();
        let introduced_child = app.world_mut().spawn(ChildOf(panel)).id();
        let replaced_child = app.world_mut().spawn(ChildOf(panel)).id();
        let same_value_child = app.world_mut().spawn(ChildOf(panel)).id();
        let aba_child = app.world_mut().spawn(ChildOf(panel)).id();

        app.update();

        assert_eq!(
            app.world().get::<RenderLayers>(panel),
            Some(&RenderLayers::layer(3)),
        );
        assert_eq!(
            app.world().get::<RenderLayers>(introduced_child),
            Some(&RenderLayers::layer(3)),
        );
        assert_eq!(
            app.world().get::<RenderLayers>(replaced_child),
            Some(&RenderLayers::layer(3)),
        );
        assert_eq!(
            app.world().get::<RenderLayers>(same_value_child),
            Some(&RenderLayers::layer(3)),
        );
        assert_eq!(
            app.world().get::<RenderLayers>(aba_child),
            Some(&RenderLayers::layer(3)),
        );
        app.world_mut()
            .entity_mut(replaced_child)
            .insert(RenderLayers::layer(9));
        app.world_mut()
            .entity_mut(same_value_child)
            .insert(RenderLayers::layer(3));
        app.world_mut()
            .entity_mut(aba_child)
            .insert(RenderLayers::layer(9));
        app.update();
        app.world_mut()
            .entity_mut(aba_child)
            .insert(RenderLayers::layer(3));
        app.update();

        app.world_mut().entity_mut(panel).remove::<DiegeticPanel>();
        app.update();

        assert_eq!(
            app.world().get::<RenderLayers>(panel),
            Some(&RenderLayers::layer(7)),
        );
        assert!(app.world().get::<RenderLayers>(introduced_child).is_none(),);
        assert_eq!(
            app.world().get::<RenderLayers>(replaced_child),
            Some(&RenderLayers::layer(9)),
        );
        assert_eq!(
            app.world().get::<RenderLayers>(same_value_child),
            Some(&RenderLayers::layer(3)),
        );
        assert_eq!(
            app.world().get::<RenderLayers>(aba_child),
            Some(&RenderLayers::layer(3)),
        );
    }

    #[test]
    fn inheriting_seed_tracks_source_propagation_before_teardown() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(DiegeticTextMeasurer::default())
            .add_plugins(HeadlessLayoutPlugin);

        let source = app
            .world_mut()
            .spawn(Cascade::Override(FontUnit(Unit::Millimeters)))
            .id();
        let panel_bundle = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .font_unit(Unit::Meters)
            .layout(|_| {})
            .build()
            .expect("panel should build");
        let panel = app
            .world_mut()
            .spawn((panel_bundle, CascadeFrom::new(source)))
            .id();
        app.update();
        app.world_mut()
            .run_system_once(move |mut commands: Commands| {
                write_owned_cascade(&mut commands, panel, panel, Cascade::<FontUnit>::Inherit);
            })
            .expect("owned cascade write should run");
        app.update();
        assert_eq!(
            app.world()
                .get::<Resolved<FontUnit>>(panel)
                .map(|resolved| resolved.0),
            Some(FontUnit(Unit::Millimeters)),
        );

        app.world_mut()
            .entity_mut(source)
            .insert(Cascade::Override(FontUnit(Unit::Points)));
        app.update();
        assert_eq!(
            app.world()
                .get::<Resolved<FontUnit>>(panel)
                .map(|resolved| resolved.0),
            Some(FontUnit(Unit::Points)),
        );

        app.world_mut().entity_mut(panel).remove::<DiegeticPanel>();
        app.update();

        assert!(app.world().get::<Cascade<FontUnit>>(panel).is_none());
        assert!(app.world().get::<Resolved<FontUnit>>(panel).is_none());
        assert_eq!(
            app.world()
                .get::<CascadeFrom>(panel)
                .map(CascadeFrom::target),
            Some(source),
        );
    }

    #[test]
    fn equal_owned_cascade_write_does_not_claim_later_application_cache() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(DiegeticTextMeasurer::default())
            .add_plugins(HeadlessLayoutPlugin);
        let panel_bundle = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .font_unit(Unit::Meters)
            .layout(|_| {})
            .build()
            .expect("panel should build");
        let panel = app.world_mut().spawn(panel_bundle).id();
        app.update();
        let derived_written = app
            .world()
            .get::<PanelCascadeOwnership<FontUnit>>(panel)
            .and_then(|ownership| ownership.resolved_written_tick)
            .expect("initial derived cache should be recorded");

        app.world_mut()
            .run_system_once(move |mut commands: Commands| {
                write_owned_cascade(
                    &mut commands,
                    panel,
                    panel,
                    Cascade::Override(FontUnit(Unit::Meters)),
                );
            })
            .expect("equal owned cascade write should run");
        app.update();
        assert_eq!(
            app.world()
                .get::<PanelCascadeOwnership<FontUnit>>(panel)
                .and_then(|ownership| ownership.resolved_written_tick),
            Some(derived_written),
        );

        app.world_mut()
            .entity_mut(panel)
            .insert(Resolved(FontUnit(Unit::Points)));
        assert!(
            app.world()
                .get::<PanelCascadeOwnership<FontUnit>>(panel)
                .is_some_and(|ownership| ownership.resolved_written_tick.is_none()),
        );
        app.world_mut().entity_mut(panel).remove::<DiegeticPanel>();
        app.update();

        assert_eq!(
            app.world()
                .get::<Resolved<FontUnit>>(panel)
                .map(|resolved| resolved.0),
            Some(FontUnit(Unit::Points)),
        );
    }

    #[test]
    fn preexisting_cache_returns_only_when_current_cache_is_derived() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(DiegeticTextMeasurer::default())
            .add_plugins(HeadlessLayoutPlugin);
        let restored_bundle = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .font_unit(Unit::Meters)
            .layout(|_| {})
            .build()
            .expect("first panel should build");
        let replaced_bundle = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .font_unit(Unit::Meters)
            .layout(|_| {})
            .build()
            .expect("second panel should build");
        let restored = app
            .world_mut()
            .spawn((restored_bundle, Resolved(FontUnit(Unit::Pixels))))
            .id();
        let replaced = app
            .world_mut()
            .spawn((replaced_bundle, Resolved(FontUnit(Unit::Pixels))))
            .id();
        app.update();
        assert_eq!(
            app.world()
                .get::<Resolved<FontUnit>>(restored)
                .map(|resolved| resolved.0),
            Some(FontUnit(Unit::Meters)),
        );
        assert_eq!(
            app.world()
                .get::<Resolved<FontUnit>>(replaced)
                .map(|resolved| resolved.0),
            Some(FontUnit(Unit::Meters)),
        );

        app.world_mut()
            .entity_mut(replaced)
            .insert(Resolved(FontUnit(Unit::Points)));
        app.world_mut()
            .entity_mut(restored)
            .remove::<DiegeticPanel>();
        app.world_mut()
            .entity_mut(replaced)
            .remove::<DiegeticPanel>();
        app.update();

        assert_eq!(
            app.world()
                .get::<Resolved<FontUnit>>(restored)
                .map(|resolved| resolved.0),
            Some(FontUnit(Unit::Pixels)),
        );
        assert_eq!(
            app.world()
                .get::<Resolved<FontUnit>>(replaced)
                .map(|resolved| resolved.0),
            Some(FontUnit(Unit::Points)),
        );
    }

    #[test]
    fn headless_teardown_leaves_no_pending_cache_bookkeeping() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(DiegeticTextMeasurer::default())
            .add_plugins(HeadlessLayoutPlugin);
        assert!(!app.world().contains_resource::<CascadeDefault<TextAlpha>>(),);
        let panel_bundle = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .layout(|_| {})
            .build()
            .expect("panel should build");
        let cache = TextAlpha(AlphaMode::Opaque);
        let panel = app.world_mut().spawn((panel_bundle, Resolved(cache))).id();
        app.update();
        assert!(app.world().get::<Cascade<TextAlpha>>(panel).is_some());

        app.world_mut().entity_mut(panel).remove::<DiegeticPanel>();
        app.update();

        assert!(app.world().get::<Cascade<TextAlpha>>(panel).is_none());
        assert_eq!(
            app.world()
                .get::<Resolved<TextAlpha>>(panel)
                .map(|resolved| resolved.0),
            Some(cache),
        );
        assert!(
            app.world()
                .get::<PanelCascadeOwnership<TextAlpha>>(panel)
                .is_none(),
        );
        assert!(
            app.world()
                .get::<PreservedResolved<TextAlpha>>(panel)
                .is_none(),
        );
    }

    fn production_teardown_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            MeshPlugin,
            TransformPlugin,
            VisibilityPlugin,
        ));
        app.init_asset::<Shader>()
            .init_asset::<ShaderBuffer>()
            .init_asset::<Image>()
            .add_plugins((PickingPlugin, InteractionPlugin, MeshPickingPlugin))
            .add_plugins((
                TextPlugin,
                HeadlessLayoutPlugin,
                WidgetsPlugin,
                RenderPlugin,
            ));
        app
    }

    fn production_teardown_tree() -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(
            El::column()
                .size(100.0, 50.0)
                .background(Color::srgb(0.2, 0.3, 0.4))
                .draw(PanelDraw::lines([PanelLine::new(
                    PanelPoint::new(0.0, 0.0),
                    PanelPoint::new(20.0, 10.0),
                )])),
            |builder| {
                builder.text(("live text", TextStyle::new(10.0)));
                builder.with(
                    El::new().size(20.0, 10.0).button("action", Button::new()),
                    |_| {},
                );
            },
        );
        builder.build()
    }

    fn widget_anchor_teardown_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TransformPlugin))
            .insert_resource(DiegeticTextMeasurer::default())
            .add_plugins((HeadlessLayoutPlugin, WidgetsPlugin, ScreenSpacePlugin));
        app.world_mut().spawn((Window::default(), PrimaryWindow));
        app
    }

    fn anchor_teardown_tree() -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(
            El::new().size(20.0, 10.0).button("target", Button::new()),
            |_| {},
        );
        builder.build()
    }

    #[test]
    fn screen_widget_owner_role_teardown_detaches_application_dependent() {
        let mut app = widget_anchor_teardown_app();
        let owner_panel = DiegeticPanel::screen()
            .size(Px(100.0), Px(50.0))
            .anchor(Anchor::TopLeft)
            .with_tree(anchor_teardown_tree())
            .build()
            .expect("screen owner should build");
        let owner = app.world_mut().spawn((owner_panel, ApplicationState)).id();
        app.update();
        let widget = resolve_widget(&mut app, owner, "target");
        let dependent_panel = DiegeticPanel::screen()
            .size(Px(30.0), Px(10.0))
            .anchor(Anchor::TopLeft)
            .layout(|_| {})
            .build()
            .expect("screen dependent should build");
        let dependent = app
            .world_mut()
            .spawn((dependent_panel, ApplicationState))
            .id();
        let target_id = PanelElementId::named("target");
        app.world_mut()
            .run_system_once(
                move |panels: PanelEntityReader,
                      widgets: PanelWidgetReader,
                      mut commands: Commands| {
                    let source = panels
                        .screen(dependent)
                        .expect("dependent is a screen panel");
                    let owner = panels.screen(owner).expect("owner is a screen panel");
                    let target = widgets
                        .typed_entity(owner, &target_id)
                        .expect("target is a screen widget");
                    commands.attach_to_widget(
                        source,
                        target,
                        PanelAttachment::new(Anchor::TopLeft, Anchor::BottomLeft),
                    );
                },
            )
            .expect("attachment system runs");
        app.update();

        assert!(
            app.world()
                .get::<ScreenWidgetAnchoredHere>(widget)
                .is_some_and(|demand| demand.contains(&dependent))
        );
        assert!(app.world().get::<ScreenWidgetAnchorProxy>(widget).is_some());
        assert!(app.world().get::<ResolvedAnchorGeometry>(widget).is_some());
        assert!(
            app.world()
                .get::<ScreenWidgetAnchoredTo>(dependent)
                .is_some()
        );

        app.world_mut().entity_mut(owner).remove::<DiegeticPanel>();
        app.update();

        let world = app.world();
        assert!(world.get_entity(owner).is_ok());
        assert!(world.get::<ApplicationState>(owner).is_some());
        assert!(world.get_entity(widget).is_err());
        assert!(world.get_entity(dependent).is_ok());
        assert!(world.get::<ApplicationState>(dependent).is_some());
        assert!(world.get::<PanelAttachmentAuthored>(dependent).is_none());
        assert!(world.get::<ScreenWidgetAnchoredTo>(dependent).is_none());
        assert!(!world.iter_entities().any(|entity| {
            entity
                .get::<PanelOwned>()
                .is_some_and(|ownership| ownership.owner() == owner)
        }));
    }

    #[test]
    fn production_children_and_indexes_leave_no_semantic_orphans() {
        let mut app = production_teardown_app();
        let tree = production_teardown_tree();
        let panel_bundle = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .with_tree(tree.clone())
            .build()
            .expect("panel should build");
        let panel = app.world_mut().spawn((panel_bundle, ApplicationState)).id();
        let application_child = app
            .world_mut()
            .spawn((ApplicationState, Pickable::default(), ChildOf(panel)))
            .id();
        app.update();
        app.update();

        let widget = resolve_widget(&mut app, panel, "action");
        assert!(app.world().get::<PanelWidget>(widget).is_some());
        let identical = app.world_mut().commands().set_tree(panel, tree);
        assert!(identical.is_ok());
        app.update();
        app.update();
        assert!(app.world().get::<crate::DiegeticPanel>(panel).is_some());
        assert_eq!(resolve_widget(&mut app, panel, "action"), widget);

        let owned_children = {
            let world = app.world_mut();
            let mut query = world.query::<(Entity, &PanelOwned)>();
            query
                .iter(world)
                .filter(|(_, ownership)| ownership.owner() == panel)
                .map(|(entity, _)| entity)
                .collect::<Vec<_>>()
        };
        assert!(owned_children.len() >= 4);
        assert!(
            owned_children
                .iter()
                .any(|entity| { app.world().get::<PanelInteractionMesh>(*entity).is_some() })
        );
        assert!(
            owned_children
                .iter()
                .any(|entity| { app.world().get::<TextRunOf>(*entity).is_some() })
        );
        assert!(owned_children.contains(&widget));
        assert!(
            app.world()
                .resource::<ResolvedSdfSurfaceRegistry>()
                .surfaces()
                .any(|surface| surface.panel_entity() == panel),
        );

        app.world_mut().entity_mut(panel).remove::<DiegeticPanel>();
        app.update();
        app.update();

        let world = app.world();
        assert!(world.get_entity(panel).is_ok());
        assert!(world.get::<ApplicationState>(panel).is_some());
        assert!(world.get_entity(application_child).is_ok());
        assert!(world.get::<Pickable>(application_child).is_some());
        assert!(
            owned_children
                .iter()
                .all(|entity| world.get_entity(*entity).is_err())
        );
        assert!(world.get::<PanelWidgetIndex>(panel).is_none());
        assert!(world.get::<PanelWidgets>(panel).is_none());
        assert!(world.get::<PanelTextRuns>(panel).is_none());
        assert!(
            !world
                .resource::<ResolvedSdfSurfaceRegistry>()
                .surfaces()
                .any(|surface| surface.panel_entity() == panel),
        );
        assert!(!world.iter_entities().any(|entity| {
            entity
                .get::<WidgetOf>()
                .is_some_and(|relationship| relationship.panel() == panel)
        }));
        assert!(!world.iter_entities().any(|entity| {
            entity
                .get::<TextRunOf>()
                .is_some_and(|relationship| relationship.panel() == panel)
        }));
        assert!(!world.iter_entities().any(|entity| {
            entity.get::<PanelInteractionMesh>().is_some()
                && entity
                    .get::<ChildOf>()
                    .is_some_and(|child_of| child_of.parent() == panel)
        }));
    }

    #[test]
    fn removing_panel_role_clears_arrangement_membership_and_runtime_state() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TransformPlugin))
            .insert_resource(DiegeticTextMeasurer::default())
            .add_plugins(HeadlessLayoutPlugin);
        let arrangement_panel = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .layout(|_| {})
            .build()
            .expect("arrangement panel should build");
        let arrangement = app
            .world_mut()
            .spawn((arrangement_panel, Transform::default(), Strip, QuadTiling))
            .id();
        let member_panel = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .layout(|_| {})
            .build()
            .expect("member panel should build");
        let member = app
            .world_mut()
            .spawn((
                member_panel,
                Transform::default(),
                ArrangedPanel::new(arrangement),
                ApplicationState,
            ))
            .id();

        app.update();
        app.update();
        app.update();

        assert!(app.world().get::<Member>(member).is_some());
        assert!(app.world().get::<MemberIndex>(member).is_some());
        assert!(app.world().get::<AnchoredTo>(member).is_some());
        assert!(app.world().get::<AnchorPose>(member).is_some());
        assert!(app.world().get::<Hinge>(member).is_some());
        assert!(app.world().get::<ResolvedAnchorGeometry>(member).is_some());
        assert!(
            app.world()
                .get::<ArrangementMembers>(arrangement)
                .is_some_and(|members| members.iter().any(|entity| entity == member)),
        );

        app.world_mut().entity_mut(member).remove::<DiegeticPanel>();
        app.update();
        app.update();

        let world = app.world();
        assert!(world.get_entity(member).is_ok());
        assert!(world.get::<ApplicationState>(member).is_some());
        assert!(world.get::<Transform>(member).is_some());
        assert!(world.get::<Member>(member).is_none());
        assert!(world.get::<MemberIndex>(member).is_none());
        assert!(world.get::<PendingMemberPlacement>(member).is_none());
        assert!(world.get::<AnchoredTo>(member).is_none());
        assert!(world.get::<AnchorPose>(member).is_none());
        assert!(world.get::<Hinge>(member).is_none());
        assert!(world.get::<PanelArrangementRuntime>(member).is_none());
        assert!(world.get::<ResolvedAnchorGeometry>(member).is_none());
        assert!(
            world
                .get::<ArrangementMembers>(arrangement)
                .is_none_or(|members| members.iter().all(|entity| entity != member)),
        );
    }

    #[test]
    fn reparented_production_widget_is_removed_with_its_panel_role() {
        let mut app = production_teardown_app();
        let panel_bundle = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .with_tree(production_teardown_tree())
            .build()
            .expect("panel should build");
        let panel = app.world_mut().spawn(panel_bundle).id();
        let application_parent = app.world_mut().spawn(ApplicationState).id();
        app.update();
        app.update();
        let widget = resolve_widget(&mut app, panel, "action");
        app.world_mut()
            .entity_mut(widget)
            .insert(ChildOf(application_parent));

        app.world_mut().entity_mut(panel).remove::<DiegeticPanel>();
        app.update();

        assert!(app.world().get_entity(widget).is_err());
        assert!(app.world().get_entity(application_parent).is_ok());
        assert!(
            app.world()
                .get::<ApplicationState>(application_parent)
                .is_some()
        );
    }

    #[test]
    fn reparented_production_widget_is_removed_when_its_panel_is_despawned() {
        let mut app = production_teardown_app();
        let panel_bundle = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .with_tree(production_teardown_tree())
            .build()
            .expect("panel should build");
        let panel = app.world_mut().spawn(panel_bundle).id();
        let application_parent = app.world_mut().spawn(ApplicationState).id();
        app.update();
        app.update();
        let widget = resolve_widget(&mut app, panel, "action");
        app.world_mut()
            .entity_mut(widget)
            .insert(ChildOf(application_parent));

        app.world_mut().entity_mut(panel).despawn();
        app.update();

        assert!(app.world().get_entity(widget).is_err());
        assert!(app.world().get_entity(application_parent).is_ok());
        assert!(
            app.world()
                .get::<ApplicationState>(application_parent)
                .is_some()
        );
    }

    #[test]
    fn deferred_panel_owned_spawn_is_removed_after_owner_role_teardown() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(DiegeticTextMeasurer::default())
            .add_plugins(HeadlessLayoutPlugin);
        let panel_bundle = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .layout(|_| {})
            .build()
            .expect("panel should build");
        let panel = app.world_mut().spawn(panel_bundle).id();
        app.update();
        app.world_mut().entity_mut(panel).remove::<DiegeticPanel>();
        let late_spawn = app
            .world_mut()
            .spawn((PanelOwned::from(panel), ApplicationState))
            .id();
        app.update();

        assert!(app.world().get_entity(late_spawn).is_err());
        assert!(app.world().get_entity(panel).is_ok());
    }

    fn resolve_widget(app: &mut App, panel: Entity, id: &str) -> Entity {
        let id = PanelElementId::named(id);
        app.world_mut()
            .run_system_once(move |reader: PanelWidgetReader| reader.entity(panel, &id))
            .expect("reader system should run")
            .expect("widget should be reified")
    }
}
