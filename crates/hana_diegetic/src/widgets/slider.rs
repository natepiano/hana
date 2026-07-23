use core::cmp::Ordering;

use bevy::picking::pointer::PointerId;
use bevy::prelude::*;

use super::PanelWidget;
use super::WidgetDisabled;
use super::WidgetKind;
use crate::PanelElementId;

/// Direction in which slider values increase.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SliderDirection {
    /// Values increase from left to right.
    #[default]
    LeftToRight,
    /// Values increase from right to left.
    RightToLeft,
    /// Values increase from bottom to top.
    BottomToTop,
    /// Values increase from top to bottom.
    TopToBottom,
}

/// Validated numeric range for a slider.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderRange {
    start: f32,
    end:   f32,
}

impl SliderRange {
    /// Creates a finite slider range whose start is strictly less than its end.
    ///
    /// # Errors
    ///
    /// Returns [`SliderConfigError::NonFiniteRange`] when either endpoint is
    /// non-finite, or [`SliderConfigError::UnorderedRange`] when `start` is not
    /// strictly less than `end`.
    pub fn new(start: f32, end: f32) -> Result<Self, SliderConfigError> {
        if !start.is_finite() || !end.is_finite() {
            return Err(SliderConfigError::NonFiniteRange);
        }
        if start >= end {
            return Err(SliderConfigError::UnorderedRange);
        }
        Ok(Self { start, end })
    }

    /// Returns the inclusive lower endpoint.
    #[must_use]
    pub const fn start(self) -> f32 { self.start }

    /// Returns the inclusive upper endpoint.
    #[must_use]
    pub const fn end(self) -> f32 { self.end }
}

/// Validated step interval for a slider.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderStep(f32);

impl SliderStep {
    /// Creates a finite, positive slider step.
    ///
    /// # Errors
    ///
    /// Returns [`SliderConfigError::NonPositiveStep`] when `step` is non-finite,
    /// zero, or negative.
    pub fn new(step: f32) -> Result<Self, SliderConfigError> {
        if !step.is_finite() || step <= 0.0 {
            return Err(SliderConfigError::NonPositiveStep);
        }
        Ok(Self(step))
    }

    /// Returns the step interval.
    #[must_use]
    pub const fn value(self) -> f32 { self.0 }
}

/// Authored configuration for a panel slider.
///
/// Attach it to an element with [`El::slider`](crate::El::slider). Reified
/// sliders store their applied value in [`SliderState`] and propose changes
/// through [`SliderChangeRequested`].
#[must_use]
#[derive(Clone, Debug, PartialEq)]
pub struct Slider {
    range:         SliderRange,
    initial_value: f32,
    step:          Option<SliderStep>,
    direction:     SliderDirection,
}

impl Slider {
    /// Creates a slider declaration with a finite initial value.
    ///
    /// # Errors
    ///
    /// Returns [`SliderConfigError::NonFiniteValue`] when `initial_value` is
    /// non-finite.
    pub fn new(range: SliderRange, initial_value: f32) -> Result<Self, SliderConfigError> {
        if !initial_value.is_finite() {
            return Err(SliderConfigError::NonFiniteValue);
        }
        Ok(Self {
            range,
            initial_value,
            step: None,
            direction: SliderDirection::default(),
        })
    }

    /// Sets the validated step interval.
    pub const fn step(mut self, step: SliderStep) -> Self {
        self.step = Some(step);
        self
    }

