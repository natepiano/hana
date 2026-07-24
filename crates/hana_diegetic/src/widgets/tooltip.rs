use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use bevy::asset::Handle;
use bevy::camera::primitives::Aabb;
use bevy::color::Color;
use bevy::image::Image;
use bevy::mesh::Mesh3d;
use bevy::platform::collections::HashMap as BevyHashMap;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use hana_valence::AnchorId;
use hana_valence::AnchorPoint;
use hana_valence::AnchoredHere;
use hana_valence::Edge;
use hana_valence::ResolvedAnchorGeometry;

use super::TooltipFor;
use crate::layout::Anchor;
use crate::layout::ChildLayoutState;
use crate::layout::El;
use crate::layout::LayoutBuilder;
use crate::layout::LayoutOnly;
use crate::layout::LayoutTree;
use crate::layout::Px;
use crate::layout::Text;
use crate::panel::PanelAnchorOffset;
use crate::panel::PanelEntity;
use crate::panel::PanelSpace;
use crate::panel::Screen;
use crate::panel::WidgetEntity;
use crate::panel::World;

const DEFAULT_SHOW_DELAY: Duration = Duration::from_millis(500);
const DEFAULT_TOOLTIP_GAP: f32 = 8.0;
const QUAD_ANCHOR_COUNT: usize = 9;
const QUAD_EDGE_COUNT: usize = 4;

/// Behavior when the described widget is disabled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TooltipDisabledPolicy {
    /// Disabled targets may show their tooltip.
    Show,
    /// Disabled targets suppress their tooltip.
    Suppress,
}

/// Placement behavior when a tooltip would extend beyond its presentation area.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TooltipPlacementPolicy {
    /// Adjust placement to keep the tooltip visible.
    KeepVisible,
    /// Preserve the authored anchors and offset.
    Fixed,
}

/// Deferred visual and behavior declaration for a tooltip.
#[derive(Component, Debug)]
pub struct Tooltip {
    blueprint:        Arc<LayoutTree>,
    authoring:        TooltipAuthoring,
    show_after:       Duration,
    hide_after:       Duration,
    disabled_policy:  TooltipDisabledPolicy,
    source_anchor:    Anchor,
    target_anchor:    Anchor,
    offset:           PanelAnchorOffset,
    placement_policy: TooltipPlacementPolicy,
}

#[derive(Debug)]
struct TooltipAuthoring {
    parent_stack: Vec<usize>,
    next_auto_id: u32,
}

impl Clone for Tooltip {
    fn clone(&self) -> Self {
        Self {
            blueprint:        Arc::clone(&self.blueprint),
            // `parent_stack` exists only for the active `Tooltip::with` call;
            // an independent blueprint clone resumes authoring at the tree root.
            authoring:        TooltipAuthoring {
                parent_stack: vec![0],
                next_auto_id: self.authoring.next_auto_id,
            },
            show_after:       self.show_after,
            hide_after:       self.hide_after,
            disabled_policy:  self.disabled_policy,
            source_anchor:    self.source_anchor,
            target_anchor:    self.target_anchor,
            offset:           self.offset,
            placement_policy: self.placement_policy,
        }
    }
}

impl PartialEq for Tooltip {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.blueprint, &other.blueprint)
            && self.show_after == other.show_after
            && self.hide_after == other.hide_after
            && self.disabled_policy == other.disabled_policy
            && self.source_anchor == other.source_anchor
            && self.target_anchor == other.target_anchor
            && self.offset == other.offset
            && self.placement_policy == other.placement_policy
    }
}

impl Tooltip {
    /// Creates a deferred `Fit` by `Fit` tooltip with `root` as its visible root.
    ///
    /// ```compile_fail
    /// use hana_diegetic::{Button, El, Tooltip};
    ///
    /// let widget = El::new().button("nested", Button::new());
    /// let _ = Tooltip::new(widget);
    /// ```
    pub fn new<L>(root: El<L, LayoutOnly>) -> Self
    where
        L: ChildLayoutState,
    {
        Self {
            blueprint:        Arc::new(LayoutBuilder::with_root(root).build()),
            authoring:        TooltipAuthoring {
                parent_stack: vec![0],
                next_auto_id: 0,
            },
            show_after:       DEFAULT_SHOW_DELAY,
            hide_after:       Duration::ZERO,
            disabled_policy:  TooltipDisabledPolicy::Suppress,
            source_anchor:    Anchor::TopCenter,
            target_anchor:    Anchor::BottomCenter,
            offset:           PanelAnchorOffset::new(Px(0.0), Px(DEFAULT_TOOLTIP_GAP)),
            placement_policy: TooltipPlacementPolicy::KeepVisible,
        }
    }

    /// Adds a visual container and authors its descendants through this tooltip.
    ///
    /// ```compile_fail
    /// use hana_diegetic::{Button, El, Tooltip};
    ///
    /// let mut tooltip = Tooltip::new(El::new());
    /// tooltip.with(El::new().button("nested", Button::new()), |_| {});
    /// ```
    pub fn with<L>(
        &mut self,
        element: El<L, LayoutOnly>,
        children: impl FnOnce(&mut Self),
    ) -> &mut Self
    where
        L: ChildLayoutState,
    {
        let parent = self.current_parent();
        let child = Arc::make_mut(&mut self.blueprint).tooltip_add_container(parent, element);
        self.authoring.parent_stack.push(child);
        children(self);
        self.authoring.parent_stack.pop();
        self
    }

    /// Adds a text leaf under the current tooltip container.
    pub fn text(&mut self, text: impl Into<Text<LayoutOnly>>) -> &mut Self {
        let parent = self.current_parent();
        Arc::make_mut(&mut self.blueprint).tooltip_add_text(
            parent,
            text,
            &mut self.authoring.next_auto_id,
        );
        self
    }

    /// Adds an image leaf under the current tooltip container.
    ///
    /// ```compile_fail
    /// use bevy::asset::Handle;
    /// use bevy::color::Color;
    /// use bevy::image::Image;
    /// use hana_diegetic::{Button, El, Tooltip};
    ///
    /// let mut tooltip = Tooltip::new(El::new());
    /// tooltip.image(
    ///     El::new().button("nested", Button::new()),
    ///     Handle::<Image>::default(),
    ///     Color::WHITE,
    /// );
    /// ```
    pub fn image<L>(
        &mut self,
        element: El<L, LayoutOnly>,
        handle: Handle<Image>,
        tint: Color,
    ) -> &mut Self
    where
        L: ChildLayoutState,
    {
        let parent = self.current_parent();
        Arc::make_mut(&mut self.blueprint).tooltip_add_image(parent, element, handle, tint);
        self
    }

    /// Sets the delay before an eligible tooltip is shown.
    #[must_use]
    pub const fn show_after(mut self, delay: Duration) -> Self {
        self.show_after = delay;
        self
    }

    /// Sets the delay before an ineligible tooltip is hidden.
    #[must_use]
    pub const fn hide_after(mut self, delay: Duration) -> Self {
        self.hide_after = delay;
        self
    }

    /// Sets disabled-target behavior.
    #[must_use]
    pub const fn disabled_policy(mut self, policy: TooltipDisabledPolicy) -> Self {
        self.disabled_policy = policy;
        self
    }

    /// Sets the tooltip anchor used for placement.
    #[must_use]
    pub const fn source_anchor(mut self, anchor: Anchor) -> Self {
        self.source_anchor = anchor;
        self
    }

