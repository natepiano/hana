//! Panel-source anchoring authoring types.
//!
//! Anchoring is a per-frame resolved attachment, not `ChildOf` parenting.
//! The pin position depends on the source and target geometry: a `Fit` panel
//! target that remeasures or a widget target whose rectangle changes moves the
//! target anchor point, so a parent-relative transform captured once would go
//! stale. Screen panels also get window-absolute translations written every
//! frame, which a parent transform would double-apply. Reparenting would
//! further couple lifetimes (target despawn despawns dependents), inherit the
//! target's scale chain, and turn an attachment cycle into a hierarchy cycle.
//! The resolvers instead keep diegetic authoring separate from the
//! coordinate-space positioners. World sources may target world panels or
//! reified world widgets; screen sources currently require panel targets.

use bevy::prelude::*;
use hana_valence::AnchorId;
use hana_valence::AnchoredTo as ValenceAnchoredTo;
use hana_valence::ResolvedAnchorOffset;

use super::CoordinateSpace;
use super::DiegeticPanel;
use super::coordinate_space::PanelSpace;
use super::lifecycle;
use super::lifecycle::PanelComponentOwnership;
use crate::layout::Anchor;
use crate::layout::Dimension;
use crate::layout::Unit;
use crate::widgets::PanelWidget;
use crate::widgets::WidgetOf;

/// Insert-only bundle that pins a panel anchor point to a target entity.
#[derive(Bundle, Clone, Copy, Debug, PartialEq)]
pub struct AnchoredToPanel {
    authored: PanelAttachmentAuthored,
    offset:   PanelAnchorOffset,
}

impl AnchoredToPanel {
    /// Creates an attachment from the source panel to `target`.
    ///
    /// A world source may target a world panel or reified world widget. Screen
    /// sources currently require panel targets.
    #[must_use]
    pub const fn new(target: Entity, source: Anchor, target_anchor: Anchor) -> Self {
        Self {
            authored: PanelAttachmentAuthored {
                target,
                source,
                target_anchor,
            },
            offset:   PanelAnchorOffset::ZERO,
        }
    }

    /// Sets the offset from the resolved target anchor point.
    #[must_use]
    pub const fn with_offset(mut self, offset: PanelAnchorOffset) -> Self {
        self.offset = offset;
        self
    }

    /// Target panel or reified world widget entity.
    #[must_use]
    pub const fn target(&self) -> Entity { self.authored.target() }

    /// Anchor point on the source panel.
    #[must_use]
    pub const fn source_anchor(&self) -> Anchor { self.authored.source_anchor() }

    /// Anchor point on the target panel or reified world widget.
    #[must_use]
    pub const fn target_anchor(&self) -> Anchor { self.authored.target_anchor() }

    /// Offset from the target anchor point.
    #[must_use]
    pub const fn offset(&self) -> PanelAnchorOffset { self.offset }

    /// Returns a copy that points at `target`.
    #[must_use]
    pub const fn retargeted(mut self, target: Entity) -> Self {
        self.authored = self.authored.retargeted(target);
        self
    }
}

/// Shared panel attachment authoring read by screen and world positioners.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
#[component(immutable)]
pub(crate) struct PanelAttachmentAuthored {
    target:        Entity,
    source:        Anchor,
    target_anchor: Anchor,
}

impl PanelAttachmentAuthored {
    /// Target panel or reified world widget entity.
    #[must_use]
    pub(crate) const fn target(&self) -> Entity { self.target }

    /// Anchor point on the source panel.
    #[must_use]
    pub(crate) const fn source_anchor(&self) -> Anchor { self.source }

    /// Anchor point on the target panel or reified world widget.
    #[must_use]
    pub(crate) const fn target_anchor(&self) -> Anchor { self.target_anchor }

    /// Returns a copy that points at `target`.
    #[must_use]
    pub(crate) const fn retargeted(mut self, target: Entity) -> Self {
        self.target = target;
        self
    }

    pub(crate) fn valence_relation(&self) -> ValenceAnchoredTo {
        ValenceAnchoredTo::new(
            self.target,
            AnchorId::from(self.source),
            AnchorId::from(self.target_anchor),
        )
    }
}

/// Offset from a target panel or reified world widget anchor point.
///
/// Coordinates are authored in target-local layout space: positive x moves
/// right, positive y moves down, positive z moves the source in front of the
/// target — along the target plane normal for world targets, toward the screen
/// camera for screen panels. Bare `f32` values resolve against the target
/// panel's layout unit; a widget target uses its owning panel's layout unit.
/// [`Px`](crate::Px), [`Mm`](crate::Mm), [`Pt`](crate::Pt), and
/// [`In`](crate::In) carry explicit units.
///
/// Screen depth selects draw order under the shared orthographic camera and
/// never changes apparent size. The camera sits at z = 1000 with a far plane
/// of 2000, so resolved depths outside `(-1000, 1000)` clip rather than
/// clamp. Panel children are coplanar with their backing and order via
/// material sort biases, not z: batched text carries a 64-unit
/// `Transparent3d` sort bias, so on the sorted screen view a back panel's
/// text composites over a front panel's backing until the panels' depths
/// differ by more than 64 logical pixels.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(PartialEq, Debug, Default)]
pub struct PanelAnchorOffset {
    x: Dimension,
    y: Dimension,
    z: Dimension,
}

const ZERO_DIMENSION: Dimension = Dimension {
    value: 0.0,
    unit:  None,
};

impl PanelAnchorOffset {
    /// Zero offset.
    pub const ZERO: Self = Self {
        x: ZERO_DIMENSION,
        y: ZERO_DIMENSION,
        z: ZERO_DIMENSION,
    };