    /// Sets the direction in which values increase.
    pub const fn direction(mut self, direction: SliderDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Builds the first-spawn runtime state for this authored slider.
    ///
    /// The authored initial value applies only here; later reifications
    /// preserve the live applied value.
    pub(crate) fn initial_state(&self) -> SliderState {
        SliderState::from_validated(self.range, self.initial_value, self.step, self.direction)
    }
}

/// Applied runtime state of a reified slider widget.
///
/// First reification constructs this component from the authored [`Slider`];
/// later reifications preserve the live applied value and revalidate it when
/// the authored range, step, or direction changes. Application code applies
/// or rejects each [`SliderChangeRequested`] proposal with
/// [`SliderState::set_value`], or opts into the uncontrolled convenience by
/// observing with [`slider_self_update`].
#[derive(Clone, Component, Debug, PartialEq)]
pub struct SliderState {
    range:     SliderRange,
    value:     f32,
    step:      Option<SliderStep>,
    direction: SliderDirection,
}

impl SliderState {
    /// Creates slider state with a finite applied value.
    ///
    /// The value snaps to the step lattice anchored at
    /// [`SliderRange::start`], then clamps into the range.
    ///
    /// # Errors
    ///
    /// Returns [`SliderConfigError::NonFiniteValue`] when `value` is
    /// non-finite.
    pub fn new(
        range: SliderRange,
        value: f32,
        step: Option<SliderStep>,
        direction: SliderDirection,
    ) -> Result<Self, SliderConfigError> {
        if !value.is_finite() {
            return Err(SliderConfigError::NonFiniteValue);
        }
        Ok(Self::from_validated(range, value, step, direction))
    }

    fn from_validated(
        range: SliderRange,
        value: f32,
        step: Option<SliderStep>,
        direction: SliderDirection,
    ) -> Self {
        Self {
            range,
            value: normalized(range, step, value),
            step,
            direction,
        }
    }

    /// Applies `value` after snapping it to the step lattice anchored at
    /// [`SliderRange::start`] and clamping it into the range.
    ///
    /// Returns whether the applied value changed.
    ///
    /// # Errors
    ///
    /// Returns [`SliderConfigError::NonFiniteValue`] when `value` is
    /// non-finite; the applied value is left untouched.
    pub fn set_value(&mut self, value: f32) -> Result<bool, SliderConfigError> {
        if !value.is_finite() {
            return Err(SliderConfigError::NonFiniteValue);
        }
        let next = normalized(self.range, self.step, value);
        let changed = next.partial_cmp(&self.value) != Some(Ordering::Equal);
        self.value = next;
        Ok(changed)
    }

    /// Returns the validated range.
    #[must_use]
    pub const fn range(&self) -> SliderRange { self.range }

    /// Returns the applied raw-domain value.
    #[must_use]
    pub const fn value(&self) -> f32 { self.value }

    /// Returns the optional step interval.
    #[must_use]
    pub const fn step(&self) -> Option<SliderStep> { self.step }

    /// Returns the direction in which values increase.
    #[must_use]
    pub const fn direction(&self) -> SliderDirection { self.direction }

    /// Whether this state already carries `authored`'s range, step, and
    /// direction; the authored initial value is spawn-only and never
    /// compared.
    pub(crate) fn matches_configuration(&self, authored: &Slider) -> bool {
        self.range == authored.range
            && self.step == authored.step
            && self.direction == authored.direction
    }

    /// Rebuilds state around `authored`'s configuration while preserving
    /// `value`, revalidating it through the same snap-then-clamp order as
    /// [`SliderState::set_value`].
    pub(crate) fn with_configuration(authored: &Slider, value: f32) -> Self {
        Self::from_validated(authored.range, value, authored.step, authored.direction)
    }