    /// Sets the described target anchor used for placement.
    #[must_use]
    pub const fn target_anchor(mut self, anchor: Anchor) -> Self {
        self.target_anchor = anchor;
        self
    }

    /// Sets the offset from the described target.
    #[must_use]
    pub const fn offset(mut self, offset: PanelAnchorOffset) -> Self {
        self.offset = offset;
        self
    }

    /// Sets the placement policy.
    #[must_use]
    pub const fn placement_policy(mut self, policy: TooltipPlacementPolicy) -> Self {
        self.placement_policy = policy;
        self
    }

    #[cfg(test)]
    fn tree(&self) -> &LayoutTree { &self.blueprint }

    fn current_parent(&self) -> usize { self.authoring.parent_stack.last().copied().unwrap_or(0) }
}

/// A typed identity for a general tooltip target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TooltipTargetEntity<Space> {
    entity: Entity,
    marker: PhantomData<fn() -> Space>,
}

impl<Space> TooltipTargetEntity<Space> {
    /// Returns the underlying Bevy entity for unrelated ECS work.
    #[must_use]
    pub const fn entity(&self) -> Entity { self.entity }

    pub(crate) const fn from_validated(entity: Entity) -> Self {
        Self {
            entity,
            marker: PhantomData,
        }
    }
}

/// Sealed coordinate-space marker accepted by [`TooltipTarget`].
pub trait TooltipTargetSpace: private::SealedSpace {
    #[doc(hidden)]
    const PANEL_SPACE: PanelSpace;
}

impl TooltipTargetSpace for World {
    const PANEL_SPACE: PanelSpace = PanelSpace::World;
}

impl TooltipTargetSpace for Screen {
    const PANEL_SPACE: PanelSpace = PanelSpace::Screen;
}

/// Typed entity handle that can be described by a tooltip.
pub trait TooltipTarget {
    /// Coordinate space supplied by the target's live placement data.
    type Space: TooltipTargetSpace;

    /// Returns the target entity.
    fn tooltip_target_entity(&self) -> Entity;

    /// Returns the captured panel entity for Hana's typed panel handle.
    #[doc(hidden)]
    fn tooltip_target_panel(&self) -> Option<Entity> { None }

    /// Returns the captured panel owner for Hana's typed widget handle.
    #[doc(hidden)]
    fn tooltip_target_widget_owner(&self) -> Option<Entity> { None }
}

impl<Space: TooltipTargetSpace> TooltipTarget for TooltipTargetEntity<Space> {
    type Space = Space;

    fn tooltip_target_entity(&self) -> Entity { self.entity }
}

impl<Space: TooltipTargetSpace> TooltipTarget for PanelEntity<Space> {
    type Space = Space;

    fn tooltip_target_entity(&self) -> Entity { self.entity() }

    fn tooltip_target_panel(&self) -> Option<Entity> { Some(self.entity()) }
}

impl<Space: TooltipTargetSpace> TooltipTarget for WidgetEntity<Space> {
    type Space = Space;

    fn tooltip_target_entity(&self) -> Entity { self.entity() }

    fn tooltip_target_widget_owner(&self) -> Option<Entity> { Some(self.owner()) }
}

/// Extension methods for checked standalone tooltip authoring.
pub trait TooltipCommandsExt {
    /// Reserves and returns a tooltip controller entity.
    fn spawn_tooltip<Target>(&mut self, target: Target, tooltip: Tooltip) -> Entity
    where
        Target: TooltipTarget;
}

/// Axis-aligned face selected as a rectangular anchor provider.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MeshFace {
    /// Face with a positive local X normal.
    PositiveX,
    /// Face with a negative local X normal.
    NegativeX,
    /// Face with a positive local Y normal.
    PositiveY,
    /// Face with a negative local Y normal.
    NegativeY,
    /// Face with a positive local Z normal.
    PositiveZ,
    /// Face with a negative local Z normal.
    NegativeZ,
}

/// Extension methods for checked general mesh anchor authoring.
pub trait MeshAnchorCommandsExt {
    /// Authors a persistent mesh-face anchor target.
    fn mesh_anchor_target(&mut self, entity: Entity, face: MeshFace) -> TooltipTargetEntity<World>;
}

/// Extension methods for checked general screen anchor authoring.
pub trait ScreenAnchorCommandsExt {
    /// Publishes screen placement data and returns a typed target handle.
    fn screen_anchor_target(
        &mut self,
        entity: Entity,
        data: crate::screen_space::ScreenAnchorTarget,
    ) -> TooltipTargetEntity<Screen>;
}

impl ScreenAnchorCommandsExt for Commands<'_, '_> {
    fn screen_anchor_target(
        &mut self,
        entity: Entity,
        data: crate::screen_space::ScreenAnchorTarget,
    ) -> TooltipTargetEntity<Screen> {
        self.queue(move |world: &mut bevy::ecs::world::World| {
            if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
                entity_mut.insert(data);
            }
        });
        TooltipTargetEntity::from_validated(entity)
    }
}

impl MeshAnchorCommandsExt for Commands<'_, '_> {
    fn mesh_anchor_target(&mut self, entity: Entity, face: MeshFace) -> TooltipTargetEntity<World> {
        self.run_system_cached_with(author_mesh_anchor_target, (entity, face));
        TooltipTargetEntity::from_validated(entity)
    }
}

impl TooltipCommandsExt for Commands<'_, '_> {
    fn spawn_tooltip<Target>(&mut self, target: Target, tooltip: Tooltip) -> Entity
    where
        Target: TooltipTarget,
    {
        let controller = self.spawn_empty().id();
        let operation = StandaloneTooltipOperation {
            controller,
            target: target.tooltip_target_entity(),
            space: Target::Space::PANEL_SPACE,
            provenance: target.tooltip_target_panel().map_or_else(
                || {
                    target
                        .tooltip_target_widget_owner()
                        .map_or(StandaloneTargetProvenance::General, |owner| {
                            StandaloneTargetProvenance::Widget { owner }
                        })
                },
                |_| StandaloneTargetProvenance::Panel,
            ),
            tooltip,
        };
        self.run_system_cached_with(apply_standalone_tooltip, operation);
        controller
    }
}

#[derive(Clone, Copy)]
enum StandaloneTargetProvenance {
    General,
    Panel,
    Widget { owner: Entity },
}

#[derive(Clone)]
struct StandaloneTooltipOperation {
    controller: Entity,
    target:     Entity,
    space:      PanelSpace,
    provenance: StandaloneTargetProvenance,
    tooltip:    Tooltip,
}

