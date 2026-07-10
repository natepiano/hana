use std::collections::VecDeque;

use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::Insert;
use bevy_ecs::prelude::Commands;
use bevy_ecs::prelude::Component;
use bevy_ecs::prelude::On;
use bevy_ecs::prelude::Query;
use bevy_ecs::prelude::ReflectComponent;
use bevy_ecs::prelude::Resource;
use bevy_ecs::prelude::World;
use bevy_platform::collections::HashSet;
use bevy_reflect::Reflect;

use super::FoldMember;
use super::FoldMembers;
use super::FoldSequence;
use super::FoldStage;
use crate::ArrangementMembers;
use crate::MemberIndex;

/// Builds explicit fold-stage relationships through deferred [`Commands`].
pub struct FoldSequenceBuilder<'commands, 'world, 'state> {
    commands: &'commands mut Commands<'world, 'state>,
    sequence: Entity,
    stages:   Vec<Vec<Entity>>,
}

impl<'commands, 'world, 'state> FoldSequenceBuilder<'commands, 'world, 'state> {
    /// Starts authoring membership for `sequence`.
    pub const fn new(commands: &'commands mut Commands<'world, 'state>, sequence: Entity) -> Self {
        Self {
            commands,
            sequence,
            stages: Vec::new(),
        }
    }

    /// Appends the next zero-based stage group.
    #[must_use]
    pub fn stage(mut self, members: impl IntoIterator<Item = Entity>) -> Self {
        self.stages.push(members.into_iter().collect());
        self
    }

    /// Validates every stage and queues the complete membership assignment.
    ///
    /// No relationship command is queued when validation fails.
    ///
    /// # Errors
    ///
    /// Returns [`FoldAuthorError::EmptyStage`] for an empty group or
    /// [`FoldAuthorError::DuplicateMember`] when an entity occurs twice.
    pub fn finish(self) -> Result<(), FoldAuthorError> {
        let mut authored_members = HashSet::<Entity>::default();
        for (stage, members) in self.stages.iter().enumerate() {
            if members.is_empty() {
                return Err(FoldAuthorError::EmptyStage(FoldStage(stage)));
            }
            for member in members {
                if !authored_members.insert(*member) {
                    return Err(FoldAuthorError::DuplicateMember(*member));
                }
            }
        }

        for (stage, members) in self.stages.into_iter().enumerate() {
            for member in members {
                self.commands
                    .entity(member)
                    .insert(FoldMember::new(self.sequence, FoldStage(stage)));
            }
        }
        Ok(())
    }
}

/// Invalid explicit input supplied to [`FoldSequenceBuilder`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FoldAuthorError {
    /// One authored stage contains no members.
    EmptyStage(FoldStage),
    /// One entity occurs in more than one authored position.
    DuplicateMember(Entity),
}

/// One-time request to replace a sequence's membership from an arrangement.
#[derive(Component, Clone, Copy, Debug, Eq, PartialEq, Reflect)]
#[reflect(Component, PartialEq, Debug, Clone)]
pub struct FoldFromArrangement {
    /// Arrangement whose insertion-ordered members will become fold stages.
    #[entities]
    pub arrangement: Entity,
}

impl FoldFromArrangement {
    /// Creates a snapshot request for `arrangement`.
    #[must_use]
    pub const fn new(arrangement: Entity) -> Self { Self { arrangement } }
}

/// Reason an arrangement snapshot request could not run yet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FoldSnapshotInvalidReason {
    /// The request entity does not carry [`FoldSequence`].
    MissingSequence,
    /// The requested arrangement entity does not exist.
    MissingArrangement,
    /// The arrangement does not yet carry [`ArrangementMembers`].
    MissingArrangementMembers,
    /// One listed arrangement member does not yet carry [`MemberIndex`].
    MissingMemberIndex(Entity),
}

/// First readiness failure emitted for one arrangement snapshot request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FoldSnapshotDiagnostic {
    /// Sequence entity carrying the retained request.
    pub sequence:    Entity,
    /// Arrangement requested by the sequence.
    pub arrangement: Entity,
    /// First observed readiness failure for this request.
    pub reason:      FoldSnapshotInvalidReason,
}

/// Bounded history containing one readiness failure per retained request.
#[derive(Debug, Resource)]
pub struct FoldSnapshotDiagnostics {
    entries:  VecDeque<FoldSnapshotDiagnostic>,
    capacity: usize,
}

impl FoldSnapshotDiagnostics {
    /// Default number of diagnostic entries retained in insertion order.
    pub const DEFAULT_CAPACITY: usize = 128;

