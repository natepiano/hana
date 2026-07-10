use std::collections::VecDeque;

use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Component;
use bevy_ecs::prelude::ReflectComponent;
use bevy_ecs::prelude::Resource;
use bevy_ecs::system::Query;
use bevy_ecs::system::ResMut;
use bevy_reflect::Reflect;

use super::FoldMember;
use super::FoldSequenceState;
use crate::Hinge;

/// Absolute unfolded and folded hinge endpoints for a fold member.
///
/// Carrying `FoldAngles` assigns ownership of [`Hinge::angle`] to
/// [`actuate_fold_hinges`]. Removing it returns ownership to other hinge
/// drivers, including [`drive_arrangement_hinges`](crate::drive_arrangement_hinges).
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
#[reflect(Component, PartialEq, Debug, Clone)]
pub struct FoldAngles {
    /// Absolute hinge angle at an eased stage fraction of zero.
    pub unfolded: f32,
    /// Absolute hinge angle at an eased stage fraction of one.
    pub folded:   f32,
}

impl FoldAngles {
    const fn invalid_reason(self) -> Option<FoldAngleInvalidReason> {
        if !self.unfolded.is_finite() {
            return Some(FoldAngleInvalidReason::NonFiniteUnfolded);
        }
        if !self.folded.is_finite() {
            return Some(FoldAngleInvalidReason::NonFiniteFolded);
        }
        None
    }
}

/// Reason a fold-angle adapter left its existing [`Hinge::angle`] unchanged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FoldAngleInvalidReason {
    /// [`FoldAngles::unfolded`] is not finite.
    NonFiniteUnfolded,
    /// [`FoldAngles::folded`] is not finite.
    NonFiniteFolded,
    /// Interpolation of finite endpoints produced a non-finite result.
    NonFiniteInterpolation,
}

/// One rejected fold-angle write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FoldAngleDiagnostic {
    /// Fold member whose hinge angle was left unchanged.
    pub member: Entity,
    /// Reason the authored angle could not be applied.
    pub reason: FoldAngleInvalidReason,
}

/// Bounded history of rejected fold-angle writes.
#[derive(Debug, Resource)]
pub struct FoldAngleDiagnostics {
    entries:  VecDeque<FoldAngleDiagnostic>,
    capacity: usize,
}

impl FoldAngleDiagnostics {
    /// Default number of diagnostic entries retained in insertion order.
    pub const DEFAULT_CAPACITY: usize = 128;

    /// Iterates over retained diagnostics in insertion order.
    pub fn entries(&self) -> impl Iterator<Item = &FoldAngleDiagnostic> { self.entries.iter() }

    /// Number of retained rejected writes.
    #[must_use]
    pub fn len(&self) -> usize { self.entries.len() }

    /// Whether no rejected writes have been retained.
    #[must_use]
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }

    fn record(&mut self, diagnostic: FoldAngleDiagnostic) {
        tracing::warn!(
            member = ?diagnostic.member,
            reason = ?diagnostic.reason,
            "fold angle write rejected"
        );
        self.entries.push_back(diagnostic);
        while self.entries.len() > self.capacity {
            self.entries.pop_front();
        }
    }
}

impl Default for FoldAngleDiagnostics {
    fn default() -> Self {
        Self {
            entries:  VecDeque::new(),
            capacity: Self::DEFAULT_CAPACITY,
        }
    }
}