fn apply_standalone_tooltip(
    In(operation): In<StandaloneTooltipOperation>,
    targets: Query<(
        Option<&crate::panel::DiegeticPanel>,
        Option<&super::PanelWidget>,
        Option<&super::WidgetOf>,
        Option<&hana_valence::ResolvedAnchorGeometry>,
        Option<&super::MeshAnchorTarget>,
        Option<&crate::screen_space::ScreenAnchorTarget>,
    )>,
    mut commands: Commands,
) {
    let Ok((panel, widget, widget_of, geometry, mesh_target, screen_target)) =
        targets.get(operation.target)
    else {
        commands.entity(operation.controller).despawn();
        return;
    };
    let owner = match operation.provenance {
        StandaloneTargetProvenance::General => None,
        StandaloneTargetProvenance::Panel => panel
            .filter(|panel| PanelSpace::from(panel.coordinate_space()) == operation.space)
            .map(|_| operation.target),
        StandaloneTargetProvenance::Widget { owner } => widget
            .zip(widget_of)
            .filter(|(_, widget_of)| widget_of.panel() == owner)
            .and_then(|_| {
                targets
                    .get(owner)
                    .ok()
                    .and_then(|(panel, ..)| panel)
                    .filter(|panel| PanelSpace::from(panel.coordinate_space()) == operation.space)
                    .map(|_| owner)
            }),
    };
    let valid_target = match operation.provenance {
        StandaloneTargetProvenance::General => match operation.space {
            PanelSpace::World => geometry.is_some() || mesh_target.is_some(),
            PanelSpace::Screen => screen_target.is_some(),
        },
        StandaloneTargetProvenance::Panel | StandaloneTargetProvenance::Widget { .. } => {
            owner.is_some()
        },
    };
    if !valid_target {
        commands.entity(operation.controller).despawn();
        return;
    }

    let mut controller = commands.entity(operation.controller);
    controller.insert((operation.tooltip, TooltipFor::new(operation.target)));
    if let Some(owner) = owner {
        controller.insert(crate::panel::PanelOwned::from(owner));
    }
}

#[derive(Clone, Copy, Component)]
pub(crate) struct MeshAnchorTarget {
    face: MeshFace,
}

#[derive(Clone, Copy, Component)]
pub(super) struct MeshAnchorGeometry;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MeshAnchorPendingCause {
    WaitingForMesh,
    WaitingForBounds,
    UnusableBounds,
}

#[derive(Clone, Copy, Component)]
pub(super) struct MeshAnchorGeometryPending {
    cause: MeshAnchorPendingCause,
}

#[derive(Default, Resource)]
pub(crate) struct MeshAnchorWarnings(HashSet<Entity>);

fn author_mesh_anchor_target(
    In((entity, face)): In<(Entity, MeshFace)>,
    meshes: Query<(), With<Mesh3d>>,
    mut commands: Commands,
) {
    if meshes.contains(entity) {
        commands.entity(entity).insert(MeshAnchorTarget { face });
    }
}

pub(super) fn update_mesh_anchor_geometry(
    changed_targets: Query<
        Entity,
        (
            With<MeshAnchorTarget>,
            With<AnchoredHere>,
            Or<(
                Changed<AnchoredHere>,
                Changed<MeshAnchorTarget>,
                Changed<Mesh3d>,
                Changed<Aabb>,
            )>,
        ),
    >,
    demanded_targets: Query<
        (
            Entity,
            Ref<MeshAnchorTarget>,
            Option<Ref<Mesh3d>>,
            Option<Ref<Aabb>>,
            Option<&MeshAnchorGeometry>,
            Option<&MeshAnchorGeometryPending>,
        ),
        (With<MeshAnchorTarget>, With<AnchoredHere>),
    >,
    mut removed_meshes: RemovedComponents<Mesh3d>,
    mut removed_bounds: RemovedComponents<Aabb>,
    mut warnings: ResMut<MeshAnchorWarnings>,
    mut commands: Commands,
) {
    let mut candidates = changed_targets.iter().collect::<HashSet<_>>();
    candidates.extend(removed_meshes.read());
    candidates.extend(removed_bounds.read());

    for entity in candidates {
        let Ok((_, target, mesh, aabb, geometry, pending)) = demanded_targets.get(entity) else {
            continue;
        };
        if mesh.is_none() {
            mark_mesh_anchor_pending(
                entity,
                MeshAnchorPendingCause::WaitingForMesh,
                &mut commands,
            );
            continue;
        }
        let Some(aabb) = aabb else {
            mark_mesh_anchor_pending(
                entity,
                MeshAnchorPendingCause::WaitingForBounds,
                &mut commands,
            );
            continue;
        };
        if geometry.is_none() || pending.is_some() || target.is_changed() || aabb.is_changed() {
            write_mesh_anchor_geometry(entity, target.face, *aabb, &mut warnings, &mut commands);
        }
    }
}

fn mark_mesh_anchor_pending(
    entity: Entity,
    cause: MeshAnchorPendingCause,
    commands: &mut Commands,
) {
    commands
        .entity(entity)
        .remove::<(ResolvedAnchorGeometry, MeshAnchorGeometry)>()
        .insert(MeshAnchorGeometryPending { cause });
}

fn write_mesh_anchor_geometry(
    entity: Entity,
    face: MeshFace,
    aabb: Aabb,
    warnings: &mut MeshAnchorWarnings,
    commands: &mut Commands,
) {
    let Some(geometry) = mesh_face_geometry(aabb, face) else {
        mark_mesh_anchor_pending(entity, MeshAnchorPendingCause::UnusableBounds, commands);
        return;
    };
    warnings.0.remove(&entity);
    commands
        .entity(entity)
        .remove::<MeshAnchorGeometryPending>()
        .insert((geometry, MeshAnchorGeometry));
}

pub(super) fn warn_pending_mesh_anchor_geometry(
    targets: Query<
        (Entity, &MeshAnchorTarget, &MeshAnchorGeometryPending),
        (With<AnchoredHere>, Changed<MeshAnchorGeometryPending>),
    >,
    mut warnings: ResMut<MeshAnchorWarnings>,
) {
    for (entity, target, pending) in &targets {
        if warnings.0.insert(entity) {
            match pending.cause {
                MeshAnchorPendingCause::WaitingForMesh => {
                    warn!("mesh anchor target {entity:?} is waiting for Mesh3d");
                },
                MeshAnchorPendingCause::WaitingForBounds => {
                    warn!("mesh anchor target {entity:?} is waiting for Aabb");
                },
                MeshAnchorPendingCause::UnusableBounds => {
                    warn!(
                        "mesh anchor target {entity:?} has no usable bounds for face {:?}",
                        target.face
                    );
                },
            }
        }
    }
}

pub(super) fn remove_mesh_anchor_geometry(
    removed: On<Remove, AnchoredHere>,
    targets: Query<
        (
            Option<&MeshAnchorGeometry>,
            Option<&MeshAnchorGeometryPending>,
        ),
        With<MeshAnchorTarget>,
    >,
    mut warnings: ResMut<MeshAnchorWarnings>,
    mut commands: Commands,
) {
    let entity = removed.entity;
    warnings.0.remove(&entity);
    if targets
        .get(entity)
        .is_ok_and(|(geometry, pending)| geometry.is_some() || pending.is_some())
    {
        commands.entity(entity).remove::<(
            ResolvedAnchorGeometry,
            MeshAnchorGeometry,
            MeshAnchorGeometryPending,
        )>();
    }
}

