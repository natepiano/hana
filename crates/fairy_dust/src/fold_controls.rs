//! Standard Hana fold playback controls for Fairy Dust examples.

use std::collections::VecDeque;

use bevy::prelude::*;
use bevy_enhanced_input::prelude::*;
use bevy_kana::Keybindings;
use hana_valence::FoldCommand;
use hana_valence::FoldCommandEvent;
use hana_valence::FoldDirection;
use hana_valence::FoldMotion;
use hana_valence::FoldPlugin;
use hana_valence::FoldSequenceState;
use hana_valence::FoldSystems;

use crate::constants::FOLD_CONTROL_DIAGNOSTIC_CAPACITY;
use crate::constants::FOLD_CONTROL_ID;
use crate::constants::FOLD_CONTROL_LABEL;
use crate::constants::FOLD_CONTROL_RESERVE_LABEL;
use crate::constants::FOLD_PLAY_CONTROL_ID;
use crate::constants::FOLD_PLAY_CONTROL_LABEL;
use crate::constants::FOLD_PLAY_RESERVE_LABEL;
use crate::constants::UNFOLD_CONTROL_ID;
use crate::constants::UNFOLD_CONTROL_LABEL;
use crate::ensure_plugin;
use crate::screen_panels;
use crate::screen_panels::ControlActivation;
use crate::screen_panels::TitleBarControlState;
use crate::screen_panels::TitleChip;
use crate::shortcuts;

/// Marks the sequence selected by Fairy Dust when multiple ready fold
/// sequences exist.
#[derive(Component)]
pub struct FairyDustFoldTarget;

/// Standard fold input associated with a routing diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FoldControlAction {
    /// One stage toward the folded endpoint.
    Fold,
    /// One stage toward the unfolded endpoint.
    Unfold,
    /// Playback that selects the other endpoint from a terminal, follows the
    /// latest step direction while idle in the interior, continues an active
    /// step to the terminal, and reverses during playback.
    Play,
}

impl FoldControlAction {
    const fn command(self) -> FoldCommand {
        match self {
            Self::Fold => FoldCommand::Step(FoldDirection::Folding),
            Self::Unfold => FoldCommand::Step(FoldDirection::Unfolding),
            Self::Play => FoldCommand::Play,
        }
    }
}

/// Reason a standard fold input could not select a sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FoldControlDiagnosticReason {
    /// No sequence has a ready [`FoldSequenceState`].
    NoReadySequence,
    /// Multiple sequences are ready, but the ready sequences do not contain
    /// exactly one [`FairyDustFoldTarget`].
    AmbiguousReadySequences {
        /// Number of ready sequences.
        ready_sequences: usize,
        /// Number of ready sequences carrying [`FairyDustFoldTarget`].
        marked_targets:  usize,
    },
}

/// One standard fold input that could not be routed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FoldControlDiagnostic {
    /// Input that could not be routed.
    pub action: FoldControlAction,
    /// Selection failure for the input.
    pub reason: FoldControlDiagnosticReason,
}

/// Bounded history of standard fold inputs that could not be routed.
#[derive(Debug, Resource)]
pub struct FoldControlDiagnostics {
    entries: VecDeque<FoldControlDiagnostic>,
}

impl FoldControlDiagnostics {
    /// Iterates over retained diagnostics in insertion order.
    pub fn entries(&self) -> impl Iterator<Item = &FoldControlDiagnostic> { self.entries.iter() }

    /// Number of retained failed inputs.
    #[must_use]
    pub fn len(&self) -> usize { self.entries.len() }

    /// Whether no failed input has been retained.
    #[must_use]
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }

    fn record(&mut self, diagnostic: FoldControlDiagnostic) {
        warn!(
            action = ?diagnostic.action,
            reason = ?diagnostic.reason,
            "fairy_dust fold input could not select a ready sequence"
        );
        self.entries.push_back(diagnostic);
        while self.entries.len() > FOLD_CONTROL_DIAGNOSTIC_CAPACITY {
            self.entries.pop_front();
        }
    }
}

impl Default for FoldControlDiagnostics {
    fn default() -> Self {
        Self {
            entries: VecDeque::with_capacity(FOLD_CONTROL_DIAGNOSTIC_CAPACITY),
        }
    }
}

