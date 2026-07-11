use std::collections::VecDeque;
use std::ops::Deref;

use bevy_ecs::change_detection::Ref;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::Discard;
use bevy_ecs::lifecycle::Insert;
use bevy_ecs::lifecycle::Remove;
use bevy_ecs::prelude::Changed;
use bevy_ecs::prelude::Commands;
use bevy_ecs::prelude::Component;
use bevy_ecs::prelude::FromWorld;
use bevy_ecs::prelude::On;
use bevy_ecs::prelude::Or;
use bevy_ecs::prelude::Query;
use bevy_ecs::prelude::ReflectComponent;
use bevy_ecs::prelude::ReflectFromWorld;
use bevy_ecs::prelude::ResMut;
use bevy_ecs::prelude::Resource;
use bevy_ecs::prelude::With;
use bevy_ecs::prelude::Without;
use bevy_ecs::prelude::World;
use bevy_kana::ToF32;
use bevy_math::curve::Curve;
use bevy_math::curve::easing::EaseFunction;
use bevy_platform::collections::HashSet;
use bevy_reflect::Reflect;
use bevy_reflect::std_traits::ReflectDefault;

use super::FoldDirection;
use super::FoldFromArrangement;
use super::FoldMotion;
use super::playback::FoldPlayback;

/// Authored configuration for one fold sequence entity.
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
#[reflect(Component, PartialEq, Debug, Clone)]
pub struct FoldSequence {
    /// Seconds required to move through one stage.
    pub step_seconds: f32,
    /// Easing applied independently to each stage fraction.
    pub easing:       EaseFunction,
    /// Playback endpoint used by the first valid membership revision.
    pub initial:      FoldEndpoint,
}

impl FoldSequence {
    /// Creates an unfolded sequence with smoother-step easing.
    #[must_use]
    pub const fn new(step_seconds: f32) -> Self {
        Self {
            step_seconds,
            easing: EaseFunction::SmootherStep,
            initial: FoldEndpoint::Unfolded,
        }
    }

    /// Sets the endpoint used by the first valid membership revision.
    #[must_use]
    pub const fn with_initial(mut self, endpoint: FoldEndpoint) -> Self {
        self.initial = endpoint;
        self
    }
}

/// Initial endpoint for a fold sequence.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Reflect)]
#[reflect(PartialEq, Debug, Default, Clone)]
pub enum FoldEndpoint {
    /// Boundary zero, where idle playback begins by folding.
    #[default]
    Unfolded,
    /// The terminal stage boundary, where idle playback begins by unfolding.
    Folded,
}

/// Zero-based playback stage assigned to a fold member.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Reflect)]
#[reflect(PartialEq, Debug, Default, Clone)]
pub struct FoldStage(
    /// Zero-based stage index.
    pub usize,
);

/// Optional relationship from a foldable entity to its sequence entity.
///
/// `FoldMember` is immutable, so changing its sequence or stage requires a
/// full component replacement. Bevy updates [`FoldMembers`] through the
/// relationship hooks.
#[derive(Component, Clone, Copy, Debug, Eq, PartialEq, Reflect)]
#[component(immutable)]
#[reflect(PartialEq, Debug, FromWorld, Clone)]
#[relationship(relationship_target = FoldMembers)]
pub struct FoldMember {
    #[relationship]
    #[entities]
    #[reflect(ignore, default = "placeholder_entity")]
    sequence:  Entity,
    /// Zero-based playback stage for this member.
    pub stage: FoldStage,
}

impl FoldMember {
    /// Creates membership in `sequence` at `stage`.
    #[must_use]
    pub const fn new(sequence: Entity, stage: FoldStage) -> Self { Self { sequence, stage } }

    /// Sequence entity that owns this membership.
    #[must_use]
    pub const fn sequence(&self) -> Entity { self.sequence }

    /// Returns a copy that points at `sequence`.
    #[must_use]
    pub const fn retargeted(mut self, sequence: Entity) -> Self {
        self.sequence = sequence;
        self
    }
}