    /// Creates an offset from two authored dimensions, with zero depth.
    #[must_use]
    pub fn new(x: impl Into<Dimension>, y: impl Into<Dimension>) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
            z: ZERO_DIMENSION,
        }
    }

    /// Sets the depth offset dimension.
    #[must_use]
    pub fn with_z(mut self, z: impl Into<Dimension>) -> Self {
        self.z = z.into();
        self
    }

    /// Horizontal offset dimension.
    #[must_use]
    pub const fn x(self) -> Dimension { self.x }

    /// Vertical offset dimension.
    #[must_use]
    pub const fn y(self) -> Dimension { self.y }

    /// Depth offset dimension.
    #[must_use]
    pub const fn z(self) -> Dimension { self.z }

    pub(crate) fn to_layout_units(self, layout_unit: Unit) -> Vec3 {
        let layout_to_points = layout_unit.to_points();
        Vec3::new(
            self.x.to_points(layout_to_points) / layout_to_points,
            self.y.to_points(layout_to_points) / layout_to_points,
            self.z.to_points(layout_to_points) / layout_to_points,
        )
    }
}

/// Authored local transform for a world panel while it is panel-attached.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(crate) struct AnchoredWorldPanelPose {
    authored_transform: Transform,
}

/// Inserts the valence world relation for world-space panel attachments.
pub(super) fn on_panel_attachment_inserted(
    inserted: On<Insert, PanelAttachmentAuthored>,
    attachments: Query<(
        &PanelAttachmentAuthored,
        Option<&DiegeticPanel>,
        Option<&Transform>,
        Option<&AnchoredWorldPanelPose>,
    )>,
    mut commands: Commands,
) {
    let entity = inserted.entity;
    let Ok((authored, panel, transform, pose)) = attachments.get(entity) else {
        return;
    };
    let Some(panel) = panel else {
        lifecycle::remove_owned_component::<ValenceAnchoredTo>(&mut commands, entity, entity);
        lifecycle::remove_owned_component::<ResolvedAnchorOffset>(&mut commands, entity, entity);
        return;
    };
    reconcile_panel_anchor(
        entity,
        PanelSpace::from(panel.coordinate_space()),
        authored,
        transform,
        pose,
        &mut commands,
    );
}

/// Reconciles the valence anchor tag when a panel's coordinate space flips.
///
/// A world<->screen conversion mutates the panel's [`CoordinateSpace`] field in
/// place, which fires no component hook; the conversion instead re-inserts the
/// [`PanelSpace`] mirror, and this observer keys the anchor state off the new
/// discriminant. Without it, a world->screen flip leaves a stale
/// `ValenceAnchoredTo` (warn-spam + phantom resolve diagnostics) and a
/// screen->world flip never restores it.
pub(super) fn on_panel_space_changed(
    inserted: On<Insert, PanelSpace>,
    panels: Query<(
        &PanelSpace,
        Option<&PanelAttachmentAuthored>,
        Option<&Transform>,
        Option<&AnchoredWorldPanelPose>,
    )>,
    mut commands: Commands,
) {
    let entity = inserted.entity;
    let Ok((space, authored, transform, pose)) = panels.get(entity) else {
        return;
    };
    let Some(authored) = authored else {
        return;
    };
    reconcile_panel_anchor(entity, *space, authored, transform, pose, &mut commands);
}

/// Sets a panel-attached entity's valence anchor state to match its space.
///
/// World: insert the valence relation, capturing the authored local transform
/// once so [`restore_inactive_world_panel_poses`] can restore it. Screen:
/// remove the world-only relation, resolved offset, and captured pose.
fn reconcile_panel_anchor(
    entity: Entity,
    space: PanelSpace,
    authored: &PanelAttachmentAuthored,
    transform: Option<&Transform>,
    pose: Option<&AnchoredWorldPanelPose>,
    commands: &mut Commands,
) {
    if space != PanelSpace::World {
        lifecycle::remove_owned_component::<ValenceAnchoredTo>(commands, entity, entity);
        lifecycle::remove_owned_component::<ResolvedAnchorOffset>(commands, entity, entity);
        commands.entity(entity).remove::<AnchoredWorldPanelPose>();
        return;
    }
    lifecycle::write_owned_component(commands, entity, entity, authored.valence_relation());
    if let (Some(transform), None) = (transform, pose) {
        commands.entity(entity).insert(AnchoredWorldPanelPose {
            authored_transform: *transform,
        });
    }
}

/// Removes valence world-only state when panel attachment authoring is removed.
pub(super) fn on_panel_attachment_removed(
    removed: On<Remove, PanelAttachmentAuthored>,
    mut commands: Commands,
) {
    lifecycle::remove_owned_component::<ValenceAnchoredTo>(
        &mut commands,
        removed.entity,
        removed.entity,
    );
    lifecycle::remove_owned_component::<ResolvedAnchorOffset>(
        &mut commands,
        removed.entity,
        removed.entity,
    );
}

/// Restores a world panel's authored local transform after panel anchoring stops.
pub(super) fn restore_inactive_world_panel_poses(
    mut commands: Commands,
    mut panels: Query<(
        Entity,
        &AnchoredWorldPanelPose,
        Option<&PanelAttachmentAuthored>,
        Option<&DiegeticPanel>,
        &mut Transform,
    )>,
) {
    for (entity, pose, attachment, panel, mut transform) in &mut panels {
        let is_world_panel = panel
            .is_some_and(|panel| matches!(panel.coordinate_space(), CoordinateSpace::World { .. }));
        if attachment.is_none() && is_world_panel {
            *transform = pose.authored_transform;
        }
        if attachment.is_none() || !is_world_panel {
            commands.entity(entity).remove::<AnchoredWorldPanelPose>();
        }
    }
}