/// Writes eased fold-sequence fractions to absolute [`Hinge::angle`] endpoints.
///
/// A missing or not-ready [`FoldSequenceState`] leaves the existing angle
/// unchanged. Invalid endpoints and non-finite interpolation results also leave
/// the angle unchanged and append a [`FoldAngleDiagnostic`].
pub fn actuate_fold_hinges(
    mut hinges: Query<(Entity, &FoldMember, &FoldAngles, &mut Hinge)>,
    sequences: Query<&FoldSequenceState>,
    mut diagnostics: ResMut<FoldAngleDiagnostics>,
) {
    for (entity, member, angles, mut hinge) in &mut hinges {
        if let Some(reason) = angles.invalid_reason() {
            diagnostics.record(FoldAngleDiagnostic {
                member: entity,
                reason,
            });
            continue;
        }

        let Ok(state) = sequences.get(member.sequence()) else {
            continue;
        };
        if !state.is_ready() {
            continue;
        }

        let fraction = state.fraction(member.stage);
        let angle = fraction.mul_add(angles.folded - angles.unfolded, angles.unfolded);
        if !angle.is_finite() {
            diagnostics.record(FoldAngleDiagnostic {
                member: entity,
                reason: FoldAngleInvalidReason::NonFiniteInterpolation,
            });
            continue;
        }
        hinge.angle = angle;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy_app::App;
    use bevy_app::PostUpdate;
    use bevy_ecs::entity::Entity;
    use bevy_ecs::schedule::IntoScheduleConfigs;
    use bevy_math::Quat;
    use bevy_math::Vec3;
    use bevy_math::curve::easing::EaseFunction;
    use bevy_time::Time;
    use bevy_time::Virtual;

    use super::FoldAngleDiagnostics;
    use super::FoldAngleInvalidReason;
    use super::FoldAngles;
    use crate::AnchorId;
    use crate::AnchorPose;
    use crate::AnchorSystems;
    use crate::AnchoredTo;
    use crate::Edge;
    use crate::FoldCommand;
    use crate::FoldCommandEvent;
    use crate::FoldDirection;
    use crate::FoldEndpoint;
    use crate::FoldMember;
    use crate::FoldPlugin;
    use crate::FoldSequence;
    use crate::FoldStage;
    use crate::Hinge;
    use crate::HingePivot;
    use crate::hinge_to_pose;
    use crate::resolve;

    const ASSERT_EPSILON: f32 = 1e-4;
    const FOLDED_ANGLE: f32 = core::f32::consts::FRAC_PI_2;
    const INITIAL_ANGLE: f32 = -1.0;
    const PARTIAL_SECONDS: f32 = 0.5;
    const REFERENCE_ANGLE: f32 = core::f32::consts::FRAC_PI_4;
    const UNFOLDED_ANGLE: f32 = 0.25;

    #[test]
    fn initial_endpoints_write_absolute_unfolded_and_folded_angles() {
        let mut app = fold_app();
        let unfolded_sequence =
            spawn_sequence(&mut app, FoldEndpoint::Unfolded, EaseFunction::Linear);
        let folded_sequence = spawn_sequence(&mut app, FoldEndpoint::Folded, EaseFunction::Linear);
        let unfolded = spawn_member(&mut app, unfolded_sequence, FoldStage(0), standard_angles());
        let folded = spawn_member(&mut app, folded_sequence, FoldStage(0), standard_angles());

        app.update();

        assert_close(hinge_angle(&app, unfolded), UNFOLDED_ANGLE);
        assert_close(hinge_angle(&app, folded), FOLDED_ANGLE);
    }

    #[test]
    fn partial_authored_easing_maps_to_absolute_endpoints() {
        let mut app = fold_app();
        let sequence = spawn_sequence(&mut app, FoldEndpoint::Unfolded, EaseFunction::QuadraticIn);
        let member = spawn_member(&mut app, sequence, FoldStage(0), standard_angles());
        app.update();

        trigger(
            &mut app,
            sequence,
            FoldCommand::Step(FoldDirection::Folding),
        );
        advance(&mut app, PARTIAL_SECONDS);

        let fraction = PARTIAL_SECONDS * PARTIAL_SECONDS;
        let expected = fraction.mul_add(FOLDED_ANGLE - UNFOLDED_ANGLE, UNFOLDED_ANGLE);
        assert_close(hinge_angle(&app, member), expected);
    }

    #[test]
    fn grouped_members_receive_identical_stage_fractions() {
        let mut app = fold_app();
        let sequence = spawn_sequence(&mut app, FoldEndpoint::Unfolded, EaseFunction::Linear);
        let first = spawn_member(&mut app, sequence, FoldStage(0), standard_angles());
        let second = spawn_member(&mut app, sequence, FoldStage(0), standard_angles());
        app.update();

        trigger(
            &mut app,
            sequence,
            FoldCommand::Step(FoldDirection::Folding),
        );
        advance(&mut app, PARTIAL_SECONDS);

        assert_close(hinge_angle(&app, first), hinge_angle(&app, second));
    }

    #[test]
    fn reverse_playback_decreases_the_absolute_angle() {
        let mut app = fold_app();
        let sequence = spawn_sequence(&mut app, FoldEndpoint::Folded, EaseFunction::Linear);
        let member = spawn_member(&mut app, sequence, FoldStage(0), standard_angles());
        app.update();

        trigger(
            &mut app,
            sequence,
            FoldCommand::Step(FoldDirection::Unfolding),
        );
        advance(&mut app, PARTIAL_SECONDS);

        let expected = PARTIAL_SECONDS.mul_add(FOLDED_ANGLE - UNFOLDED_ANGLE, UNFOLDED_ANGLE);
        assert_close(hinge_angle(&app, member), expected);
    }

    #[test]
    fn missing_and_not_ready_sequences_preserve_existing_angles() {
        let mut app = fold_app();
        let missing_sequence = app.world_mut().spawn_empty().id();
        let missing = spawn_member(&mut app, missing_sequence, FoldStage(0), standard_angles());

        let invalid_sequence =
            spawn_sequence(&mut app, FoldEndpoint::Unfolded, EaseFunction::Linear);
        let invalid = spawn_member(&mut app, invalid_sequence, FoldStage(1), standard_angles());

        app.update();

        assert_close(hinge_angle(&app, missing), INITIAL_ANGLE);
        assert_close(hinge_angle(&app, invalid), INITIAL_ANGLE);
    }

    #[test]
    fn non_finite_endpoints_diagnose_and_preserve_existing_angle() {
        let mut app = fold_app();
        let sequence = spawn_sequence(&mut app, FoldEndpoint::Unfolded, EaseFunction::Linear);
        let member = spawn_member(
            &mut app,
            sequence,
            FoldStage(0),
            FoldAngles {
                unfolded: f32::NAN,
                folded:   FOLDED_ANGLE,
            },
        );

        app.update();

        assert_close(hinge_angle(&app, member), INITIAL_ANGLE);
        let diagnostics = app.world().resource::<FoldAngleDiagnostics>();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries().next().map(|entry| entry.reason),
            Some(FoldAngleInvalidReason::NonFiniteUnfolded),
        );
    }

    #[test]
    fn non_finite_folded_endpoint_diagnoses_and_preserves_existing_angle() {
        let mut app = fold_app();
        let sequence = spawn_sequence(&mut app, FoldEndpoint::Unfolded, EaseFunction::Linear);
        let member = spawn_member(
            &mut app,
            sequence,
            FoldStage(0),
            FoldAngles {
                unfolded: UNFOLDED_ANGLE,
                folded:   f32::INFINITY,
            },
        );

        app.update();

        assert_close(hinge_angle(&app, member), INITIAL_ANGLE);
        let diagnostics = app.world().resource::<FoldAngleDiagnostics>();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries().next().map(|entry| entry.reason),
            Some(FoldAngleInvalidReason::NonFiniteFolded),
        );
    }

    #[test]
    fn overflowing_finite_endpoints_diagnose_and_preserve_existing_angle() {
        let mut app = fold_app();
        let sequence = spawn_sequence(&mut app, FoldEndpoint::Unfolded, EaseFunction::Linear);
        let member = spawn_member(
            &mut app,
            sequence,
            FoldStage(0),
            FoldAngles {
                unfolded: f32::MAX,
                folded:   -f32::MAX,
            },
        );

        app.update();

        assert_close(hinge_angle(&app, member), INITIAL_ANGLE);
        let diagnostics = app.world().resource::<FoldAngleDiagnostics>();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries().next().map(|entry| entry.reason),
            Some(FoldAngleInvalidReason::NonFiniteInterpolation),
        );
    }

    #[test]
    fn hinge_to_pose_reads_the_current_frame_actuated_angle() {
        let mut app = fold_app();
        app.add_systems(PostUpdate, hinge_to_pose.in_set(AnchorSystems::AnimatePose));
        let sequence = spawn_sequence(&mut app, FoldEndpoint::Unfolded, EaseFunction::Linear);
        let member = spawn_member(&mut app, sequence, FoldStage(0), standard_angles());
        app.world_mut()
            .entity_mut(member)
            .insert((resolve::quad_geometry(), AnchorPose::default()));
        app.update();

        trigger(
            &mut app,
            sequence,
            FoldCommand::Step(FoldDirection::Folding),
        );
        advance(&mut app, PARTIAL_SECONDS);

        let angle = PARTIAL_SECONDS.mul_add(FOLDED_ANGLE - UNFOLDED_ANGLE, UNFOLDED_ANGLE);
        let pose = app.world().get::<AnchorPose>(member).copied();
        assert_eq!(
            pose.map(|pose| pose.rotation),
            Some(Quat::from_rotation_x(angle))
        );
    }

    #[test]
    fn reference_endpoint_uses_absolute_rotation_with_zero_pivot_compensation() {
        let mut app = fold_app();
        app.add_systems(PostUpdate, hinge_to_pose.in_set(AnchorSystems::AnimatePose));
        let sequence = spawn_sequence(&mut app, FoldEndpoint::Unfolded, EaseFunction::Linear);
        let target = app.world_mut().spawn_empty().id();
        let member = spawn_member(
            &mut app,
            sequence,
            FoldStage(0),
            FoldAngles {
                unfolded: REFERENCE_ANGLE,
                folded:   FOLDED_ANGLE,
            },
        );
        app.world_mut().entity_mut(member).insert((
            resolve::quad_geometry(),
            AnchorPose::default(),
            HingePivot {
                offset:          Vec3::new(0.0, 0.0, 0.5),
                reference_angle: REFERENCE_ANGLE,
            },
            AnchoredTo::new(target, AnchorId::Vertex(0), AnchorId::Center),
        ));

        app.update();

        assert_close(hinge_angle(&app, member), REFERENCE_ANGLE);
        let pose = app.world().get::<AnchorPose>(member).copied();
        assert_eq!(
            pose,
            Some(AnchorPose {
                rotation:    Quat::from_rotation_x(REFERENCE_ANGLE),
                translation: Vec3::ZERO,
            }),
        );
    }

    fn fold_app() -> App {
        let mut app = App::new();
        app.insert_resource(Time::<Virtual>::default())
            .add_plugins(FoldPlugin);
        app
    }

    fn spawn_sequence(app: &mut App, endpoint: FoldEndpoint, easing: EaseFunction) -> Entity {
        let mut sequence = FoldSequence::new(1.0).with_initial(endpoint);
        sequence.easing = easing;
        app.world_mut().spawn(sequence).id()
    }

    fn spawn_member(
        app: &mut App,
        sequence: Entity,
        stage: FoldStage,
        angles: FoldAngles,
    ) -> Entity {
        app.world_mut()
            .spawn((
                FoldMember::new(sequence, stage),
                angles,
                Hinge {
                    edge:  Edge {
                        start: AnchorId::Vertex(0),
                        end:   AnchorId::Vertex(1),
                    },
                    angle: INITIAL_ANGLE,
                },
            ))
            .id()
    }

    fn standard_angles() -> FoldAngles {
        FoldAngles {
            unfolded: UNFOLDED_ANGLE,
            folded:   FOLDED_ANGLE,
        }
    }

    fn trigger(app: &mut App, sequence: Entity, command: FoldCommand) {
        app.world_mut()
            .trigger(FoldCommandEvent::new(sequence, command));
    }

    fn advance(app: &mut App, seconds: f32) {
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .advance_by(Duration::from_secs_f32(seconds));
        app.update();
    }

    fn hinge_angle(app: &App, entity: Entity) -> f32 {
        app.world()
            .get::<Hinge>(entity)
            .map_or(INITIAL_ANGLE, |hinge| hinge.angle)
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= ASSERT_EPSILON,
            "actual {actual}, expected {expected}",
        );
    }
}