    /// Iterates over retained snapshot diagnostics in insertion order.
    pub fn entries(&self) -> impl Iterator<Item = &FoldSnapshotDiagnostic> { self.entries.iter() }

    /// Number of retained snapshot diagnostics.
    #[must_use]
    pub fn len(&self) -> usize { self.entries.len() }

    /// Whether no snapshot diagnostic has been retained.
    #[must_use]
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }

    fn record(&mut self, diagnostic: FoldSnapshotDiagnostic) {
        tracing::warn!(
            sequence = ?diagnostic.sequence,
            arrangement = ?diagnostic.arrangement,
            reason = ?diagnostic.reason,
            "fold arrangement snapshot is not ready"
        );
        self.entries.push_back(diagnostic);
        while self.entries.len() > self.capacity {
            self.entries.pop_front();
        }
    }
}

impl Default for FoldSnapshotDiagnostics {
    fn default() -> Self {
        Self {
            entries:  VecDeque::new(),
            capacity: Self::DEFAULT_CAPACITY,
        }
    }
}

#[derive(Component, Clone, Copy, Debug, Default)]
pub(super) struct FoldSnapshotDiagnosticEmitted;

pub(super) fn on_fold_from_arrangement_inserted(
    inserted: On<Insert, FoldFromArrangement>,
    mut commands: Commands,
) {
    commands
        .entity(inserted.entity)
        .remove::<FoldSnapshotDiagnosticEmitted>();
}

pub(super) fn snapshot_fold_arrangements(
    mut commands: Commands,
    requests: Query<(Entity, &FoldFromArrangement)>,
) {
    for (sequence, request) in &requests {
        let request = *request;
        commands.queue(move |world: &mut World| apply_snapshot(world, sequence, request));
    }
}

fn snapshot_members(
    world: &World,
    request: FoldFromArrangement,
) -> Result<Vec<Entity>, FoldSnapshotInvalidReason> {
    if !world.entities().contains(request.arrangement) {
        return Err(FoldSnapshotInvalidReason::MissingArrangement);
    }
    let Some(arrangement_members) = world.get::<ArrangementMembers>(request.arrangement) else {
        return Err(FoldSnapshotInvalidReason::MissingArrangementMembers);
    };
    let members = arrangement_members.iter().collect::<Vec<_>>();
    if let Some(member) = members
        .iter()
        .copied()
        .find(|member| world.get::<MemberIndex>(*member).is_none())
    {
        return Err(FoldSnapshotInvalidReason::MissingMemberIndex(member));
    }
    Ok(members)
}

fn apply_snapshot(world: &mut World, sequence: Entity, request: FoldFromArrangement) {
    if world.get::<FoldFromArrangement>(sequence) != Some(&request) {
        return;
    }
    if world.get::<FoldSequence>(sequence).is_none() {
        record_snapshot_failure(
            world,
            sequence,
            request,
            FoldSnapshotInvalidReason::MissingSequence,
        );
        return;
    }
    let members = match snapshot_members(world, request) {
        Ok(members) => members,
        Err(reason) => {
            record_snapshot_failure(world, sequence, request, reason);
            return;
        },
    };
    let snapshot = members.iter().copied().collect::<HashSet<_>>();
    let existing = world
        .get::<FoldMembers>(sequence)
        .map(FoldMembers::iter)
        .map(Iterator::collect::<Vec<_>>);
    if let Some(existing) = existing {
        for member in existing
            .iter()
            .copied()
            .filter(|member| !snapshot.contains(member))
        {
            world.entity_mut(member).remove::<FoldMember>();
        }
    }
    for (stage, member) in members.iter().copied().enumerate() {
        world
            .entity_mut(member)
            .insert(FoldMember::new(sequence, FoldStage(stage)));
    }
    world
        .entity_mut(sequence)
        .remove::<(FoldFromArrangement, FoldSnapshotDiagnosticEmitted)>();
    tracing::debug!(
        ?sequence,
        arrangement = ?request.arrangement,
        stages = members.len(),
        "fold arrangement snapshot applied"
    );
}

fn record_snapshot_failure(
    world: &mut World,
    sequence: Entity,
    request: FoldFromArrangement,
    reason: FoldSnapshotInvalidReason,
) {
    if world
        .get::<FoldSnapshotDiagnosticEmitted>(sequence)
        .is_some()
    {
        return;
    }
    world
        .resource_mut::<FoldSnapshotDiagnostics>()
        .record(FoldSnapshotDiagnostic {
            sequence,
            arrangement: request.arrangement,
            reason,
        });
    world
        .entity_mut(sequence)
        .insert(FoldSnapshotDiagnosticEmitted);
}

