use std::cmp::Ordering;

use bevy_ecs::entity::Entity;
use bevy_ecs::event::EntityEvent;
use bevy_ecs::prelude::On;
use bevy_ecs::prelude::Query;
use bevy_ecs::prelude::Res;
use bevy_kana::ToF32;
use bevy_kana::ToUsize;
use bevy_reflect::Reflect;
use bevy_time::Time;
use bevy_time::Virtual;

use super::FoldEndpoint;
use super::FoldSequence;
use super::FoldSequenceState;

/// Direction established by a step or selected for terminal playback.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Reflect)]
#[reflect(PartialEq, Debug, Clone)]
pub enum FoldDirection {
    /// Moves from boundary zero toward the terminal folded boundary.
    Folding,
    /// Moves from the terminal folded boundary toward boundary zero.
    Unfolding,
}

/// Current kind of fold playback.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Reflect)]
#[reflect(PartialEq, Debug, Clone)]
pub enum FoldMotion {
    /// No fold playback is active.
    Idle,
    /// Playback is moving to one or more queued stage boundaries.
    Step,
    /// Playback is moving to a terminal boundary and can reverse without stopping.
    Play,
}

/// Requested fold playback operation.
#[derive(Clone, Copy, Debug, Reflect)]
#[reflect(Debug, Clone)]
pub enum FoldCommand {
    /// Moves one stage boundary in the requested direction.
    Step(FoldDirection),
    /// Plays toward a terminal boundary, or reverses active terminal playback.
    ///
    /// At an endpoint, playback selects the opposite endpoint. At an idle
    /// interior position, it continues the latest step direction. During a
    /// step it promotes that direction, and during playback it reverses without
    /// changing the continuous position.
    Play,
}

/// Entity-targeted transport for a [`FoldCommand`].
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct FoldCommandEvent {
    #[event_target]
    sequence_entity: Entity,
    /// Playback operation requested for the sequence entity.
    pub command:     FoldCommand,
}

impl FoldCommandEvent {
    /// Creates a command event targeting `sequence_entity`.
    #[must_use]
    pub const fn new(sequence_entity: Entity, command: FoldCommand) -> Self {
        Self {
            sequence_entity,
            command,
        }
    }

    /// Sequence entity targeted by this event.
    #[must_use]
    pub const fn sequence_entity(&self) -> Entity { self.sequence_entity }
}

#[derive(Clone, Copy, Debug, Reflect)]
pub(super) struct FoldPlayback {
    position:  f32,
    target:    usize,
    direction: FoldDirection,
    motion:    FoldMotion,
}

impl FoldPlayback {
    pub(super) fn initial(endpoint: FoldEndpoint, stages: usize) -> Self {
        match endpoint {
            FoldEndpoint::Unfolded => Self {
                position:  0.0,
                target:    0,
                direction: FoldDirection::Folding,
                motion:    FoldMotion::Idle,
            },
            FoldEndpoint::Folded => Self {
                position:  stages.to_f32(),
                target:    stages,
                direction: FoldDirection::Unfolding,
                motion:    FoldMotion::Idle,
            },
        }
    }

    pub(super) const fn uninitialized() -> Self {
        Self {
            position:  0.0,
            target:    0,
            direction: FoldDirection::Folding,
            motion:    FoldMotion::Idle,
        }
    }

    pub(super) fn clamp_to(&mut self, stages: usize) {
        self.position = self.position.min(stages.to_f32());
        self.target = self.target.min(stages);
    }

    pub(super) const fn stop(&mut self) { self.motion = FoldMotion::Idle; }

    pub(super) fn apply(&mut self, command: FoldCommand, stages: usize) {
        match command {
            FoldCommand::Step(direction) => self.step(direction, stages),
            FoldCommand::Play => self.play(stages),
        }
    }

    pub(super) fn advance(&mut self, stage_delta: f32) {
        if self.motion == FoldMotion::Idle {
            return;
        }

        let target = self.target.to_f32();
        self.position = match self.position.total_cmp(&target) {
            Ordering::Less => (self.position + stage_delta).min(target),
            Ordering::Equal => target,
            Ordering::Greater => (self.position - stage_delta).max(target),
        };
        if self.position.total_cmp(&target).is_eq() {
            self.motion = FoldMotion::Idle;
        }
    }

    pub(super) const fn position(&self) -> f32 { self.position }