#[derive(Component)]
struct FoldControlContext;

#[derive(InputAction)]
#[action_output(bool)]
struct FoldStep;

#[derive(InputAction)]
#[action_output(bool)]
struct UnfoldStep;

#[derive(InputAction)]
#[action_output(bool)]
struct PlayFold;

#[derive(InputAction)]
#[action_output(bool)]
struct FoldShift;

#[derive(Resource)]
struct FoldControlsInstalled;

#[derive(Clone, Copy)]
struct ReadySequence {
    entity:    Entity,
    direction: FoldDirection,
    motion:    FoldMotion,
}

enum ReadySelection {
    None,
    Selected(ReadySequence),
    Ambiguous {
        ready_sequences: usize,
        marked_targets:  usize,
    },
}

pub(crate) fn install(app: &mut App) {
    if app.world().contains_resource::<FoldControlsInstalled>() {
        return;
    }
    app.insert_resource(FoldControlsInstalled);
    ensure_plugin(app, FoldPlugin);
    ensure_plugin(app, EnhancedInputPlugin);
    shortcuts::install(app);
    shortcuts::reserve_key::<FoldControlContext>(app, KeyCode::Space, FOLD_CONTROL_RESERVE_LABEL);
    shortcuts::reserve_key::<FoldControlContext>(app, KeyCode::KeyP, FOLD_PLAY_RESERVE_LABEL);
    screen_panels::register_title_control(app, TitleChip::new(FOLD_CONTROL_ID, FOLD_CONTROL_LABEL));
    screen_panels::register_title_control(
        app,
        TitleChip::new(UNFOLD_CONTROL_ID, UNFOLD_CONTROL_LABEL),
    );
    screen_panels::register_title_control(
        app,
        TitleChip::new(FOLD_PLAY_CONTROL_ID, FOLD_PLAY_CONTROL_LABEL),
    );
    app.init_resource::<FoldControlDiagnostics>()
        .add_input_context::<FoldControlContext>()
        .add_systems(Startup, spawn_fold_control_actions)
        .add_systems(
            PostUpdate,
            sync_fold_control_chips
                .after(FoldSystems::Advance)
                .before(screen_panels::refresh_changed_title_bar),
        )
        .add_observer(on_fold_step)
        .add_observer(on_unfold_step)
        .add_observer(on_play_fold);
}

fn spawn_fold_control_actions(mut commands: Commands) {
    commands.spawn((
        FoldControlContext,
        Actions::<FoldControlContext>::spawn(SpawnWith(
            |spawner: &mut ActionSpawner<FoldControlContext>| {
                let keybindings = Keybindings::new::<FoldShift>(spawner, ActionSettings::default());
                keybindings.spawn_key::<FoldStep>(spawner, KeyCode::Space);
                keybindings.spawn_shift_key::<UnfoldStep>(spawner, KeyCode::Space);
                keybindings.spawn_key::<PlayFold>(spawner, KeyCode::KeyP);
            },
        )),
    ));
}

fn on_fold_step(
    _: On<Start<FoldStep>>,
    sequences: Query<(Entity, &FoldSequenceState, Has<FairyDustFoldTarget>)>,
    diagnostics: ResMut<FoldControlDiagnostics>,
    commands: Commands,
) {
    route_action(FoldControlAction::Fold, &sequences, diagnostics, commands);
}

fn on_unfold_step(
    _: On<Start<UnfoldStep>>,
    sequences: Query<(Entity, &FoldSequenceState, Has<FairyDustFoldTarget>)>,
    diagnostics: ResMut<FoldControlDiagnostics>,
    commands: Commands,
) {
    route_action(FoldControlAction::Unfold, &sequences, diagnostics, commands);
}

fn on_play_fold(
    _: On<Start<PlayFold>>,
    sequences: Query<(Entity, &FoldSequenceState, Has<FairyDustFoldTarget>)>,
    diagnostics: ResMut<FoldControlDiagnostics>,
    commands: Commands,
) {
    route_action(FoldControlAction::Play, &sequences, diagnostics, commands);
}