#[derive(Clone, Copy)]
enum AnchorTargetMetrics {
    Panel {
        layout_unit: Unit,
        layout_size: Vec2,
        world_size:  Vec2,
    },
    Widget {
        layout_unit:           Unit,
        world_per_layout_unit: Vec2,
    },
}

impl AnchorTargetMetrics {
    const fn layout_unit(self) -> Unit {
        match self {
            Self::Panel { layout_unit, .. } | Self::Widget { layout_unit, .. } => layout_unit,
        }
    }

    fn world_per_layout_unit(self) -> Option<Vec2> {
        match self {
            Self::Panel {
                layout_size,
                world_size,
                ..
            } => Some(Vec2::new(
                world_size.x / layout_size.x,
                world_size.y / layout_size.y,
            )),
            Self::Widget {
                world_per_layout_unit,
                ..
            } => valid_offset_size(world_per_layout_unit).then_some(world_per_layout_unit),
        }
    }
}

/// Resolves diegetic world-panel offsets into valence resolver-frame offsets.
pub(super) fn write_panel_anchor_offsets(
    mut commands: Commands,
    attachments: Query<(
        Entity,
        &PanelAttachmentAuthored,
        &PanelAnchorOffset,
        &DiegeticPanel,
        Ref<ValenceAnchoredTo>,
        Option<&PanelComponentOwnership<ValenceAnchoredTo>>,
    )>,
    panel_targets: Query<(&DiegeticPanel, &GlobalTransform)>,
    widget_targets: Query<&WidgetOf, With<PanelWidget>>,
) {
    for (entity, authored, offset, source_panel, relation, ownership) in &attachments {
        if !ownership.is_some_and(|ownership| ownership.owns(entity, relation.last_changed())) {
            lifecycle::remove_owned_component::<ResolvedAnchorOffset>(
                &mut commands,
                entity,
                entity,
            );
            continue;
        }
        if !matches!(
            source_panel.coordinate_space(),
            CoordinateSpace::World { .. }
        ) {
            lifecycle::remove_owned_component::<ResolvedAnchorOffset>(
                &mut commands,
                entity,
                entity,
            );
            continue;
        }
        let Some(offset) =
            lowered_world_offset(authored.target(), *offset, &panel_targets, &widget_targets)
        else {
            lifecycle::remove_owned_component::<ResolvedAnchorOffset>(
                &mut commands,
                entity,
                entity,
            );
            continue;
        };
        lifecycle::write_owned_component(
            &mut commands,
            entity,
            entity,
            ResolvedAnchorOffset(offset),
        );
    }
}

fn lowered_world_offset(
    target: Entity,
    offset: PanelAnchorOffset,
    panel_targets: &Query<(&DiegeticPanel, &GlobalTransform)>,
    widget_targets: &Query<&WidgetOf, With<PanelWidget>>,
) -> Option<Vec3> {
    let metrics = anchor_target_metrics(target, panel_targets, widget_targets)?;
    let offset = offset.to_layout_units(metrics.layout_unit());
    if !offset.is_finite() {
        return None;
    }
    let world_per_layout_unit = metrics.world_per_layout_unit()?;
    Some(Vec3::new(
        offset.x * world_per_layout_unit.x,
        -offset.y * world_per_layout_unit.y,
        offset.z * world_per_layout_unit.x,
    ))
}

fn anchor_target_metrics(
    target: Entity,
    panel_targets: &Query<(&DiegeticPanel, &GlobalTransform)>,
    widget_targets: &Query<&WidgetOf, With<PanelWidget>>,
) -> Option<AnchorTargetMetrics> {
    if let Ok((target_panel, target_global)) = panel_targets.get(target) {
        return panel_target_metrics(target_panel, target_global);
    }
    let widget_of = widget_targets.get(target).ok()?;
    let (owner_panel, owner_global) = panel_targets.get(widget_of.panel()).ok()?;
    if !matches!(
        owner_panel.coordinate_space(),
        CoordinateSpace::World { .. }
    ) {
        return None;
    }
    // A reified widget's authored `Transform` is translation-only, so its
    // `WidgetOf` owner supplies the effective scale before child propagation.
    let owner_scale = transform_scale(owner_global)?;
    let layout_to_world = owner_panel.layout_unit().to_points() * owner_panel.points_to_world();
    let world_per_layout_unit = owner_scale * layout_to_world;
    Some(AnchorTargetMetrics::Widget {
        layout_unit: owner_panel.layout_unit(),
        world_per_layout_unit,
    })
}

fn panel_target_metrics(
    target_panel: &DiegeticPanel,
    target_global: &GlobalTransform,
) -> Option<AnchorTargetMetrics> {
    if !matches!(
        target_panel.coordinate_space(),
        CoordinateSpace::World { .. }
    ) {
        return None;
    }
    let layout_size = Vec2::new(target_panel.width(), target_panel.height());
    if !valid_offset_size(layout_size) {
        return None;
    }
    let world_size = target_world_size(target_panel, target_global)?;
    Some(AnchorTargetMetrics::Panel {
        layout_unit: target_panel.layout_unit(),
        layout_size,
        world_size,
    })
}

fn target_world_size(panel: &DiegeticPanel, transform: &GlobalTransform) -> Option<Vec2> {
    let scale = transform_scale(transform)?;
    let size = Vec2::new(
        panel.world_width() * scale.x,
        panel.world_height() * scale.y,
    );
    valid_offset_size(size).then_some(size)
}

fn transform_scale(transform: &GlobalTransform) -> Option<Vec2> {
    let affine = transform.affine();
    let scale = Vec2::new(
        affine.transform_vector3(Vec3::X).length(),
        affine.transform_vector3(Vec3::Y).length(),
    );
    valid_offset_size(scale).then_some(scale)
}