fn mesh_face_geometry(aabb: Aabb, face: MeshFace) -> Option<ResolvedAnchorGeometry> {
    let center = Vec3::from(aabb.center);
    let half_extents = Vec3::from(aabb.half_extents);
    if !center.is_finite() || !half_extents.is_finite() {
        return None;
    }
    let frame = mesh_face_frame(face);
    let right = frame * Vec3::X;
    let up = frame * Vec3::Y;
    let normal = frame * Vec3::Z;
    let right_extent = right.abs().dot(half_extents);
    let up_extent = up.abs().dot(half_extents);
    let normal_extent = normal.abs().dot(half_extents);
    if right_extent <= 0.0 || up_extent <= 0.0 {
        return None;
    }
    let face_center = center + normal * normal_extent;
    let mut points = BevyHashMap::with_capacity(QUAD_ANCHOR_COUNT);
    for anchor in [
        Anchor::TopLeft,
        Anchor::TopRight,
        Anchor::BottomRight,
        Anchor::BottomLeft,
        Anchor::TopCenter,
        Anchor::CenterRight,
        Anchor::BottomCenter,
        Anchor::CenterLeft,
        Anchor::Center,
    ] {
        let (x_fraction, y_fraction) = anchor.offset_fraction();
        let x = x_fraction.mul_add(2.0, -1.0) * right_extent;
        let y = y_fraction.mul_add(-2.0, 1.0) * up_extent;
        points.insert(
            AnchorId::from(anchor),
            AnchorPoint {
                position: face_center + right * x + up * y,
                frame:    Some(frame),
            },
        );
    }
    let edges = vec![
        Edge {
            start: AnchorId::from(Anchor::TopLeft),
            end:   AnchorId::from(Anchor::TopRight),
        },
        Edge {
            start: AnchorId::from(Anchor::TopRight),
            end:   AnchorId::from(Anchor::BottomRight),
        },
        Edge {
            start: AnchorId::from(Anchor::BottomRight),
            end:   AnchorId::from(Anchor::BottomLeft),
        },
        Edge {
            start: AnchorId::from(Anchor::BottomLeft),
            end:   AnchorId::from(Anchor::TopLeft),
        },
    ];
    debug_assert_eq!(edges.len(), QUAD_EDGE_COUNT);
    Some(ResolvedAnchorGeometry { points, edges })
}