impl FromWorld for FoldMember {
    fn from_world(_: &mut World) -> Self { Self::new(Entity::PLACEHOLDER, FoldStage::default()) }
}

const fn placeholder_entity() -> Entity { Entity::PLACEHOLDER }

/// Reverse relationship target containing a sequence's fold members.
///
/// Members remain in relationship insertion order. Distinct members may share
/// a [`FoldStage`].
#[derive(Component, Debug, Default, Reflect)]
#[reflect(FromWorld, Default)]
#[relationship_target(relationship = FoldMember)]
pub struct FoldMembers(Vec<Entity>);

impl FoldMembers {
    /// Iterates over member entities in relationship insertion order.
    pub fn iter(&self) -> impl Iterator<Item = Entity> + '_ { self.0.iter().copied() }

    /// Number of member entities in this sequence.
    #[must_use]
    pub const fn len(&self) -> usize { self.0.len() }

    /// Whether this sequence has no member entities.
    #[must_use]
    pub const fn is_empty(&self) -> bool { self.0.is_empty() }
}

impl Deref for FoldMembers {
    type Target = [Entity];

    fn deref(&self) -> &Self::Target { &self.0 }
}

/// Validated read-only runtime state for a [`FoldSequence`].
#[derive(Component, Clone, Copy, Debug, Reflect)]
#[reflect(Component, Debug, Clone)]
pub struct FoldSequenceState {
    stages:      Option<usize>,
    playback:    FoldPlayback,
    easing:      EaseFunction,
    initialized: bool,
    revision:    u64,
}

impl FoldSequenceState {
    const fn uninitialized() -> Self {
        Self {
            stages:      None,
            playback:    FoldPlayback::uninitialized(),
            easing:      EaseFunction::SmootherStep,
            initialized: false,
            revision:    0,
        }
    }

    /// Whether the authored configuration and current membership are valid.
    #[must_use]
    pub const fn is_ready(&self) -> bool { self.stages.is_some() }

    /// Derived stage count, or zero while the sequence is not ready.
    #[must_use]
    pub const fn stage_count(&self) -> usize {
        match self.stages {
            Some(stages) => stages,
            None => 0,
        }
    }

    /// Continuous playback position in stage-boundary units.
    #[must_use]
    pub const fn position(&self) -> f32 { self.playback.position() }

    /// Integer stage boundary currently targeted by playback.
    #[must_use]
    pub const fn target(&self) -> usize { self.playback.target() }

    /// Direction most recently selected by a step or terminal playback.
    #[must_use]
    pub const fn direction(&self) -> FoldDirection { self.playback.direction() }

    /// Current playback kind.
    #[must_use]
    pub const fn motion(&self) -> FoldMotion { self.playback.motion() }

    /// Eased fold fraction for `stage` at the current continuous position.
    #[must_use]
    pub fn fraction(&self, stage: FoldStage) -> f32 {
        let fraction = (self.position() - stage.0.to_f32()).clamp(0.0, 1.0);
        self.easing.sample_clamped(fraction)
    }

    pub(super) fn apply(&mut self, command: super::FoldCommand) {
        if let Some(stages) = self.stages {
            self.playback.apply(command, stages);
        }
    }

    pub(super) fn advance(&mut self, stage_delta: f32) { self.playback.advance(stage_delta); }

    fn accept(&mut self, sequence: &FoldSequence, stages: usize) {
        if self.initialized {
            self.playback.clamp_to(stages);
        } else {
            self.playback = FoldPlayback::initial(sequence.initial, stages);
            self.initialized = true;
        }
        self.easing = sequence.easing;
        self.stages = Some(stages);
    }

    const fn reject(&mut self) {
        self.stages = None;
        self.playback.stop();
    }

    const fn next_revision(&mut self) -> u64 {
        self.revision = self.revision.saturating_add(1);
        self.revision
    }
}

