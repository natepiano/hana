//! State-backed keymap contexts derived from current world data.

use std::sync::Mutex;

use bevy::app::App;
use bevy::app::Plugin;
use bevy::ecs::change_detection::CheckChangeTicks;
use bevy::ecs::error::ErrorContext;
use bevy::ecs::schedule::BoxedCondition;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::SystemCondition;
use bevy::ecs::system::IntoSystem;
use bevy::ecs::system::RunSystemError;
use bevy::prelude::NextState;
use bevy::prelude::On;
use bevy::prelude::PreUpdate;
use bevy::prelude::Resource;
use bevy::prelude::State;
use bevy::prelude::World;
use bevy::state::app::AppExtStates;
use bevy::state::app::StatesPlugin;
use bevy::state::state::FreelyMutableState;
use bevy::utils::DebugName;

use crate::Diagnostic;
use crate::DiagnosticSeverity;
use crate::KeymapContext;
use crate::KeymapPlugin;
use crate::KeymapSystems;
use crate::condition::ContextSource;
use crate::condition::condition_diagnostic;
use crate::condition::register_context;
use crate::condition::retain_context_diagnostics;
use crate::condition::sync_state_condition;

pub(crate) struct DerivedContextPlugin<C> {
    keymap_plugin:   KeymapPlugin,
    derived_context: Mutex<Option<DerivedContext<C>>>,
}

impl<C> DerivedContextPlugin<C> {
    pub(crate) const fn new(
        keymap_plugin: KeymapPlugin,
        derived_context: DerivedContext<C>,
    ) -> Self {
        Self {
            keymap_plugin,
            derived_context: Mutex::new(Some(derived_context)),
        }
    }
}

impl<C> Plugin for DerivedContextPlugin<C>
where
    C: KeymapContext + FreelyMutableState,
{
    fn build(&self, app: &mut App) {
        self.keymap_plugin.install(app);
        if !app.is_plugin_added::<StatesPlugin>() {
            app.add_plugins(StatesPlugin);
        }
        if register_context::<C>(app, ContextSource::Derived).is_ok() {
            let derived_context = self
                .derived_context
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let Some(mut derived_context) = derived_context else {
                return;
            };
            let diagnostics = validate_derived_context(&mut derived_context);
            retain_context_diagnostics(app, &diagnostics);
            app.insert_state(derived_context.fallback);
            for (_, condition) in &mut derived_context.rules {
                condition.initialize(app.world_mut());
            }
            app.insert_resource(DerivedContextRules::from(derived_context))
                .add_observer(check_derived_context_change_ticks::<C>)
                .add_systems(
                    PreUpdate,
                    evaluate_derived_context::<C>.before(KeymapSystems::UpdateActiveCondition),
                )
                .add_systems(
                    PreUpdate,
                    sync_state_condition::<C>.in_set(KeymapSystems::UpdateActiveCondition),
                );
        }
    }

    fn finish(&self, app: &mut App) { KeymapPlugin::finish_assembly(app); }

    fn is_unique(&self) -> bool { false }
}

/// An ordered table that derives a state-backed keymap context from world data.
///
/// Rules are evaluated in declaration order, and the first matching rule selects its context.
/// The fallback applies when no rule matches. Each rule must be a predicate over current world
/// data. Do not use `Changed<T>`, `Added<T>`, `RemovedComponents<T>`, `MessageReader<T>`, or
/// `Local<T>` history: lower-priority rules are not evaluated while an earlier rule matches.
pub struct DerivedContext<C> {
    fallback: C,
    rules:    Vec<(C, BoxedCondition)>,
}