    pub(super) const fn target(&self) -> usize { self.target }

    pub(super) const fn direction(&self) -> FoldDirection { self.direction }

    pub(super) const fn motion(&self) -> FoldMotion { self.motion }

    fn step(&mut self, direction: FoldDirection, stages: usize) {
        let same_direction = self.direction == direction;
        self.direction = direction;

        if self.at_terminal(direction, stages) {
            self.target = match direction {
                FoldDirection::Folding => stages,
                FoldDirection::Unfolding => 0,
            };
            self.motion = FoldMotion::Idle;
            return;
        }

        self.target = match (same_direction, self.motion, direction) {
            (true, FoldMotion::Step, FoldDirection::Folding) => {
                self.target.saturating_add(1).min(stages)
            },
            (true, FoldMotion::Step, FoldDirection::Unfolding) => self.target.saturating_sub(1),
            (_, _, FoldDirection::Folding) => self
                .position
                .floor()
                .to_usize()
                .saturating_add(1)
                .min(stages),
            (_, _, FoldDirection::Unfolding) => self.position.ceil().to_usize().saturating_sub(1),
        };
        self.motion = FoldMotion::Step;
    }

    fn play(&mut self, stages: usize) {
        if stages == 0 {
            self.target = 0;
            self.motion = FoldMotion::Idle;
            return;
        }

        self.direction = match self.motion {
            FoldMotion::Play => match self.direction {
                FoldDirection::Folding => FoldDirection::Unfolding,
                FoldDirection::Unfolding => FoldDirection::Folding,
            },
            FoldMotion::Idle if self.at_terminal(FoldDirection::Unfolding, stages) => {
                FoldDirection::Folding
            },
            FoldMotion::Idle if self.at_terminal(FoldDirection::Folding, stages) => {
                FoldDirection::Unfolding
            },
            FoldMotion::Idle | FoldMotion::Step => self.direction,
        };
        self.target = match self.direction {
            FoldDirection::Folding => stages,
            FoldDirection::Unfolding => 0,
        };
        self.motion = FoldMotion::Play;
    }

    fn at_terminal(&self, direction: FoldDirection, stages: usize) -> bool {
        match direction {
            FoldDirection::Folding => self.position.total_cmp(&stages.to_f32()).is_eq(),
            FoldDirection::Unfolding => self.position.total_cmp(&0.0).is_eq(),
        }
    }
}

pub(super) fn on_fold_command(
    event: On<FoldCommandEvent>,
    mut sequences: Query<&mut FoldSequenceState>,
) {
    if let Ok(mut state) = sequences.get_mut(event.sequence_entity()) {
        state.apply(event.command);
    }
}