/// Reason a fold sequence revision is not ready.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FoldInvalidReason {
    /// `FoldSequence::step_seconds` is not finite.
    NonFiniteStepSeconds,
    /// `FoldSequence::step_seconds` is zero or negative.
    NonPositiveStepSeconds,
    /// A member entity occurs more than once in [`FoldMembers`].
    DuplicateMember(Entity),
    /// A reverse relationship entry has no corresponding [`FoldMember`].
    MissingMember(Entity),
    /// A member's source relationship points at another sequence entity.
    MismatchedSequence(Entity),
    /// No member uses this stage even though a later stage is present.
    MissingStage(FoldStage),
    /// The largest authored stage cannot be converted into a stage count.
    StageCountOverflow,
}

/// One invalid validated revision of a fold sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FoldDiagnostic {
    /// Sequence entity whose revision is invalid.
    pub sequence: Entity,
    /// Monotonic validated revision number for this sequence state.
    pub revision: u64,
    /// First deterministic validation failure for this revision.
    pub reason:   FoldInvalidReason,
}

/// Bounded history containing one entry per invalid validated revision.
#[derive(Debug, Resource)]
pub struct FoldDiagnostics {
    entries:  VecDeque<FoldDiagnostic>,
    capacity: usize,
}

impl FoldDiagnostics {
    /// Default number of diagnostic entries retained in insertion order.
    pub const DEFAULT_CAPACITY: usize = 128;

    /// Iterates over retained diagnostics in revision order.
    pub fn entries(&self) -> impl Iterator<Item = &FoldDiagnostic> { self.entries.iter() }

    /// Number of retained invalid revisions.
    #[must_use]
    pub fn len(&self) -> usize { self.entries.len() }

    /// Whether no invalid revision has been retained.
    #[must_use]
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }

    fn record(&mut self, diagnostic: FoldDiagnostic) {
        tracing::warn!(
            sequence = ?diagnostic.sequence,
            revision = diagnostic.revision,
            reason = ?diagnostic.reason,
            "fold sequence revision is invalid"
        );
        self.entries.push_back(diagnostic);
        while self.entries.len() > self.capacity {
            self.entries.pop_front();
        }
    }
}

impl Default for FoldDiagnostics {
    fn default() -> Self {
        Self {
            entries:  VecDeque::new(),
            capacity: Self::DEFAULT_CAPACITY,
        }
    }
}

#[derive(Component, Clone, Copy, Debug, Default)]
pub(super) struct FoldValidationPending;

pub(super) fn on_fold_member_inserted(
    inserted: On<Insert, FoldMember>,
    members: Query<&FoldMember>,
    mut commands: Commands,
) {
    if let Ok(member) = members.get(inserted.entity) {
        mark_pending(&mut commands, member.sequence());
    }
}

pub(super) fn on_fold_member_discarded(
    discarded: On<Discard, FoldMember>,
    members: Query<&FoldMember>,
    mut commands: Commands,
) {
    if let Ok(member) = members.get(discarded.entity) {
        mark_pending(&mut commands, member.sequence());
    }
}

pub(super) fn on_fold_sequence_inserted(
    inserted: On<Insert, FoldSequence>,
    mut commands: Commands,
) {
    mark_pending(&mut commands, inserted.entity);
}

pub(super) fn on_fold_sequence_removed(removed: On<Remove, FoldSequence>, mut commands: Commands) {
    if let Ok(mut sequence) = commands.get_entity(removed.entity) {
        sequence.remove::<(FoldSequenceState, FoldValidationPending)>();
    }
}

fn mark_pending(commands: &mut Commands, sequence: Entity) {
    if let Ok(mut entity) = commands.get_entity(sequence) {
        entity.insert(FoldValidationPending);
    }
}

type ValidationFilter = Or<(
    Changed<FoldSequence>,
    Changed<FoldMembers>,
    With<FoldValidationPending>,
)>;