fn route_action(
    action: FoldControlAction,
    sequences: &Query<(Entity, &FoldSequenceState, Has<FairyDustFoldTarget>)>,
    mut diagnostics: ResMut<FoldControlDiagnostics>,
    mut commands: Commands,
) {
    match select_ready_sequence(sequences) {
        ReadySelection::Selected(sequence) => {
            commands.trigger(FoldCommandEvent::new(sequence.entity, action.command()));
        },
        ReadySelection::None => diagnostics.record(FoldControlDiagnostic {
            action,
            reason: FoldControlDiagnosticReason::NoReadySequence,
        }),
        ReadySelection::Ambiguous {
            ready_sequences,
            marked_targets,
        } => diagnostics.record(FoldControlDiagnostic {
            action,
            reason: FoldControlDiagnosticReason::AmbiguousReadySequences {
                ready_sequences,
                marked_targets,
            },
        }),
    }
}

fn select_ready_sequence(
    sequences: &Query<(Entity, &FoldSequenceState, Has<FairyDustFoldTarget>)>,
) -> ReadySelection {
    let mut ready_sequences = 0;
    let mut marked_targets = 0;
    let mut sole_ready = None;
    let mut sole_marked = None;

    for (entity, state, marked) in sequences.iter() {
        if !state.is_ready() {
            continue;
        }
        let sequence = ReadySequence {
            entity,
            direction: state.direction(),
            motion: state.motion(),
        };
        ready_sequences += 1;
        sole_ready = Some(sequence);
        if marked {
            marked_targets += 1;
            sole_marked = Some(sequence);
        }
    }

    match (ready_sequences, marked_targets) {
        (0, _) => ReadySelection::None,
        (1, _) => sole_ready.map_or(ReadySelection::None, ReadySelection::Selected),
        (_, 1) => sole_marked.map_or(ReadySelection::None, ReadySelection::Selected),
        _ => ReadySelection::Ambiguous {
            ready_sequences,
            marked_targets,
        },
    }
}

fn sync_fold_control_chips(
    sequences: Query<(Entity, &FoldSequenceState, Has<FairyDustFoldTarget>)>,
    mut bars: Query<&mut TitleBarControlState>,
) {
    let selection = select_ready_sequence(&sequences);
    let (fold, unfold, play) = match selection {
        ReadySelection::Selected(sequence) => match sequence.motion {
            FoldMotion::Step => match sequence.direction {
                FoldDirection::Folding => (
                    ControlActivation::Active,
                    ControlActivation::Inactive,
                    ControlActivation::Inactive,
                ),
                FoldDirection::Unfolding => (
                    ControlActivation::Inactive,
                    ControlActivation::Active,
                    ControlActivation::Inactive,
                ),
            },
            FoldMotion::Play => (
                ControlActivation::Inactive,
                ControlActivation::Inactive,
                ControlActivation::Active,
            ),
            FoldMotion::Idle => (
                ControlActivation::Inactive,
                ControlActivation::Inactive,
                ControlActivation::Inactive,
            ),
        },
        ReadySelection::None | ReadySelection::Ambiguous { .. } => (
            ControlActivation::Inactive,
            ControlActivation::Inactive,
            ControlActivation::Inactive,
        ),
    };

    for mut bar in &mut bars {
        bar.set_active(FOLD_CONTROL_ID, fold);
        bar.set_active(UNFOLD_CONTROL_ID, unfold);
        bar.set_active(FOLD_PLAY_CONTROL_ID, play);
    }
}

#[cfg(test)]
mod tests {
    use std::panic::AssertUnwindSafe;
    use std::time::Duration;

    use bevy::prelude::*;
    use bevy_enhanced_input::prelude::*;
    use hana_valence::FoldEndpoint;
    use hana_valence::FoldMember;
    use hana_valence::FoldSequence;
    use hana_valence::FoldStage;

    use super::*;
    use crate::cube_spin;
    use crate::screen_panels::TitleBarControlState;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum CommandSnapshot {
        Fold,
        Unfold,
        Play,
    }

    impl From<FoldCommand> for CommandSnapshot {
        fn from(command: FoldCommand) -> Self {
            match command {
                FoldCommand::Step(FoldDirection::Folding) => Self::Fold,
                FoldCommand::Step(FoldDirection::Unfolding) => Self::Unfold,
                FoldCommand::Play => Self::Play,
            }
        }
    }

    #[derive(Resource, Default)]
    struct CapturedCommands(Vec<(Entity, CommandSnapshot)>);