    /// Computes the raw-domain target for `adjustment` without normalizing it,
    /// or `None` when the adjustment's numeric input is non-finite or a
    /// step-relative adjustment finds no step. Normalization is deferred to
    /// [`SliderState::set_value`] when the application accepts the proposal.
    fn adjusted_value(&self, adjustment: SliderAdjustment) -> Option<f32> {
        let target = match adjustment {
            SliderAdjustment::Absolute(value) => value,
            SliderAdjustment::Relative(delta) => self.value + delta,
            SliderAdjustment::RelativeSteps(steps) => {
                let step = self.step?;
                steps.mul_add(step.value(), self.value)
            },
        };
        target.is_finite().then_some(target)
    }
}

fn normalized(range: SliderRange, step: Option<SliderStep>, value: f32) -> f32 {
    let snapped = step.map_or(value, |step| {
        ((value - range.start()) / step.value())
            .round()
            .mul_add(step.value(), range.start())
    });
    snapped.clamp(range.start(), range.end())
}

/// Proposes a new applied value for a slider widget.
///
/// Hana emits proposals and never applies them; application code applies or
/// rejects each with [`SliderState::set_value`], or installs
/// [`slider_self_update`] for uncontrolled sliders. Semantic and remote
/// requests are final and carry no pointer.
#[derive(Clone, Debug, EntityEvent)]
pub struct SliderChangeRequested {
    /// Live slider entity receiving the proposal.
    #[event_target]
    pub entity:     Entity,
    /// Panel-local slider id.
    pub id:         PanelElementId,
    /// Proposed raw-domain value. It is not snapped or clamped; application
    /// code normalizes it through [`SliderState::set_value`] when it accepts
    /// the proposal.
    pub value:      f32,
    /// Whether this proposal completes its originating interaction.
    pub is_final:   bool,
    /// Proposing pointer, or `None` for semantic or remote requests.
    pub pointer_id: Option<PointerId>,
}

/// Requests a computed slider-value proposal without applying it.
///
/// The handler reads the target's [`SliderState`], computes the raw-domain
/// target value, and emits one final [`SliderChangeRequested`] with no
/// pointer. A controller starting from an authored `(panel, id)` resolves the
/// widget entity through [`PanelWidgetReader`](crate::PanelWidgetReader)
/// before constructing the request. Disabled sliders ignore requests.
#[derive(Clone, Copy, Debug, EntityEvent)]
pub struct RequestSliderAdjustment {
    /// Live slider entity to adjust.
    #[event_target]
    pub entity:     Entity,
    /// How to compute the proposed value from the applied value.
    pub adjustment: SliderAdjustment,
}

/// Value computation for a [`RequestSliderAdjustment`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SliderAdjustment {
    /// Proposes the given raw-domain value.
    Absolute(f32),
    /// Proposes the applied value plus the given raw-domain delta.
    Relative(f32),
    /// Proposes the applied value plus the given number of step intervals.
    ///
    /// Emits no proposal when the slider has no step.
    RelativeSteps(f32),
}

/// Emits one final [`SliderChangeRequested`] proposal for a computed
/// adjustment on an enabled slider.
pub(super) fn handle_adjustment_request(
    request: On<RequestSliderAdjustment>,
    sliders: Query<(&PanelWidget, &WidgetKind, &SliderState, Has<WidgetDisabled>)>,
    mut commands: Commands,
) {
    let entity = request.event_target();
    let Ok((widget, kind, state, disabled)) = sliders.get(entity) else {
        return;
    };
    if *kind != WidgetKind::Slider || disabled {
        return;
    }
    let Some(value) = state.adjusted_value(request.adjustment) else {
        return;
    };
    commands.trigger(SliderChangeRequested {
        entity,
        id: widget.id().clone(),
        value,
        is_final: true,
        pointer_id: None,
    });
}

/// Opt-in uncontrolled-slider observer that applies every proposal.
///
/// Hana never installs this observer; an application that does not own its
/// slider values adds it with
/// `app.add_observer(slider_self_update)`. Controlled sliders instead observe
/// [`SliderChangeRequested`] directly and decide with
/// [`SliderState::set_value`].
pub fn slider_self_update(change: On<SliderChangeRequested>, mut sliders: Query<&mut SliderState>) {
    let Ok(mut state) = sliders.get_mut(change.event_target()) else {
        return;
    };
    match state.bypass_change_detection().set_value(change.value) {
        Ok(true) => state.set_changed(),
        Ok(false) => {},
        Err(error) => warn!("ignored invalid slider proposal: {error}"),
    }
}