pub(super) fn validate_fold_sequences(
    mut commands: Commands,
    sequences: Query<
        (
            Entity,
            &FoldSequence,
            Option<Ref<FoldMembers>>,
            Option<&FoldSequenceState>,
        ),
        (ValidationFilter, Without<FoldFromArrangement>),
    >,
    members: Query<&FoldMember>,
    mut diagnostics: ResMut<FoldDiagnostics>,
) {
    for (entity, sequence, fold_members, current_state) in &sequences {
        let mut state = current_state
            .copied()
            .unwrap_or_else(FoldSequenceState::uninitialized);
        let revision = state.next_revision();
        match validate_sequence(entity, sequence, fold_members.as_deref(), &members) {
            Ok(stages) => state.accept(sequence, stages),
            Err(reason) => {
                state.reject();
                diagnostics.record(FoldDiagnostic {
                    sequence: entity,
                    revision,
                    reason,
                });
            },
        }
        commands
            .entity(entity)
            .insert(state)
            .remove::<FoldValidationPending>();
    }
}

fn validate_sequence(
    sequence_entity: Entity,
    sequence: &FoldSequence,
    fold_members: Option<&FoldMembers>,
    members: &Query<&FoldMember>,
) -> Result<usize, FoldInvalidReason> {
    if !sequence.step_seconds.is_finite() {
        return Err(FoldInvalidReason::NonFiniteStepSeconds);
    }
    if sequence.step_seconds <= 0.0 {
        return Err(FoldInvalidReason::NonPositiveStepSeconds);
    }

    let Some(fold_members) = fold_members else {
        return Ok(0);
    };
    let mut member_entities = HashSet::<Entity>::default();
    let mut stage_values = HashSet::<usize>::default();
    for member_entity in fold_members.iter() {
        if !member_entities.insert(member_entity) {
            return Err(FoldInvalidReason::DuplicateMember(member_entity));
        }
        let Ok(member) = members.get(member_entity) else {
            return Err(FoldInvalidReason::MissingMember(member_entity));
        };
        if member.sequence() != sequence_entity {
            return Err(FoldInvalidReason::MismatchedSequence(member_entity));
        }
        stage_values.insert(member.stage.0);
    }

    let Some(maximum) = stage_values.iter().copied().max() else {
        return Ok(0);
    };
    let Some(stages) = maximum.checked_add(1) else {
        return Err(FoldInvalidReason::StageCountOverflow);
    };
    if stage_values.len() != stages {
        let mut sorted_stages = stage_values.into_iter().collect::<Vec<_>>();
        sorted_stages.sort_unstable();
        let missing = sorted_stages
            .iter()
            .copied()
            .enumerate()
            .find_map(|(expected, actual)| (expected != actual).then_some(expected))
            .unwrap_or(sorted_stages.len());
        return Err(FoldInvalidReason::MissingStage(FoldStage(missing)));
    }

    Ok(stages)
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;
    use std::time::Duration;

    use bevy_app::App;
    use bevy_ecs::entity::Entity;
    use bevy_ecs::prelude::AppTypeRegistry;
    use bevy_ecs::prelude::ReflectComponent;
    use bevy_ecs::world::World;
    use bevy_math::curve::easing::EaseFunction;
    use bevy_reflect::TypeRegistry;
    use bevy_time::Time;
    use bevy_time::Virtual;

    use super::FoldDiagnostic;
    use super::FoldDiagnostics;
    use super::FoldEndpoint;
    use super::FoldInvalidReason;
    use super::FoldMember;
    use super::FoldMembers;
    use super::FoldSequence;
    use super::FoldSequenceState;
    use super::FoldStage;
    use crate::FoldCommand;
    use crate::FoldCommandEvent;
    use crate::FoldDirection;
    use crate::FoldMotion;
    use crate::FoldPlugin;

    type StateSnapshot = (bool, usize, f32, usize, FoldDirection, FoldMotion);

    fn fold_app() -> App {
        let mut app = App::new();
        app.insert_resource(Time::<Virtual>::default())
            .add_plugins(FoldPlugin);
        app
    }

    fn spawn_member(app: &mut App, sequence: Entity, stage: usize) -> Entity {
        app.world_mut()
            .spawn(FoldMember::new(sequence, FoldStage(stage)))
            .id()
    }

    fn state_snapshot(world: &World, sequence: Entity) -> Option<StateSnapshot> {
        world.get::<FoldSequenceState>(sequence).map(|state| {
            (
                state.is_ready(),
                state.stage_count(),
                state.position(),
                state.target(),
                state.direction(),
                state.motion(),
            )
        })
    }

    fn member_entities(world: &World, sequence: Entity) -> Vec<Entity> {
        world
            .get::<FoldMembers>(sequence)
            .map(FoldMembers::iter)
            .map(Iterator::collect)
            .unwrap_or_default()
    }

    fn diagnostics(world: &World) -> Vec<FoldDiagnostic> {
        world
            .resource::<FoldDiagnostics>()
            .entries()
            .copied()
            .collect()
    }

    #[test]
    fn sequence_before_members_revalidates_from_empty_to_contiguous() {
        let mut app = fold_app();
        let sequence = app.world_mut().spawn(FoldSequence::new(1.0)).id();

        app.update();

        assert_eq!(
            state_snapshot(app.world(), sequence),
            Some((true, 0, 0.0, 0, FoldDirection::Folding, FoldMotion::Idle,))
        );

        let first = spawn_member(&mut app, sequence, 0);
        let second = spawn_member(&mut app, sequence, 1);
        app.update();

        assert_eq!(member_entities(app.world(), sequence), vec![first, second]);
        assert_eq!(
            state_snapshot(app.world(), sequence),
            Some((true, 2, 0.0, 0, FoldDirection::Folding, FoldMotion::Idle,))
        );
    }

    #[test]
    fn members_before_sequence_initialize_when_configuration_arrives() {
        let mut app = fold_app();
        let sequence = app.world_mut().spawn_empty().id();
        let first = spawn_member(&mut app, sequence, 0);
        let second = spawn_member(&mut app, sequence, 1);
        app.update();

        assert_eq!(state_snapshot(app.world(), sequence), None);
        assert_eq!(member_entities(app.world(), sequence), vec![first, second]);

        app.world_mut()
            .entity_mut(sequence)
            .insert(FoldSequence::new(1.0));
        app.update();

        assert_eq!(
            state_snapshot(app.world(), sequence),
            Some((true, 2, 0.0, 0, FoldDirection::Folding, FoldMotion::Idle,))
        );
    }

    #[test]
    fn grouped_stages_accept_multiple_distinct_members() {
        let mut app = fold_app();
        let sequence = app.world_mut().spawn(FoldSequence::new(1.0)).id();
        spawn_member(&mut app, sequence, 0);
        spawn_member(&mut app, sequence, 0);
        spawn_member(&mut app, sequence, 1);

        app.update();

        assert_eq!(
            state_snapshot(app.world(), sequence).map(|state| (state.0, state.1)),
            Some((true, 2))
        );
        assert!(diagnostics(app.world()).is_empty());
    }

    #[test]
    fn grouped_stage_fractions_share_authored_easing() {
        let mut app = fold_app();
        let mut authored = FoldSequence::new(1.0);
        authored.easing = EaseFunction::QuadraticIn;
        let sequence = app.world_mut().spawn(authored).id();
        let first = spawn_member(&mut app, sequence, 0);
        let second = spawn_member(&mut app, sequence, 0);
        let later = spawn_member(&mut app, sequence, 1);
        app.update();

        app.world_mut().trigger(FoldCommandEvent::new(
            sequence,
            FoldCommand::Step(FoldDirection::Folding),
        ));
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .advance_by(Duration::from_secs_f32(0.5));
        app.update();

        let fractions = [first, second, later].map(|member_entity| {
            app.world()
                .get::<FoldMember>(member_entity)
                .and_then(|member| {
                    app.world()
                        .get::<FoldSequenceState>(sequence)
                        .map(|state| state.fraction(member.stage))
                })
        });
        assert_eq!(fractions, [Some(0.25), Some(0.25), Some(0.0)]);
    }

    #[test]
    fn stage_gap_disables_readiness_and_diagnoses_once_per_revision() {
        let mut app = fold_app();
        let sequence = app.world_mut().spawn(FoldSequence::new(1.0)).id();
        spawn_member(&mut app, sequence, 0);
        spawn_member(&mut app, sequence, 2);

        app.update();

        assert_eq!(
            state_snapshot(app.world(), sequence),
            Some((false, 0, 0.0, 0, FoldDirection::Folding, FoldMotion::Idle,))
        );
        assert_eq!(
            diagnostics(app.world())
                .as_slice()
                .first()
                .map(|diagnostic| diagnostic.reason),
            Some(FoldInvalidReason::MissingStage(FoldStage(1)))
        );

        app.update();

        assert_eq!(diagnostics(app.world()).len(), 1);
    }

    #[test]
    fn duplicate_reverse_member_disables_readiness() {
        let mut app = fold_app();
        let member = app.world_mut().spawn_empty().id();
        let sequence = app
            .world_mut()
            .spawn((FoldSequence::new(1.0), FoldMembers(vec![member, member])))
            .id();
        app.world_mut()
            .entity_mut(member)
            .insert(FoldMember::new(sequence, FoldStage(0)));

        app.update();

        assert_eq!(
            state_snapshot(app.world(), sequence).map(|state| state.0),
            Some(false)
        );
        assert_eq!(
            diagnostics(app.world())
                .as_slice()
                .first()
                .map(|diagnostic| diagnostic.reason),
            Some(FoldInvalidReason::DuplicateMember(member))
        );
    }

    #[test]
    fn folded_initialization_uses_derived_terminal_boundary() {
        let mut app = fold_app();
        let sequence = app.world_mut().spawn_empty().id();
        spawn_member(&mut app, sequence, 0);
        spawn_member(&mut app, sequence, 1);
        app.world_mut()
            .entity_mut(sequence)
            .insert(FoldSequence::new(1.0).with_initial(FoldEndpoint::Folded));

        app.update();

        assert_eq!(
            state_snapshot(app.world(), sequence),
            Some((true, 2, 2.0, 2, FoldDirection::Unfolding, FoldMotion::Idle,))
        );
    }

    #[test]
    fn later_growth_preserves_folded_numeric_position_and_target() {
        let mut app = fold_app();
        let sequence = app
            .world_mut()
            .spawn(FoldSequence::new(1.0).with_initial(FoldEndpoint::Folded))
            .id();
        spawn_member(&mut app, sequence, 0);
        app.update();

        assert_eq!(
            state_snapshot(app.world(), sequence).map(|state| (state.1, state.2, state.3)),
            Some((1, 1.0, 1))
        );

        spawn_member(&mut app, sequence, 1);
        app.update();

        assert_eq!(
            state_snapshot(app.world(), sequence).map(|state| (state.1, state.2, state.3)),
            Some((2, 1.0, 1))
        );
    }

    #[test]
    fn removal_clamps_position_and_target_to_lower_terminal_boundary() {
        let mut app = fold_app();
        let sequence = app
            .world_mut()
            .spawn(FoldSequence::new(1.0).with_initial(FoldEndpoint::Folded))
            .id();
        spawn_member(&mut app, sequence, 0);
        spawn_member(&mut app, sequence, 1);
        let last = spawn_member(&mut app, sequence, 2);
        app.update();

        assert_eq!(
            state_snapshot(app.world(), sequence).map(|state| (state.1, state.2, state.3)),
            Some((3, 3.0, 3))
        );

        app.world_mut().entity_mut(last).remove::<FoldMember>();
        app.update();

        assert_eq!(
            state_snapshot(app.world(), sequence).map(|state| (state.1, state.2, state.3)),
            Some((2, 2.0, 2))
        );
    }

    #[test]
    fn relationship_replacement_and_removal_revalidate_both_sequences() {
        let mut app = fold_app();
        let first_sequence = app.world_mut().spawn(FoldSequence::new(1.0)).id();
        let second_sequence = app.world_mut().spawn(FoldSequence::new(1.0)).id();
        let member = spawn_member(&mut app, first_sequence, 0);
        app.update();

        app.world_mut()
            .entity_mut(member)
            .insert(FoldMember::new(second_sequence, FoldStage(0)));
        app.update();

        assert!(member_entities(app.world(), first_sequence).is_empty());
        assert_eq!(member_entities(app.world(), second_sequence), vec![member]);
        assert_eq!(
            state_snapshot(app.world(), first_sequence).map(|state| state.1),
            Some(0)
        );
        assert_eq!(
            state_snapshot(app.world(), second_sequence).map(|state| state.1),
            Some(1)
        );

        app.world_mut().entity_mut(member).remove::<FoldMember>();
        app.update();

        assert!(member_entities(app.world(), second_sequence).is_empty());
        assert_eq!(
            state_snapshot(app.world(), second_sequence).map(|state| state.1),
            Some(0)
        );
    }

    #[test]
    fn invalid_configuration_mutations_create_distinct_diagnostics() {
        let mut app = fold_app();
        let sequence = app.world_mut().spawn(FoldSequence::new(f32::NAN)).id();

        app.update();

        assert_eq!(diagnostics(app.world()).len(), 1);
        assert_eq!(
            diagnostics(app.world())
                .as_slice()
                .first()
                .map(|diagnostic| diagnostic.reason),
            Some(FoldInvalidReason::NonFiniteStepSeconds)
        );

        if let Some(mut authored) = app
            .world_mut()
            .entity_mut(sequence)
            .get_mut::<FoldSequence>()
        {
            authored.step_seconds = 0.0;
        }
        app.update();

        assert_eq!(diagnostics(app.world()).len(), 2);
        assert_eq!(
            diagnostics(app.world())
                .as_slice()
                .get(1)
                .map(|diagnostic| diagnostic.reason),
            Some(FoldInvalidReason::NonPositiveStepSeconds)
        );

        if let Some(mut authored) = app
            .world_mut()
            .entity_mut(sequence)
            .get_mut::<FoldSequence>()
        {
            authored.step_seconds = 1.0;
        }
        app.update();

        assert_eq!(
            state_snapshot(app.world(), sequence).map(|state| state.0),
            Some(true)
        );
        assert_eq!(diagnostics(app.world()).len(), 2);
    }

    #[test]
    fn removing_sequence_configuration_removes_runtime_state() {
        let mut app = fold_app();
        let sequence = app.world_mut().spawn(FoldSequence::new(1.0)).id();
        app.update();
        assert!(state_snapshot(app.world(), sequence).is_some());

        app.world_mut()
            .entity_mut(sequence)
            .remove::<FoldSequence>();

        assert_eq!(state_snapshot(app.world(), sequence), None);
    }

    #[test]
    fn fold_components_are_available_to_reflection() {
        let app = fold_app();
        {
            let registry = app.world().resource::<AppTypeRegistry>();
            let mut registry = registry.write();
            registry.register::<FoldSequence>();
            registry.register::<FoldMember>();
            registry.register::<FoldMembers>();
            registry.register::<FoldSequenceState>();
        }
        {
            let registry = app.world().resource::<AppTypeRegistry>().read();

            assert!(registry.get(TypeId::of::<FoldSequence>()).is_some());
            assert!(registry.get(TypeId::of::<FoldMember>()).is_some());
            assert!(registry.get(TypeId::of::<FoldMembers>()).is_some());
            assert!(registry.get(TypeId::of::<FoldSequenceState>()).is_some());
            assert!(has_reflect_component::<FoldSequence>(&registry));
            assert!(!has_reflect_component::<FoldMember>(&registry));
            assert!(!has_reflect_component::<FoldMembers>(&registry));
            assert!(has_reflect_component::<FoldSequenceState>(&registry));
            drop(registry);
        }
    }

    fn has_reflect_component<T: 'static>(registry: &TypeRegistry) -> bool {
        registry
            .get(TypeId::of::<T>())
            .and_then(|registration| registration.data::<ReflectComponent>())
            .is_some()
    }
}
