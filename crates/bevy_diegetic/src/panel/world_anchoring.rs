//! World-space panel attachment resolution.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::transform::helper::TransformHelper;

use super::AnchoredToPanel;
use super::AttachmentResolveAction;
use super::AttachmentResolveCandidate;
use super::AttachmentResolveDiagnostics;
use super::AttachmentResolveReasons;
use super::CoordinateSpace;
use super::DiegeticPanel;
use super::PanelAnchorGeometryError;
use super::PanelAnchorPose;
use super::PanelPlane;
use super::resolve_panel_attachments;
use crate::layout::Anchor;

const ORTHONORMAL_EPSILON: f32 = 1e-4;
const UNIFORM_SCALE_EPSILON: f32 = 1e-4;

pub(crate) type WorldAnchorResolveDiagnostics =
    AttachmentResolveDiagnostics<WorldAnchorResolveSkip>;

/// Restores a world panel's authored local transform after world anchoring stops.
pub(super) fn restore_inactive_world_panel_poses(
    mut commands: Commands,
    mut panels: Query<(
        Entity,
        &AnchoredWorldPanelPose,
        Option<&AnchoredToPanel>,
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

/// Resolves world-space panel attachments for this frame.
pub(super) fn resolve_world_space_panel_attachments(
    mut commands: Commands,
    entities: Query<()>,
    attachments: Query<(Entity, &AnchoredToPanel)>,
    panels: Query<(Entity, &'static DiegeticPanel)>,
    poses: Query<&'static AnchoredWorldPanelPose>,
    mut params: ParamSet<(WorldAnchorReadParam, Query<&'static mut Transform>)>,
    mut diagnostics: ResMut<WorldAnchorResolveDiagnostics>,
) {
    let candidates = classify_candidates(&attachments, &panels, &entities);
    resolve_panel_attachments(
        candidates,
        world_attachment_resolve_reasons(),
        &mut diagnostics,
        |action| match action {
            AttachmentResolveAction::Place {
                source,
                target,
                attachment,
            } => place_world_attachment(
                source,
                target,
                attachment,
                &panels,
                &poses,
                &mut params,
                &mut commands,
            ),
            AttachmentResolveAction::Fallback { source } => {
                restore_authored_pose(source, &poses, &mut params, &mut commands);
                Ok(())
            },
        },
    );
}

#[derive(SystemParam)]
pub(super) struct WorldAnchorReadParam<'w, 's> {
    transforms:   TransformHelper<'w, 's>,
    local:        Query<'w, 's, &'static Transform>,
    parents:      Query<'w, 's, &'static ChildOf>,
    anchor_poses: Query<'w, 's, &'static PanelAnchorPose>,
}

impl WorldAnchorReadParam<'_, '_> {
    fn placement(
        &self,
        source: Entity,
        source_panel: &DiegeticPanel,
        target: Entity,
        target_panel: &DiegeticPanel,
        attachment: AnchoredToPanel,
        has_authored_pose: bool,
    ) -> Result<WorldAnchorPlacement, WorldAnchorResolveSkip> {
        let source_transform = self
            .local
            .get(source)
            .map_err(|_| WorldAnchorResolveSkip::SourceTransformUnavailable)?;
        let source_global = self
            .transforms
            .compute_global_transform(source)
            .map_err(|_| WorldAnchorResolveSkip::SourceTransformUnavailable)?;
        let target_global = self
            .transforms
            .compute_global_transform(target)
            .map_err(|_| WorldAnchorResolveSkip::TargetTransformUnavailable)?;
        let target_plane = PanelPlane::from_panel(target_panel, &target_global)
            .map_err(target_plane_skip_reason)?;
        PanelPlane::from_panel(source_panel, &source_global).map_err(source_plane_skip_reason)?;
        let source_scale = source_global.to_scale_rotation_translation().0;
        if !source_scale.is_finite() {
            return Err(WorldAnchorResolveSkip::SourcePanelPlaneInvalid);
        }

        let anchor_pose = self.anchor_poses.get(source).copied().unwrap_or_default();
        let target_point = target_anchor_point(target_panel, target_plane, attachment)?
            + plane_frame_translation(target_plane, anchor_pose.translation);
        let source_offset =
            scaled_source_anchor_offset(source_panel, attachment.source_anchor, source_scale);
        let desired_rotation = plane_rotation(target_plane) * anchor_pose.rotation;
        let desired_translation = target_point - desired_rotation * source_offset;
        let desired_global = GlobalTransform::from(Transform {
            translation: desired_translation,
            rotation:    desired_rotation,
            scale:       source_scale,
        });
        let local_transform = self.desired_local_transform(source, desired_global)?;
        let captured_pose = (!has_authored_pose).then_some(AnchoredWorldPanelPose {
            authored_transform: *source_transform,
        });

        Ok(WorldAnchorPlacement {
            local_transform,
            captured_pose,
        })
    }

    fn desired_local_transform(
        &self,
        source: Entity,
        desired_global: GlobalTransform,
    ) -> Result<Transform, WorldAnchorResolveSkip> {
        let Ok(parent) = self.parents.get(source) else {
            return Ok(desired_global.compute_transform());
        };
        let parent_global = self
            .transforms
            .compute_global_transform(parent.parent())
            .map_err(|_| WorldAnchorResolveSkip::UnsupportedWorldParentTransform)?;
        validate_supported_parent_transform(&parent_global)?;
        Ok(desired_global.reparented_to(&parent_global))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct WorldAnchorPlacement {
    local_transform: Transform,
    captured_pose:   Option<AnchoredWorldPanelPose>,
}

fn classify_candidates(
    attachments: &Query<(Entity, &AnchoredToPanel)>,
    panels: &Query<(Entity, &DiegeticPanel)>,
    entities: &Query<()>,
) -> Vec<AttachmentResolveCandidate<WorldAnchorResolveSkip>> {
    let mut candidates = Vec::new();
    for (source, attachment) in attachments {
        let Some(candidate) = classify_candidate(source, *attachment, panels, entities) else {
            continue;
        };
        candidates.push(candidate);
    }
    candidates
}

fn classify_candidate(
    source: Entity,
    attachment: AnchoredToPanel,
    panels: &Query<(Entity, &DiegeticPanel)>,
    entities: &Query<()>,
) -> Option<AttachmentResolveCandidate<WorldAnchorResolveSkip>> {
    let target = attachment.target();
    match validate_candidate(source, attachment, panels, entities) {
        Ok(CandidateScope::WorldToWorld) => Some(AttachmentResolveCandidate::Active {
            source,
            target,
            attachment,
        }),
        Ok(CandidateScope::HandledByScreenResolver) => None,
        Err(reason) => Some(AttachmentResolveCandidate::Skipped {
            source,
            target,
            reason,
        }),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CandidateScope {
    WorldToWorld,
    HandledByScreenResolver,
}

fn validate_candidate(
    source: Entity,
    attachment: AnchoredToPanel,
    panels: &Query<(Entity, &DiegeticPanel)>,
    entities: &Query<()>,
) -> Result<CandidateScope, WorldAnchorResolveSkip> {
    let target = attachment.target();
    let Ok((_, source_panel)) = panels.get(source) else {
        return Err(WorldAnchorResolveSkip::SourceWithoutPanel);
    };
    if matches!(
        source_panel.coordinate_space(),
        CoordinateSpace::Screen { .. }
    ) {
        return Ok(CandidateScope::HandledByScreenResolver);
    }
    if source == target {
        return Err(WorldAnchorResolveSkip::SelfAttachment);
    }
    if !entities.contains(target) {
        return Err(WorldAnchorResolveSkip::TargetMissing);
    }
    let Ok((_, target_panel)) = panels.get(target) else {
        return Err(WorldAnchorResolveSkip::TargetWithoutPanel);
    };
    if matches!(
        target_panel.coordinate_space(),
        CoordinateSpace::Screen { .. }
    ) {
        return Err(WorldAnchorResolveSkip::MixedCoordinateSpace);
    }
    Ok(CandidateScope::WorldToWorld)
}

fn place_world_attachment(
    source: Entity,
    target: Entity,
    attachment: AnchoredToPanel,
    panels: &Query<(Entity, &DiegeticPanel)>,
    poses: &Query<&AnchoredWorldPanelPose>,
    params: &mut ParamSet<(WorldAnchorReadParam, Query<&mut Transform>)>,
    commands: &mut Commands,
) -> Result<(), WorldAnchorResolveSkip> {
    let placement = world_anchor_placement(source, target, attachment, panels, poses, params)?;
    {
        let mut transform_query = params.p1();
        let Ok(mut transform) = transform_query.get_mut(source) else {
            return Err(WorldAnchorResolveSkip::SourceTransformUnavailable);
        };
        *transform = placement.local_transform;
    }
    if let Some(pose) = placement.captured_pose {
        commands.entity(source).insert(pose);
    }
    Ok(())
}

fn world_anchor_placement(
    source: Entity,
    target: Entity,
    attachment: AnchoredToPanel,
    panels: &Query<(Entity, &DiegeticPanel)>,
    poses: &Query<&AnchoredWorldPanelPose>,
    params: &mut ParamSet<(WorldAnchorReadParam, Query<&mut Transform>)>,
) -> Result<WorldAnchorPlacement, WorldAnchorResolveSkip> {
    let Ok((_, source_panel)) = panels.get(source) else {
        return Err(WorldAnchorResolveSkip::SourceWithoutPanel);
    };
    let Ok((_, target_panel)) = panels.get(target) else {
        return Err(WorldAnchorResolveSkip::TargetWithoutPanel);
    };
    let has_authored_pose = poses.get(source).is_ok();
    params.p0().placement(
        source,
        source_panel,
        target,
        target_panel,
        attachment,
        has_authored_pose,
    )
}

const fn world_attachment_resolve_reasons() -> AttachmentResolveReasons<WorldAnchorResolveSkip> {
    AttachmentResolveReasons {
        blocked_by_skipped_dependency: WorldAnchorResolveSkip::BlockedBySkippedDependency,
        cycle:                         WorldAnchorResolveSkip::Cycle,
        blocked_by_cycle:              WorldAnchorResolveSkip::BlockedByCycle,
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(crate) struct AnchoredWorldPanelPose {
    authored_transform: Transform,
}

/// Why a world-space attachment did not resolve in the current frame.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum WorldAnchorResolveSkip {
    SourceWithoutPanel,
    TargetMissing,
    TargetWithoutPanel,
    SelfAttachment,
    MixedCoordinateSpace,
    SourceTransformUnavailable,
    TargetTransformUnavailable,
    SourcePanelPlaneInvalid,
    TargetPanelPlaneInvalid,
    UnsupportedWorldParentTransform,
    Cycle,
    BlockedByCycle,
    BlockedBySkippedDependency,
}

fn restore_authored_pose(
    source: Entity,
    poses: &Query<&AnchoredWorldPanelPose>,
    params: &mut ParamSet<(WorldAnchorReadParam, Query<&mut Transform>)>,
    commands: &mut Commands,
) {
    let Ok(pose) = poses.get(source) else {
        return;
    };
    let mut transform_query = params.p1();
    if let Ok(mut transform) = transform_query.get_mut(source) {
        *transform = pose.authored_transform;
    }
    commands.entity(source).remove::<AnchoredWorldPanelPose>();
}

fn target_anchor_point(
    target_panel: &DiegeticPanel,
    target_plane: PanelPlane,
    attachment: AnchoredToPanel,
) -> Result<Vec3, WorldAnchorResolveSkip> {
    let target_point = target_plane.point(attachment.target_anchor);
    let offset = target_offset_meters(target_panel, target_plane, attachment)?;
    Ok(
        target_point + target_plane.right() * offset.x - target_plane.up() * offset.y
            + target_plane.normal() * offset.z,
    )
}

fn target_offset_meters(
    target_panel: &DiegeticPanel,
    target_plane: PanelPlane,
    attachment: AnchoredToPanel,
) -> Result<Vec3, WorldAnchorResolveSkip> {
    let panel_size = Vec2::new(target_panel.width(), target_panel.height());
    if !panel_size.is_finite() || panel_size.x <= 0.0 || panel_size.y <= 0.0 {
        return Err(WorldAnchorResolveSkip::TargetPanelPlaneInvalid);
    }
    let offset = attachment
        .offset
        .to_layout_units(target_panel.layout_unit());
    if !offset.is_finite() {
        return Err(WorldAnchorResolveSkip::TargetPanelPlaneInvalid);
    }
    let target_size = target_plane.size();
    Ok(Vec3::new(
        offset.x * target_size.x / panel_size.x,
        offset.y * target_size.y / panel_size.y,
        offset.z * target_size.x / panel_size.x,
    ))
}

fn plane_frame_translation(plane: PanelPlane, translation: Vec3) -> Vec3 {
    plane.right() * translation.x + plane.up() * translation.y + plane.normal() * translation.z
}

fn scaled_source_anchor_offset(
    panel: &DiegeticPanel,
    source_anchor: Anchor,
    source_scale: Vec3,
) -> Vec3 {
    let size = Vec2::new(panel.world_width(), panel.world_height());
    let source_offset = anchor_offset(source_anchor, size);
    let panel_offset = anchor_offset(panel.anchor(), size);
    Vec3::new(
        source_offset.x - panel_offset.x,
        panel_offset.y - source_offset.y,
        0.0,
    ) * source_scale
}

fn plane_rotation(plane: PanelPlane) -> Quat {
    Quat::from_mat3(&Mat3::from_cols(plane.right(), plane.up(), plane.normal()))
}

fn anchor_offset(anchor: Anchor, size: Vec2) -> Vec2 {
    let (x, y) = anchor.offset(size.x, size.y);
    Vec2::new(x, y)
}

const fn source_plane_skip_reason(error: PanelAnchorGeometryError) -> WorldAnchorResolveSkip {
    match error {
        PanelAnchorGeometryError::InvalidPanelPlane
        | PanelAnchorGeometryError::InvalidPanelSize
        | PanelAnchorGeometryError::PanelMissing
        | PanelAnchorGeometryError::WindowMissing
        | PanelAnchorGeometryError::WindowZeroSized => {
            WorldAnchorResolveSkip::SourcePanelPlaneInvalid
        },
        PanelAnchorGeometryError::TransformUnavailable => {
            WorldAnchorResolveSkip::SourceTransformUnavailable
        },
    }
}

const fn target_plane_skip_reason(error: PanelAnchorGeometryError) -> WorldAnchorResolveSkip {
    match error {
        PanelAnchorGeometryError::InvalidPanelPlane
        | PanelAnchorGeometryError::InvalidPanelSize
        | PanelAnchorGeometryError::PanelMissing
        | PanelAnchorGeometryError::WindowMissing
        | PanelAnchorGeometryError::WindowZeroSized => {
            WorldAnchorResolveSkip::TargetPanelPlaneInvalid
        },
        PanelAnchorGeometryError::TransformUnavailable => {
            WorldAnchorResolveSkip::TargetTransformUnavailable
        },
    }
}

fn validate_supported_parent_transform(
    parent: &GlobalTransform,
) -> Result<(), WorldAnchorResolveSkip> {
    let affine = parent.affine();
    let x_axis = affine.transform_vector3(Vec3::X);
    let y_axis = affine.transform_vector3(Vec3::Y);
    let z_axis = affine.transform_vector3(Vec3::Z);
    let x_scale = x_axis.length();
    let y_scale = y_axis.length();
    let z_scale = z_axis.length();
    if !x_scale.is_finite()
        || !y_scale.is_finite()
        || !z_scale.is_finite()
        || x_scale <= ORTHONORMAL_EPSILON
        || y_scale <= ORTHONORMAL_EPSILON
        || z_scale <= ORTHONORMAL_EPSILON
    {
        return Err(WorldAnchorResolveSkip::UnsupportedWorldParentTransform);
    }
    let average_scale = (x_scale + y_scale + z_scale) / 3.0;
    if (x_scale - average_scale).abs() > UNIFORM_SCALE_EPSILON
        || (y_scale - average_scale).abs() > UNIFORM_SCALE_EPSILON
        || (z_scale - average_scale).abs() > UNIFORM_SCALE_EPSILON
    {
        return Err(WorldAnchorResolveSkip::UnsupportedWorldParentTransform);
    }

    let x_axis = x_axis / x_scale;
    let y_axis = y_axis / y_scale;
    let z_axis = z_axis / z_scale;
    if x_axis.dot(y_axis).abs() > ORTHONORMAL_EPSILON
        || x_axis.dot(z_axis).abs() > ORTHONORMAL_EPSILON
        || y_axis.dot(z_axis).abs() > ORTHONORMAL_EPSILON
        || x_axis.cross(y_axis).dot(z_axis) <= 0.0
    {
        return Err(WorldAnchorResolveSkip::UnsupportedWorldParentTransform);
    }
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::sync::Arc;

    use bevy::prelude::*;
    use bevy::transform::TransformPlugin;

    use super::AnchoredWorldPanelPose;
    use super::WorldAnchorResolveDiagnostics;
    use super::WorldAnchorResolveSkip;
    use crate::AnchoredToPanel;
    use crate::HeadlessLayoutPlugin;
    use crate::Mm;
    use crate::PanelAnchorOffset;
    use crate::PanelAnchorPose;
    use crate::PanelPlane;
    use crate::Px;
    use crate::layout::Anchor;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;
    use crate::panel::DiegeticPanel;
    use crate::text::DiegeticTextMeasurer;

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
        let panel = DiegeticPanel::world()
            .size(Mm(200.0), Mm(100.0))
            .world_width(2.0)
            .anchor(anchor)
            .layout(|_| {})
            .build();
        assert!(panel.is_ok(), "world panel should build");
        panel.unwrap_or_else(|_| DiegeticPanel::default())
    }

    fn screen_panel() -> DiegeticPanel {
        let panel = DiegeticPanel::screen()
            .size(Px(100.0), Px(40.0))
            .screen_position(10.0, 10.0)
            .layout(|_| {})
            .build();
        assert!(panel.is_ok(), "screen panel should build");
        panel.unwrap_or_else(|_| DiegeticPanel::default())
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

    fn transform(app: &App, entity: Entity) -> Transform {
        let transform = app.world().get::<Transform>(entity).copied();
        assert!(transform.is_some(), "entity should have Transform");
        transform.unwrap_or_default()
    }

    fn global_transform(app: &App, entity: Entity) -> GlobalTransform {
        let transform = app.world().get::<GlobalTransform>(entity).copied();
        assert!(transform.is_some(), "entity should have GlobalTransform");
        transform.unwrap_or_default()
    }

    fn panel_plane(app: &App, entity: Entity) -> PanelPlane {
        let panel = app
            .world()
            .get::<DiegeticPanel>(entity)
            .expect("entity should have DiegeticPanel");
        PanelPlane::from_panel(panel, &global_transform(app, entity))
            .expect("panel should have a valid world plane")
    }

    fn panel_anchor_point(app: &App, entity: Entity, anchor: Anchor) -> Vec3 {
        panel_plane(app, entity).point(anchor)
    }

    fn assert_current_diagnostic(
        app: &App,
        source: Entity,
        target: Entity,
        reason: WorldAnchorResolveSkip,
    ) {
        let diagnostics = app.world().resource::<WorldAnchorResolveDiagnostics>();
        assert!(
            diagnostics.current().any(|entry| entry.source == source
                && entry.target == target
                && entry.reason == reason),
            "missing current diagnostic {reason:?}",
        );
    }

    #[test]
    fn world_anchoring_places_source_anchor_against_target_anchor_same_frame() {
        let mut app = app_with_world_anchoring();
        let target = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                Transform::from_xyz(1.0, 2.0, 0.0),
            ))
            .id();
        let source_authored = Transform::from_xyz(-5.0, 0.5, 0.0);
        let source = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                source_authored,
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft)
                    .with_offset(PanelAnchorOffset::new(Mm(25.0), Mm(50.0))),
            ))
            .id();

        if let Some(mut transform) = app.world_mut().get_mut::<Transform>(target) {
            transform.translation = Vec3::new(3.0, 4.0, 0.0);
        }
        app.update();

        let source_transform = transform(&app, source);
        assert_close_3d(source_transform.translation, Vec3::new(3.25, 2.5, 0.0));
        assert_close_quat(source_transform.rotation, Quat::IDENTITY);
        assert_eq!(
            app.world()
                .get::<AnchoredWorldPanelPose>(source)
                .map(|pose| pose.authored_transform),
            Some(source_authored)
        );
    }

    #[test]
    fn world_z_offset_displaces_along_target_plane_normal() {
        let mut app = app_with_world_anchoring();
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
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft)
                    .with_offset(PanelAnchorOffset::new(Mm(25.0), Mm(50.0)).with_z(Mm(30.0))),
            ))
            .id();

        app.update();

        // The 200 mm panel spans 2 m, so 30 mm of depth is 0.3 m along +Z.
        assert_close_3d(
            transform(&app, source).translation,
            Vec3::new(1.25, 0.5, 0.3),
        );
    }

    #[test]
    fn world_bare_z_offset_resolves_against_target_layout_unit() {
        let mut app = app_with_world_anchoring();
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
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft)
                    .with_offset(PanelAnchorOffset::ZERO.with_z(30.0)),
            ))
            .id();

        app.update();

        // A bare 30.0 resolves as 30 layout units (mm here) = 0.3 m.
        assert_close_3d(
            transform(&app, source).translation,
            Vec3::new(1.0, 1.0, 0.3),
        );
    }

    #[test]
    fn world_z_offset_follows_rotated_target_normal() {
        let mut app = app_with_world_anchoring();
        let target_rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let target = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                Transform::from_xyz(1.0, 2.0, 0.0).with_rotation(target_rotation),
            ))
            .id();
        let source = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                Transform::default(),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::TopLeft)
                    .with_offset(PanelAnchorOffset::ZERO.with_z(Mm(100.0))),
            ))
            .id();

        app.update();

        // The rotated plane normal is +X, so 1 m of depth displaces along X.
        let source_transform = transform(&app, source);
        assert_close_3d(source_transform.translation, Vec3::new(2.0, 2.0, 0.0));
        assert_close_quat(source_transform.rotation, target_rotation);
    }

    #[test]
    fn world_z_offset_composes_with_xy_and_projects_back_to_target_point() {
        let mut app = app_with_world_anchoring();
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
                AnchoredToPanel::new(target, Anchor::BottomRight, Anchor::Center)
                    .with_offset(PanelAnchorOffset::new(Mm(25.0), Mm(50.0)).with_z(Mm(30.0))),
            ))
            .id();

        app.update();

        let source_translation = transform(&app, source).translation;
        assert_close_3d(source_translation, Vec3::new(0.25, 2.0, 0.3));

        // The pinned BottomRight point minus the normal displacement lands on
        // the target Center point plus the x/y offset.
        let pinned = source_translation + Vec3::new(2.0, -1.0, 0.0);
        assert_close_3d(pinned - Vec3::Z * 0.3, Vec3::new(2.25, 1.0, 0.0));
    }

    #[test]
    fn world_anchoring_copies_target_plane_rotation() {
        let mut app = app_with_world_anchoring();
        let target_rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let target = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                Transform::from_xyz(1.0, 2.0, 0.0).with_rotation(target_rotation),
            ))
            .id();
        let source = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                Transform::from_xyz(0.0, 0.0, 0.0),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomRight),
            ))
            .id();

        app.update();

        let source_transform = transform(&app, source);
        assert_close_3d(source_transform.translation, Vec3::new(2.0, 4.0, 0.0));
        assert_close_quat(source_transform.rotation, target_rotation);
    }

    #[test]
    fn panel_anchor_pose_normal_rotation_spins_coplanar_about_fixed_pin() {
        let mut app = app_with_world_anchoring();
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
            ))
            .id();

        app.update();

        let target_pin = panel_anchor_point(&app, target, Anchor::Center);
        assert_close_3d(panel_anchor_point(&app, source, Anchor::Center), target_pin);

        let pose_rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        app.world_mut().entity_mut(source).insert(PanelAnchorPose {
            rotation:    pose_rotation,
            translation: Vec3::ZERO,
        });
        app.update();

        assert_close_3d(panel_anchor_point(&app, source, Anchor::Center), target_pin);
        assert_close_quat(transform(&app, source).rotation, pose_rotation);
        assert_close_3d(panel_plane(&app, source).normal(), Vec3::Z);
    }

    #[test]
    fn panel_anchor_pose_right_axis_rotation_hinges_about_fixed_pin() {
        let mut app = app_with_world_anchoring();
        let target = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                Transform::from_xyz(1.0, 2.0, 0.0),
            ))
            .id();
        let pose_rotation = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
        let source = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                Transform::default(),
                AnchoredToPanel::new(target, Anchor::Center, Anchor::Center),
                PanelAnchorPose {
                    rotation:    pose_rotation,
                    translation: Vec3::ZERO,
                },
            ))
            .id();

        app.update();

        assert_close_3d(
            panel_anchor_point(&app, source, Anchor::Center),
            panel_anchor_point(&app, target, Anchor::Center),
        );
        assert_close_quat(transform(&app, source).rotation, pose_rotation);
        assert_close_3d(panel_plane(&app, source).normal(), Vec3::NEG_Y);
    }

    #[test]
    fn panel_anchor_pose_translation_displaces_pin_after_static_offset() {
        let mut app = app_with_world_anchoring();
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
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::TopLeft)
                    .with_offset(PanelAnchorOffset::new(Mm(25.0), Mm(50.0)).with_z(Mm(30.0))),
                PanelAnchorPose {
                    rotation:    Quat::IDENTITY,
                    translation: Vec3::new(0.5, 0.25, 0.75),
                },
            ))
            .id();

        app.update();

        assert_close_3d(
            panel_anchor_point(&app, source, Anchor::TopLeft),
            Vec3::new(1.75, 1.75, 1.05),
        );
    }

    #[test]
    fn panel_anchor_pose_composes_after_target_plane_rotation() {
        let mut app = app_with_world_anchoring();
        let target_rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let pose_rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let target = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                Transform::from_xyz(1.0, 2.0, 0.0).with_rotation(target_rotation),
            ))
            .id();
        let source = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                Transform::default(),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::TopLeft),
                PanelAnchorPose {
                    rotation:    pose_rotation,
                    translation: Vec3::ZERO,
                },
            ))
            .id();

        app.update();

        assert_close_quat(
            transform(&app, source).rotation,
            target_rotation * pose_rotation,
        );
    }

    #[test]
    fn removing_panel_anchor_pose_returns_to_plain_snap_without_recapturing_authored_pose() {
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
                PanelAnchorPose {
                    rotation:    Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                    translation: Vec3::new(0.25, 0.5, 0.0),
                },
            ))
            .id();

        app.update();
        assert_eq!(
            app.world()
                .get::<AnchoredWorldPanelPose>(source)
                .map(|pose| pose.authored_transform),
            Some(authored)
        );

        app.world_mut()
            .entity_mut(source)
            .remove::<PanelAnchorPose>();
        app.update();

        let source_transform = transform(&app, source);
        assert_close_3d(source_transform.translation, Vec3::new(2.0, 0.0, 0.0));
        assert_close_quat(source_transform.rotation, Quat::IDENTITY);
        assert_eq!(
            app.world()
                .get::<AnchoredWorldPanelPose>(source)
                .map(|pose| pose.authored_transform),
            Some(authored)
        );
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
    fn world_anchoring_skips_non_uniform_parent_and_restores_pose() {
        let mut app = app_with_world_anchoring();
        let parent = app
            .world_mut()
            .spawn(Transform::from_scale(Vec3::new(2.0, 1.0, 1.0)))
            .id();
        let target = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                Transform::from_xyz(2.0, 1.0, 0.0),
            ))
            .id();
        let authored = Transform::from_xyz(0.25, 0.5, 0.0);
        let source = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                authored,
                ChildOf(parent),
                AnchoredWorldPanelPose {
                    authored_transform: authored,
                },
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft),
            ))
            .id();

        app.update();

        assert_eq!(transform(&app, source), authored);
        assert!(app.world().get::<AnchoredWorldPanelPose>(source).is_none());
        assert_current_diagnostic(
            &app,
            source,
            target,
            WorldAnchorResolveSkip::UnsupportedWorldParentTransform,
        );
    }

    #[test]
    fn world_anchoring_removal_restores_authored_pose() {
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
    }

    #[test]
    fn world_anchoring_chain_resolves_in_one_update() {
        let mut app = app_with_world_anchoring();
        let root = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                Transform::from_xyz(1.0, 2.0, 0.0),
            ))
            .id();
        let middle = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                Transform::from_xyz(0.0, 0.0, 0.0),
                AnchoredToPanel::new(root, Anchor::TopLeft, Anchor::BottomLeft),
            ))
            .id();
        let leaf = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                Transform::from_xyz(0.0, 0.0, 0.0),
                AnchoredToPanel::new(middle, Anchor::TopLeft, Anchor::BottomLeft),
            ))
            .id();

        app.update();

        assert_close_3d(
            transform(&app, middle).translation,
            Vec3::new(1.0, 1.0, 0.0),
        );
        assert_close_3d(transform(&app, leaf).translation, Vec3::new(1.0, 0.0, 0.0));
    }

    #[test]
    fn world_anchoring_cycle_restores_pose_and_reports_cycle() {
        let mut app = app_with_world_anchoring();
        let a_authored = Transform::from_xyz(1.0, 0.0, 0.0);
        let b_authored = Transform::from_xyz(3.0, 0.0, 0.0);
        let a = app
            .world_mut()
            .spawn((world_panel(Anchor::TopLeft), a_authored))
            .id();
        let b = app
            .world_mut()
            .spawn((world_panel(Anchor::TopLeft), b_authored))
            .id();
        app.world_mut().entity_mut(a).insert((
            AnchoredWorldPanelPose {
                authored_transform: a_authored,
            },
            AnchoredToPanel::new(b, Anchor::TopLeft, Anchor::BottomLeft),
        ));
        app.world_mut().entity_mut(b).insert((
            AnchoredWorldPanelPose {
                authored_transform: b_authored,
            },
            AnchoredToPanel::new(a, Anchor::TopLeft, Anchor::BottomLeft),
        ));

        app.update();

        assert_eq!(transform(&app, a), a_authored);
        assert_eq!(transform(&app, b), b_authored);
        assert_current_diagnostic(&app, a, b, WorldAnchorResolveSkip::Cycle);
        assert_current_diagnostic(&app, b, a, WorldAnchorResolveSkip::Cycle);
    }

    #[test]
    fn world_anchoring_screen_target_is_diagnosed_as_mixed_space() {
        let mut app = app_with_world_anchoring();
        let target = app
            .world_mut()
            .spawn((
                screen_panel(),
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id();
        let source = app
            .world_mut()
            .spawn((
                world_panel(Anchor::TopLeft),
                Transform::from_xyz(0.0, 0.0, 0.0),
                AnchoredToPanel::new(target, Anchor::TopLeft, Anchor::BottomLeft),
            ))
            .id();

        app.update();

        assert_current_diagnostic(
            &app,
            source,
            target,
            WorldAnchorResolveSkip::MixedCoordinateSpace,
        );
    }

    mod anchor_animation {
        use super::*;
        use crate::panel::PanelSystems;
        use crate::panel::world_anchoring;

        const LIFT: f32 = 0.5;

        #[derive(Resource)]
        struct PoseLift(f32);

        fn lift_anchored_pose(lift: Res<PoseLift>, mut poses: Query<&mut PanelAnchorPose>) {
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
                    PanelAnchorPose {
                        rotation:    Quat::IDENTITY,
                        translation: Vec3::ZERO,
                    },
                ))
                .id();
            (target, source)
        }

        #[test]
        fn pose_written_in_animation_set_lands_this_frame() {
            let mut app = app_with_world_anchoring();
            app.insert_resource(PoseLift(LIFT));
            app.add_systems(
                PostUpdate,
                lift_anchored_pose.in_set(PanelSystems::AnimateAnchorPose),
            );
            let (target, source) = spawn_lift_scene(&mut app);

            app.update();

            let target_pin = panel_anchor_point(&app, target, Anchor::Center);
            assert_close_3d(
                panel_anchor_point(&app, source, Anchor::Center),
                target_pin + Vec3::Z * LIFT,
            );
        }

        #[test]
        fn pose_written_after_resolver_lands_next_frame() {
            let mut app = app_with_world_anchoring();
            app.insert_resource(PoseLift(LIFT));
            app.add_systems(
                PostUpdate,
                lift_anchored_pose.after(world_anchoring::resolve_world_space_panel_attachments),
            );
            let (target, source) = spawn_lift_scene(&mut app);

            app.update();
            let target_pin = panel_anchor_point(&app, target, Anchor::Center);
            assert_close_3d(panel_anchor_point(&app, source, Anchor::Center), target_pin);

            app.update();
            assert_close_3d(
                panel_anchor_point(&app, source, Anchor::Center),
                target_pin + Vec3::Z * LIFT,
            );
        }
    }
}