#[cfg(test)]
mod tests {
    use bevy_app::App;
    use bevy_app::Update;
    use bevy_ecs::entity::Entity;
    use bevy_ecs::prelude::World;
    use bevy_ecs::schedule::ApplyDeferred;
    use bevy_ecs::schedule::IntoScheduleConfigs;
    use bevy_ecs::schedule::Schedule;
    use bevy_time::Time;
    use bevy_time::Virtual;

    use super::FoldAuthorError;
    use super::FoldFromArrangement;
    use super::FoldSequenceBuilder;
    use super::FoldSnapshotDiagnostics;
    use super::FoldSnapshotInvalidReason;
    use crate::Accordion;
    use crate::FoldEndpoint;
    use crate::FoldMember;
    use crate::FoldMembers;
    use crate::FoldPlugin;
    use crate::FoldSequence;
    use crate::FoldSequenceState;
    use crate::FoldStage;
    use crate::Member;
    use crate::MemberIndex;
    use crate::assign_member_indices;
    use crate::on_member_added;
    use crate::on_member_removed;

    const STEP_SECONDS: f32 = 1.0;

    fn fold_app() -> App {
        let mut app = App::new();
        app.insert_resource(Time::<Virtual>::default())
            .add_plugins(FoldPlugin);
        app
    }

    fn arrangement_app() -> App {
        let mut app = fold_app();
        app.add_observer(on_member_added)
            .add_observer(on_member_removed)
            .add_systems(Update, assign_member_indices);
        app
    }

    fn spawn_arrangement(app: &mut App, count: usize) -> (Entity, Vec<Entity>) {
        let arrangement = app.world_mut().spawn(Accordion::default()).id();
        let members = (0..count)
            .map(|_| app.world_mut().spawn(Member { arrangement }).id())
            .collect();
        (arrangement, members)
    }

    fn member_stages(world: &World, members: &[Entity]) -> Vec<Option<FoldStage>> {
        members
            .iter()
            .map(|member| world.get::<FoldMember>(*member).map(|fold| fold.stage))
            .collect()
    }

    fn sequence_members(world: &World, sequence: Entity) -> Vec<Entity> {
        world
            .get::<FoldMembers>(sequence)
            .map(FoldMembers::iter)
            .map(Iterator::collect)
            .unwrap_or_default()
    }

    #[test]
    fn builder_authors_grouped_lid_and_walls_with_consecutive_stages() {
        let mut world = World::new();
        let sequence = world.spawn_empty().id();
        let [lid, north, south, east, west, later] =
            core::array::from_fn(|_| world.spawn_empty().id());

        let result = FoldSequenceBuilder::new(&mut world.commands(), sequence)
            .stage([lid])
            .stage([north, south, east, west])
            .stage([later])
            .finish();
        world.flush();

        assert_eq!(result, Ok(()));
        assert_eq!(
            member_stages(&world, &[lid, north, south, east, west, later]),
            vec![
                Some(FoldStage(0)),
                Some(FoldStage(1)),
                Some(FoldStage(1)),
                Some(FoldStage(1)),
                Some(FoldStage(1)),
                Some(FoldStage(2)),
            ]
        );
    }

    #[test]
    fn builder_errors_queue_no_partial_membership_writes() {
        let mut world = World::new();
        let sequence = world.spawn_empty().id();
        let first = world.spawn_empty().id();
        let second = world.spawn_empty().id();

        let empty_result = FoldSequenceBuilder::new(&mut world.commands(), sequence)
            .stage([first])
            .stage([])
            .finish();
        world.flush();

        assert_eq!(empty_result, Err(FoldAuthorError::EmptyStage(FoldStage(1))));
        assert_eq!(member_stages(&world, &[first, second]), vec![None, None]);

        let duplicate_result = FoldSequenceBuilder::new(&mut world.commands(), sequence)
            .stage([first, second])
            .stage([first])
            .finish();
        world.flush();

        assert_eq!(
            duplicate_result,
            Err(FoldAuthorError::DuplicateMember(first))
        );
        assert_eq!(member_stages(&world, &[first, second]), vec![None, None]);
    }

    #[test]
    fn snapshot_waits_for_assignment_and_reports_each_request_once() {
        let mut app = fold_app();
        let (arrangement, members) = spawn_arrangement(&mut app, 2);
        let sequence = app
            .world_mut()
            .spawn((
                FoldSequence::new(STEP_SECONDS),
                FoldFromArrangement::new(arrangement),
            ))
            .id();

        app.update();
        app.update();

        assert!(app.world().get::<FoldFromArrangement>(sequence).is_some());
        assert_eq!(member_stages(app.world(), &members), vec![None, None]);
        let diagnostics = app.world().resource::<FoldSnapshotDiagnostics>();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.entries().next().map(|entry| entry.reason),
            Some(FoldSnapshotInvalidReason::MissingArrangementMembers)
        );