fn mesh_face_frame(face: MeshFace) -> Quat {
    match face {
        MeshFace::PositiveX => Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
        MeshFace::NegativeX => Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
        MeshFace::PositiveY => Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
        MeshFace::NegativeY => Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        MeshFace::PositiveZ => Quat::IDENTITY,
        MeshFace::NegativeZ => Quat::from_rotation_y(std::f32::consts::PI),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ComputedTooltipRecord {
    widget_id: crate::PanelElementId,
    tooltip:   Tooltip,
}

impl ComputedTooltipRecord {
    pub(crate) const fn new(widget_id: crate::PanelElementId, tooltip: Tooltip) -> Self {
        Self { widget_id, tooltip }
    }

    pub(crate) const fn widget_id(&self) -> &crate::PanelElementId { &self.widget_id }

    pub(crate) const fn tooltip(&self) -> &Tooltip { &self.tooltip }
}

#[derive(Component, Default)]
pub(crate) struct TooltipControllerIndex(HashMap<crate::PanelElementId, Entity>);

impl TooltipControllerIndex {
    pub(crate) fn entity(&self, id: &crate::PanelElementId) -> Option<Entity> {
        self.0.get(id).copied()
    }

    pub(crate) fn entities(&self) -> impl Iterator<Item = Entity> + '_ { self.0.values().copied() }

    pub(crate) fn replace(&mut self, index: HashMap<crate::PanelElementId, Entity>) {
        self.0 = index;
    }
}

mod private {
    pub trait SealedSpace {}

    impl SealedSpace for crate::panel::World {}

    impl SealedSpace for crate::panel::Screen {}
}

#[cfg(test)]
mod tests {
    use bevy::ecs::error;
    use bevy::ecs::system::RunSystemOnce;
    use hana_valence::AnchoredTo;

    use super::*;
    use crate::Button;
    use crate::DiegeticPanel;
    use crate::DiegeticPanelCommands;
    use crate::HeadlessLayoutPlugin;
    use crate::LayoutBuilder;
    use crate::Mm;
    use crate::PanelElementId;
    use crate::PanelWidgetReader;
    use crate::Slider;
    use crate::SliderRange;
    use crate::WidgetOf;
    use crate::layout::El;
    use crate::layout::LayoutTreeChange;
    use crate::text::DiegeticTextMeasurer;
    use crate::widgets::WidgetsPlugin;

    const GEOMETRY_EPSILON: f32 = 1e-4;

    struct ApplicationWorldTarget(Entity);

    #[derive(Default, Resource)]
    struct TooltipCleanupCount(usize);

    impl TooltipTarget for ApplicationWorldTarget {
        type Space = World;

        fn tooltip_target_entity(&self) -> Entity { self.0 }
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(DiegeticTextMeasurer::default())
            .add_plugins((HeadlessLayoutPlugin, WidgetsPlugin));
        app
    }

    fn tooltip_tree(tooltip: Option<Tooltip>) -> crate::LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        let button = El::new().button("action", Button::new());
        match tooltip {
            Some(tooltip) => {
                builder.with(button.tooltip(tooltip), |_| {});
            },
            None => {
                builder.with(button, |_| {});
            },
        }
        builder.build()
    }

    fn slider_tooltip_tree(slider: Slider, tooltip: Option<Tooltip>) -> crate::LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        let slider = El::new().slider("action", slider);
        match tooltip {
            Some(tooltip) => builder.with(slider.tooltip(tooltip), |_| {}),
            None => builder.with(slider, |_| {}),
        };
        builder.build()
    }

    fn spawn_panel(app: &mut App, tree: crate::LayoutTree) -> Entity {
        let result = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .with_tree(tree)
            .build();
        assert!(result.is_ok());
        let panel = result.map_or(Entity::PLACEHOLDER, |panel| {
            app.world_mut().spawn(panel).id()
        });
        assert_ne!(panel, Entity::PLACEHOLDER);
        panel
    }

    fn widget(app: &mut App, panel: Entity) -> Entity {
        let widget = app
            .world_mut()
            .run_system_once(move |reader: PanelWidgetReader| {
                reader.entity(panel, &PanelElementId::named("action"))
            })
            .ok()
            .flatten()
            .unwrap_or(Entity::PLACEHOLDER);
        assert_ne!(widget, Entity::PLACEHOLDER);
        widget
    }

    fn controller(app: &App, widget: Entity) -> Entity {
        let controller = app
            .world()
            .get::<super::super::Tooltips>(widget)
            .and_then(|tooltips| tooltips.iter().next())
            .unwrap_or(Entity::PLACEHOLDER);
        assert_ne!(controller, Entity::PLACEHOLDER);
        controller
    }

    fn count_tooltip_cleanup(
        _despawn: On<Despawn, Tooltip>,
        mut cleanup_count: ResMut<TooltipCleanupCount>,
    ) {
        cleanup_count.0 += 1;
    }

    fn center_anchor_geometry() -> ResolvedAnchorGeometry {
        let mut points = BevyHashMap::default();
        points.insert(
            AnchorId::Center,
            AnchorPoint {
                position: Vec3::ZERO,
                frame:    None,
            },
        );
        ResolvedAnchorGeometry {
            points,
            edges: Vec::new(),
        }
    }

    fn demand_mesh_target(app: &mut App, target: Entity) -> Entity {
        app.world_mut()
            .spawn((
                center_anchor_geometry(),
                Transform::default(),
                GlobalTransform::default(),
                AnchoredTo::new(target, AnchorId::Center, AnchorId::Center),
            ))
            .id()
    }

    fn settle_mesh_anchor_geometry(app: &mut App) {
        app.update();
        app.update();
    }

    fn mesh_anchor_center(app: &App, target: Entity) -> Option<Vec3> {
        app.world()
            .get::<ResolvedAnchorGeometry>(target)
            .and_then(|geometry| geometry.points.get(&AnchorId::Center))
            .map(|point| point.position)
    }

    #[track_caller]
    fn assert_vec3_close(actual: Vec3, expected: Vec3) {
        assert!(
            actual.abs_diff_eq(expected, GEOMETRY_EPSILON),
            "expected {expected:?}, got {actual:?}",
        );
    }

    #[track_caller]
    fn assert_quat_close(actual: Quat, expected: Quat) {
        assert!(
            actual.abs_diff_eq(expected, GEOMETRY_EPSILON)
                || actual.abs_diff_eq(-expected, GEOMETRY_EPSILON),
            "expected {expected:?}, got {actual:?}",
        );
    }

    #[test]
    fn defaults_and_clone_identity_match_the_public_contract() {
        let tooltip = Tooltip::new(El::new());
        let clone = tooltip.clone();
        let policy = clone.clone().show_after(Duration::from_secs(1));

        assert_eq!(tooltip, clone);
        assert!(Arc::ptr_eq(&tooltip.blueprint, &clone.blueprint));
        assert!(Arc::ptr_eq(&tooltip.blueprint, &policy.blueprint));
        assert_eq!(tooltip.show_after, DEFAULT_SHOW_DELAY);
        assert_eq!(tooltip.hide_after, Duration::ZERO);
        assert_eq!(tooltip.disabled_policy, TooltipDisabledPolicy::Suppress);
        assert_eq!(tooltip.source_anchor, Anchor::TopCenter);
        assert_eq!(tooltip.target_anchor, Anchor::BottomCenter);
        assert_eq!(
            tooltip.offset,
            PanelAnchorOffset::new(Px(0.0), Px(DEFAULT_TOOLTIP_GAP))
        );
        assert_eq!(
            tooltip.placement_policy,
            TooltipPlacementPolicy::KeepVisible
        );
    }

    #[test]
    fn blueprint_mutation_uses_copy_on_write() {
        let tooltip = Tooltip::new(El::new());
        let mut changed = tooltip.clone();
        assert!(Arc::ptr_eq(&tooltip.blueprint, &changed.blueprint));

        changed.text("details");

        assert_ne!(tooltip, changed);
        assert!(!Arc::ptr_eq(&tooltip.blueprint, &changed.blueprint));
        assert_eq!(tooltip.tree().len(), 1);
        assert_eq!(changed.tree().len(), 2);
    }

    #[test]
    fn clone_captured_during_nested_authoring_resumes_at_the_root() {
        let mut tooltip = Tooltip::new(El::column());
        let mut captured = None;
        tooltip.with(El::row(), |tooltip| {
            tooltip.text("nested");
            captured = Some(tooltip.clone());
        });
        assert!(captured.is_some());
        let Some(mut captured) = captured else {
            return;
        };

        captured.text("captured root child");
        tooltip.text("original root child");

        let mut expected_captured = LayoutBuilder::with_root(El::column());
        expected_captured.with(El::row(), |builder| {
            builder.text("nested");
        });
        expected_captured.text("captured root child");
        let expected_captured = expected_captured.build();

        let mut expected_original = LayoutBuilder::with_root(El::column());
        expected_original.with(El::row(), |builder| {
            builder.text("nested");
        });
        expected_original.text("original root child");
        let expected_original = expected_original.build();

        assert_eq!(
            captured.tree().classify_change(&expected_captured),
            LayoutTreeChange::Identical
        );
        assert_eq!(
            tooltip.tree().classify_change(&expected_original),
            LayoutTreeChange::Identical
        );
        assert_eq!(
            captured.tree().element_id(2),
            Some(&PanelElementId::auto(0))
        );
        assert_eq!(
            captured.tree().element_id(3),
            Some(&PanelElementId::auto(1))
        );
        assert!(!Arc::ptr_eq(&tooltip.blueprint, &captured.blueprint));
    }

    #[test]
    fn nested_authoring_restores_the_parent_cursor() {
        let mut tooltip = Tooltip::new(El::column());
        tooltip.with(El::row(), |tooltip| {
            tooltip.text("nested");
        });
        tooltip.text("root child");

        assert_eq!(tooltip.tree().len(), 4);
        assert_eq!(tooltip.tree().field_display_text(1), Some("nested"));
        assert_eq!(tooltip.tree().element_text(2), Some("nested"));
        assert_eq!(tooltip.tree().element_text(3), Some("root child"));
    }

    #[test]
    fn tooltip_visual_authoring_matches_an_ordinary_layout_tree() {
        let root = El::column()
            .gap(Px(3.0))
            .background(Color::BLACK)
            .corner_radius(4.0);
        let row = El::row()
            .gap(Px(2.0))
            .background(Color::srgb(0.1, 0.2, 0.3));
        let overlay = El::overlay()
            .size(Px(18.0), Px(12.0))
            .background(Color::srgb(0.3, 0.2, 0.1));
        let image = El::new().size(Px(8.0), Px(6.0)).corner_radius(2.0);
        let text_style = crate::TextStyle::new(Px(11.0)).with_color(Color::srgb(0.8, 0.9, 1.0));
        let image_handle = Handle::<Image>::default();
        let image_tint = Color::srgb(0.7, 0.8, 0.9);

        let mut tooltip = Tooltip::new(root.clone());
        tooltip.with(row.clone(), |tooltip| {
            tooltip.text(Text::new("details", text_style.clone()));
            tooltip.with(overlay.clone(), |tooltip| {
                tooltip.image(image.clone(), image_handle.clone(), image_tint);
            });
        });

        let mut builder = LayoutBuilder::with_root(root);
        builder.with(row, |builder| {
            builder.text(Text::new("details", text_style));
            builder.with(overlay, |builder| {
                builder.image(image, image_handle, image_tint);
            });
        });
        let ordinary = builder.build();

        assert_eq!(
            tooltip.tree().classify_change(&ordinary),
            LayoutTreeChange::Identical
        );
        assert_eq!(
            ordinary.classify_change(tooltip.tree()),
            LayoutTreeChange::Identical
        );
    }

    #[test]
    fn associated_controller_reuses_identity_across_tree_replacements() {
        let mut app = test_app();
        let tooltip = Tooltip::new(El::new());
        let tree = tooltip_tree(Some(tooltip.clone()));
        let tree_without_tooltip = tooltip_tree(None);
        assert_eq!(
            tree_without_tooltip.classify_change(&tree),
            LayoutTreeChange::VisualOnly
        );
        let panel = spawn_panel(&mut app, tree_without_tooltip);
        app.update();
        let widget = widget(&mut app, panel);
        let button_declaration = app.world().get::<super::super::WidgetSpec>(widget).cloned();
        assert!(button_declaration.is_some());
        assert!(app.world().get::<super::super::Tooltips>(widget).is_none());

        let attached = app.world_mut().commands().set_tree(panel, tree.clone());
        assert!(attached.is_ok());
        app.update();
        assert_eq!(self::widget(&mut app, panel), widget);
        assert_eq!(
            app.world().get::<super::super::WidgetSpec>(widget),
            button_declaration.as_ref()
        );
        let original = controller(&app, widget);
        assert_eq!(
            app.world()
                .get::<super::super::Tooltips>(widget)
                .map(|tooltips| tooltips.iter().collect::<Vec<_>>()),
            Some(vec![original])
        );
        assert!(app.world().get::<AnchoredHere>(widget).is_none());
        assert!(
            app.world()
                .get::<super::super::ScreenWidgetAnchoredHere>(widget)
                .is_none()
        );
        assert!(app.world().get::<DiegeticPanel>(original).is_none());
        assert!(
            app.world()
                .get::<crate::ComputedDiegeticPanel>(original)
                .is_none()
        );
        assert!(app.world().get::<crate::PanelTextRuns>(original).is_none());
        assert!(
            app.world()
                .get::<crate::panel::PanelAttachmentAuthored>(original)
                .is_none()
        );
        assert!(app.world().get::<AnchoredTo>(original).is_none());
        assert!(
            app.world()
                .get::<super::super::ScreenWidgetAnchoredTo>(original)
                .is_none()
        );
        assert!(app.world().get::<Aabb>(original).is_none());
        assert!(
            app.world()
                .get::<ResolvedAnchorGeometry>(original)
                .is_none()
        );
        assert_eq!(
            app.world()
                .get::<TooltipFor>(original)
                .map(TooltipFor::target),
            Some(widget)
        );

        let indexed_controller = app
            .world()
            .get::<TooltipControllerIndex>(panel)
            .and_then(|index| index.entity(&PanelElementId::named("action")));
        assert_eq!(indexed_controller, Some(original));
        let computed_tick = app
            .world()
            .entity(panel)
            .get_ref::<crate::ComputedDiegeticPanel>()
            .map(|computed| computed.last_changed());
        assert!(computed_tick.is_some());

        let identical = app.world_mut().commands().set_tree(panel, tree.clone());
        assert!(identical.is_ok());
        app.update();
        assert_eq!(controller(&app, widget), original);
        assert_eq!(
            app.world()
                .get::<TooltipControllerIndex>(panel)
                .and_then(|index| index.entity(&PanelElementId::named("action"))),
            indexed_controller
        );
        assert_eq!(
            app.world()
                .entity(panel)
                .get_ref::<crate::ComputedDiegeticPanel>()
                .map(|computed| computed.last_changed()),
            computed_tick
        );

        let policy = tooltip.show_after(Duration::from_secs(1));
        let policy_tree = tooltip_tree(Some(policy.clone()));
        assert_eq!(
            tree.classify_change(&policy_tree),
            LayoutTreeChange::VisualOnly
        );
        let replaced = app.world_mut().commands().set_tree(panel, policy_tree);
        assert!(replaced.is_ok());
        app.update();
        assert_eq!(controller(&app, widget), original);
        assert_eq!(app.world().get::<Tooltip>(original), Some(&policy));
        assert_eq!(
            app.world().get::<super::super::WidgetSpec>(widget),
            button_declaration.as_ref()
        );

        let mut blueprint = Tooltip::new(El::column());
        blueprint.text("replacement");
        let replaced = app
            .world_mut()
            .commands()
            .set_tree(panel, tooltip_tree(Some(blueprint.clone())));
        assert!(replaced.is_ok());
        app.update();
        assert_eq!(controller(&app, widget), original);
        assert_eq!(app.world().get::<Tooltip>(original), Some(&blueprint));
    }

    #[test]
    fn attaching_a_tooltip_preserves_the_slider_declaration_and_identity() {
        let mut app = test_app();
        let Ok(range) = SliderRange::new(0.0, 1.0) else {
            return;
        };
        let Ok(slider) = Slider::new(range, 0.25) else {
            return;
        };
        let tree_without_tooltip = slider_tooltip_tree(slider.clone(), None);
        let tree = slider_tooltip_tree(slider.clone(), Some(Tooltip::new(El::new())));
        let declaration = super::super::WidgetSpec::Slider(slider);

        let panel = spawn_panel(&mut app, tree_without_tooltip);
        app.update();
        let widget = widget(&mut app, panel);
        assert_eq!(
            app.world().get::<super::super::WidgetSpec>(widget),
            Some(&declaration)
        );
        assert!(app.world().get::<super::super::Tooltips>(widget).is_none());

        assert!(app.world_mut().commands().set_tree(panel, tree).is_ok());
        app.update();

        assert_eq!(self::widget(&mut app, panel), widget);
        assert_eq!(
            app.world().get::<super::super::WidgetSpec>(widget),
            Some(&declaration)
        );
        assert_eq!(
            app.world()
                .get::<TooltipFor>(controller(&app, widget))
                .map(TooltipFor::target),
            Some(widget)
        );
    }

    #[test]
    fn tree_replacement_linked_cleanup_precedes_index_retirement_once() {
        let mut app = test_app();
        app.set_error_handler(error::panic)
            .init_resource::<TooltipCleanupCount>()
            .add_observer(count_tooltip_cleanup);
        let panel = spawn_panel(&mut app, tooltip_tree(Some(Tooltip::new(El::new()))));
        app.update();
        let widget = widget(&mut app, panel);
        let controller = controller(&app, widget);

        let replacement = LayoutBuilder::new(100.0, 50.0).build();
        let result = app.world_mut().commands().set_tree(panel, replacement);
        assert!(result.is_ok());
        app.update();

        assert!(app.world().get_entity(widget).is_err());
        assert!(app.world().get_entity(controller).is_err());
        assert_eq!(
            app.world()
                .get::<TooltipControllerIndex>(panel)
                .and_then(|index| index.entity(&PanelElementId::named("action"))),
            None
        );
        assert_eq!(app.world().resource::<TooltipCleanupCount>().0, 1);
    }

    #[test]
    fn retirement_despawns_a_live_malformed_indexed_controller() {
        let mut app = test_app();
        let panel = spawn_panel(&mut app, tooltip_tree(None));
        app.update();
        let malformed = app.world_mut().spawn_empty().id();
        let mut replacement = HashMap::new();
        replacement.insert(PanelElementId::named("action"), malformed);
        let index = app.world_mut().get_mut::<TooltipControllerIndex>(panel);
        assert!(index.is_some());
        if let Some(mut index) = index {
            index.replace(replacement);
        }

        let computed = app
            .world_mut()
            .get_mut::<crate::ComputedDiegeticPanel>(panel);
        assert!(computed.is_some());
        if let Some(mut computed) = computed {
            computed.set_changed();
        }
        app.update();

        assert!(app.world().get_entity(malformed).is_err());
    }

    #[test]
    fn ordinary_panel_reifies_widget_layout_text() {
        let mut app = test_app();
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text(
            Text::new("action", crate::TextStyle::default())
                .layout(El::new().button("action", Button::new())),
        );
        let panel = spawn_panel(&mut app, builder.build());

        app.update();

        let widget = widget(&mut app, panel);
        assert!(
            app.world()
                .get::<super::super::PanelWidget>(widget)
                .is_some()
        );
    }

    #[test]
    fn missing_standalone_target_removes_reserved_controller() {
        let mut app = test_app();
        let target = app.world_mut().spawn_empty().id();
        let handle = TooltipTargetEntity::<World>::from_validated(target);
        let controller = app
            .world_mut()
            .commands()
            .spawn_tooltip(handle, Tooltip::new(El::new()));
        app.world_mut().despawn(target);
        app.update();

        assert!(app.world().get_entity(controller).is_err());
    }

    #[test]
    fn stale_panel_handle_does_not_fall_back_to_general_geometry() {
        let mut app = test_app();
        let target = spawn_panel(&mut app, tooltip_tree(None));
        let handle = PanelEntity::<World>::from_validated(target, PanelSpace::World);
        app.world_mut().entity_mut(target).remove::<DiegeticPanel>();
        app.world_mut()
            .entity_mut(target)
            .insert(center_anchor_geometry());
        let controller = app
            .world_mut()
            .commands()
            .spawn_tooltip(handle, Tooltip::new(El::new()));
        app.update();

        assert!(app.world().get::<ResolvedAnchorGeometry>(target).is_some());
        assert!(app.world().get_entity(controller).is_err());
    }

    #[test]
    fn stale_widget_owner_does_not_fall_back_to_general_geometry() {
        let mut app = test_app();
        let owner = spawn_panel(&mut app, tooltip_tree(Some(Tooltip::new(El::new()))));
        let replacement_owner = spawn_panel(&mut app, tooltip_tree(None));
        app.update();
        let target = widget(&mut app, owner);
        let handle = WidgetEntity::<World>::from_validated(target, owner, PanelSpace::World);
        app.world_mut()
            .entity_mut(target)
            .insert((WidgetOf::new(replacement_owner), center_anchor_geometry()));
        let controller = app
            .world_mut()
            .commands()
            .spawn_tooltip(handle, Tooltip::new(El::new()));
        app.update();

        assert!(app.world().get::<ResolvedAnchorGeometry>(target).is_some());
        assert!(app.world().get_entity(controller).is_err());
    }

    #[test]
    fn general_target_implementations_do_not_invent_panel_ownership() {
        let mut app = test_app();
        let target = spawn_panel(&mut app, tooltip_tree(None));
        app.world_mut()
            .entity_mut(target)
            .insert(center_anchor_geometry());
        let general_handle = TooltipTargetEntity::<World>::from_validated(target);
        let general_controller = app
            .world_mut()
            .commands()
            .spawn_tooltip(general_handle, Tooltip::new(El::new()));
        let application_controller = app
            .world_mut()
            .commands()
            .spawn_tooltip(ApplicationWorldTarget(target), Tooltip::new(El::new()));
        app.update();

        assert!(app.world().get::<Tooltip>(general_controller).is_some());
        assert!(
            app.world()
                .get::<crate::panel::PanelOwned>(general_controller)
                .is_none()
        );
        assert!(app.world().get::<Tooltip>(application_controller).is_some());
        assert!(
            app.world()
                .get::<crate::panel::PanelOwned>(application_controller)
                .is_none()
        );
    }

    #[test]
    fn linked_target_despawn_removes_standalone_controller() {
        let mut app = test_app();
        let geometry = mesh_face_geometry(
            Aabb {
                center:       Vec3A::ZERO,
                half_extents: Vec3A::ONE,
            },
            MeshFace::PositiveZ,
        );
        assert!(geometry.is_some());
        let target = geometry.map_or(Entity::PLACEHOLDER, |geometry| {
            app.world_mut().spawn(geometry).id()
        });
        assert_ne!(target, Entity::PLACEHOLDER);
        let handle = TooltipTargetEntity::<World>::from_validated(target);
        let controller = app
            .world_mut()
            .commands()
            .spawn_tooltip(handle, Tooltip::new(El::new()));
        app.update();
        assert!(app.world().get::<Tooltip>(controller).is_some());

        app.world_mut().despawn(target);

        assert!(app.world().get_entity(controller).is_err());
    }

    #[test]
    fn mesh_face_provider_tracks_bounds_without_transform_rebuilds() {
        let mut app = test_app();
        let target = app
            .world_mut()
            .spawn((
                Mesh3d::default(),
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id();
        app.world_mut()
            .commands()
            .mesh_anchor_target(target, MeshFace::PositiveZ);
        let source = app
            .world_mut()
            .spawn((
                center_anchor_geometry(),
                Transform::default(),
                GlobalTransform::default(),
                AnchoredTo::new(target, AnchorId::Center, AnchorId::Center),
            ))
            .id();
        app.update();
        assert!(app.world().get::<ResolvedAnchorGeometry>(target).is_none());

        app.world_mut().entity_mut(target).insert(Aabb {
            center:       Vec3A::ZERO,
            half_extents: Vec3A::new(1.0, 2.0, 3.0),
        });
        app.update();
        let geometry = app.world().get::<ResolvedAnchorGeometry>(target);
        assert!(geometry.is_some());
        let Some(geometry) = geometry else {
            return;
        };
        assert_eq!(geometry.points.len(), QUAD_ANCHOR_COUNT);
        for (anchor, expected) in [
            (Anchor::TopLeft, Vec3::new(-1.0, 2.0, 3.0)),
            (Anchor::TopRight, Vec3::new(1.0, 2.0, 3.0)),
            (Anchor::BottomRight, Vec3::new(1.0, -2.0, 3.0)),
            (Anchor::BottomLeft, Vec3::new(-1.0, -2.0, 3.0)),
            (Anchor::TopCenter, Vec3::new(0.0, 2.0, 3.0)),
            (Anchor::CenterRight, Vec3::new(1.0, 0.0, 3.0)),
            (Anchor::BottomCenter, Vec3::new(0.0, -2.0, 3.0)),
            (Anchor::CenterLeft, Vec3::new(-1.0, 0.0, 3.0)),
            (Anchor::Center, Vec3::new(0.0, 0.0, 3.0)),
        ] {
            let point = geometry.points.get(&AnchorId::from(anchor));
            assert!(point.is_some());
            if let Some(point) = point {
                assert_vec3_close(point.position, expected);
                assert!(point.frame.is_some());
                if let Some(frame) = point.frame {
                    assert_quat_close(frame, Quat::IDENTITY);
                }
            }
        }
        assert_eq!(
            geometry.edges.as_slice(),
            &[
                Edge {
                    start: AnchorId::from(Anchor::TopLeft),
                    end:   AnchorId::from(Anchor::TopRight),
                },
                Edge {
                    start: AnchorId::from(Anchor::TopRight),
                    end:   AnchorId::from(Anchor::BottomRight),
                },
                Edge {
                    start: AnchorId::from(Anchor::BottomRight),
                    end:   AnchorId::from(Anchor::BottomLeft),
                },
                Edge {
                    start: AnchorId::from(Anchor::BottomLeft),
                    end:   AnchorId::from(Anchor::TopLeft),
                },
            ]
        );
        assert_eq!(
            app.world()
                .get::<Transform>(source)
                .map(|transform| transform.translation),
            Some(Vec3::new(0.0, 0.0, 3.0))
        );
        let before_tick = app
            .world()
            .entity(target)
            .get_ref::<ResolvedAnchorGeometry>()
            .map(|geometry| geometry.last_changed());
        assert!(before_tick.is_some());

        let translated = Transform::from_translation(Vec3::X);
        app.world_mut()
            .entity_mut(target)
            .insert((translated, GlobalTransform::from(translated)));
        app.update();
        let after_transform_tick = app
            .world()
            .entity(target)
            .get_ref::<ResolvedAnchorGeometry>()
            .map(|geometry| geometry.last_changed());
        assert_eq!(after_transform_tick, before_tick);

        app.world_mut().entity_mut(target).insert(Aabb {
            center:       Vec3A::ZERO,
            half_extents: Vec3A::splat(2.0),
        });
        app.update();
        assert_eq!(
            app.world()
                .get::<ResolvedAnchorGeometry>(target)
                .and_then(|geometry| geometry.points.get(&AnchorId::Center))
                .map(|point| point.position),
            Some(Vec3::new(0.0, 0.0, 2.0))
        );
        assert_eq!(
            app.world()
                .get::<Transform>(source)
                .map(|transform| transform.translation),
            Some(Vec3::new(1.0, 0.0, 2.0))
        );

        app.world_mut().despawn(source);
        app.update();
        assert!(app.world().get::<ResolvedAnchorGeometry>(target).is_none());
    }

    #[test]
    fn transform_only_changes_do_not_rebuild_mesh_anchor_geometry() {
        let mut app = test_app();
        let target = app
            .world_mut()
            .spawn((
                Mesh3d::default(),
                Aabb {
                    center:       Vec3A::ZERO,
                    half_extents: Vec3A::ONE,
                },
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id();
        app.world_mut()
            .commands()
            .mesh_anchor_target(target, MeshFace::PositiveZ);
        demand_mesh_target(&mut app, target);
        settle_mesh_anchor_geometry(&mut app);
        let before = app
            .world()
            .entity(target)
            .get_ref::<ResolvedAnchorGeometry>()
            .map(|geometry| geometry.last_changed());
        assert!(before.is_some());

        app.world_mut()
            .entity_mut(target)
            .insert(Transform::from_translation(Vec3::X));
        app.update();

        let after = app
            .world()
            .entity(target)
            .get_ref::<ResolvedAnchorGeometry>()
            .map(|geometry| geometry.last_changed());
        assert_eq!(after, before);
    }

    #[test]
    fn mesh_component_removal_retires_geometry_until_readdition_recovers() {
        let mut app = test_app();
        let target = app
            .world_mut()
            .spawn((
                Mesh3d::default(),
                Aabb {
                    center:       Vec3A::ZERO,
                    half_extents: Vec3A::ONE,
                },
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id();
        app.world_mut()
            .commands()
            .mesh_anchor_target(target, MeshFace::PositiveZ);
        demand_mesh_target(&mut app, target);
        settle_mesh_anchor_geometry(&mut app);
        assert_eq!(
            mesh_anchor_center(&app, target),
            Some(Vec3::new(0.0, 0.0, 1.0))
        );

        app.world_mut().entity_mut(target).remove::<Mesh3d>();
        app.update();
        assert!(app.world().get::<ResolvedAnchorGeometry>(target).is_none());
        assert!(app.world().get::<MeshAnchorGeometry>(target).is_none());
        assert_eq!(
            app.world()
                .get::<MeshAnchorGeometryPending>(target)
                .map(|pending| pending.cause),
            Some(MeshAnchorPendingCause::WaitingForMesh)
        );

        app.world_mut().entity_mut(target).insert(Mesh3d::default());
        app.update();
        assert_eq!(
            mesh_anchor_center(&app, target),
            Some(Vec3::new(0.0, 0.0, 1.0))
        );
    }

    #[test]
    fn mesh_component_removed_before_first_demand_does_not_publish_retained_bounds() {
        let mut app = test_app();
        let target = app
            .world_mut()
            .spawn((
                Mesh3d::default(),
                Aabb {
                    center:       Vec3A::ZERO,
                    half_extents: Vec3A::ONE,
                },
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id();
        app.world_mut()
            .commands()
            .mesh_anchor_target(target, MeshFace::PositiveZ);
        app.update();
        assert!(app.world().get::<Aabb>(target).is_some());

        app.world_mut().entity_mut(target).remove::<Mesh3d>();
        app.update();
        assert!(app.world().get::<Aabb>(target).is_some());
        demand_mesh_target(&mut app, target);
        app.update();

        assert!(app.world().get::<ResolvedAnchorGeometry>(target).is_none());
        assert!(app.world().get::<MeshAnchorGeometry>(target).is_none());
        assert_eq!(
            app.world()
                .get::<MeshAnchorGeometryPending>(target)
                .map(|pending| pending.cause),
            Some(MeshAnchorPendingCause::WaitingForMesh)
        );
    }

    #[test]
    fn bounds_removal_retires_geometry_until_current_bounds_return() {
        let mut app = test_app();
        let target = app
            .world_mut()
            .spawn((
                Mesh3d::default(),
                Aabb {
                    center:       Vec3A::ZERO,
                    half_extents: Vec3A::ONE,
                },
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id();
        app.world_mut()
            .commands()
            .mesh_anchor_target(target, MeshFace::PositiveZ);
        let source = app
            .world_mut()
            .spawn((
                center_anchor_geometry(),
                Transform::default(),
                GlobalTransform::default(),
                AnchoredTo::new(target, AnchorId::Center, AnchorId::Center),
            ))
            .id();
        settle_mesh_anchor_geometry(&mut app);
        assert_eq!(
            mesh_anchor_center(&app, target),
            Some(Vec3::new(0.0, 0.0, 1.0))
        );

        app.world_mut().entity_mut(target).remove::<Aabb>();
        app.update();
        assert!(app.world().get::<ResolvedAnchorGeometry>(target).is_none());
        assert!(app.world().get::<MeshAnchorGeometry>(target).is_none());

        app.world_mut().entity_mut(target).insert(Aabb {
            center:       Vec3A::ZERO,
            half_extents: Vec3A::splat(3.0),
        });
        app.update();
        assert_eq!(
            mesh_anchor_center(&app, target),
            Some(Vec3::new(0.0, 0.0, 3.0))
        );
        assert_eq!(
            app.world()
                .get::<Transform>(source)
                .map(|transform| transform.translation),
            Some(Vec3::new(0.0, 0.0, 3.0))
        );
    }

    #[test]
    fn selected_face_change_refreshes_local_geometry() {
        let mut app = test_app();
        let target = app
            .world_mut()
            .spawn((
                Mesh3d::default(),
                Aabb {
                    center:       Vec3A::ZERO,
                    half_extents: Vec3A::new(1.0, 2.0, 3.0),
                },
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id();
        app.world_mut()
            .commands()
            .mesh_anchor_target(target, MeshFace::PositiveZ);
        demand_mesh_target(&mut app, target);
        settle_mesh_anchor_geometry(&mut app);
        assert_eq!(
            mesh_anchor_center(&app, target),
            Some(Vec3::new(0.0, 0.0, 3.0))
        );

        app.world_mut()
            .commands()
            .mesh_anchor_target(target, MeshFace::PositiveX);
        app.update();

        assert!(
            mesh_anchor_center(&app, target)
                .is_some_and(|center| center.abs_diff_eq(Vec3::X, 1e-5))
        );
    }

    #[test]
    fn panel_role_removal_cleans_associated_and_standalone_controllers() {
        let mut app = test_app();
        let panel = spawn_panel(&mut app, tooltip_tree(Some(Tooltip::new(El::new()))));
        let panel_handle = crate::PanelEntity::<World>::from_validated(panel, PanelSpace::World);
        let standalone = app
            .world_mut()
            .commands()
            .spawn_tooltip(panel_handle, Tooltip::new(El::new()));
        app.update();
        let widget = widget(&mut app, panel);
        let associated = controller(&app, widget);
        assert!(app.world().get::<Tooltip>(standalone).is_some());

        app.world_mut().entity_mut(panel).remove::<DiegeticPanel>();
        app.update();

        assert!(app.world().get_entity(panel).is_ok());
        assert!(app.world().get_entity(associated).is_err());
        assert!(app.world().get_entity(standalone).is_err());
    }

    #[test]
    fn panel_despawn_cleans_associated_and_standalone_controllers() {
        let mut app = test_app();
        let panel = spawn_panel(&mut app, tooltip_tree(Some(Tooltip::new(El::new()))));
        let panel_handle = crate::PanelEntity::<World>::from_validated(panel, PanelSpace::World);
        let standalone = app
            .world_mut()
            .commands()
            .spawn_tooltip(panel_handle, Tooltip::new(El::new()));
        app.update();
        let widget = widget(&mut app, panel);
        let associated = controller(&app, widget);
        assert!(app.world().get::<Tooltip>(standalone).is_some());

        app.world_mut().despawn(panel);
        app.update();

        assert!(app.world().get_entity(associated).is_err());
        assert!(app.world().get_entity(standalone).is_err());
    }
}