impl<C> DerivedContext<C>
where
    C: KeymapContext + FreelyMutableState,
{
    /// Creates a context table with the variant selected when no rule matches.
    #[must_use]
    pub fn new(fallback: C) -> Self {
        Self {
            fallback,
            rules: Vec::new(),
        }
    }

    /// Adds one context rule after all earlier rules.
    ///
    /// The condition must depend only on current world data. History-based parameters such as
    /// `Changed<T>`, `Added<T>`, `RemovedComponents<T>`, `MessageReader<T>`, and `Local<T>` do
    /// not advance while an earlier rule matches, so they can observe stale history when this rule
    /// becomes eligible again.
    #[must_use]
    pub fn when<M>(mut self, context: C, condition: impl SystemCondition<M>) -> Self {
        self.rules
            .push((context, Box::new(IntoSystem::into_system(condition))));
        self
    }
}

#[derive(Resource)]
struct DerivedContextRules<C> {
    fallback: C,
    rules:    Vec<(C, BoxedCondition)>,
}

impl<C> From<DerivedContext<C>> for DerivedContextRules<C> {
    fn from(derived_context: DerivedContext<C>) -> Self {
        Self {
            fallback: derived_context.fallback,
            rules:    derived_context.rules,
        }
    }
}

fn validate_derived_context<C>(derived_context: &mut DerivedContext<C>) -> Vec<Diagnostic>
where
    C: KeymapContext + FreelyMutableState,
{
    let mut diagnostics = Vec::new();

    for context in C::iter().filter(|context| *context != derived_context.fallback) {
        if !derived_context
            .rules
            .iter()
            .any(|(rule_context, _)| *rule_context == context)
        {
            diagnostics.push(condition_diagnostic(
                context.as_ref(),
                DiagnosticSeverity::Advisory,
                format!(
                    "Derived context `{}` has no rule and can never become active.",
                    context.as_ref()
                ),
            ));
        }
    }

    derived_context.rules.retain(|(context, _)| {
        if *context == derived_context.fallback {
            diagnostics.push(condition_diagnostic(
                context.as_ref(),
                DiagnosticSeverity::Advisory,
                format!(
                    "Derived context rule for fallback `{}` was removed because the fallback applies when no rule matches.",
                    context.as_ref()
                ),
            ));
            false
        } else {
            true
        }
    });

    for context in C::iter().filter(|context| *context != derived_context.fallback) {
        if derived_context
            .rules
            .iter()
            .filter(|(rule_context, _)| *rule_context == context)
            .count()
            > 1
        {
            diagnostics.push(condition_diagnostic(
                context.as_ref(),
                DiagnosticSeverity::Advisory,
                format!(
                    "Derived context `{}` has more than one rule; combine alternative predicates with `.or_else()` when they share one priority position.",
                    context.as_ref()
                ),
            ));
        }
    }

    diagnostics
}

fn check_derived_context_change_ticks<C: KeymapContext>(
    check_change_ticks: On<CheckChangeTicks>,
    mut derived_context_rules: bevy::prelude::ResMut<DerivedContextRules<C>>,
) {
    for (_, condition) in &mut derived_context_rules.rules {
        condition.check_change_tick(*check_change_ticks);
    }
}