pub(super) fn advance_fold_sequences(
    time: Res<Time<Virtual>>,
    mut sequences: Query<(&FoldSequence, &mut FoldSequenceState)>,
) {
    for (sequence, mut state) in &mut sequences {
        if state.is_ready() {
            state.advance(time.delta_secs() / sequence.step_seconds);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy_app::App;
    use bevy_ecs::entity::Entity;
    use bevy_ecs::world::World;
    use bevy_time::Time;
    use bevy_time::Virtual;

    use super::FoldCommand;
    use super::FoldCommandEvent;
    use super::FoldDirection;
    use super::FoldMotion;
    use crate::FoldEndpoint;
    use crate::FoldMember;
    use crate::FoldPlugin;
    use crate::FoldSequence;
    use crate::FoldSequenceState;
    use crate::FoldStage;

    type StateSnapshot = (f32, usize, FoldDirection, FoldMotion);

    fn fold_app() -> App {
        let mut app = App::new();
        app.insert_resource(Time::<Virtual>::default())
            .add_plugins(FoldPlugin);
        app
    }

    fn spawn_sequence(
        app: &mut App,
        stages: usize,
        endpoint: FoldEndpoint,
        step_seconds: f32,
    ) -> Entity {
        let sequence = app
            .world_mut()
            .spawn(FoldSequence::new(step_seconds).with_initial(endpoint))
            .id();
        for stage in 0..stages {
            app.world_mut()
                .spawn(FoldMember::new(sequence, FoldStage(stage)));
        }
        app.update();
        sequence
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

    fn snapshot(world: &World, sequence: Entity) -> Option<StateSnapshot> {
        world.get::<FoldSequenceState>(sequence).map(|state| {
            (
                state.position(),
                state.target(),
                state.direction(),
                state.motion(),
            )
        })
    }

    #[test]
    fn command_event_exposes_its_target_without_mutability() {
        let sequence = Entity::PLACEHOLDER;
        let event = FoldCommandEvent::new(sequence, FoldCommand::Step(FoldDirection::Folding));

        assert_eq!(event.sequence_entity(), sequence);
        assert!(matches!(
            event.command,
            FoldCommand::Step(FoldDirection::Folding)
        ));
    }

    #[test]
    fn integer_and_fractional_steps_advance_in_both_directions() {
        let mut app = fold_app();
        let folding = spawn_sequence(&mut app, 3, FoldEndpoint::Unfolded, 1.0);
        let unfolding = spawn_sequence(&mut app, 3, FoldEndpoint::Folded, 1.0);

        trigger(&mut app, folding, FoldCommand::Step(FoldDirection::Folding));
        trigger(
            &mut app,
            unfolding,
            FoldCommand::Step(FoldDirection::Unfolding),
        );
        advance(&mut app, 0.25);

        assert_eq!(
            snapshot(app.world(), folding),
            Some((0.25, 1, FoldDirection::Folding, FoldMotion::Step))
        );
        assert_eq!(
            snapshot(app.world(), unfolding),
            Some((2.75, 2, FoldDirection::Unfolding, FoldMotion::Step))
        );

        advance(&mut app, 0.75);

        assert_eq!(
            snapshot(app.world(), folding),
            Some((1.0, 1, FoldDirection::Folding, FoldMotion::Idle))
        );
        assert_eq!(
            snapshot(app.world(), unfolding),
            Some((2.0, 2, FoldDirection::Unfolding, FoldMotion::Idle))
        );
    }

    #[test]
    fn same_direction_steps_extend_the_queued_target() {
        let mut app = fold_app();
        let sequence = spawn_sequence(&mut app, 4, FoldEndpoint::Unfolded, 1.0);

        trigger(
            &mut app,
            sequence,
            FoldCommand::Step(FoldDirection::Folding),
        );
        advance(&mut app, 0.25);
        trigger(
            &mut app,
            sequence,
            FoldCommand::Step(FoldDirection::Folding),
        );
        trigger(
            &mut app,
            sequence,
            FoldCommand::Step(FoldDirection::Folding),
        );

        assert_eq!(
            snapshot(app.world(), sequence),
            Some((0.25, 3, FoldDirection::Folding, FoldMotion::Step))
        );

        advance(&mut app, 2.75);

        assert_eq!(
            snapshot(app.world(), sequence),
            Some((3.0, 3, FoldDirection::Folding, FoldMotion::Idle))
        );
    }

    #[test]
    fn queued_unfolding_steps_extend_toward_zero() {
        let mut app = fold_app();
        let sequence = spawn_sequence(&mut app, 4, FoldEndpoint::Folded, 1.0);

        trigger(
            &mut app,
            sequence,
            FoldCommand::Step(FoldDirection::Unfolding),
        );
        advance(&mut app, 0.25);
        trigger(
            &mut app,
            sequence,
            FoldCommand::Step(FoldDirection::Unfolding),
        );
        trigger(
            &mut app,
            sequence,
            FoldCommand::Step(FoldDirection::Unfolding),
        );

        assert_eq!(
            snapshot(app.world(), sequence),
            Some((3.75, 1, FoldDirection::Unfolding, FoldMotion::Step))
        );
    }

    #[test]
    fn reversing_queued_steps_targets_the_adjacent_boundary_without_snapping() {
        let mut app = fold_app();
        let sequence = spawn_sequence(&mut app, 4, FoldEndpoint::Unfolded, 1.0);

        trigger(
            &mut app,
            sequence,
            FoldCommand::Step(FoldDirection::Folding),
        );
        trigger(
            &mut app,
            sequence,
            FoldCommand::Step(FoldDirection::Folding),
        );
        advance(&mut app, 0.25);
        trigger(
            &mut app,
            sequence,
            FoldCommand::Step(FoldDirection::Unfolding),
        );

        assert_eq!(
            snapshot(app.world(), sequence),
            Some((0.25, 0, FoldDirection::Unfolding, FoldMotion::Step))
        );

        advance(&mut app, 0.25);

        assert_eq!(
            snapshot(app.world(), sequence),
            Some((0.0, 0, FoldDirection::Unfolding, FoldMotion::Idle))
        );
    }

    #[test]
    fn reversing_play_targets_the_adjacent_boundary_in_both_directions() {
        let mut app = fold_app();
        let folding = spawn_sequence(&mut app, 4, FoldEndpoint::Unfolded, 1.0);
        let unfolding = spawn_sequence(&mut app, 4, FoldEndpoint::Folded, 1.0);

        trigger(&mut app, folding, FoldCommand::Play);
        trigger(&mut app, unfolding, FoldCommand::Play);
        advance(&mut app, 1.25);
        trigger(
            &mut app,
            folding,
            FoldCommand::Step(FoldDirection::Unfolding),
        );
        trigger(
            &mut app,
            unfolding,
            FoldCommand::Step(FoldDirection::Folding),
        );

        assert_eq!(
            snapshot(app.world(), folding),
            Some((1.25, 1, FoldDirection::Unfolding, FoldMotion::Step))
        );
        assert_eq!(
            snapshot(app.world(), unfolding),
            Some((2.75, 3, FoldDirection::Folding, FoldMotion::Step))
        );
    }

    #[test]
    fn same_direction_step_interrupts_play_at_the_adjacent_boundary() {
        let mut app = fold_app();
        let sequence = spawn_sequence(&mut app, 4, FoldEndpoint::Unfolded, 1.0);

        trigger(&mut app, sequence, FoldCommand::Play);
        advance(&mut app, 1.25);
        trigger(
            &mut app,
            sequence,
            FoldCommand::Step(FoldDirection::Folding),
        );

        assert_eq!(
            snapshot(app.world(), sequence),
            Some((1.25, 2, FoldDirection::Folding, FoldMotion::Step))
        );
    }

    #[test]
    fn reversal_at_an_exact_boundary_moves_to_the_neighbor_or_stays_terminal() {
        let mut app = fold_app();
        let sequence = spawn_sequence(&mut app, 3, FoldEndpoint::Unfolded, 1.0);

        trigger(
            &mut app,
            sequence,
            FoldCommand::Step(FoldDirection::Folding),
        );
        trigger(
            &mut app,
            sequence,
            FoldCommand::Step(FoldDirection::Unfolding),
        );
        assert_eq!(
            snapshot(app.world(), sequence),
            Some((0.0, 0, FoldDirection::Unfolding, FoldMotion::Idle))
        );

        trigger(
            &mut app,
            sequence,
            FoldCommand::Step(FoldDirection::Folding),
        );
        advance(&mut app, 1.0);
        trigger(
            &mut app,
            sequence,
            FoldCommand::Step(FoldDirection::Unfolding),
        );

        assert_eq!(
            snapshot(app.world(), sequence),
            Some((1.0, 0, FoldDirection::Unfolding, FoldMotion::Step))
        );
    }

    #[test]
    fn idle_play_moves_from_each_endpoint_to_the_other_endpoint() {
        let mut app = fold_app();
        let unfolded = spawn_sequence(&mut app, 3, FoldEndpoint::Unfolded, 1.0);
        let folded = spawn_sequence(&mut app, 3, FoldEndpoint::Folded, 1.0);

        trigger(
            &mut app,
            unfolded,
            FoldCommand::Step(FoldDirection::Unfolding),
        );
        trigger(&mut app, folded, FoldCommand::Step(FoldDirection::Folding));
        trigger(&mut app, unfolded, FoldCommand::Play);
        trigger(&mut app, folded, FoldCommand::Play);

        assert_eq!(
            snapshot(app.world(), unfolded),
            Some((0.0, 3, FoldDirection::Folding, FoldMotion::Play))
        );
        assert_eq!(
            snapshot(app.world(), folded),
            Some((3.0, 0, FoldDirection::Unfolding, FoldMotion::Play))
        );
    }

    #[test]
    fn repeated_play_reverses_fractional_playback_in_both_directions() {
        let mut app = fold_app();
        let folding = spawn_sequence(&mut app, 4, FoldEndpoint::Unfolded, 1.0);
        let unfolding = spawn_sequence(&mut app, 4, FoldEndpoint::Folded, 1.0);

        trigger(&mut app, folding, FoldCommand::Play);
        trigger(&mut app, unfolding, FoldCommand::Play);
        advance(&mut app, 0.75);
        trigger(&mut app, folding, FoldCommand::Play);
        trigger(&mut app, unfolding, FoldCommand::Play);

        assert_eq!(
            snapshot(app.world(), folding),
            Some((0.75, 0, FoldDirection::Unfolding, FoldMotion::Play))
        );
        assert_eq!(
            snapshot(app.world(), unfolding),
            Some((3.25, 4, FoldDirection::Folding, FoldMotion::Play))
        );
    }

    #[test]
    fn play_promotes_fractional_steps_in_both_directions() {
        let mut app = fold_app();
        let folding = spawn_sequence(&mut app, 4, FoldEndpoint::Unfolded, 1.0);
        let unfolding = spawn_sequence(&mut app, 4, FoldEndpoint::Folded, 1.0);

        trigger(&mut app, folding, FoldCommand::Step(FoldDirection::Folding));
        trigger(
            &mut app,
            unfolding,
            FoldCommand::Step(FoldDirection::Unfolding),
        );
        advance(&mut app, 0.25);
        trigger(&mut app, folding, FoldCommand::Play);
        trigger(&mut app, unfolding, FoldCommand::Play);

        assert_eq!(
            snapshot(app.world(), folding),
            Some((0.25, 4, FoldDirection::Folding, FoldMotion::Play))
        );
        assert_eq!(
            snapshot(app.world(), unfolding),
            Some((3.75, 0, FoldDirection::Unfolding, FoldMotion::Play))
        );
    }

    #[test]
    fn idle_interior_play_continues_the_last_step_direction() {
        let mut app = fold_app();
        let sequence = spawn_sequence(&mut app, 4, FoldEndpoint::Folded, 1.0);

        trigger(
            &mut app,
            sequence,
            FoldCommand::Step(FoldDirection::Unfolding),
        );
        advance(&mut app, 1.0);
        trigger(&mut app, sequence, FoldCommand::Play);

        assert_eq!(
            snapshot(app.world(), sequence),
            Some((3.0, 0, FoldDirection::Unfolding, FoldMotion::Play))
        );

        advance(&mut app, 3.0);
        trigger(
            &mut app,
            sequence,
            FoldCommand::Step(FoldDirection::Folding),
        );
        advance(&mut app, 1.0);
        trigger(&mut app, sequence, FoldCommand::Play);

        assert_eq!(
            snapshot(app.world(), sequence),
            Some((1.0, 4, FoldDirection::Folding, FoldMotion::Play))
        );
    }

    #[test]
    fn empty_and_not_ready_sequences_remain_idle() {
        let mut app = fold_app();
        let empty = spawn_sequence(&mut app, 0, FoldEndpoint::Unfolded, 1.0);
        let invalid = spawn_sequence(&mut app, 1, FoldEndpoint::Unfolded, 0.0);

        trigger(&mut app, empty, FoldCommand::Step(FoldDirection::Folding));
        trigger(&mut app, empty, FoldCommand::Play);
        trigger(&mut app, invalid, FoldCommand::Step(FoldDirection::Folding));
        trigger(&mut app, invalid, FoldCommand::Play);
        advance(&mut app, 10.0);

        assert_eq!(
            snapshot(app.world(), empty),
            Some((0.0, 0, FoldDirection::Folding, FoldMotion::Idle))
        );
        assert_eq!(
            snapshot(app.world(), invalid),
            Some((0.0, 0, FoldDirection::Folding, FoldMotion::Idle))
        );
        assert_eq!(
            app.world()
                .get::<FoldSequenceState>(invalid)
                .map(FoldSequenceState::is_ready),
            Some(false)
        );
    }

    #[test]
    fn large_deltas_clamp_exactly_at_each_target() {
        let mut app = fold_app();
        let sequence = spawn_sequence(&mut app, 3, FoldEndpoint::Unfolded, 0.5);

        trigger(
            &mut app,
            sequence,
            FoldCommand::Step(FoldDirection::Folding),
        );
        advance(&mut app, 10.0);

        assert_eq!(
            snapshot(app.world(), sequence),
            Some((1.0, 1, FoldDirection::Folding, FoldMotion::Idle))
        );

        trigger(
            &mut app,
            sequence,
            FoldCommand::Step(FoldDirection::Unfolding),
        );
        advance(&mut app, 10.0);

        assert_eq!(
            snapshot(app.world(), sequence),
            Some((0.0, 0, FoldDirection::Unfolding, FoldMotion::Idle))
        );
    }
}