    #[derive(Component)]
    struct CubeMarker;

    #[derive(Component)]
    struct AlgorithmContext;

    #[derive(InputAction)]
    #[action_output(bool)]
    struct ToggleAlgorithm;

    #[derive(Resource, Default)]
    struct AlgorithmToggles(usize);

    fn test_app() -> App {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Time>()
            .init_resource::<Time<Real>>()
            .insert_resource(Time::<Virtual>::default())
            .init_resource::<CapturedCommands>()
            .add_observer(capture_command);
        install(&mut app);
        app.finish();
        app.update();
        app
    }

    fn capture_command(event: On<FoldCommandEvent>, mut captured: ResMut<CapturedCommands>) {
        captured.0.push((
            event.sequence_entity(),
            CommandSnapshot::from(event.command),
        ));
    }

    fn spawn_sequence(app: &mut App, stages: usize, endpoint: FoldEndpoint) -> Entity {
        let sequence = app
            .world_mut()
            .spawn(FoldSequence::new(1.0).with_initial(endpoint))
            .id();
        for stage in 0..stages {
            app.world_mut()
                .spawn(FoldMember::new(sequence, FoldStage(stage)));
        }
        app.update();
        sequence
    }

    fn press(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(key);
        app.update();
    }

    fn press_shift_space(app: &mut App, shift: KeyCode) {
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(shift);
            keys.press(KeyCode::Space);
        }
        app.update();
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(KeyCode::Space);
            keys.release(shift);
        }
        app.update();
    }

    fn captured(app: &App) -> &[(Entity, CommandSnapshot)] {
        &app.world().resource::<CapturedCommands>().0
    }

    fn chip_activation(app: &App, bar: Entity, control: &str) -> ControlActivation {
        app.world()
            .get::<TitleBarControlState>(bar)
            .map_or(ControlActivation::Inactive, |state| {
                state.activation(control)
            })
    }

    #[test]
    fn fold_control_installation_is_idempotent() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Time>()
            .init_resource::<Time<Real>>()
            .insert_resource(Time::<Virtual>::default());

        install(&mut app);
        install(&mut app);
        app.finish();
        app.update();

        assert!(app.is_plugin_added::<FoldPlugin>());
        assert!(app.is_plugin_added::<EnhancedInputPlugin>());
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<FoldControlContext>>()
                .iter(app.world())
                .count(),
            1
        );
        assert_eq!(
            app.world_mut()
                .query::<&Action<FoldStep>>()
                .iter(app.world())
                .count(),
            1
        );
        assert_eq!(
            app.world_mut()
                .query::<&Action<UnfoldStep>>()
                .iter(app.world())
                .count(),
            1
        );
        assert_eq!(
            app.world_mut()
                .query::<&Action<PlayFold>>()
                .iter(app.world())
                .count(),
            1
        );
    }

    #[test]
    fn bare_and_shift_space_bindings_route_separately_for_either_shift_key() {
        let mut app = test_app();
        let sequence = spawn_sequence(&mut app, 2, FoldEndpoint::Unfolded);

        press(&mut app, KeyCode::Space);
        press_shift_space(&mut app, KeyCode::ShiftLeft);
        press_shift_space(&mut app, KeyCode::ShiftRight);

        assert_eq!(
            captured(&app),
            &[
                (sequence, CommandSnapshot::Fold),
                (sequence, CommandSnapshot::Unfold),
                (sequence, CommandSnapshot::Unfold),
            ]
        );
    }

    #[test]
    fn cube_spin_play_key_conflicts_with_fold_play() {
        let mut app = App::new();
        install(&mut app);

        let collision = std::panic::catch_unwind(AssertUnwindSafe(|| {
            cube_spin::install::<CubeMarker>(&mut app, cube_spin::CubeSpinConfig::default());
        }));

        assert!(collision.is_err());
    }

    #[test]
    fn passive_zero_ready_sequences_keeps_chips_inactive_without_diagnostics() {
        let mut app = test_app();
        let bar = app.world_mut().spawn(TitleBarControlState::default()).id();

        app.update();

        assert_eq!(
            chip_activation(&app, bar, FOLD_CONTROL_ID),
            ControlActivation::Inactive
        );
        assert_eq!(
            chip_activation(&app, bar, UNFOLD_CONTROL_ID),
            ControlActivation::Inactive
        );
        assert_eq!(
            chip_activation(&app, bar, FOLD_PLAY_CONTROL_ID),
            ControlActivation::Inactive
        );
        assert!(app.world().resource::<FoldControlDiagnostics>().is_empty());
    }

    #[test]
    fn input_with_zero_ready_sequences_records_one_stable_diagnostic() {
        let mut app = test_app();

        press(&mut app, KeyCode::KeyP);

        let diagnostics = app.world().resource::<FoldControlDiagnostics>();
        assert_eq!(
            diagnostics.entries().copied().collect::<Vec<_>>(),
            vec![FoldControlDiagnostic {
                action: FoldControlAction::Play,
                reason: FoldControlDiagnosticReason::NoReadySequence,
            }]
        );
    }

    #[test]
    fn sole_ready_sequence_routes_without_a_marker() {
        let mut app = test_app();
        let sequence = spawn_sequence(&mut app, 1, FoldEndpoint::Unfolded);

        press(&mut app, KeyCode::Space);

        assert_eq!(captured(&app), &[(sequence, CommandSnapshot::Fold)]);
        assert!(
            app.world()
                .get::<FoldSequenceState>(sequence)
                .is_some_and(|state| {
                    state.direction() == FoldDirection::Folding
                        && state.motion() == FoldMotion::Step
                })
        );
    }

    #[test]
    fn exactly_one_marked_ready_sequence_routes_among_multiple() {
        let mut app = test_app();
        spawn_sequence(&mut app, 1, FoldEndpoint::Unfolded);
        let selected = spawn_sequence(&mut app, 1, FoldEndpoint::Unfolded);
        app.world_mut()
            .entity_mut(selected)
            .insert(FairyDustFoldTarget);

        press(&mut app, KeyCode::KeyP);

        assert_eq!(captured(&app), &[(selected, CommandSnapshot::Play)]);
    }

    #[test]
    fn multiple_unmarked_ready_sequences_are_ambiguous() {
        let mut app = test_app();
        spawn_sequence(&mut app, 1, FoldEndpoint::Unfolded);
        spawn_sequence(&mut app, 1, FoldEndpoint::Unfolded);

        press(&mut app, KeyCode::Space);

        assert!(captured(&app).is_empty());
        assert_eq!(
            app.world()
                .resource::<FoldControlDiagnostics>()
                .entries()
                .copied()
                .collect::<Vec<_>>(),
            vec![FoldControlDiagnostic {
                action: FoldControlAction::Fold,
                reason: FoldControlDiagnosticReason::AmbiguousReadySequences {
                    ready_sequences: 2,
                    marked_targets:  0,
                },
            }]
        );
    }

    #[test]
    fn multiple_marked_ready_sequences_are_ambiguous() {
        let mut app = test_app();
        let first = spawn_sequence(&mut app, 1, FoldEndpoint::Unfolded);
        let second = spawn_sequence(&mut app, 1, FoldEndpoint::Unfolded);
        app.world_mut()
            .entity_mut(first)
            .insert(FairyDustFoldTarget);
        app.world_mut()
            .entity_mut(second)
            .insert(FairyDustFoldTarget);

        press(&mut app, KeyCode::KeyP);

        assert!(captured(&app).is_empty());
        assert_eq!(
            app.world()
                .resource::<FoldControlDiagnostics>()
                .entries()
                .next()
                .copied(),
            Some(FoldControlDiagnostic {
                action: FoldControlAction::Play,
                reason: FoldControlDiagnosticReason::AmbiguousReadySequences {
                    ready_sequences: 2,
                    marked_targets:  2,
                },
            })
        );
    }

    #[test]
    fn invalid_marked_target_does_not_activate_or_select_controls() {
        let mut app = test_app();
        spawn_sequence(&mut app, 1, FoldEndpoint::Unfolded);
        spawn_sequence(&mut app, 1, FoldEndpoint::Unfolded);
        let invalid = app
            .world_mut()
            .spawn((FoldSequence::new(-1.0), FairyDustFoldTarget))
            .id();
        let bar = app.world_mut().spawn(TitleBarControlState::default()).id();
        app.update();

        press(&mut app, KeyCode::Space);

        assert!(captured(&app).is_empty());
        assert_eq!(
            chip_activation(&app, bar, FOLD_CONTROL_ID),
            ControlActivation::Inactive
        );
        assert!(
            app.world()
                .get::<FoldSequenceState>(invalid)
                .is_some_and(|state| !state.is_ready())
        );
    }

    #[test]
    fn step_chip_clears_on_the_exact_settle_frame() {
        let mut app = test_app();
        let sequence = spawn_sequence(&mut app, 1, FoldEndpoint::Unfolded);
        let bar = app.world_mut().spawn(TitleBarControlState::default()).id();
        app.world_mut().trigger(FoldCommandEvent::new(
            sequence,
            FoldCommand::Step(FoldDirection::Folding),
        ));
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .advance_by(Duration::from_millis(500));

        app.update();
        assert_eq!(
            chip_activation(&app, bar, FOLD_CONTROL_ID),
            ControlActivation::Active
        );

        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .advance_by(Duration::from_millis(500));
        app.update();
        assert_eq!(
            chip_activation(&app, bar, FOLD_CONTROL_ID),
            ControlActivation::Inactive
        );
        assert!(
            app.world()
                .get::<FoldSequenceState>(sequence)
                .is_some_and(|state| state.motion() == FoldMotion::Idle)
        );
    }

    #[test]
    fn play_chip_stays_active_after_reversal_and_clears_on_the_exact_settle_frame() {
        let mut app = test_app();
        let sequence = spawn_sequence(&mut app, 2, FoldEndpoint::Unfolded);
        let bar = app.world_mut().spawn(TitleBarControlState::default()).id();
        app.world_mut()
            .trigger(FoldCommandEvent::new(sequence, FoldCommand::Play));
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .advance_by(Duration::from_secs(1));

        app.update();
        assert_eq!(
            chip_activation(&app, bar, FOLD_PLAY_CONTROL_ID),
            ControlActivation::Active
        );

        app.world_mut()
            .trigger(FoldCommandEvent::new(sequence, FoldCommand::Play));
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .advance_by(Duration::ZERO);
        app.update();
        assert_eq!(
            chip_activation(&app, bar, FOLD_PLAY_CONTROL_ID),
            ControlActivation::Active
        );
        assert!(
            app.world()
                .get::<FoldSequenceState>(sequence)
                .is_some_and(|state| {
                    state.direction() == FoldDirection::Unfolding
                        && state.motion() == FoldMotion::Play
                })
        );

        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .advance_by(Duration::from_secs(1));
        app.update();
        assert_eq!(
            chip_activation(&app, bar, FOLD_PLAY_CONTROL_ID),
            ControlActivation::Inactive
        );
        assert!(
            app.world()
                .get::<FoldSequenceState>(sequence)
                .is_some_and(|state| state.motion() == FoldMotion::Idle)
        );
    }

    #[test]
    fn fold_controls_coexist_with_an_example_owned_bei_action() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Time>()
            .init_resource::<Time<Real>>()
            .insert_resource(Time::<Virtual>::default())
            .init_resource::<CapturedCommands>()
            .init_resource::<AlgorithmToggles>()
            .add_observer(capture_command);
        install(&mut app);
        app.add_input_context::<AlgorithmContext>()
            .add_systems(Startup, spawn_algorithm_action)
            .add_observer(on_toggle_algorithm);
        app.finish();
        app.update();
        let sequence = spawn_sequence(&mut app, 1, FoldEndpoint::Unfolded);

        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::KeyT);
            keys.press(KeyCode::Space);
        }
        app.update();

        assert_eq!(app.world().resource::<AlgorithmToggles>().0, 1);
        assert_eq!(captured(&app), &[(sequence, CommandSnapshot::Fold)]);
    }

    fn spawn_algorithm_action(mut commands: Commands) {
        commands.spawn((
            AlgorithmContext,
            actions!(
                AlgorithmContext[(Action::<ToggleAlgorithm>::new(), bindings![KeyCode::KeyT],)]
            ),
        ));
    }

    fn on_toggle_algorithm(_: On<Start<ToggleAlgorithm>>, mut toggles: ResMut<AlgorithmToggles>) {
        toggles.0 += 1;
    }
}