fn evaluate_derived_context<C>(world: &mut World)
where
    C: KeymapContext + FreelyMutableState,
{
    world.resource_scope(
        |world, mut derived_context_rules: bevy::prelude::Mut<DerivedContextRules<C>>| {
            let context = derived_context_rules
                .rules
                .iter_mut()
                .find_map(
                    |(context, condition)| match condition.run_readonly((), &*world) {
                        Ok(true) => Some(*context),
                        Ok(false) | Err(RunSystemError::Skipped(_)) => None,
                        Err(RunSystemError::Failed(error)) => {
                            world.fallback_error_handler()(
                                error,
                                ErrorContext::RunCondition {
                                    name:     condition.name(),
                                    last_run: condition.get_last_run(),
                                    system:   DebugName::borrowed(
                                        "hana_rubric::derived_context::evaluate_derived_context",
                                    ),
                                    on_set:   false,
                                },
                            );
                            None
                        },
                    },
                )
                .unwrap_or(derived_context_rules.fallback);

            if world.resource::<State<C>>().get() != &context {
                world.resource_mut::<NextState<C>>().set(context);
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    use bevy::ecs::error::BevyError;
    use bevy::ecs::error::ErrorContext;
    use bevy::ecs::error::FallbackErrorHandler;
    use bevy::ecs::schedule::SystemCondition;
    use bevy::ecs::schedule::common_conditions::any_with_component;
    use bevy::input::ButtonInput;
    use bevy::input::keyboard::KeyCode;
    use bevy::prelude::App;
    use bevy::prelude::AppTypeRegistry;
    use bevy::prelude::Component;
    use bevy::prelude::Entity;
    use bevy::prelude::Event;
    use bevy::prelude::NextState;
    use bevy::prelude::On;
    use bevy::prelude::PreUpdate;
    use bevy::prelude::Query;
    use bevy::prelude::Reflect;
    use bevy::prelude::ReflectEvent;
    use bevy::prelude::Res;
    use bevy::prelude::ResMut;
    use bevy::prelude::Resource;
    use bevy::prelude::Single;
    use bevy::prelude::State;
    use bevy::prelude::States;
    use bevy::prelude::With;
    use bevy::state::app::StatesPlugin;
    use bevy::state::state::StateTransition;
    use strum::AsRefStr;
    use strum::EnumIter;
    use strum::EnumMessage;

    use super::DerivedContext;
    use super::DerivedContextRules;
    use crate::ActiveCondition;
    use crate::Capability;
    use crate::DiagnosticKind;
    use crate::DiagnosticSeverity;
    use crate::HoldPhase;
    use crate::KeymapCommand;
    use crate::KeymapLoadFailures;
    use crate::KeymapPlugin;
    use crate::ReflectKeymapCommand;
    use crate::condition::ConditionRegistry;

    static CONDITION_ERRORS: AtomicUsize = AtomicUsize::new(0);

    fn count_condition_error(_: BevyError, error_context: ErrorContext) {
        if matches!(error_context, ErrorContext::RunCondition { .. }) {
            CONDITION_ERRORS.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[derive(Default, Event, Reflect)]
    #[reflect(Event, KeymapCommand)]
    struct RestingDispatch;

    impl KeymapCommand for RestingDispatch {
        const ID: &'static str = "derived_context::resting_dispatch";
        const TITLE: &'static str = "Resting Dispatch";
        const DESCRIPTION: &'static str = "Dispatches while the derived context is resting.";
        const CAPABILITY: Capability = Capability::OneShot;

        fn build() -> Self { Self }

        fn hold_phase(&self) -> Option<HoldPhase> { None }
    }

    #[derive(Default, Event, Reflect)]
    #[reflect(Event, KeymapCommand)]
    struct ActiveDispatch;

    impl KeymapCommand for ActiveDispatch {
        const ID: &'static str = "derived_context::active_dispatch";
        const TITLE: &'static str = "Active Dispatch";
        const DESCRIPTION: &'static str = "Dispatches while the derived context is active.";
        const CAPABILITY: Capability = Capability::OneShot;

        fn build() -> Self { Self }

        fn hold_phase(&self) -> Option<HoldPhase> { None }
    }

    #[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, Hash, PartialEq, States)]
    #[strum(serialize_all = "snake_case")]
    enum BasicContext {
        #[strum(message = "While no test fact is active")]
        Resting,
        #[strum(message = "While the test fact is active")]
        Active,
    }

    #[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, Hash, PartialEq, States)]
    #[strum(serialize_all = "snake_case")]
    enum OverlapContext {
        #[strum(message = "While no overlapping rule matches")]
        Resting,
        #[strum(message = "While the first overlapping rule matches")]
        First,
        #[strum(message = "While the second overlapping rule matches")]
        Second,
    }

    #[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, Hash, PartialEq, States)]
    #[strum(serialize_all = "snake_case")]
    enum ValidationContext {
        #[strum(message = "While validation uses its fallback")]
        Resting,
        #[strum(message = "While validation selects its first variant")]
        First,
        #[strum(message = "While validation selects its second variant")]
        Second,
    }

    #[derive(AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, Hash, PartialEq, States)]
    #[strum(serialize_all = "snake_case")]
    enum SingleVariantContext {
        #[strum(message = "While the only context variant is active")]
        Only,
    }

    #[derive(
        AsRefStr, Clone, Copy, Debug, EnumIter, EnumMessage, Eq, Hash, PartialEq, Resource, States,
    )]
    #[strum(serialize_all = "snake_case")]
    enum ConflictContext {
        #[strum(message = "While the conflicting context is resting")]
        Resting,
        #[strum(message = "While the conflicting context is active")]
        Active,
    }

    #[derive(Component)]
    struct ActiveFact;

    #[derive(Component)]
    struct LeftFact;

    #[derive(Component)]
    struct RightFact;

    #[derive(Component)]
    struct LinkedEntity(Entity);

    #[derive(Component)]
    struct LinkedMarker;

    #[derive(Component)]
    struct SingleMarker;

    #[derive(Resource)]
    struct RequiredResource;

    #[derive(Default, Resource)]
    struct DispatchCounts {
        active:  usize,
        resting: usize,
    }

    fn always_true() -> bool { true }

    fn linked_marker_matches(
        linked_entities: Query<&LinkedEntity>,
        linked_markers: Query<(), With<LinkedMarker>>,
    ) -> bool {
        linked_entities
            .iter()
            .any(|linked_entity| linked_markers.contains(linked_entity.0))
    }

    fn exactly_one_marker(_: Single<&SingleMarker>) -> bool { true }

    fn requires_resource(_: Res<RequiredResource>) -> bool { true }

    fn app_with_derived_context<C>(derived_context: DerivedContext<C>) -> App
    where
        C: crate::KeymapContext + bevy::state::state::FreelyMutableState,
    {
        let mut app = App::new();
        app.add_plugins(StatesPlugin)
            .add_plugins(KeymapPlugin::new().for_derived_context(derived_context));
        app
    }

    fn state<C: Copy + States>(app: &App) -> C { *app.world().resource::<State<C>>().get() }

    fn context_advisories(app: &App, condition_name: &str) -> usize {
        app.world()
            .resource::<KeymapLoadFailures>()
            .retained_diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.kind == DiagnosticKind::Context
                    && diagnostic.severity == DiagnosticSeverity::Advisory
                    && diagnostic.context == condition_name
            })
            .count()
    }

    fn all_context_advisories(app: &App) -> usize {
        app.world()
            .resource::<KeymapLoadFailures>()
            .retained_diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.kind == DiagnosticKind::Context
                    && diagnostic.severity == DiagnosticSeverity::Advisory
            })
            .count()
    }

    #[test]
    fn empty_world_resolves_to_the_declared_fallback() {
        let mut app = app_with_derived_context(DerivedContext::new(BasicContext::Resting));
        app.world_mut()
            .insert_resource(State::new(BasicContext::Active));

        app.update();

        assert_eq!(state::<BasicContext>(&app), BasicContext::Resting);
    }

    #[test]
    fn true_rule_selects_its_nonfallback_variant() {
        let mut app = app_with_derived_context(
            DerivedContext::new(BasicContext::Resting)
                .when(BasicContext::Active, any_with_component::<ActiveFact>),
        );
        app.world_mut().spawn(ActiveFact);

        app.update();

        assert_eq!(state::<BasicContext>(&app), BasicContext::Active);
    }

    #[test]
    fn declaration_order_decides_overlapping_rules() {
        let mut first_app = app_with_derived_context(
            DerivedContext::new(OverlapContext::Resting)
                .when(OverlapContext::First, always_true)
                .when(OverlapContext::Second, always_true),
        );
        let mut second_app = app_with_derived_context(
            DerivedContext::new(OverlapContext::Resting)
                .when(OverlapContext::Second, always_true)
                .when(OverlapContext::First, always_true),
        );

        first_app.update();
        second_app.update();

        assert_eq!(state::<OverlapContext>(&first_app), OverlapContext::First);
        assert_eq!(state::<OverlapContext>(&second_app), OverlapContext::Second);
        assert_eq!(all_context_advisories(&first_app), 0);
        assert_eq!(all_context_advisories(&second_app), 0);
    }

    #[test]
    fn conjunction_requires_both_marker_facts() {
        let derived_context = || {
            DerivedContext::new(BasicContext::Resting).when(
                BasicContext::Active,
                any_with_component::<LeftFact>.and_then(any_with_component::<RightFact>),
            )
        };
        let mut left_app = app_with_derived_context(derived_context());
        left_app
            .world_mut()
            .insert_resource(State::new(BasicContext::Active));
        left_app.world_mut().spawn(LeftFact);
        let mut right_app = app_with_derived_context(derived_context());
        right_app
            .world_mut()
            .insert_resource(State::new(BasicContext::Active));
        right_app.world_mut().spawn(RightFact);
        let mut both_app = app_with_derived_context(derived_context());
        both_app.world_mut().spawn((LeftFact, RightFact));

        left_app.update();
        right_app.update();
        both_app.update();

        assert_eq!(state::<BasicContext>(&left_app), BasicContext::Resting);
        assert_eq!(state::<BasicContext>(&right_app), BasicContext::Resting);
        assert_eq!(state::<BasicContext>(&both_app), BasicContext::Active);
    }

    #[test]
    fn custom_system_rule_correlates_entities() {
        let derived_context = || {
            DerivedContext::new(BasicContext::Resting)
                .when(BasicContext::Active, linked_marker_matches)
        };
        let mut correlated_app = app_with_derived_context(derived_context());
        let marked_entity = correlated_app.world_mut().spawn(LinkedMarker).id();
        correlated_app
            .world_mut()
            .spawn(LinkedEntity(marked_entity));
        let mut split_app = app_with_derived_context(derived_context());
        split_app
            .world_mut()
            .insert_resource(State::new(BasicContext::Active));
        let marked_entity = split_app.world_mut().spawn(LinkedMarker).id();
        let other_entity = split_app.world_mut().spawn_empty().id();
        split_app.world_mut().spawn(LinkedEntity(other_entity));

        correlated_app.update();
        split_app.update();

        assert_eq!(state::<BasicContext>(&correlated_app), BasicContext::Active);
        assert_eq!(state::<BasicContext>(&split_app), BasicContext::Resting);
        assert_ne!(marked_entity, other_entity);
    }

    #[test]
    fn evaluator_returns_to_fallback_when_a_rule_stops_matching() {
        let mut app = app_with_derived_context(
            DerivedContext::new(BasicContext::Resting)
                .when(BasicContext::Active, any_with_component::<ActiveFact>),
        );
        let active_fact = app.world_mut().spawn(ActiveFact).id();
        app.update();
        assert_eq!(state::<BasicContext>(&app), BasicContext::Active);

        app.world_mut()
            .entity_mut(active_fact)
            .remove::<ActiveFact>();
        app.update();

        assert_eq!(state::<BasicContext>(&app), BasicContext::Resting);
    }

    #[test]
    fn active_condition_reflects_the_derived_variant_on_the_next_update() {
        let mut app = app_with_derived_context(
            DerivedContext::new(BasicContext::Resting)
                .when(BasicContext::Active, any_with_component::<ActiveFact>),
        );
        app.world_mut().spawn(ActiveFact);

        app.update();
        assert_eq!(state::<BasicContext>(&app), BasicContext::Active);
        app.update();

        let expected_handle = app
            .world()
            .resource::<ConditionRegistry>()
            .resolve("active");
        assert_eq!(
            app.world().resource::<ActiveCondition>().handle(),
            expected_handle
        );
    }

    #[test]
    fn second_context_source_is_rejected_in_both_registration_orders() {
        let derived_context = || {
            DerivedContext::new(ConflictContext::Resting).when(ConflictContext::Active, always_true)
        };
        let mut control_app = App::new();
        control_app
            .add_plugins(StatesPlugin)
            .add_plugins(KeymapPlugin::new().for_derived_context(derived_context()))
            .add_plugins(KeymapPlugin::new().for_context::<ConflictContext>());

        control_app.update();

        assert_eq!(
            state::<ConflictContext>(&control_app),
            ConflictContext::Active
        );
        assert_eq!(
            control_app
                .world()
                .resource::<KeymapLoadFailures>()
                .retained_diagnostics
                .iter()
                .filter(|diagnostic| {
                    diagnostic.kind == DiagnosticKind::Context
                        && diagnostic.message.contains("derived")
                })
                .count(),
            1
        );

        let mut rejection_app = App::new();
        rejection_app
            .insert_resource(ConflictContext::Resting)
            .add_plugins(StatesPlugin)
            .add_plugins(KeymapPlugin::new().for_context::<ConflictContext>())
            .add_plugins(KeymapPlugin::new().for_derived_context(derived_context()));

        assert_eq!(
            rejection_app
                .world()
                .resource::<KeymapLoadFailures>()
                .retained_diagnostics
                .iter()
                .filter(|diagnostic| {
                    diagnostic.kind == DiagnosticKind::Context
                        && diagnostic.message.contains("resource-backed")
                })
                .count(),
            1
        );
        assert!(
            !rejection_app
                .world()
                .contains_resource::<State<ConflictContext>>()
        );
        assert!(
            !rejection_app
                .world()
                .contains_resource::<DerivedContextRules<ConflictContext>>()
        );
    }

    #[test]
    fn unreachable_nonfallback_variant_produces_one_advisory() {
        let mut app = app_with_derived_context(DerivedContext::new(BasicContext::Resting));
        app.world_mut()
            .insert_resource(State::new(BasicContext::Active));

        app.update();

        assert_eq!(state::<BasicContext>(&app), BasicContext::Resting);
        assert_eq!(context_advisories(&app, "active"), 1);
    }

    #[test]
    fn fallback_targeting_rule_is_removed_before_evaluation() {
        let mut app = app_with_derived_context(
            DerivedContext::new(BasicContext::Resting)
                .when(BasicContext::Resting, always_true)
                .when(BasicContext::Active, always_true),
        );

        app.update();

        assert_eq!(state::<BasicContext>(&app), BasicContext::Active);
        assert_eq!(context_advisories(&app, "resting"), 1);
    }

    #[test]
    fn duplicate_nonfallback_rules_produce_one_advisory_and_still_evaluate() {
        let mut app = app_with_derived_context(
            DerivedContext::new(BasicContext::Resting)
                .when(BasicContext::Active, || false)
                .when(BasicContext::Active, always_true),
        );

        app.update();

        assert_eq!(state::<BasicContext>(&app), BasicContext::Active);
        assert_eq!(context_advisories(&app, "active"), 1);
    }

    #[test]
    fn evaluator_writes_next_state_only_when_the_context_changes() {
        let mut app = app_with_derived_context(
            DerivedContext::new(BasicContext::Resting)
                .when(BasicContext::Active, any_with_component::<ActiveFact>),
        );
        app.world_mut().spawn(ActiveFact);

        app.world_mut().run_schedule(PreUpdate);

        assert!(matches!(
            app.world().resource::<NextState<BasicContext>>(),
            NextState::Pending(BasicContext::Active)
        ));
        app.world_mut().run_schedule(StateTransition);
        app.world_mut().run_schedule(PreUpdate);

        assert!(matches!(
            app.world().resource::<NextState<BasicContext>>(),
            NextState::Unchanged
        ));
    }

    #[test]
    fn skipped_rule_falls_through_without_panicking() {
        let mut app = app_with_derived_context(
            DerivedContext::new(BasicContext::Resting)
                .when(BasicContext::Active, exactly_one_marker),
        );
        app.world_mut()
            .insert_resource(State::new(BasicContext::Active));

        app.update();
        assert_eq!(state::<BasicContext>(&app), BasicContext::Resting);

        app.world_mut().spawn(SingleMarker);
        app.update();

        assert_eq!(state::<BasicContext>(&app), BasicContext::Active);
    }

    #[test]
    fn failed_rule_uses_the_fallback_error_handler_and_continues() {
        CONDITION_ERRORS.store(0, Ordering::Relaxed);
        let mut app = app_with_derived_context(
            DerivedContext::new(OverlapContext::Resting)
                .when(OverlapContext::First, requires_resource)
                .when(OverlapContext::Second, always_true),
        );
        app.world_mut()
            .insert_resource(FallbackErrorHandler(count_condition_error));

        app.update();

        assert_eq!(CONDITION_ERRORS.load(Ordering::Relaxed), 1);
        assert_eq!(state::<OverlapContext>(&app), OverlapContext::Second);
    }

    #[test]
    fn routing_uses_the_previous_context_until_the_next_preupdate() {
        let mut app = App::new();
        register_dispatch_commands(&mut app);
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<DispatchCounts>()
            .add_plugins(StatesPlugin)
            .add_plugins(
                KeymapPlugin::new()
                    .with_defaults(
                        r#"{
                            "bindings": [
                                { "context": "resting", "bindings": {
                                    "g": "derived_context::resting_dispatch"
                                }},
                                { "context": "active", "bindings": {
                                    "g": "derived_context::active_dispatch"
                                }}
                            ]
                        }"#,
                    )
                    .for_derived_context(
                        DerivedContext::new(BasicContext::Resting)
                            .when(BasicContext::Active, any_with_component::<ActiveFact>),
                    ),
            );
        app.world_mut().add_observer(
            |_: On<RestingDispatch>, mut dispatch_counts: ResMut<DispatchCounts>| {
                dispatch_counts.resting += 1;
            },
        );
        app.world_mut().add_observer(
            |_: On<ActiveDispatch>, mut dispatch_counts: ResMut<DispatchCounts>| {
                dispatch_counts.active += 1;
            },
        );
        app.finish();
        app.update();
        app.world_mut().spawn(ActiveFact);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyG);

        app.update();

        assert_eq!(app.world().resource::<DispatchCounts>().resting, 1);
        assert_eq!(app.world().resource::<DispatchCounts>().active, 0);
        app.update();
        let expected_handle = app
            .world()
            .resource::<ConditionRegistry>()
            .resolve("active");
        assert_eq!(
            app.world().resource::<ActiveCondition>().handle(),
            expected_handle
        );
    }

    #[test]
    fn empty_tables_report_each_unreachable_variant_and_none_for_single_variants() {
        let mut multi_variant_app =
            app_with_derived_context(DerivedContext::new(ValidationContext::Resting));
        multi_variant_app
            .world_mut()
            .insert_resource(State::new(ValidationContext::First));

        multi_variant_app.update();

        assert_eq!(
            state::<ValidationContext>(&multi_variant_app),
            ValidationContext::Resting
        );
        assert_eq!(context_advisories(&multi_variant_app, "first"), 1);
        assert_eq!(context_advisories(&multi_variant_app, "second"), 1);

        let single_variant_app =
            app_with_derived_context(DerivedContext::new(SingleVariantContext::Only));

        assert_eq!(all_context_advisories(&single_variant_app), 0);
    }

    fn register_dispatch_commands(app: &mut App) {
        app.world_mut().insert_resource(AppTypeRegistry::default());
        let app_type_registry = app.world().resource::<AppTypeRegistry>().clone();
        let mut type_registry = app_type_registry.write();
        type_registry.register::<ActiveDispatch>();
        type_registry.register::<RestingDispatch>();
    }
}