/// Invalid slider authoring configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum SliderConfigError {
    /// A range endpoint was non-finite.
    #[error("slider range endpoints must be finite")]
    NonFiniteRange,
    /// The range start was not strictly less than the range end.
    #[error("slider range start must be less than its end")]
    UnorderedRange,
    /// A slider value was non-finite.
    #[error("slider value must be finite")]
    NonFiniteValue,
    /// A slider step was non-finite, zero, or negative.
    #[error("slider step must be finite and positive")]
    NonPositiveStep,
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::input::ButtonState;
    use bevy::input::InputPlugin;
    use bevy::input::keyboard::Key;
    use bevy::input::keyboard::KeyboardInput;
    use bevy::input::keyboard::NativeKey;
    use bevy::picking::pointer::PointerId;
    use bevy::prelude::*;
    use bevy_enhanced_input::prelude::ActionSettings;
    use bevy_enhanced_input::prelude::ActionSpawner;
    use bevy_enhanced_input::prelude::Actions;
    use bevy_enhanced_input::prelude::EnhancedInputPlugin;
    use bevy_enhanced_input::prelude::Fire;
    use bevy_enhanced_input::prelude::InputAction;
    use bevy_enhanced_input::prelude::InputContextAppExt;
    use bevy_kana::Keybindings;

    use super::RequestSliderAdjustment;
    use super::Slider;
    use super::SliderAdjustment;
    use super::SliderChangeRequested;
    use super::SliderConfigError;
    use super::SliderDirection;
    use super::SliderRange;
    use super::SliderState;
    use super::SliderStep;
    use super::slider_self_update;
    use crate::DiegeticPanel;
    use crate::DiegeticPanelCommands;
    use crate::El;
    use crate::HeadlessLayoutPlugin;
    use crate::LayoutBuilder;
    use crate::LayoutTree;
    use crate::Mm;
    use crate::PanelElementId;
    use crate::PanelWidgetReader;
    use crate::WidgetInteractivity;
    use crate::text::DiegeticTextMeasurer;
    use crate::widgets::WidgetDisabled;
    use crate::widgets::WidgetsPlugin;

    #[test]
    fn range_rejects_non_finite_endpoints() {
        assert_eq!(
            SliderRange::new(f32::NAN, 1.0),
            Err(SliderConfigError::NonFiniteRange)
        );
        assert_eq!(
            SliderRange::new(0.0, f32::INFINITY),
            Err(SliderConfigError::NonFiniteRange)
        );
    }

    #[test]
    fn range_rejects_unordered_endpoints() {
        assert_eq!(
            SliderRange::new(1.0, 1.0),
            Err(SliderConfigError::UnorderedRange)
        );
        assert_eq!(
            SliderRange::new(2.0, 1.0),
            Err(SliderConfigError::UnorderedRange)
        );
    }

    #[test]
    fn slider_rejects_non_finite_initial_value() {
        let Ok(range) = SliderRange::new(0.0, 1.0) else {
            return;
        };
        assert_eq!(
            Slider::new(range, f32::NAN),
            Err(SliderConfigError::NonFiniteValue)
        );
    }

    #[test]
    fn step_rejects_non_positive_or_non_finite_values() {
        for invalid in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                SliderStep::new(invalid),
                Err(SliderConfigError::NonPositiveStep)
            );
        }
    }

    #[test]
    fn valid_slider_retains_authored_configuration() {
        let Ok(range) = SliderRange::new(-1.0, 2.0) else {
            return;
        };
        let Ok(step) = SliderStep::new(0.25) else {
            return;
        };
        let Ok(slider) = Slider::new(range, 0.5) else {
            return;
        };
        let slider = slider.step(step).direction(SliderDirection::TopToBottom);

        assert_eq!(slider.range, range);
        assert!((slider.initial_value - 0.5).abs() <= f32::EPSILON);
        assert_eq!(slider.step, Some(step));
        assert_eq!(slider.direction, SliderDirection::TopToBottom);
    }

    #[test]
    fn error_messages_are_stable() {
        let messages = [
            (
                SliderConfigError::NonFiniteRange,
                "slider range endpoints must be finite",
            ),
            (
                SliderConfigError::UnorderedRange,
                "slider range start must be less than its end",
            ),
            (
                SliderConfigError::NonFiniteValue,
                "slider value must be finite",
            ),
            (
                SliderConfigError::NonPositiveStep,
                "slider step must be finite and positive",
            ),
        ];

        for (error, message) in messages {
            assert_eq!(error.to_string(), message);
        }
    }

    #[derive(Default, Resource)]
    struct RecordedProposals(Vec<(Entity, f32, bool, Option<PointerId>)>);

    #[derive(Resource)]
    struct SliderTarget(Entity);

    #[derive(Component)]
    struct SliderInputContext;

    #[derive(InputAction)]
    #[action_output(bool)]
    struct AdjustShift;

    #[derive(InputAction)]
    #[action_output(bool)]
    struct AdjustHeld;

    fn record_proposal(change: On<SliderChangeRequested>, mut recorded: ResMut<RecordedProposals>) {
        recorded.0.push((
            change.event_target(),
            change.value,
            change.is_final,
            change.pointer_id,
        ));
    }

    fn send_held_adjustment(
        _: On<Fire<AdjustHeld>>,
        target: Res<SliderTarget>,
        mut commands: Commands,
    ) {
        commands.trigger(RequestSliderAdjustment {
            entity:     target.0,
            adjustment: SliderAdjustment::RelativeSteps(1.0),
        });
    }

    fn spawn_slider_input(mut commands: Commands) {
        commands.spawn((
            SliderInputContext,
            Actions::<SliderInputContext>::spawn(SpawnWith(spawn_slider_actions)),
        ));
    }

    fn spawn_slider_actions(spawner: &mut ActionSpawner<SliderInputContext>) {
        let keybindings = Keybindings::new::<AdjustShift>(spawner, ActionSettings::default());
        keybindings.spawn_key::<AdjustHeld>(spawner, KeyCode::ArrowRight);
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(DiegeticTextMeasurer::default())
            .add_plugins((HeadlessLayoutPlugin, WidgetsPlugin))
            .init_resource::<RecordedProposals>()
            .add_observer(record_proposal);
        app
    }

    fn range(start: f32, end: f32) -> SliderRange {
        SliderRange::new(start, end).expect("range should validate")
    }

    fn step(interval: f32) -> SliderStep {
        SliderStep::new(interval).expect("step should validate")
    }

    fn slider_tree(id: &str, slider: Slider) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(El::new().size(20.0, 10.0).slider(id, slider), |_| {});
        builder.build()
    }

    fn spawn_panel(app: &mut App, tree: LayoutTree) -> Entity {
        let panel = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .with_tree(tree)
            .build()
            .expect("panel should build");
        app.world_mut().spawn(panel).id()
    }

    fn resolve_widget(app: &mut App, panel: Entity, id: &str) -> Entity {
        let id = PanelElementId::named(id);
        app.world_mut()
            .run_system_once(move |reader: PanelWidgetReader| reader.entity(panel, &id))
            .ok()
            .flatten()
            .expect("widget should be reified")
    }

    fn request_adjustment(app: &mut App, entity: Entity, adjustment: SliderAdjustment) {
        app.world_mut()
            .trigger(RequestSliderAdjustment { entity, adjustment });
        app.world_mut().flush();
    }

    fn slider_value(app: &App, widget: Entity) -> f32 {
        app.world()
            .get::<SliderState>(widget)
            .expect("widget should carry slider state")
            .value()
    }

    fn press_key(app: &mut App, window: Entity, key_code: KeyCode) {
        app.world_mut().write_message(KeyboardInput {
            key_code,
            logical_key: Key::Unidentified(NativeKey::Unidentified),
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window,
        });
    }

    #[track_caller]
    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-6,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn state_snaps_to_lattice_anchored_at_range_start() {
        // The lattice for range start 1.0 and step 0.4 is 1.0, 1.4, 1.8; a
        // zero-anchored lattice would land on 1.6 instead.
        let state = SliderState::new(
            range(1.0, 2.0),
            1.55,
            Some(step(0.4)),
            SliderDirection::LeftToRight,
        )
        .expect("state should validate");
        assert_close(state.value(), 1.4);
    }

    #[test]
    fn state_clamps_after_snapping() {
        let mut state = SliderState::new(
            range(0.0, 1.0),
            0.5,
            Some(step(0.4)),
            SliderDirection::LeftToRight,
        )
        .expect("state should validate");

        // Snapping 5.0 lands on 5.2's lattice point far above the range; the
        // final clamp keeps the range end even though it is off-lattice.
        assert_eq!(state.set_value(5.0), Ok(true));
        assert_close(state.value(), 1.0);
        assert_eq!(state.set_value(-3.0), Ok(true));
        assert_close(state.value(), 0.0);
    }

    #[test]
    fn state_rejects_non_finite_values_without_touching_applied() {
        assert_eq!(
            SliderState::new(
                range(0.0, 1.0),
                f32::NAN,
                None,
                SliderDirection::LeftToRight
            ),
            Err(SliderConfigError::NonFiniteValue)
        );

        let mut state = SliderState::new(range(0.0, 1.0), 0.5, None, SliderDirection::LeftToRight)
            .expect("state should validate");
        assert_eq!(
            state.set_value(f32::INFINITY),
            Err(SliderConfigError::NonFiniteValue)
        );
        assert_close(state.value(), 0.5);
    }

    #[test]
    fn set_value_reports_whether_the_applied_value_changed() {
        let mut state = SliderState::new(
            range(0.0, 1.0),
            0.5,
            Some(step(0.25)),
            SliderDirection::LeftToRight,
        )
        .expect("state should validate");

        assert_eq!(state.set_value(0.55), Ok(false));
        assert_close(state.value(), 0.5);
        assert_eq!(state.set_value(0.7), Ok(true));
        assert_close(state.value(), 0.75);

        // -0.0 equals 0.0 numerically, so replacing one with the other is
        // not a change.
        let mut state = SliderState::new(range(-1.0, 1.0), 0.0, None, SliderDirection::LeftToRight)
            .expect("state should validate");
        assert_eq!(state.set_value(-0.0), Ok(false));
    }

    #[test]
    fn reify_retains_each_authored_direction() {
        let directions = [
            ("ltr", SliderDirection::LeftToRight),
            ("rtl", SliderDirection::RightToLeft),
            ("btt", SliderDirection::BottomToTop),
            ("ttb", SliderDirection::TopToBottom),
        ];
        let mut app = test_app();
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        for (id, direction) in directions {
            let slider = Slider::new(range(0.0, 1.0), 0.5)
                .expect("slider should validate")
                .direction(direction);
            builder.with(El::new().size(20.0, 10.0).slider(id, slider), |_| {});
        }
        let panel = spawn_panel(&mut app, builder.build());
        app.update();

        for (id, direction) in directions {
            let widget = resolve_widget(&mut app, panel, id);
            let state = app
                .world()
                .get::<SliderState>(widget)
                .expect("slider should carry state");
            assert_eq!(state.direction(), direction);
        }
    }

    #[test]
    fn first_spawn_normalizes_out_of_range_and_off_step_initial_values() {
        let mut app = test_app();
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        for (id, initial_value) in [("over", 25.0), ("off", 4.4)] {
            let slider = Slider::new(range(0.0, 10.0), initial_value)
                .expect("slider should validate")
                .step(step(3.0));
            builder.with(El::new().size(20.0, 10.0).slider(id, slider), |_| {});
        }
        let panel = spawn_panel(&mut app, builder.build());
        app.update();

        let over = resolve_widget(&mut app, panel, "over");
        assert_close(slider_value(&app, over), 10.0);
        let off = resolve_widget(&mut app, panel, "off");
        assert_close(slider_value(&app, off), 3.0);
    }

    #[test]
    fn proposals_apply_only_through_explicit_app_decisions() {
        let mut app = test_app();
        let slider = Slider::new(range(0.0, 1.0), 0.5).expect("slider should validate");
        let panel = spawn_panel(&mut app, slider_tree("level", slider));
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");

        request_adjustment(&mut app, widget, SliderAdjustment::Absolute(0.8));
        {
            let recorded = &app.world().resource::<RecordedProposals>().0;
            assert_eq!(recorded.len(), 1, "the request should emit one proposal");
            let (target, value, is_final, pointer_id) = recorded[0];
            assert_eq!(target, widget);
            assert_close(value, 0.8);
            assert!(is_final);
            assert_eq!(pointer_id, None);
        }
        assert_close(slider_value(&app, widget), 0.5);

        // The app accepts the first proposal explicitly.
        let applied = app
            .world_mut()
            .get_mut::<SliderState>(widget)
            .expect("widget should carry slider state")
            .set_value(0.8);
        assert_eq!(applied, Ok(true));
        assert_close(slider_value(&app, widget), 0.8);

        // The app rejects the second proposal by not applying it.
        request_adjustment(&mut app, widget, SliderAdjustment::Absolute(0.2));
        assert_eq!(app.world().resource::<RecordedProposals>().0.len(), 2);
        assert_close(slider_value(&app, widget), 0.8);
    }

    #[test]
    fn opt_in_self_update_applies_each_proposal() {
        let mut app = test_app();
        app.add_observer(slider_self_update);
        let slider = Slider::new(range(0.0, 10.0), 5.0)
            .expect("slider should validate")
            .step(step(0.5));
        let panel = spawn_panel(&mut app, slider_tree("level", slider));
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");

        request_adjustment(&mut app, widget, SliderAdjustment::Absolute(7.3));
        assert_close(slider_value(&app, widget), 7.5);
        request_adjustment(&mut app, widget, SliderAdjustment::Relative(1.2));
        assert_close(slider_value(&app, widget), 8.5);
        request_adjustment(&mut app, widget, SliderAdjustment::RelativeSteps(2.0));
        assert_close(slider_value(&app, widget), 9.5);
        assert_eq!(app.world().resource::<RecordedProposals>().0.len(), 3);
    }

    #[test]
    fn self_update_marks_state_changed_only_when_the_applied_value_changes() {
        let mut app = test_app();
        app.add_observer(slider_self_update);
        let slider = Slider::new(range(0.0, 1.0), 0.5).expect("slider should validate");
        let panel = spawn_panel(&mut app, slider_tree("level", slider));
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");

        request_adjustment(&mut app, widget, SliderAdjustment::Absolute(2.0));
        assert_close(slider_value(&app, widget), 1.0);
        let applied_tick = app
            .world()
            .entity(widget)
            .get_ref::<SliderState>()
            .expect("widget should carry slider state")
            .last_changed();

        // The repeated proposal clamps to the same endpoint; applying it must
        // not wake change detection.
        request_adjustment(&mut app, widget, SliderAdjustment::Absolute(2.0));
        assert_close(slider_value(&app, widget), 1.0);
        assert_eq!(
            app.world()
                .entity(widget)
                .get_ref::<SliderState>()
                .expect("widget should carry slider state")
                .last_changed(),
            applied_tick,
            "an unchanged proposal must not mark slider state changed",
        );
    }

    #[test]
    fn adjustment_request_emits_raw_target_that_set_value_normalizes_to_endpoint() {
        let mut app = test_app();
        app.add_observer(slider_self_update);
        // Range end 1.0 is off the step-0.3 lattice; applied 0.9 sits on it.
        let slider = Slider::new(range(0.0, 1.0), 0.9)
            .expect("slider should validate")
            .step(step(0.3));
        let panel = spawn_panel(&mut app, slider_tree("level", slider));
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");
        assert_close(slider_value(&app, widget), 0.9);

        // One step up from 0.9 targets raw 1.2; the proposal carries that raw
        // value without pre-normalizing it.
        request_adjustment(&mut app, widget, SliderAdjustment::RelativeSteps(1.0));
        let recorded = &app.world().resource::<RecordedProposals>().0;
        assert_eq!(recorded.len(), 1);
        assert_close(recorded[0].1, 1.2);

        // set_value snaps 1.2 back onto the lattice then clamps into range, so
        // the slider lands on the range endpoint 1.0. Pre-normalizing the
        // proposal to 1.0 would instead snap back to 0.9.
        assert_close(slider_value(&app, widget), 1.0);
    }

    #[test]
    fn remote_requests_resolve_from_panel_and_id_and_validate_input() {
        let mut app = test_app();
        let slider = Slider::new(range(0.0, 10.0), 5.0).expect("slider should validate");
        let panel = spawn_panel(&mut app, slider_tree("level", slider));
        app.update();

        // An app controller starts from the authored `(panel, id)` pair and
        // resolves the live widget entity before constructing the request.
        let widget = resolve_widget(&mut app, panel, "level");
        request_adjustment(&mut app, widget, SliderAdjustment::Relative(0.4));
        let recorded = &app.world().resource::<RecordedProposals>().0;
        assert_eq!(recorded.len(), 1);
        assert_close(recorded[0].1, 5.4);

        // A step-relative request without a step and non-finite numeric
        // input both emit no proposal.
        request_adjustment(&mut app, widget, SliderAdjustment::RelativeSteps(1.0));
        request_adjustment(&mut app, widget, SliderAdjustment::Absolute(f32::NAN));
        request_adjustment(&mut app, widget, SliderAdjustment::Relative(f32::INFINITY));
        assert_eq!(app.world().resource::<RecordedProposals>().0.len(), 1);
    }

    #[test]
    fn disabled_sliders_ignore_semantic_requests() {
        let mut app = test_app();
        let slider = Slider::new(range(0.0, 1.0), 0.5).expect("slider should validate");
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(
            El::new()
                .size(20.0, 10.0)
                .slider("level", slider)
                .widget_interactivity(WidgetInteractivity::Disabled),
            |_| {},
        );
        let panel = spawn_panel(&mut app, builder.build());
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");
        assert!(app.world().get::<WidgetDisabled>(widget).is_some());

        request_adjustment(&mut app, widget, SliderAdjustment::Absolute(0.8));
        assert!(app.world().resource::<RecordedProposals>().0.is_empty());
        assert_close(slider_value(&app, widget), 0.5);
    }

    #[test]
    fn reuse_preserves_live_value_and_revalidates_configuration_changes() {
        let mut app = test_app();
        let slider = Slider::new(range(0.0, 10.0), 4.0).expect("slider should validate");
        let panel = spawn_panel(&mut app, slider_tree("level", slider));
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");
        let applied = app
            .world_mut()
            .get_mut::<SliderState>(widget)
            .expect("widget should carry slider state")
            .set_value(9.0);
        assert_eq!(applied, Ok(true));
        let live_tick = app
            .world()
            .entity(widget)
            .get_ref::<SliderState>()
            .expect("widget should carry slider state")
            .last_changed();

        // An authored initial-value change alone is spawn-only: the reused
        // widget keeps its live value and the state is not rewritten.
        let replacement = Slider::new(range(0.0, 10.0), 2.0).expect("slider should validate");
        app.world_mut()
            .commands()
            .set_tree(panel, slider_tree("level", replacement))
            .expect("replacement tree should be accepted");
        app.update();
        assert_eq!(resolve_widget(&mut app, panel, "level"), widget);
        assert_close(slider_value(&app, widget), 9.0);
        assert_eq!(
            app.world()
                .entity(widget)
                .get_ref::<SliderState>()
                .expect("widget should retain slider state")
                .last_changed(),
            live_tick,
            "an unrelated reify must not rewrite slider state",
        );

        // An authored range change updates the configuration and revalidates
        // the preserved live value against it.
        let narrowed = Slider::new(range(0.0, 5.0), 2.0).expect("slider should validate");
        app.world_mut()
            .commands()
            .set_tree(panel, slider_tree("level", narrowed))
            .expect("narrowed tree should be accepted");
        app.update();
        let state = app
            .world()
            .get::<SliderState>(widget)
            .expect("widget should retain slider state");
        assert_close(state.range().end(), 5.0);
        assert_close(state.value(), 5.0);
    }

    #[test]
    fn held_kana_action_sends_one_adjustment_request_per_fire() {
        let mut app = test_app();
        app.add_plugins((InputPlugin, EnhancedInputPlugin))
            .add_input_context::<SliderInputContext>()
            .add_systems(Startup, spawn_slider_input)
            .add_observer(send_held_adjustment)
            .add_observer(slider_self_update);
        let slider = Slider::new(range(0.0, 10.0), 5.0)
            .expect("slider should validate")
            .step(step(0.5));
        let panel = spawn_panel(&mut app, slider_tree("level", slider));
        let window = app
            .world_mut()
            .spawn(Window {
                focused: true,
                ..default()
            })
            .id();
        app.finish();
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");
        app.insert_resource(SliderTarget(widget));

        press_key(&mut app, window, KeyCode::ArrowRight);
        app.update();
        assert_eq!(app.world().resource::<RecordedProposals>().0.len(), 1);
        assert_close(slider_value(&app, widget), 5.5);

        // The key stays held; each later frame fires the action again and
        // each fire sends exactly one request.
        app.update();
        assert_eq!(app.world().resource::<RecordedProposals>().0.len(), 2);
        assert_close(slider_value(&app, widget), 6.0);
    }
}