fn valid_offset_size(size: Vec2) -> bool { size.is_finite() && size.x > 0.0 && size.y > 0.0 }

/// Resolver-owned screen pose override for a panel's configured anchor.
///
/// `depth` is the resolved `translation.z` when the screen resolver produced
/// one; `authored_depth` captures the pre-resolution z so removing the
/// attachment restores it. `rotation` and `authored_rotation` mirror that
/// capture-and-restore path for the resolved in-plane z angle.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ResolvedScreenPanelPosition {
    pub(crate) anchor_position:   Option<Vec2>,
    pub(crate) depth:             Option<f32>,
    pub(crate) authored_depth:    Option<f32>,
    pub(crate) rotation:          Option<f32>,
    pub(crate) authored_rotation: Option<f32>,
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::any::TypeId;
    use std::sync::Arc;

    use bevy::ecs::reflect::ReflectComponent;
    use bevy::prelude::*;
    use bevy::transform::TransformPlugin;
    use bevy::window::PrimaryWindow;
    use bevy::window::Window;
    use hana_valence::AnchorPose;
    use hana_valence::AnchorSystems;
    use hana_valence::AnchoredHere;
    use hana_valence::AnchoredTo as ValenceAnchoredTo;
    use hana_valence::ResolveDiagnostics;
    use hana_valence::ResolveSkip;
    use hana_valence::ResolvedAnchorGeometry;
    use hana_valence::ResolvedAnchorOffset;

    use super::AnchoredToPanel;
    use super::AnchoredWorldPanelPose;
    use super::PanelAnchorOffset;
    use super::PanelAttachmentAuthored;
    use crate::Button;
    use crate::El;
    use crate::HeadlessLayoutPlugin;
    use crate::LayoutBuilder;
    use crate::Mm;
    use crate::PanelScreenConversion;
    use crate::PanelSystems;
    use crate::PanelWorldConversion;
    use crate::Pt;
    use crate::Px;
    use crate::layout::Anchor;
    use crate::layout::Dimension;
    use crate::layout::Sizing;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;
    use crate::layout::Unit;
    use crate::panel::DiegeticPanel;
    use crate::panel::DiegeticPanelCommands;
    use crate::panel::conversion::SavedWorldRestoreMode;
    use crate::panel::coordinate_space::PanelSpace;
    use crate::text::DiegeticTextMeasurer;
    use crate::widgets::PanelWidgets;
    use crate::widgets::WidgetSystems;
    use crate::widgets::WidgetsPlugin;

    const DIAGNOSTIC_REPEAT_COUNT: u32 = 2;
    const TRANSFORM_SCALE: f32 = 2.0;
    const WIDGET_HEIGHT: f32 = 10.0;
    const WIDGET_OFFSET_MILLIMETERS: f32 = 5.0;
    const WIDGET_OFFSET_PIXELS: f32 = 10.0;
    const WIDGET_OFFSET_POINTS: f32 = 3.0;
    const WIDGET_OWNER_ROTATION: f32 = 0.4;
    const WIDGET_OWNER_TRANSLATION: Vec3 = Vec3::new(1.0, 2.0, 3.0);
    const WIDGET_WIDTH: f32 = 20.0;
    const WORLD_WIDGET_PANEL_HEIGHT: f32 = 100.0;
    const WORLD_WIDGET_PANEL_WIDTH: f32 = 200.0;

    fn reverse_targets(world: &World, target: Entity) -> Vec<Entity> {
        world
            .get::<AnchoredHere>(target)
            .map(|targets| targets.iter().collect())
            .unwrap_or_default()
    }

    #[test]
    fn world_bundle_insert_creates_valence_relation_and_reverse_index() {
        let mut app = app_with_world_anchoring();
        let target = app
            .world_mut()
            .spawn((world_panel(Anchor::TopLeft), Transform::default()))
            .id();
        let authored_transform = Transform::from_xyz(0.25, 0.5, 0.75);
        let source = app
            .world_mut()
            .spawn((
                world_panel(Anchor::Center),
                authored_transform,
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft),
            ))
            .id();

        app.update();

        let relation = app
            .world()
            .get::<ValenceAnchoredTo>(source)
            .expect("world attachment has valence relation");
        assert_eq!(relation.target(), target);
        assert_eq!(reverse_targets(app.world(), target), vec![source]);
        assert_eq!(
            app.world()
                .get::<AnchoredWorldPanelPose>(source)
                .map(|pose| pose.authored_transform),
            Some(authored_transform),
        );
    }

    #[test]
    fn new_defaults_depth_to_zero_and_with_z_sets_it() {
        let offset = PanelAnchorOffset::new(Px(1.0), Px(2.0));
        assert_eq!(
            offset.z(),
            Dimension {
                value: 0.0,
                unit:  None,
            }
        );

        let offset = offset.with_z(Mm(3.0));
        assert_eq!(offset.x(), Px(1.0).into());
        assert_eq!(offset.y(), Px(2.0).into());
        assert_eq!(offset.z(), Mm(3.0).into());
    }

    #[test]
    fn to_layout_units_converts_depth_like_x_and_y() {
        let offset = PanelAnchorOffset::new(Mm(10.0), Mm(20.0)).with_z(Mm(30.0));
        let resolved = offset.to_layout_units(Unit::Millimeters);

        assert!(
            resolved.abs_diff_eq(Vec3::new(10.0, 20.0, 30.0), 1e-4),
            "expected (10, 20, 30), got {resolved:?}",
        );
    }

    #[test]
    fn anchor_types_are_registered_with_expected_reflect_component_data() {
        let mut app = App::new();
        app.add_plugins(HeadlessLayoutPlugin);

        let registry = app.world().resource::<AppTypeRegistry>().read();
        let authored_registered = registry
            .get(TypeId::of::<PanelAttachmentAuthored>())
            .is_some();
        let offset_registered = registry.get(TypeId::of::<PanelAnchorOffset>()).is_some();
        let valence_source_has_reflect_component = registry
            .get(TypeId::of::<ValenceAnchoredTo>())
            .expect("AnchoredTo is registered")
            .data::<ReflectComponent>()
            .is_some();
        let reverse_has_reflect_component = registry
            .get(TypeId::of::<AnchoredHere>())
            .expect("AnchoredHere is registered")
            .data::<ReflectComponent>()
            .is_some();
        let pose_has_reflect_component = registry
            .get(TypeId::of::<AnchorPose>())
            .expect("AnchorPose is registered")
            .data::<ReflectComponent>()
            .is_some();
        let geometry_has_reflect_component = registry
            .get(TypeId::of::<ResolvedAnchorGeometry>())
            .expect("ResolvedAnchorGeometry is registered")
            .data::<ReflectComponent>()
            .is_some();
        let resolved_offset_has_reflect_component = registry
            .get(TypeId::of::<ResolvedAnchorOffset>())
            .expect("ResolvedAnchorOffset is registered")
            .data::<ReflectComponent>()
            .is_some();
        drop(registry);

        assert!(!authored_registered);
        assert!(offset_registered);
        assert!(!valence_source_has_reflect_component);
        assert!(!reverse_has_reflect_component);
        assert!(pose_has_reflect_component);
        assert!(geometry_has_reflect_component);
        assert!(resolved_offset_has_reflect_component);
    }

    #[test]
    fn world_anchoring_respects_source_scale_and_parent_rotation() {
        let mut app = app_with_world_anchoring();
        let parent = app
            .world_mut()
            .spawn(Transform::from_rotation(Quat::from_rotation_z(
                std::f32::consts::FRAC_PI_2,
            )))
            .id();
        let target = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                Transform::from_xyz(2.0, 1.0, 0.0),
            ))
            .id();
        let source = app
            .world_mut()
            .spawn((
                world_panel(Anchor::Center),
                Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::splat(0.5)),
                ChildOf(parent),
                AnchoredToPanel::new(target, Anchor::BottomRight, Anchor::TopLeft),
            ))
            .id();

        app.update();

        let source_global = global_transform(&app, source);
        let (scale, rotation, translation) = source_global.to_scale_rotation_translation();
        assert_close_3d(translation, Vec3::new(1.5, 1.25, 0.0));
        assert_close_quat(rotation, Quat::IDENTITY);
        assert_close_3d(scale, Vec3::splat(0.5));
    }

    #[test]
    fn pose_written_in_animation_set_lands_this_frame() {
        let mut app = app_with_world_anchoring();
        app.insert_resource(PoseLift(0.5));
        app.add_systems(
            PostUpdate,
            lift_anchored_pose.in_set(AnchorSystems::AnimatePose),
        );
        let (target, source) = spawn_lift_scene(&mut app);

        app.update();

        let target_pin = panel_anchor_point(&app, target, Anchor::Center);
        assert_close_3d(
            panel_anchor_point(&app, source, Anchor::Center),
            target_pin + Vec3::Z * 0.5,
        );
    }

    #[test]
    fn world_offset_lowering_tracks_target_size_and_layout_unit() {
        let mut app = app_with_world_anchoring();
        let target = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                Transform::from_scale(Vec3::splat(2.0)),
            ))
            .id();
        let source = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                Transform::default(),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft)
                    .with_offset(PanelAnchorOffset::new(Mm(50.0), Pt(72.0)).with_z(Mm(25.0))),
            ))
            .id();

        app.update();
        assert_close_3d(resolved_offset(&app, source), Vec3::new(1.0, -0.508, 0.5));

        app.world_mut()
            .get_mut::<DiegeticPanel>(target)
            .expect("target panel exists")
            .set_width(400.0);
        app.world_mut()
            .get_mut::<DiegeticPanel>(target)
            .expect("target panel exists")
            .set_height(200.0);
        app.update();

        assert_close_3d(resolved_offset(&app, source), Vec3::new(0.5, -0.254, 0.25));
    }

    #[test]
    fn widget_target_offset_uses_owner_units_and_transformed_scale() {
        let mut app = app_with_world_anchoring();
        app.add_plugins(WidgetsPlugin);
        let mut tree = LayoutBuilder::new(WORLD_WIDGET_PANEL_WIDTH, WORLD_WIDGET_PANEL_HEIGHT);
        tree.with(
            El::new()
                .size(WIDGET_WIDTH, WIDGET_HEIGHT)
                .button("offset-target", Button::new()),
            |_| {},
        );
        let target = app
            .world_mut()
            .spawn((
                world_panel(Anchor::Center),
                Transform::from_translation(WIDGET_OWNER_TRANSLATION)
                    .with_rotation(Quat::from_rotation_z(WIDGET_OWNER_ROTATION))
                    .with_scale(Vec3::splat(TRANSFORM_SCALE)),
            ))
            .id();
        app.world_mut()
            .commands()
            .set_tree(target, tree.build())
            .expect("widget target tree should be accepted");
        app.update();
        let widget = app
            .world()
            .get::<PanelWidgets>(target)
            .and_then(|widgets| widgets.first().copied())
            .expect("target widget should be reified");
        let widget_local = transform(&app, widget);
        assert_ne!(
            widget_local.translation,
            Vec3::ZERO,
            "widget target should be off the owner panel origin",
        );
        let owner_global = global_transform(&app, target);
        let expected_widget_position = owner_global.transform_point(widget_local.translation);
        assert_close_3d(
            global_transform(&app, widget).translation(),
            expected_widget_position,
        );
        let offset =
            PanelAnchorOffset::new(Px(WIDGET_OFFSET_PIXELS), Mm(WIDGET_OFFSET_MILLIMETERS))
                .with_z(Pt(WIDGET_OFFSET_POINTS));
        let source = app
            .world_mut()
            .spawn((
                world_panel(Anchor::Center),
                Transform::default(),
                AnchoredToPanel::new(widget, Anchor::Center, Anchor::TopRight).with_offset(offset),
            ))
            .id();

        app.update();

        assert_close_3d(
            global_transform(&app, widget).translation(),
            expected_widget_position,
        );
        let target_panel = app
            .world()
            .get::<DiegeticPanel>(target)
            .expect("target panel should remain live");
        let world_per_layout_unit =
            target_panel.world_width() / target_panel.width() * TRANSFORM_SCALE;
        let expected = Vec3::new(
            WIDGET_OFFSET_PIXELS * Unit::Pixels.to_points() / Unit::Millimeters.to_points()
                * world_per_layout_unit,
            -WIDGET_OFFSET_MILLIMETERS * world_per_layout_unit,
            WIDGET_OFFSET_POINTS * Unit::Points.to_points() / Unit::Millimeters.to_points()
                * world_per_layout_unit,
        );
        assert_close_3d(resolved_offset(&app, source), expected);
        assert_eq!(
            app.world()
                .get::<AnchoredHere>(widget)
                .map(AnchoredHere::len),
            Some(1),
        );
        assert_eq!(
            app.world()
                .get::<ValenceAnchoredTo>(widget)
                .map(ValenceAnchoredTo::target),
            Some(target),
        );
        assert!(
            app.world().get::<ResolvedAnchorGeometry>(source).is_some(),
            "source panel should publish anchor geometry",
        );
        assert!(
            app.world().get::<ResolvedAnchorGeometry>(widget).is_some(),
            "demanded widget should publish anchor geometry",
        );
        let widget_rotation = global_transform(&app, widget).rotation();
        assert_close_3d(
            resolved_anchor_point(&app, source, Anchor::Center),
            resolved_anchor_point(&app, widget, Anchor::TopRight) + widget_rotation * expected,
        );
    }

    #[test]
    fn newly_reified_widget_uses_owner_scale_for_offset_on_first_resolver_pass() {
        let mut app = app_with_world_anchoring();
        app.add_plugins(WidgetsPlugin);
        app.add_systems(
            Update,
            attach_source_to_new_widget
                .after(WidgetSystems::ReifyCommandsApplied)
                .before(PanelSystems::ResolvePanelAttachments),
        );
        let target = app
            .world_mut()
            .spawn((
                world_panel(Anchor::Center),
                Transform::from_translation(WIDGET_OWNER_TRANSLATION)
                    .with_rotation(Quat::from_rotation_z(WIDGET_OWNER_ROTATION))
                    .with_scale(Vec3::splat(TRANSFORM_SCALE)),
            ))
            .id();
        let source = app
            .world_mut()
            .spawn((world_panel(Anchor::Center), Transform::default()))
            .id();
        app.insert_resource(PendingWidgetTarget {
            owner_panel:  target,
            source_panel: source,
            widget:       None,
        });

        app.update();

        let owner_scale = global_transform(&app, target)
            .to_scale_rotation_translation()
            .0;
        assert_close_3d(owner_scale, Vec3::splat(TRANSFORM_SCALE));
        let mut tree = LayoutBuilder::new(WORLD_WIDGET_PANEL_WIDTH, WORLD_WIDGET_PANEL_HEIGHT);
        tree.with(
            El::row().size(WORLD_WIDGET_PANEL_WIDTH, WORLD_WIDGET_PANEL_HEIGHT),
            |tree| {
                tree.with(El::new().size(WIDGET_WIDTH, WIDGET_HEIGHT), |_| {});
                tree.with(
                    El::new()
                        .size(WIDGET_WIDTH, WIDGET_HEIGHT)
                        .button("same-frame-offset-target", Button::new()),
                    |_| {},
                );
            },
        );
        app.world_mut()
            .commands()
            .set_tree(target, tree.build())
            .expect("widget target tree should be accepted");

        app.update();

        let widget = app
            .world()
            .resource::<PendingWidgetTarget>()
            .widget
            .expect("new widget should be targeted in its reification frame");
        let widget_local = transform(&app, widget);
        assert_ne!(widget_local.translation, Vec3::ZERO);
        assert_close_3d(widget_local.scale, Vec3::ONE);
        let target_panel = app
            .world()
            .get::<DiegeticPanel>(target)
            .expect("target panel should remain live");
        let owner_global = global_transform(&app, target);
        let (owner_scale, owner_rotation, _) = owner_global.to_scale_rotation_translation();
        let (expected_widget_center, expected_widget_corner) =
            expected_widget_center_and_corner(target_panel, &owner_global);
        let widget_global = global_transform(&app, widget);
        let widget_scale = widget_global.to_scale_rotation_translation().0;
        assert_close_3d(widget_global.translation(), expected_widget_center);
        assert_close_3d(widget_scale, owner_scale);
        let world_per_layout_unit =
            target_panel.world_width() / target_panel.width() * owner_scale.x;
        let expected_offset = Vec3::new(
            WIDGET_OFFSET_PIXELS * Unit::Pixels.to_points() / Unit::Millimeters.to_points()
                * world_per_layout_unit,
            -WIDGET_OFFSET_MILLIMETERS * world_per_layout_unit,
            WIDGET_OFFSET_POINTS * Unit::Points.to_points() / Unit::Millimeters.to_points()
                * world_per_layout_unit,
        );
        assert_close_3d(resolved_offset(&app, source), expected_offset);
        assert_close_3d(
            global_transform(&app, source).translation(),
            expected_widget_corner + owner_rotation * expected_offset,
        );
    }

    #[test]
    fn missing_geometry_diagnostics_deduplicate_by_attachment_key() {
        let mut app = app_with_world_anchoring();
        let target = app
            .world_mut()
            .spawn((Transform::default(), GlobalTransform::default()))
            .id();
        let source = app
            .world_mut()
            .spawn((
                world_panel(Anchor::Center),
                Transform::default(),
                AnchoredToPanel::new(target, Anchor::Center, Anchor::Center),
            ))
            .id();

        app.update();
        app.update();

        let diagnostics = app.world().resource::<ResolveDiagnostics>();
        let entries = diagnostics
            .entries()
            .filter(|entry| {
                entry.source == source
                    && entry.target == target
                    && entry.reason == ResolveSkip::MissingTargetGeometry
            })
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1);
        assert!(
            entries
                .first()
                .is_some_and(|entry| entry.count == DIAGNOSTIC_REPEAT_COUNT),
        );
    }

    #[test]
    fn removing_panel_attachment_restores_authored_world_pose() {
        let mut app = app_with_world_anchoring();
        let target = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                Transform::from_xyz(2.0, 1.0, 0.0),
            ))
            .id();
        let authored = Transform::from_xyz(-1.0, -2.0, 0.0);
        let source = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                authored,
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft),
            ))
            .id();

        app.update();
        assert_ne!(transform(&app, source), authored);

        app.world_mut()
            .entity_mut(source)
            .remove::<AnchoredToPanel>();
        app.update();

        assert_eq!(transform(&app, source), authored);
        assert!(app.world().get::<AnchoredWorldPanelPose>(source).is_none());
        assert!(app.world().get::<ValenceAnchoredTo>(source).is_none());
    }

    #[derive(Resource)]
    struct PoseLift(f32);

    #[derive(Resource)]
    struct PendingWidgetTarget {
        owner_panel:  Entity,
        source_panel: Entity,
        widget:       Option<Entity>,
    }

    fn attach_source_to_new_widget(
        mut commands: Commands,
        mut pending: ResMut<PendingWidgetTarget>,
        panel_widgets: Query<&PanelWidgets>,
    ) {
        if pending.widget.is_some() {
            return;
        }
        let Some(widget) = panel_widgets
            .get(pending.owner_panel)
            .ok()
            .and_then(|widgets| widgets.first().copied())
        else {
            return;
        };
        let offset =
            PanelAnchorOffset::new(Px(WIDGET_OFFSET_PIXELS), Mm(WIDGET_OFFSET_MILLIMETERS))
                .with_z(Pt(WIDGET_OFFSET_POINTS));
        commands.entity(pending.source_panel).insert(
            AnchoredToPanel::new(widget, Anchor::Center, Anchor::TopRight).with_offset(offset),
        );
        pending.widget = Some(widget);
    }

    fn expected_widget_center_and_corner(
        target_panel: &DiegeticPanel,
        owner_global: &GlobalTransform,
    ) -> (Vec3, Vec3) {
        let authored_widget_min = Vec2::new(WIDGET_WIDTH, 0.0);
        let authored_widget_size = Vec2::new(WIDGET_WIDTH, WIDGET_HEIGHT);
        let authored_widget_center = authored_widget_min + authored_widget_size / 2.0;
        let widget_layout_to_world =
            target_panel.layout_unit().to_points() * target_panel.points_to_world();
        let (panel_anchor_x, panel_anchor_y) = target_panel.anchor_offsets();
        let widget_center_local = Vec3::new(
            authored_widget_center
                .x
                .mul_add(widget_layout_to_world, -panel_anchor_x),
            (-authored_widget_center.y).mul_add(widget_layout_to_world, panel_anchor_y),
            0.0,
        );
        let widget_half_extents_local = Vec3::new(
            authored_widget_size.x * widget_layout_to_world / 2.0,
            authored_widget_size.y * widget_layout_to_world / 2.0,
            0.0,
        );
        (
            owner_global.transform_point(widget_center_local),
            owner_global.transform_point(widget_center_local + widget_half_extents_local),
        )
    }

    fn app_with_world_anchoring() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(TransformPlugin);
        app.insert_resource(DiegeticTextMeasurer {
            measure_fn: Arc::new(|_text: &str, measure: &TextMeasure| TextDimensions {
                width:       measure.size,
                height:      measure.size,
                line_height: measure.size,
            }),
        });
        app.add_plugins(HeadlessLayoutPlugin);
        app
    }

    fn world_panel(anchor: Anchor) -> DiegeticPanel {
        DiegeticPanel::world()
            .size(Mm(200.0), Mm(100.0))
            .world_width(2.0)
            .anchor(anchor)
            .layout(|_| {})
            .build()
            .expect("world panel builds")
    }

    fn transform(app: &App, entity: Entity) -> Transform {
        app.world()
            .get::<Transform>(entity)
            .copied()
            .expect("entity has Transform")
    }

    fn global_transform(app: &App, entity: Entity) -> GlobalTransform {
        app.world()
            .get::<GlobalTransform>(entity)
            .copied()
            .expect("entity has GlobalTransform")
    }

    fn resolved_offset(app: &App, entity: Entity) -> Vec3 {
        app.world()
            .get::<ResolvedAnchorOffset>(entity)
            .copied()
            .expect("entity has ResolvedAnchorOffset")
            .0
    }

    fn panel_anchor_point(app: &App, entity: Entity, anchor: Anchor) -> Vec3 {
        let transform = transform(app, entity);
        let panel = app
            .world()
            .get::<DiegeticPanel>(entity)
            .expect("entity has DiegeticPanel");
        let size = Vec2::new(panel.world_width(), panel.world_height());
        let panel_offset = anchor_offset(panel.anchor(), size);
        let source_offset = anchor_offset(anchor, size);
        transform.translation
            + transform.rotation
                * Vec3::new(
                    source_offset.x - panel_offset.x,
                    panel_offset.y - source_offset.y,
                    0.0,
                )
    }

    fn resolved_anchor_point(app: &App, entity: Entity, anchor: Anchor) -> Vec3 {
        let geometry = app
            .world()
            .get::<ResolvedAnchorGeometry>(entity)
            .expect("entity has ResolvedAnchorGeometry");
        let point = geometry
            .points
            .get(&hana_valence::AnchorId::from(anchor))
            .expect("geometry contains requested anchor");
        global_transform(app, entity).transform_point(point.position)
    }

    fn anchor_offset(anchor: Anchor, size: Vec2) -> Vec2 {
        let (x, y) = anchor.offset(size.x, size.y);
        Vec2::new(x, y)
    }

    fn assert_close_3d(actual: Vec3, expected: Vec3) {
        assert!(
            actual.abs_diff_eq(expected, 1e-4),
            "expected {expected:?}, got {actual:?}",
        );
    }

    fn assert_close_quat(actual: Quat, expected: Quat) {
        assert!(
            actual.abs_diff_eq(expected, 1e-4) || actual.abs_diff_eq(-expected, 1e-4),
            "expected {expected:?}, got {actual:?}",
        );
    }

    fn lift_anchored_pose(lift: Res<PoseLift>, mut poses: Query<&mut AnchorPose>) {
        for mut pose in &mut poses {
            pose.translation.z = lift.0;
        }
    }

    fn spawn_lift_scene(app: &mut App) -> (Entity, Entity) {
        let target = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                Transform::from_xyz(1.0, 2.0, 0.0),
            ))
            .id();
        let source = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                Transform::default(),
                AnchoredToPanel::new(target, Anchor::Center, Anchor::Center),
                AnchorPose {
                    rotation:    Quat::IDENTITY,
                    translation: Vec3::ZERO,
                },
            ))
            .id();
        (target, source)
    }

    #[test]
    fn screen_attachment_does_not_enter_valence_resolve_diagnostics() {
        let mut app = app_with_world_anchoring();
        let target = app
            .world_mut()
            .spawn((screen_panel(), Transform::default()))
            .id();
        let source = app
            .world_mut()
            .spawn((
                screen_panel(),
                Transform::default(),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft),
            ))
            .id();

        app.update();

        assert!(app.world().get::<ValenceAnchoredTo>(source).is_none());
        let diagnostics = app.world().resource::<ResolveDiagnostics>();
        assert!(diagnostics.current().next().is_none());
    }

    fn screen_panel() -> DiegeticPanel {
        DiegeticPanel::screen()
            .size(Px(100.0), Px(40.0))
            .screen_position(10.0, 10.0)
            .layout(|_| {})
            .build()
            .expect("screen panel builds")
    }

    #[test]
    fn world_screen_round_trip_keeps_valence_anchor_in_sync() {
        let mut app = app_with_world_anchoring();
        app.world_mut().spawn((
            Window {
                resolution: (800_u32, 600_u32).into(),
                ..default()
            },
            PrimaryWindow,
        ));
        let target = app
            .world_mut()
            .spawn((world_panel(Anchor::TopLeft), Transform::default()))
            .id();
        let source = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                Transform::from_xyz(0.25, 0.5, 0.75),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft),
            ))
            .id();

        app.update();
        assert_eq!(
            app.world().get::<PanelSpace>(source),
            Some(&PanelSpace::World)
        );
        assert!(
            app.world().get::<ValenceAnchoredTo>(source).is_some(),
            "world panel starts anchored"
        );

        let to_screen =
            PanelScreenConversion::at_pixels(Vec2::new(400.0, 300.0), Vec2::new(200.0, 100.0));
        app.world_mut()
            .commands()
            .apply_panel_screen_conversion(source, to_screen);
        app.update();

        assert_eq!(
            app.world().get::<PanelSpace>(source),
            Some(&PanelSpace::Screen)
        );
        assert!(
            app.world().get::<ValenceAnchoredTo>(source).is_none(),
            "screen conversion drops the stale valence anchor"
        );
        let diagnostics = app.world().resource::<ResolveDiagnostics>();
        assert!(diagnostics.current().next().is_none());

        let to_world = PanelWorldConversion {
            transform:           Transform::default(),
            size:                Vec2::new(2.0, 1.0),
            panel_size:          Vec2::new(200.0, 100.0),
            layout_unit:         Unit::Millimeters,
            anchor:              Some(Anchor::TopLeft),
            width:               Sizing::Fixed(Dimension {
                value: 200.0,
                unit:  Some(Unit::Millimeters),
            }),
            height:              Sizing::Fixed(Dimension {
                value: 100.0,
                unit:  Some(Unit::Millimeters),
            }),
            world_width:         Some(2.0),
            world_height:        Some(1.0),
            restore_saved_world: SavedWorldRestoreMode::Skip,
        };
        app.world_mut()
            .commands()
            .apply_panel_world_conversion(source, to_world);
        app.update();

        assert_eq!(
            app.world().get::<PanelSpace>(source),
            Some(&PanelSpace::World)
        );
        assert!(
            app.world().get::<ValenceAnchoredTo>(source).is_some(),
            "world conversion restores the valence anchor"
        );
    }
}