        app.add_systems(Update, assign_member_indices);
        app.update();

        assert!(app.world().get::<FoldFromArrangement>(sequence).is_none());
        assert_eq!(
            member_stages(app.world(), &members),
            vec![Some(FoldStage(0)), Some(FoldStage(1))]
        );
    }

    #[test]
    fn snapshot_retries_while_a_listed_member_lacks_an_index() {
        let mut app = fold_app();
        let (arrangement, members) = spawn_arrangement(&mut app, 2);
        let mut assignment = Schedule::default();
        assignment.add_systems((assign_member_indices, ApplyDeferred).chain());
        assignment.run(app.world_mut());
        let pending = members[1];
        let assigned_index = app.world().get::<MemberIndex>(pending).copied();
        app.world_mut().entity_mut(pending).remove::<MemberIndex>();
        let sequence = app
            .world_mut()
            .spawn((
                FoldSequence::new(STEP_SECONDS),
                FoldFromArrangement::new(arrangement),
            ))
            .id();

        app.update();

        assert!(app.world().get::<FoldFromArrangement>(sequence).is_some());
        assert_eq!(member_stages(app.world(), &members), vec![None, None]);
        assert_eq!(
            app.world()
                .resource::<FoldSnapshotDiagnostics>()
                .entries()
                .next()
                .map(|entry| entry.reason),
            Some(FoldSnapshotInvalidReason::MissingMemberIndex(pending))
        );

        if let Some(member_index) = assigned_index {
            app.world_mut().entity_mut(pending).insert(member_index);
        }
        app.update();

        assert!(app.world().get::<FoldFromArrangement>(sequence).is_none());
        assert_eq!(
            member_stages(app.world(), &members),
            vec![Some(FoldStage(0)), Some(FoldStage(1))]
        );
    }

    #[test]
    fn failed_requests_for_missing_sequence_and_arrangement_are_retained() {
        let mut app = fold_app();
        let missing_arrangement = app.world_mut().spawn_empty().id();
        app.world_mut().despawn(missing_arrangement);
        let missing_sequence = app
            .world_mut()
            .spawn(FoldFromArrangement::new(Entity::PLACEHOLDER))
            .id();
        let sequence = app
            .world_mut()
            .spawn((
                FoldSequence::new(STEP_SECONDS),
                FoldFromArrangement::new(missing_arrangement),
            ))
            .id();

        app.update();

        assert!(
            app.world()
                .get::<FoldFromArrangement>(missing_sequence)
                .is_some()
        );
        assert!(app.world().get::<FoldFromArrangement>(sequence).is_some());
        let reasons = app
            .world()
            .resource::<FoldSnapshotDiagnostics>()
            .entries()
            .map(|entry| entry.reason)
            .collect::<Vec<_>>();
        assert!(reasons.contains(&FoldSnapshotInvalidReason::MissingSequence));
        assert!(reasons.contains(&FoldSnapshotInvalidReason::MissingArrangement));
    }

    #[test]
    fn snapshot_uses_insertion_order_and_fresh_stages_for_gapped_indexes() {
        let mut app = arrangement_app();
        let (arrangement, members) = spawn_arrangement(&mut app, 3);
        app.update();
        let removed = members[1];
        app.world_mut().entity_mut(removed).remove::<Member>();
        app.update();

        assert_eq!(
            app.world()
                .get::<MemberIndex>(members[2])
                .map(|index| index.index),
            Some(3)
        );
        let sequence = app
            .world_mut()
            .spawn((
                FoldSequence::new(STEP_SECONDS),
                FoldFromArrangement::new(arrangement),
            ))
            .id();
        app.update();

        assert_eq!(
            sequence_members(app.world(), sequence),
            vec![members[0], members[2]]
        );
        assert_eq!(
            member_stages(app.world(), &[members[0], members[2]]),
            vec![Some(FoldStage(0)), Some(FoldStage(1))]
        );
    }

    #[test]
    fn folded_initialization_uses_completed_snapshot_terminal_boundary() {
        let mut app = arrangement_app();
        let (arrangement, _) = spawn_arrangement(&mut app, 3);
        app.update();
        let sequence = app
            .world_mut()
            .spawn((
                FoldSequence::new(STEP_SECONDS).with_initial(FoldEndpoint::Folded),
                FoldFromArrangement::new(arrangement),
            ))
            .id();

        app.update();

        let state = app.world().get::<FoldSequenceState>(sequence);
        assert_eq!(state.map(FoldSequenceState::stage_count), Some(3));
        assert_eq!(state.map(FoldSequenceState::position), Some(3.0));
        assert!(app.world().get::<FoldFromArrangement>(sequence).is_none());
    }

    #[test]
    fn pending_resnapshot_retains_prior_valid_state_until_reconciliation() {
        let mut app = arrangement_app();
        let (first_arrangement, first_members) = spawn_arrangement(&mut app, 2);
        app.update();
        let sequence = app
            .world_mut()
            .spawn((
                FoldSequence::new(STEP_SECONDS),
                FoldFromArrangement::new(first_arrangement),
            ))
            .id();
        app.update();

        let pending_arrangement = app.world_mut().spawn(Accordion::default()).id();
        app.world_mut()
            .entity_mut(sequence)
            .insert(FoldFromArrangement::new(pending_arrangement));
        app.update();

        assert_eq!(
            app.world()
                .get::<FoldSequenceState>(sequence)
                .map(FoldSequenceState::stage_count),
            Some(2)
        );
        assert_eq!(sequence_members(app.world(), sequence), first_members);
        assert!(app.world().get::<FoldFromArrangement>(sequence).is_some());
    }

    #[test]
    fn resnapshot_removes_absent_members_and_replaces_reordered_stages() {
        let mut app = arrangement_app();
        let (arrangement, members) = spawn_arrangement(&mut app, 3);
        app.update();
        let sequence = app
            .world_mut()
            .spawn((
                FoldSequence::new(STEP_SECONDS),
                FoldFromArrangement::new(arrangement),
            ))
            .id();
        app.update();

        app.world_mut().entity_mut(members[0]).remove::<Member>();
        app.world_mut().entity_mut(members[1]).remove::<Member>();
        app.world_mut()
            .entity_mut(members[1])
            .insert(Member { arrangement });
        app.update();
        app.world_mut()
            .entity_mut(sequence)
            .insert(FoldFromArrangement::new(arrangement));
        app.update();

        assert_eq!(
            sequence_members(app.world(), sequence),
            vec![members[2], members[1]]
        );
        assert_eq!(app.world().get::<FoldMember>(members[0]), None);
        assert_eq!(
            member_stages(app.world(), &[members[2], members[1]]),
            vec![Some(FoldStage(0)), Some(FoldStage(1))]
        );
    }

    #[test]
    fn arrangement_mutation_requires_an_explicit_resnapshot() {
        let mut app = arrangement_app();
        let (arrangement, members) = spawn_arrangement(&mut app, 2);
        app.update();
        let sequence = app
            .world_mut()
            .spawn((
                FoldSequence::new(STEP_SECONDS),
                FoldFromArrangement::new(arrangement),
            ))
            .id();
        app.update();

        let added = app.world_mut().spawn(Member { arrangement }).id();
        app.update();

        assert_eq!(sequence_members(app.world(), sequence), members);
        assert_eq!(app.world().get::<FoldMember>(added), None);

        app.world_mut()
            .entity_mut(sequence)
            .insert(FoldFromArrangement::new(arrangement));
        app.update();

        assert_eq!(
            sequence_members(app.world(), sequence),
            vec![members[0], members[1], added]
        );
        assert_eq!(
            app.world()
                .get::<FoldMember>(added)
                .map(|member| member.stage),
            Some(FoldStage(2))
        );
    }

    #[test]
    fn explicit_and_snapshot_authoring_produce_equivalent_membership() {
        let mut app = arrangement_app();
        let (arrangement, members) = spawn_arrangement(&mut app, 3);
        app.update();
        let automatic = app
            .world_mut()
            .spawn((
                FoldSequence::new(STEP_SECONDS),
                FoldFromArrangement::new(arrangement),
            ))
            .id();
        app.update();
        let automatic_stages = member_stages(app.world(), &members);

        let manual = app.world_mut().spawn(FoldSequence::new(STEP_SECONDS)).id();
        let result = FoldSequenceBuilder::new(&mut app.world_mut().commands(), manual)
            .stage([members[0]])
            .stage([members[1]])
            .stage([members[2]])
            .finish();
        app.world_mut().flush();

        assert_eq!(result, Ok(()));
        assert_eq!(member_stages(app.world(), &members), automatic_stages);
        assert!(members.iter().all(|member| {
            app.world()
                .get::<FoldMember>(*member)
                .is_some_and(|fold_member| fold_member.sequence() == manual)
        }));
        assert_ne!(automatic, manual);
    }
}
