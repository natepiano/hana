use core::cmp::Ordering;
use std::collections::HashMap;

use bevy::camera::NormalizedRenderTarget;
use bevy::camera::RenderTarget;
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::system::SystemParam;
use bevy::ecs::world::DeferredWorld;
use bevy::picking::hover::PickingInteraction;
use bevy::picking::pointer::PointerId;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::PanelWidget;
use super::VisualSlotId;
use super::VisualSlotOverride;
use super::WidgetDisabled;
use super::WidgetFocusVisible;
use super::WidgetKind;
use super::WidgetOf;
use super::WidgetSpec;
use super::WidgetVisualOverrides;
use super::WidgetVisualSlots;
use super::capture::WidgetCaptures;
use super::capture::trigger_immediate;
use super::visual;
use crate::DiegeticPanel;
use crate::PanelElementId;
use crate::layout::BoundingBox;
use crate::render::CapturedCameraRay;
use crate::render::project_flat_panel_ray_hit;

/// Registers slider runtime state, pointer capture, and adjustment handling.
pub(super) struct SliderPlugin;

impl Plugin for SliderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SliderCaptures>()
            .add_observer(handle_adjustment_request)
            .add_observer(grab_from_pointer)
            .add_observer(drag_from_pointer)
            .add_observer(release_from_pointer)
            .add_observer(release_from_drag_end)
            .add_observer(cancel_from_pointer)
            .add_observer(cancel_from_pointer_removal)
            .add_observer(cancel_from_disabled)
            .add_observer(cancel_from_widget_removal)
            .add_observer(cancel_before_widget_despawn)
            .add_observer(handle_semantic_intent);
    }
}

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

/// Per-state presentation values for one slider root state layer.
#[derive(Clone, Debug, Default, PartialEq)]
struct SliderStateValues {
    background:   Option<Color>,
    border_color: Option<Color>,
    material:     Option<Handle<StandardMaterial>>,
}

/// One [`SliderStateValues`] layer per widget state, authored by the direct
/// `Slider` root-state builders.
#[derive(Clone, Debug, Default, PartialEq)]
struct SliderStatePresentation {
    hovered:  SliderStateValues,
    pressed:  SliderStateValues,
    focused:  SliderStateValues,
    disabled: SliderStateValues,
}

/// Authored configuration for a panel slider.
///
/// Attach it to an element with [`El::slider`](crate::El::slider). Reified
/// sliders store their applied value in [`SliderState`] and propose changes
/// through [`SliderChangeRequested`]. The normal surface stays on the
/// element's ordinary [`El::background`](crate::El::background),
/// [`El::border`](crate::El::border), and [`El::material`](crate::El::material)
/// declarations; the root-state builders here patch only that root surface's
/// retained records at runtime, layering normal → focused → hovered → pressed →
/// disabled per property with missing values falling through to the prior
/// layer. Child thumb and label appearance stays application-authored and
/// constant.
#[must_use]
#[derive(Clone, Debug, PartialEq)]
pub struct Slider {
    range:         SliderRange,
    initial_value: f32,
    step:          Option<SliderStep>,
    direction:     SliderDirection,
    states:        Option<Box<SliderStatePresentation>>,
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
            states: None,
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

    /// Sets the root background color shown while a pointer hovers the slider.
    ///
    /// Requires an authored [`El::background`](crate::El::background) on the
    /// slider element.
    pub fn hovered_background(mut self, color: Color) -> Self {
        self.states_mut().hovered.background = Some(color);
        self
    }

    /// Sets the root background color shown while the slider is dragged.
    ///
    /// Requires an authored [`El::background`](crate::El::background) on the
    /// slider element.
    pub fn pressed_background(mut self, color: Color) -> Self {
        self.states_mut().pressed.background = Some(color);
        self
    }

    /// Sets the root background color shown while the slider's keyboard focus
    /// indicator is visible.
    ///
    /// Requires an authored [`El::background`](crate::El::background) on the
    /// slider element.
    pub fn focused_background(mut self, color: Color) -> Self {
        self.states_mut().focused.background = Some(color);
        self
    }

    /// Sets the root background color shown while the slider is disabled.
    ///
    /// Requires an authored [`El::background`](crate::El::background) on the
    /// slider element.
    pub fn disabled_background(mut self, color: Color) -> Self {
        self.states_mut().disabled.background = Some(color);
        self
    }

    /// Sets the root border color shown while a pointer hovers the slider.
    ///
    /// Requires an authored [`El::border`](crate::El::border) on the slider
    /// element; border widths and radii stay as authored.
    pub fn hovered_border_color(mut self, color: Color) -> Self {
        self.states_mut().hovered.border_color = Some(color);
        self
    }

    /// Sets the root border color shown while the slider is dragged.
    ///
    /// Requires an authored [`El::border`](crate::El::border) on the slider
    /// element; border widths and radii stay as authored.
    pub fn pressed_border_color(mut self, color: Color) -> Self {
        self.states_mut().pressed.border_color = Some(color);
        self
    }

    /// Sets the root border color shown while the slider's keyboard focus
    /// indicator is visible.
    ///
    /// Requires an authored [`El::border`](crate::El::border) on the slider
    /// element; border widths and radii stay as authored.
    pub fn focused_border_color(mut self, color: Color) -> Self {
        self.states_mut().focused.border_color = Some(color);
        self
    }

    /// Sets the root border color shown while the slider is disabled.
    ///
    /// Requires an authored [`El::border`](crate::El::border) on the slider
    /// element; border widths and radii stay as authored.
    pub fn disabled_border_color(mut self, color: Color) -> Self {
        self.states_mut().disabled.border_color = Some(color);
        self
    }

    /// Sets the root surface material shown while a pointer hovers the slider.
    ///
    /// Applies to both the authored fill and border. Requires an authored root
    /// surface — [`El::background`](crate::El::background) or
    /// [`El::border`](crate::El::border) — on the slider element.
    pub fn hovered_material(mut self, material: Handle<StandardMaterial>) -> Self {
        self.states_mut().hovered.material = Some(material);
        self
    }

    /// Sets the root surface material shown while the slider is dragged.
    ///
    /// Applies to both the authored fill and border. Requires an authored root
    /// surface — [`El::background`](crate::El::background) or
    /// [`El::border`](crate::El::border) — on the slider element.
    pub fn pressed_material(mut self, material: Handle<StandardMaterial>) -> Self {
        self.states_mut().pressed.material = Some(material);
        self
    }

    /// Sets the root surface material shown while the slider's keyboard focus
    /// indicator is visible.
    ///
    /// Applies to both the authored fill and border. Requires an authored root
    /// surface — [`El::background`](crate::El::background) or
    /// [`El::border`](crate::El::border) — on the slider element.
    pub fn focused_material(mut self, material: Handle<StandardMaterial>) -> Self {
        self.states_mut().focused.material = Some(material);
        self
    }

    /// Sets the root surface material shown while the slider is disabled.
    ///
    /// Applies to both the authored fill and border. Requires an authored root
    /// surface — [`El::background`](crate::El::background) or
    /// [`El::border`](crate::El::border) — on the slider element.
    pub fn disabled_material(mut self, material: Handle<StandardMaterial>) -> Self {
        self.states_mut().disabled.material = Some(material);
        self
    }

    fn states_mut(&mut self) -> &mut SliderStatePresentation {
        self.states.get_or_insert_with(Default::default).as_mut()
    }

    /// Whether any state layer authors a background color.
    pub(crate) fn has_state_background(&self) -> bool {
        self.states.as_deref().is_some_and(|states| {
            states.focused.background.is_some()
                || states.hovered.background.is_some()
                || states.pressed.background.is_some()
                || states.disabled.background.is_some()
        })
    }

    /// Whether any state layer authors a border color.
    pub(crate) fn has_state_border_color(&self) -> bool {
        self.states.as_deref().is_some_and(|states| {
            states.focused.border_color.is_some()
                || states.hovered.border_color.is_some()
                || states.pressed.border_color.is_some()
                || states.disabled.border_color.is_some()
        })
    }

    /// Whether any state layer authors a surface material.
    pub(crate) fn has_state_material(&self) -> bool {
        self.states.as_deref().is_some_and(|states| {
            states.focused.material.is_some()
                || states.hovered.material.is_some()
                || states.pressed.material.is_some()
                || states.disabled.material.is_some()
        })
    }

    /// Composes the desired root-slot override for the active state set.
    ///
    /// Each property layers independently in the fixed order normal → focused →
    /// hovered → pressed → disabled; a state without a value for a property
    /// leaves the prior layer intact, and `None` means the authored normal
    /// value.
    fn state_override(&self, active: [bool; 4]) -> VisualSlotOverride {
        let Some(states) = self.states.as_deref() else {
            return VisualSlotOverride::default();
        };
        let mut layered = SliderStateValues::default();
        for (active, values) in active.into_iter().zip([
            &states.focused,
            &states.hovered,
            &states.pressed,
            &states.disabled,
        ]) {
            if !active {
                continue;
            }
            if let Some(background) = values.background {
                layered.background = Some(background);
            }
            if let Some(border_color) = values.border_color {
                layered.border_color = Some(border_color);
            }
            if let Some(material) = &values.material {
                layered.material = Some(material.clone());
            }
        }
        VisualSlotOverride {
            fill_color: layered.background,
            border_color: layered.border_color,
            material: layered.material,
            ..VisualSlotOverride::default()
        }
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

/// A pointer position cannot be projected onto the slider's directed travel.
///
/// A marked thumb whose extent meets or exceeds the content extent has no
/// visible travel, and a headless slider with zero active-axis content extent
/// has no directed interval; both leave nothing to propose. Non-finite
/// active-axis geometry and a negative or non-finite thumb extent are
/// rejected the same way.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProjectionUnavailable;

/// Projects a panel-layout pointer position into a raw slider-domain target.
///
/// The directed interval follows the thumb center's path inside `content` —
/// the slider root's content box, excluding border and padding: a marked
/// thumb insets each directed endpoint by half `thumb_extent` on the active
/// axis, while a headless slider (`thumb_extent` of `None`) travels the full
/// directed content extent. The pointer's active-axis coordinate is clamped
/// into that interval, normalized to `[0, 1]`, and mapped through the raw
/// `range`. The result carries no snap or step normalization; application
/// acceptance through [`SliderState::set_value`] performs the only snap and
/// clamp.
pub(crate) fn project_pointer_value(
    pointer: Vec2,
    content: BoundingBox,
    thumb_extent: Option<f32>,
    range: SliderRange,
    direction: SliderDirection,
) -> Result<f32, ProjectionUnavailable> {
    let travel = directed_thumb_travel(content, thumb_extent, direction)?;
    let pointer_coordinate = match direction {
        SliderDirection::LeftToRight | SliderDirection::RightToLeft => pointer.x,
        SliderDirection::BottomToTop | SliderDirection::TopToBottom => pointer.y,
    };
    let clamped =
        pointer_coordinate.clamp(travel.start.min(travel.end), travel.start.max(travel.end));
    let fraction = (clamped - travel.start) / (travel.end - travel.start);
    let target = range
        .start()
        .mul_add(1.0 - fraction, range.end() * fraction);
    target
        .is_finite()
        .then_some(target)
        .ok_or(ProjectionUnavailable)
}

/// Active-axis coordinates the thumb center reaches at the directed range
/// start and end.
///
/// `start` maps to the normalized value `0` and `end` to `1`, so both pointer
/// projection and value presentation share one directed interval.
#[derive(Clone, Copy)]
struct DirectedThumbTravel {
    start: f32,
    end:   f32,
}

/// Computes the directed thumb-center interval inside `content`.
///
/// A marked thumb insets each directed endpoint by half `thumb_extent`; a
/// headless slider (`thumb_extent` of `None`) travels the full directed content
/// extent. Non-finite active-axis geometry, a negative or non-finite thumb
/// extent, and a thumb extent that meets or exceeds the content extent all
/// leave no directed travel and return [`ProjectionUnavailable`].
fn directed_thumb_travel(
    content: BoundingBox,
    thumb_extent: Option<f32>,
    direction: SliderDirection,
) -> Result<DirectedThumbTravel, ProjectionUnavailable> {
    let (axis_start, axis_extent) = match direction {
        SliderDirection::LeftToRight | SliderDirection::RightToLeft => (content.x, content.width),
        SliderDirection::BottomToTop | SliderDirection::TopToBottom => (content.y, content.height),
    };
    if !axis_start.is_finite() || !axis_extent.is_finite() || axis_extent <= 0.0 {
        return Err(ProjectionUnavailable);
    }
    if thumb_extent.is_some_and(|extent| !extent.is_finite() || extent < 0.0) {
        return Err(ProjectionUnavailable);
    }
    let endpoint_inset = thumb_extent.map_or(0.0, |extent| extent * 0.5);
    if endpoint_inset.mul_add(-2.0, axis_extent) <= 0.0 {
        return Err(ProjectionUnavailable);
    }
    let range_start_coordinate = axis_start + endpoint_inset;
    let range_end_coordinate = axis_start + axis_extent - endpoint_inset;
    let (start, end) = match direction {
        SliderDirection::LeftToRight | SliderDirection::TopToBottom => {
            (range_start_coordinate, range_end_coordinate)
        },
        SliderDirection::RightToLeft | SliderDirection::BottomToTop => {
            (range_end_coordinate, range_start_coordinate)
        },
    };
    Ok(DirectedThumbTravel { start, end })
}

/// Reports the beginning of a pointer-driven slider drag.
#[derive(Clone, Debug, EntityEvent)]
pub struct SliderGrabbed {
    /// Live slider entity the pointer grabbed.
    #[event_target]
    pub entity:     Entity,
    /// Panel-local slider id.
    pub id:         PanelElementId,
    /// Pointer that began the drag.
    pub pointer_id: PointerId,
}

/// Reports the valid release of a pointer-driven slider drag.
///
/// The matching final [`SliderChangeRequested`] proposal is emitted first,
/// after the pointer's shared occupancy is freed.
#[derive(Clone, Debug, EntityEvent)]
pub struct SliderReleased {
    /// Live slider entity the pointer released.
    #[event_target]
    pub entity:     Entity,
    /// Panel-local slider id.
    pub id:         PanelElementId,
    /// Pointer that completed the drag.
    pub pointer_id: PointerId,
}

/// Reports a pointer-driven slider drag that ended without a valid release.
#[derive(Clone, Debug, EntityEvent)]
pub struct SliderCanceled {
    /// Live slider entity whose drag was canceled.
    #[event_target]
    pub entity:     Entity,
    /// Panel-local slider id.
    pub id:         PanelElementId,
    /// Pointer whose drag was canceled.
    pub pointer_id: PointerId,
    /// Reason the drag ended without a valid release.
    pub cause:      SliderCancelCause,
}

/// Reason a pointer-driven slider drag was canceled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SliderCancelCause {
    /// The pointer input stream reported cancellation.
    PointerCanceled,
    /// The captured pointer entity or its [`PointerId`] was removed.
    PointerRemoved,
    /// The captured pointer released without an active drag to complete.
    CaptureLost,
    /// The slider became disabled while dragging.
    Disabled,
    /// A drag or release position could no longer project onto the slider.
    ProjectionUnavailable,
    /// The widget was removed or its owning panel role ended.
    WidgetRemoved,
    /// The same widget id changed to another widget kind.
    WidgetKindChanged,
    /// Semantic input explicitly canceled the drag.
    Explicit,
}

/// Live marker for a captured slider drag.
///
/// Removing or despawning it runs [`emit_slider_terminal`], which frees the
/// shared occupancy and emits exactly one terminal event from the recorded
/// [`SliderTerminal`] outcome.
#[derive(Component)]
#[component(
    on_remove = emit_slider_terminal,
    on_despawn = emit_slider_terminal
)]
pub(crate) struct SliderDrag;

/// Recorded terminal outcome for one captured slider drag.
#[derive(Clone, Copy)]
enum SliderTerminal {
    /// The drag is live; no terminal has been requested.
    Pending,
    /// A valid release reprojected the given raw-domain value.
    Release(f32),
    /// The drag was canceled for the given reason.
    Cancel(SliderCancelCause),
}

/// Slider-only capture payload keyed by the captured widget entity.
///
/// Pointer/widget occupancy and raw-action ordering live in the shared
/// [`WidgetCaptures`]; this holds only the slider facts terminal emission and
/// reprojection need: the panel-local id, the captured camera entity and
/// normalized render target, the latest raw projected target, and the typed
/// terminal outcome.
struct SliderCapture {
    id:                PanelElementId,
    camera:            Entity,
    captured_target:   NormalizedRenderTarget,
    latest_raw_target: f32,
    terminal:          SliderTerminal,
}

/// A projected slider press that shared occupancy rejected, awaiting recapture
/// by the raw dispatcher once its pointer and widget free within the batch.
struct PendingSliderPress {
    entity:          Entity,
    sequence:        u64,
    id:              PanelElementId,
    camera:          Entity,
    captured_target: NormalizedRenderTarget,
    value:           f32,
}

/// Slider drag payloads and rejected-press pending records.
#[derive(Default, Resource)]
pub(crate) struct SliderCaptures {
    drags:   HashMap<Entity, SliderCapture>,
    pending: HashMap<PointerId, PendingSliderPress>,
}

impl SliderCaptures {
    fn begin(
        &mut self,
        entity: Entity,
        id: PanelElementId,
        camera: Entity,
        captured_target: NormalizedRenderTarget,
        value: f32,
    ) {
        self.drags.insert(
            entity,
            SliderCapture {
                id,
                camera,
                captured_target,
                latest_raw_target: value,
                terminal: SliderTerminal::Pending,
            },
        );
    }

    /// Returns the captured camera entity and normalized render target for a
    /// live drag, cloned so reprojection can borrow this resource mutably next.
    fn capture_context(&self, entity: Entity) -> Option<(Entity, NormalizedRenderTarget)> {
        self.drags
            .get(&entity)
            .map(|capture| (capture.camera, capture.captured_target.clone()))
    }

    fn update_raw_target(&mut self, entity: Entity, value: f32) {
        if let Some(capture) = self.drags.get_mut(&entity) {
            capture.latest_raw_target = value;
        }
    }

    /// Records a valid release outcome when the drag is still pending, storing
    /// the reprojected value as both the terminal payload and latest raw
    /// target. Returns whether the outcome was recorded.
    fn set_release(&mut self, entity: Entity, value: f32) -> bool {
        match self.drags.get_mut(&entity) {
            Some(capture) if matches!(capture.terminal, SliderTerminal::Pending) => {
                capture.terminal = SliderTerminal::Release(value);
                capture.latest_raw_target = value;
                true
            },
            _ => false,
        }
    }

    /// Records a cancellation when the drag is still pending. Returns whether
    /// the outcome was recorded.
    fn cancel(&mut self, entity: Entity, cause: SliderCancelCause) -> bool {
        match self.drags.get_mut(&entity) {
            Some(capture) if matches!(capture.terminal, SliderTerminal::Pending) => {
                capture.terminal = SliderTerminal::Cancel(cause);
                true
            },
            _ => false,
        }
    }

    fn take(&mut self, entity: Entity) -> Option<SliderCapture> { self.drags.remove(&entity) }

    fn record_pending(&mut self, pointer_id: PointerId, pending: PendingSliderPress) {
        self.pending.insert(pointer_id, pending);
    }

    fn take_pending(&mut self, pointer_id: PointerId) -> Option<PendingSliderPress> {
        self.pending.remove(&pointer_id)
    }

    pub(crate) fn has_pending(&self) -> bool { !self.pending.is_empty() }

    pub(crate) fn clear_pending(&mut self) { self.pending.clear(); }

    #[cfg(test)]
    pub(crate) fn latest_raw_target(&self, entity: Entity) -> Option<f32> {
        self.drags
            .get(&entity)
            .map(|capture| capture.latest_raw_target)
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool { self.drags.is_empty() && self.pending.is_empty() }
}

/// Live camera, panel, and window state for reprojecting a slider pointer.
#[derive(SystemParam)]
pub(crate) struct SliderProjection<'w, 's> {
    cameras: Query<
        'w,
        's,
        (
            &'static Camera,
            &'static GlobalTransform,
            Option<&'static RenderTarget>,
        ),
    >,
    panels:  Query<'w, 's, (&'static DiegeticPanel, &'static GlobalTransform)>,
    primary: Query<'w, 's, Entity, With<PrimaryWindow>>,
}

impl SliderProjection<'_, '_> {
    /// Projects `viewport_position` through the live captured camera and panel
    /// onto the slider's raw-domain travel.
    ///
    /// Any [`FlatPanelRayError`](crate::render::panel_geometry::FlatPanelRayError)
    /// or [`ProjectionUnavailable`] maps to
    /// [`SliderCancelCause::ProjectionUnavailable`].
    fn project(
        &self,
        camera_entity: Entity,
        captured_target: &NormalizedRenderTarget,
        viewport_position: Vec2,
        panel_entity: Entity,
        content: BoundingBox,
        thumb_extent: Option<f32>,
        range: SliderRange,
        direction: SliderDirection,
    ) -> Result<f32, SliderCancelCause> {
        let Ok((camera, camera_transform, render_target)) = self.cameras.get(camera_entity) else {
            return Err(SliderCancelCause::ProjectionUnavailable);
        };
        let Ok((panel, panel_transform)) = self.panels.get(panel_entity) else {
            return Err(SliderCancelCause::ProjectionUnavailable);
        };
        let primary_window = self.primary.single().ok();
        let captured_camera_ray = CapturedCameraRay {
            camera,
            camera_transform,
            render_target,
            primary_window,
            captured_target,
            viewport_position,
        };
        let pointer_local =
            project_flat_panel_ray_hit(&captured_camera_ray, panel, panel_transform)
                .map_err(|_| SliderCancelCause::ProjectionUnavailable)?;
        project_pointer_value(pointer_local, content, thumb_extent, range, direction)
            .map_err(|_| SliderCancelCause::ProjectionUnavailable)
    }
}

/// Active-axis extent of the marked thumb's solved border box, or `None` when
/// the slider marks no thumb.
fn thumb_extent(slots: &WidgetVisualSlots, direction: SliderDirection) -> Option<f32> {
    let thumb = slots.border_box(VisualSlotId::SLIDER_THUMB)?;
    Some(match direction {
        SliderDirection::LeftToRight | SliderDirection::RightToLeft => thumb.width,
        SliderDirection::BottomToTop | SliderDirection::TopToBottom => thumb.height,
    })
}

fn project_widget(
    projection: &SliderProjection<'_, '_>,
    camera: Entity,
    captured_target: &NormalizedRenderTarget,
    viewport_position: Vec2,
    panel: Entity,
    state: &SliderState,
    slots: &WidgetVisualSlots,
) -> Result<f32, SliderCancelCause> {
    let Some(content) = slots.content_box(VisualSlotId::SLIDER_ROOT) else {
        return Err(SliderCancelCause::ProjectionUnavailable);
    };
    projection.project(
        camera,
        captured_target,
        viewport_position,
        panel,
        content,
        thumb_extent(slots, state.direction()),
        state.range(),
        state.direction(),
    )
}

/// Layout-frame translation delta — layout points with Y increasing downward —
/// that moves the marked thumb so its center sits at the applied value, or
/// `None` when the slider marks no thumb, the active-axis content geometry is
/// non-finite or zero-extent, or the thumb extent is non-finite or negative.
///
/// The delta stays in layout coordinates so it shares the solved slot geometry
/// with pointer projection; [`present_slider_state`] converts it once through
/// the owning panel to the panel-local render frame
/// [`VisualSlotOverride::offset`] is consumed in.
///
/// A finite oversized thumb — active-axis extent at or beyond the content extent
/// — centers on the content box's active axis; every other finite case solves
/// the desired center from the same directed endpoint interval pointer
/// projection uses, then subtracts the thumb's solved authored center. The
/// returned delta preserves the thumb's cross axis and authored draw depth.
fn thumb_translation(slots: &WidgetVisualSlots, state: &SliderState) -> Option<Vec2> {
    let content = slots.content_box(VisualSlotId::SLIDER_ROOT)?;
    let thumb = slots.border_box(VisualSlotId::SLIDER_THUMB)?;
    let direction = state.direction();
    let range = state.range();
    let (thumb_extent, authored_center, axis_start, axis_extent) = match direction {
        SliderDirection::LeftToRight | SliderDirection::RightToLeft => {
            (thumb.width, thumb.center().0, content.x, content.width)
        },
        SliderDirection::BottomToTop | SliderDirection::TopToBottom => {
            (thumb.height, thumb.center().1, content.y, content.height)
        },
    };
    // Only finite `T >= C` geometry centers the thumb. Non-finite
    // active-axis geometry, a zero-extent content box, and a non-finite or
    // negative thumb extent stay unavailable and manufacture no translation, so
    // that `directed_thumb_travel`'s only remaining error is the oversized-thumb
    // case handled below.
    if !axis_start.is_finite() || !axis_extent.is_finite() || axis_extent <= 0.0 {
        return None;
    }
    if !thumb_extent.is_finite() || thumb_extent < 0.0 {
        return None;
    }
    let content_center = axis_extent.mul_add(0.5, axis_start);
    let range_extent = range.end() - range.start();
    let fraction = ((state.value() - range.start()) / range_extent).clamp(0.0, 1.0);
    let desired_center = match directed_thumb_travel(content, Some(thumb_extent), direction) {
        Ok(travel) => travel.start + fraction * (travel.end - travel.start),
        Err(ProjectionUnavailable) => content_center,
    };
    let delta = desired_center - authored_center;
    if !delta.is_finite() {
        return None;
    }
    Some(match direction {
        SliderDirection::LeftToRight | SliderDirection::RightToLeft => Vec2::new(delta, 0.0),
        SliderDirection::BottomToTop | SliderDirection::TopToBottom => Vec2::new(0.0, delta),
    })
}

/// Run condition for [`present_slider_state`]: reports whether any authored
/// presentation or presented state input changed on a live slider since the
/// last run.
///
/// The changed query filters to [`WidgetKind::Slider`] so an unrelated button
/// change never wakes the all-slider walk. `Changed<WidgetSpec>` /
/// `Changed<WidgetVisualSlots>` cover reify and re-authoring,
/// `Changed<SliderState>` covers the applied value, `Changed<PickingInteraction>`
/// covers the hover/pressed aggregate, and `Changed` on [`WidgetFocusVisible`],
/// [`WidgetDisabled`], and [`SliderDrag`] covers marker insertion. Each
/// [`RemovedComponents`] stream is drained every run and its removals are kept
/// only for entities still live as sliders, reporting the edges back to normal.
pub(super) fn presentation_inputs_changed(
    changed: Query<
        &WidgetKind,
        (
            With<WidgetOf>,
            Or<(
                Changed<WidgetSpec>,
                Changed<WidgetVisualSlots>,
                Changed<PickingInteraction>,
                Changed<WidgetFocusVisible>,
                Changed<WidgetDisabled>,
                Changed<SliderState>,
                Changed<SliderDrag>,
            )>,
        ),
    >,
    kinds: Query<&WidgetKind, With<WidgetOf>>,
    mut removed_interactions: RemovedComponents<PickingInteraction>,
    mut removed_focus: RemovedComponents<WidgetFocusVisible>,
    mut removed_disabled: RemovedComponents<WidgetDisabled>,
    mut removed_drags: RemovedComponents<SliderDrag>,
) -> bool {
    // `count` drains every stream so a consumed removal cannot re-trigger a
    // later quiet frame, and each removal counts only for an entity still live
    // as a slider — `WidgetKind::Slider` with `With<WidgetOf>`, matching the
    // writer — so an unrelated button removal or a widget whose panel role
    // ended never wakes the walk.
    let slider_removals = removed_interactions
        .read()
        .chain(removed_focus.read())
        .chain(removed_disabled.read())
        .chain(removed_drags.read())
        .filter(|&entity| matches!(kinds.get(entity), Ok(WidgetKind::Slider)))
        .count();
    slider_removals > 0 || changed.iter().any(|kind| *kind == WidgetKind::Slider)
}

/// Maps each live slider's state onto its root and thumb visual-slot overrides.
///
/// Runs after `WidgetSystems::FocusCommandsApplied`, so application and
/// keyboard-traversal indicator commands, as well as pointer-driven indicator
/// removal, are visible in the same frame. It runs only when
/// [`presentation_inputs_changed`] reports a relevant slider edge, so a quiet
/// frame never walks the live sliders. Hover reads the all-pointer
/// [`PickingInteraction`] aggregate and pressed reads the private [`SliderDrag`]
/// marker; [`SliderCaptures`] stays lifecycle authority and is never consulted
/// for presentation. Writes go through [`visual::write_slot_override`], which
/// compares immutably first, so an unchanged state never marks
/// [`WidgetVisualOverrides`] changed. The thumb slot is always resolved so a
/// same-id slider re-authored from a marked thumb to no thumb clears its stale
/// translation through that same writer.
pub(super) fn present_slider_state(
    sliders: Query<
        (
            Entity,
            &WidgetSpec,
            &WidgetKind,
            &WidgetOf,
            &SliderState,
            &WidgetVisualSlots,
            Option<&PickingInteraction>,
            Has<WidgetDisabled>,
            Has<WidgetFocusVisible>,
            Has<SliderDrag>,
        ),
        With<WidgetOf>,
    >,
    panels: Query<&DiegeticPanel>,
    mut overrides: Query<&mut WidgetVisualOverrides>,
    mut commands: Commands,
) {
    for (
        entity,
        authored,
        kind,
        widget_of,
        state,
        slots,
        interaction,
        disabled,
        focused,
        dragging,
    ) in &sliders
    {
        if *kind != WidgetKind::Slider {
            continue;
        }
        let WidgetSpec::Slider(slider) = authored else {
            continue;
        };
        if slots.element_index(VisualSlotId::SLIDER_ROOT).is_some() {
            let active = [
                focused,
                matches!(
                    interaction,
                    Some(PickingInteraction::Hovered | PickingInteraction::Pressed)
                ),
                dragging,
                disabled,
            ];
            visual::write_slot_override(
                entity,
                VisualSlotId::SLIDER_ROOT,
                slider.state_override(active),
                &mut overrides,
                &mut commands,
            );
        }
        // Always resolve the thumb slot so a same-id slider re-authored from a
        // marked thumb to no thumb clears its stale translation through the
        // shared `write_slot_override` clear path. The layout-frame delta is
        // converted once through the owning `DiegeticPanel` to the panel-local
        // render frame the retained routes consume; an absent thumb, a missing
        // panel, or an invalid scale yields the default override, which the
        // immutable-before-mutable writer treats as a clear rather than a
        // manufactured offset.
        let render_offset = thumb_translation(slots, state).and_then(|layout_delta| {
            let panel = panels.get(widget_of.panel()).ok()?;
            visual::layout_delta_to_render_offset(layout_delta, panel.points_to_world())
        });
        let thumb_override =
            render_offset.map_or_else(VisualSlotOverride::default, |offset| VisualSlotOverride {
                offset: Some(offset),
                ..VisualSlotOverride::default()
            });
        visual::write_slot_override(
            entity,
            VisualSlotId::SLIDER_THUMB,
            thumb_override,
            &mut overrides,
            &mut commands,
        );
    }
}

fn begin_drag(
    slider_captures: &mut SliderCaptures,
    commands: &mut Commands<'_, '_>,
    entity: Entity,
    id: PanelElementId,
    camera: Entity,
    captured_target: NormalizedRenderTarget,
    value: f32,
    pointer_id: PointerId,
) {
    slider_captures.begin(entity, id.clone(), camera, captured_target, value);
    commands.entity(entity).insert(SliderDrag);
    commands.trigger(SliderGrabbed {
        entity,
        id: id.clone(),
        pointer_id,
    });
    commands.trigger(SliderChangeRequested {
        entity,
        id,
        value,
        is_final: false,
        pointer_id: Some(pointer_id),
    });
}

/// Projects a slider press and, on success, claims occupancy and begins the
/// drag; a rejected but projected press is recorded for raw recapture.
pub(super) fn grab_from_pointer(
    mut press: On<Pointer<Press>>,
    widgets: Query<
        (
            &PanelWidget,
            &WidgetKind,
            &WidgetOf,
            &SliderState,
            &WidgetVisualSlots,
            Has<WidgetDisabled>,
            Has<SliderDrag>,
        ),
        With<WidgetOf>,
    >,
    projection: SliderProjection,
    mut captures: ResMut<WidgetCaptures>,
    mut slider_captures: ResMut<SliderCaptures>,
    mut commands: Commands,
) {
    if press.button != PointerButton::Primary {
        return;
    }
    let entity = press.event_target();
    let Ok((widget, kind, widget_of, state, slots, disabled, dragging)) = widgets.get(entity)
    else {
        return;
    };
    if *kind != WidgetKind::Slider {
        return;
    }
    press.propagate(false);
    if disabled {
        return;
    }
    let camera = press.hit.camera;
    let captured_target = press.pointer_location.target.clone();
    let viewport_position = press.pointer_location.position;
    let Ok(value) = project_widget(
        &projection,
        camera,
        &captured_target,
        viewport_position,
        widget_of.panel(),
        state,
        slots,
    ) else {
        return;
    };
    let Some(sequence) = captures.observe_press(press.pointer_id, entity) else {
        return;
    };
    if !dragging && captures.try_capture(press.pointer_id, entity, sequence) {
        begin_drag(
            &mut slider_captures,
            &mut commands,
            entity,
            widget.id().clone(),
            camera,
            captured_target,
            value,
            press.pointer_id,
        );
    } else {
        slider_captures.record_pending(
            press.pointer_id,
            PendingSliderPress {
                entity,
                sequence,
                id: widget.id().clone(),
                camera,
                captured_target,
                value,
            },
        );
    }
}

/// Reprojects each drag and proposes the new raw target, canceling the drag
/// when the drag position no longer projects.
pub(super) fn drag_from_pointer(
    mut drag: On<Pointer<Drag>>,
    widgets: Query<(
        &PanelWidget,
        &WidgetKind,
        &WidgetOf,
        &SliderState,
        &WidgetVisualSlots,
    )>,
    projection: SliderProjection,
    captures: Res<WidgetCaptures>,
    mut slider_captures: ResMut<SliderCaptures>,
    mut commands: Commands,
) {
    if drag.button != PointerButton::Primary {
        return;
    }
    let entity = drag.event_target();
    let Ok((widget, kind, widget_of, state, slots)) = widgets.get(entity) else {
        return;
    };
    if *kind != WidgetKind::Slider {
        return;
    }
    drag.propagate(false);
    if !captures.captures(drag.pointer_id, entity) {
        return;
    }
    let Some((camera, captured_target)) = slider_captures.capture_context(entity) else {
        return;
    };
    match project_widget(
        &projection,
        camera,
        &captured_target,
        drag.pointer_location.position,
        widget_of.panel(),
        state,
        slots,
    ) {
        Ok(value) => {
            slider_captures.update_raw_target(entity, value);
            commands.trigger(SliderChangeRequested {
                entity,
                id: widget.id().clone(),
                value,
                is_final: false,
                pointer_id: Some(drag.pointer_id),
            });
        },
        Err(cause) => {
            cancel_slider_drag(entity, cause, &mut slider_captures, &mut commands);
        },
    }
}

pub(super) fn release_from_pointer(
    mut release: On<Pointer<Release>>,
    widgets: Query<(
        &PanelWidget,
        &WidgetKind,
        &WidgetOf,
        &SliderState,
        &WidgetVisualSlots,
    )>,
    projection: SliderProjection,
    captures: Res<WidgetCaptures>,
    mut slider_captures: ResMut<SliderCaptures>,
    mut commands: Commands,
) {
    if release.button != PointerButton::Primary {
        return;
    }
    let entity = release.event_target();
    let Ok((_, kind, ..)) = widgets.get(entity) else {
        return;
    };
    if *kind != WidgetKind::Slider {
        return;
    }
    release.propagate(false);
    if !captures.captures(release.pointer_id, entity) {
        return;
    }
    resolve_release(
        entity,
        release.pointer_location.position,
        &widgets,
        &projection,
        &mut slider_captures,
        &mut commands,
    );
}

pub(super) fn release_from_drag_end(
    mut drag_end: On<Pointer<DragEnd>>,
    widgets: Query<(
        &PanelWidget,
        &WidgetKind,
        &WidgetOf,
        &SliderState,
        &WidgetVisualSlots,
    )>,
    projection: SliderProjection,
    captures: Res<WidgetCaptures>,
    mut slider_captures: ResMut<SliderCaptures>,
    mut commands: Commands,
) {
    if drag_end.button != PointerButton::Primary {
        return;
    }
    let entity = drag_end.event_target();
    let Ok((_, kind, ..)) = widgets.get(entity) else {
        return;
    };
    if *kind != WidgetKind::Slider {
        return;
    }
    if !captures.captures(drag_end.pointer_id, entity) {
        return;
    }
    drag_end.propagate(false);
    resolve_release(
        entity,
        drag_end.pointer_location.position,
        &widgets,
        &projection,
        &mut slider_captures,
        &mut commands,
    );
}

/// Reprojects the release position, records the release or projection-loss
/// outcome, and removes [`SliderDrag`] so its hook emits the terminal. Returns
/// whether a terminal was recorded, so the shared dispatcher marks the pointer
/// freed only when typed terminal processing will release shared occupancy. A
/// query miss, wrong kind, or absent captured payload records nothing and
/// returns false.
pub(super) fn resolve_release(
    entity: Entity,
    viewport_position: Vec2,
    widgets: &Query<
        '_,
        '_,
        (
            &PanelWidget,
            &WidgetKind,
            &WidgetOf,
            &SliderState,
            &WidgetVisualSlots,
        ),
    >,
    projection: &SliderProjection<'_, '_>,
    slider_captures: &mut SliderCaptures,
    commands: &mut Commands<'_, '_>,
) -> bool {
    let Ok((_, kind, widget_of, state, slots)) = widgets.get(entity) else {
        return false;
    };
    if *kind != WidgetKind::Slider {
        return false;
    }
    let Some((camera, captured_target)) = slider_captures.capture_context(entity) else {
        return false;
    };
    let recorded = match project_widget(
        projection,
        camera,
        &captured_target,
        viewport_position,
        widget_of.panel(),
        state,
        slots,
    ) {
        Ok(value) => slider_captures.set_release(entity, value),
        Err(cause) => slider_captures.cancel(entity, cause),
    };
    if recorded {
        commands.entity(entity).remove::<SliderDrag>();
    }
    recorded
}

pub(super) fn cancel_from_pointer(
    mut cancel: On<Pointer<Cancel>>,
    widgets: Query<&WidgetKind>,
    captures: Res<WidgetCaptures>,
    mut slider_captures: ResMut<SliderCaptures>,
    mut commands: Commands,
) {
    let entity = cancel.event_target();
    let Ok(kind) = widgets.get(entity) else {
        return;
    };
    if *kind != WidgetKind::Slider {
        return;
    }
    if captures.captures(cancel.pointer_id, entity) {
        cancel.propagate(false);
        cancel_slider_drag(
            entity,
            SliderCancelCause::PointerCanceled,
            &mut slider_captures,
            &mut commands,
        );
    }
}

pub(super) fn cancel_from_pointer_removal(
    removed: On<Remove, PointerId>,
    pointers: Query<&PointerId>,
    captures: Res<WidgetCaptures>,
    mut slider_captures: ResMut<SliderCaptures>,
    mut commands: Commands,
) {
    let Ok(&pointer_id) = pointers.get(removed.entity) else {
        return;
    };
    let Some(widget) = captures.widget(pointer_id) else {
        return;
    };
    cancel_slider_drag(
        widget,
        SliderCancelCause::PointerRemoved,
        &mut slider_captures,
        &mut commands,
    );
}

pub(super) fn cancel_from_disabled(
    disabled: On<Add, WidgetDisabled>,
    mut slider_captures: ResMut<SliderCaptures>,
    mut commands: Commands,
) {
    cancel_slider_drag(
        disabled.entity,
        SliderCancelCause::Disabled,
        &mut slider_captures,
        &mut commands,
    );
}

pub(super) fn cancel_from_widget_removal(
    removed: On<Remove, PanelWidget>,
    mut slider_captures: ResMut<SliderCaptures>,
    mut commands: Commands,
) {
    cancel_slider_drag(
        removed.entity,
        SliderCancelCause::WidgetRemoved,
        &mut slider_captures,
        &mut commands,
    );
}

pub(super) fn cancel_before_widget_despawn(
    despawn: On<Despawn, PanelWidget>,
    mut slider_captures: ResMut<SliderCaptures>,
) {
    slider_captures.cancel(despawn.entity, SliderCancelCause::WidgetRemoved);
}

pub(super) fn handle_semantic_intent(
    intent: On<super::SemanticWidgetIntent>,
    widgets: Query<&WidgetKind>,
    mut slider_captures: ResMut<SliderCaptures>,
    mut commands: Commands,
) {
    let entity = intent.event_target();
    let Ok(kind) = widgets.get(entity) else {
        return;
    };
    if *kind != WidgetKind::Slider {
        return;
    }
    if matches!(intent.event(), super::SemanticWidgetIntent::Cancel { .. }) {
        cancel_slider_drag(
            entity,
            SliderCancelCause::Explicit,
            &mut slider_captures,
            &mut commands,
        );
    }
}

/// Records a slider cancellation and, when newly recorded, removes
/// [`SliderDrag`] so its hook emits the terminal. Returns whether a terminal
/// was recorded, so the shared dispatcher knows shared occupancy will free.
pub(crate) fn cancel_slider_drag(
    entity: Entity,
    cause: SliderCancelCause,
    slider_captures: &mut SliderCaptures,
    commands: &mut Commands<'_, '_>,
) -> bool {
    let canceled = slider_captures.cancel(entity, cause);
    if canceled {
        commands.entity(entity).remove::<SliderDrag>();
    }
    canceled
}

/// Cancels every active slider drag owned by `panel`, mirroring
/// [`finalize_panel_buttons`](super::finalize_panel_buttons) for teardown.
pub(crate) fn finalize_panel_sliders(
    panel: Entity,
    slider_drags: &Query<'_, '_, (Entity, &WidgetOf), With<SliderDrag>>,
    slider_captures: &mut SliderCaptures,
    commands: &mut Commands<'_, '_>,
) {
    for (entity, widget_of) in slider_drags {
        if widget_of.panel() == panel
            && slider_captures.cancel(entity, SliderCancelCause::WidgetRemoved)
        {
            commands.entity(entity).remove::<SliderDrag>();
        }
    }
}

/// Recaptures a slider press the raw dispatcher freed within the same batch,
/// using the projection stored when its initial press was rejected.
pub(super) fn capture_reconciled_press(
    world: &mut World,
    entity: Entity,
    pointer_id: PointerId,
    sequence: u64,
) {
    let is_slider = world
        .get::<WidgetKind>(entity)
        .is_some_and(|kind| *kind == WidgetKind::Slider);
    let Some(pending) = world
        .resource_mut::<SliderCaptures>()
        .take_pending(pointer_id)
    else {
        return;
    };
    if !is_slider
        || pending.entity != entity
        || pending.sequence != sequence
        || world.get::<WidgetOf>(entity).is_none()
        || world.get::<WidgetDisabled>(entity).is_some()
        || world.get::<SliderDrag>(entity).is_some()
    {
        return;
    }
    if !world
        .resource_mut::<WidgetCaptures>()
        .try_capture(pointer_id, entity, sequence)
    {
        return;
    }
    let PendingSliderPress {
        id,
        camera,
        captured_target,
        value,
        ..
    } = pending;
    world.resource_mut::<SliderCaptures>().begin(
        entity,
        id.clone(),
        camera,
        captured_target,
        value,
    );
    world.entity_mut(entity).insert(SliderDrag);
    world.trigger(SliderGrabbed {
        entity,
        id: id.clone(),
        pointer_id,
    });
    world.trigger(SliderChangeRequested {
        entity,
        id,
        value,
        is_final: false,
        pointer_id: Some(pointer_id),
    });
}

/// Frees shared occupancy and emits one terminal event for a removed drag.
fn emit_slider_terminal(mut world: DeferredWorld, context: HookContext) {
    let entity = context.entity;
    let Some(capture) = world
        .get_resource_mut::<SliderCaptures>()
        .and_then(|mut captures| captures.take(entity))
    else {
        return;
    };
    let Some(pointer_id) = world
        .get_resource_mut::<WidgetCaptures>()
        .and_then(|mut captures| captures.release_widget(entity))
    else {
        warn!(
            "slider {:?} ({entity}) has an active drag but no capture owner; skipping terminal \
             events",
            capture.id
        );
        return;
    };
    let SliderCapture { id, terminal, .. } = capture;
    match terminal {
        SliderTerminal::Release(value) => {
            trigger_immediate(
                &mut world,
                SliderChangeRequested {
                    entity,
                    id: id.clone(),
                    value,
                    is_final: true,
                    pointer_id: Some(pointer_id),
                },
            );
            trigger_immediate(
                &mut world,
                SliderReleased {
                    entity,
                    id,
                    pointer_id,
                },
            );
        },
        SliderTerminal::Cancel(cause) => {
            trigger_immediate(
                &mut world,
                SliderCanceled {
                    entity,
                    id,
                    pointer_id,
                    cause,
                },
            );
        },
        SliderTerminal::Pending => {
            trigger_immediate(
                &mut world,
                SliderCanceled {
                    entity,
                    id,
                    pointer_id,
                    cause: SliderCancelCause::CaptureLost,
                },
            );
        },
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
    use bevy::camera::NormalizedRenderTarget;
    use bevy::camera::PerspectiveProjection;
    use bevy::camera::Projection;
    use bevy::camera::RenderTarget;
    use bevy::camera::RenderTargetInfo;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::input::ButtonState;
    use bevy::input::InputPlugin;
    use bevy::input::keyboard::Key;
    use bevy::input::keyboard::KeyboardInput;
    use bevy::input::keyboard::NativeKey;
    use bevy::picking::InteractionPlugin;
    use bevy::picking::PickingPlugin;
    use bevy::picking::PickingSettings;
    use bevy::picking::PickingSystems;
    use bevy::picking::backend::HitData;
    use bevy::picking::backend::PointerHits;
    use bevy::picking::events::PointerState;
    use bevy::picking::events::pointer_events;
    use bevy::picking::hover::HoverMap;
    use bevy::picking::hover::PickingInteraction;
    use bevy::picking::hover::PreviousHoverMap;
    use bevy::picking::pointer::Location;
    use bevy::picking::pointer::PointerAction;
    use bevy::picking::pointer::PointerId;
    use bevy::picking::pointer::PointerInput;
    use bevy::picking::pointer::PointerLocation;
    use bevy::picking::pointer::PointerMap;
    use bevy::picking::pointer::update_pointer_map;
    use bevy::prelude::*;
    use bevy::window::PrimaryWindow;
    use bevy::window::WindowRef;
    use bevy_enhanced_input::prelude::ActionSettings;
    use bevy_enhanced_input::prelude::ActionSpawner;
    use bevy_enhanced_input::prelude::Actions;
    use bevy_enhanced_input::prelude::EnhancedInputPlugin;
    use bevy_enhanced_input::prelude::Fire;
    use bevy_enhanced_input::prelude::InputAction;
    use bevy_enhanced_input::prelude::InputContextAppExt;
    use bevy_kana::Keybindings;

    use super::super::capture::WidgetCaptures;
    use super::super::capture::reconcile_pointer_input;
    use super::ProjectionUnavailable;
    use super::RequestSliderAdjustment;
    use super::Slider;
    use super::SliderAdjustment;
    use super::SliderCancelCause;
    use super::SliderCanceled;
    use super::SliderCaptures;
    use super::SliderChangeRequested;
    use super::SliderConfigError;
    use super::SliderDirection;
    use super::SliderDrag;
    use super::SliderGrabbed;
    use super::SliderRange;
    use super::SliderReleased;
    use super::SliderState;
    use super::SliderStep;
    use super::project_pointer_value;
    use super::slider_self_update;
    use crate::AlignX;
    use crate::AlignY;
    use crate::Border;
    use crate::Button;
    use crate::ButtonClicked;
    use crate::ButtonPressed;
    use crate::ButtonReleased;
    use crate::ComputedDiegeticPanel;
    use crate::DiegeticPanel;
    use crate::DiegeticPanelCommands;
    use crate::El;
    use crate::HeadlessLayoutPlugin;
    use crate::LayoutBuilder;
    use crate::LayoutTree;
    use crate::Mm;
    use crate::PanelBuildError;
    use crate::PanelElementId;
    use crate::PanelWidgetReader;
    use crate::RequestWidgetFocus;
    use crate::WidgetInputPlugin;
    use crate::WidgetInteractivity;
    use crate::layout::BoundingBox;
    use crate::text::DiegeticTextMeasurer;
    use crate::widgets::ButtonPress;
    use crate::widgets::SemanticWidgetIntent;
    use crate::widgets::VisualSlotId;
    use crate::widgets::VisualSlotOverride;
    use crate::widgets::WidgetDisabled;
    use crate::widgets::WidgetKind;
    use crate::widgets::WidgetOf;
    use crate::widgets::WidgetVisualOverrides;
    use crate::widgets::WidgetVisualSlots;
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

    fn content_box(x: f32, y: f32, width: f32, height: f32) -> BoundingBox {
        BoundingBox {
            x,
            y,
            width,
            height,
        }
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

    #[test]
    fn thumb_center_endpoints_project_exact_range_endpoints_in_all_directions() {
        // Content box (10, 20) sized 40x16 with an 8-point thumb: the thumb
        // center travels x 14..46 horizontally and y 24..32 vertically. The
        // cross-axis pointer coordinate is ignored.
        let content = content_box(10.0, 20.0, 40.0, 16.0);
        let projection_range = range(0.25, 0.75);
        let cases = [
            (
                SliderDirection::LeftToRight,
                Vec2::new(14.0, -3.0),
                Vec2::new(46.0, 99.0),
            ),
            (
                SliderDirection::RightToLeft,
                Vec2::new(46.0, -3.0),
                Vec2::new(14.0, 99.0),
            ),
            (
                SliderDirection::TopToBottom,
                Vec2::new(-3.0, 24.0),
                Vec2::new(99.0, 32.0),
            ),
            (
                SliderDirection::BottomToTop,
                Vec2::new(-3.0, 32.0),
                Vec2::new(99.0, 24.0),
            ),
        ];

        for (direction, start_center, end_center) in cases {
            let at_start = project_pointer_value(
                start_center,
                content,
                Some(8.0),
                projection_range,
                direction,
            );
            assert_eq!(
                at_start.map(f32::to_bits),
                Ok(projection_range.start().to_bits()),
                "{direction:?} range-start endpoint",
            );
            let at_end =
                project_pointer_value(end_center, content, Some(8.0), projection_range, direction);
            assert_eq!(
                at_end.map(f32::to_bits),
                Ok(projection_range.end().to_bits()),
                "{direction:?} range-end endpoint",
            );
        }
    }

    #[test]
    fn pointer_outside_directed_travel_clamps_to_range_endpoints() {
        let content = content_box(10.0, 20.0, 40.0, 16.0);
        let projection_range = range(-2.0, 6.0);

        let before_start = project_pointer_value(
            Vec2::new(-100.0, 0.0),
            content,
            Some(8.0),
            projection_range,
            SliderDirection::LeftToRight,
        );
        assert_eq!(
            before_start.map(f32::to_bits),
            Ok(projection_range.start().to_bits()),
        );
        let past_end = project_pointer_value(
            Vec2::new(500.0, 0.0),
            content,
            Some(8.0),
            projection_range,
            SliderDirection::LeftToRight,
        );
        assert_eq!(
            past_end.map(f32::to_bits),
            Ok(projection_range.end().to_bits()),
        );

        // A reversed direction clamps the same way: below the content box is
        // before the range start for BottomToTop.
        let reversed_before = project_pointer_value(
            Vec2::new(0.0, 500.0),
            content,
            None,
            projection_range,
            SliderDirection::BottomToTop,
        );
        assert_eq!(
            reversed_before.map(f32::to_bits),
            Ok(projection_range.start().to_bits()),
        );
        let reversed_past = project_pointer_value(
            Vec2::new(0.0, -500.0),
            content,
            None,
            projection_range,
            SliderDirection::BottomToTop,
        );
        assert_eq!(
            reversed_past.map(f32::to_bits),
            Ok(projection_range.end().to_bits()),
        );
    }

    #[test]
    fn headless_projection_travels_the_full_directed_content_extent() {
        let content = content_box(10.0, 20.0, 40.0, 16.0);
        let projection_range = range(0.0, 8.0);

        let at_left = project_pointer_value(
            Vec2::new(10.0, 0.0),
            content,
            None,
            projection_range,
            SliderDirection::LeftToRight,
        );
        assert_eq!(
            at_left.map(f32::to_bits),
            Ok(projection_range.start().to_bits()),
        );
        let at_right = project_pointer_value(
            Vec2::new(50.0, 0.0),
            content,
            None,
            projection_range,
            SliderDirection::LeftToRight,
        );
        assert_eq!(
            at_right.map(f32::to_bits),
            Ok(projection_range.end().to_bits()),
        );
        let at_quarter = project_pointer_value(
            Vec2::new(20.0, 0.0),
            content,
            None,
            projection_range,
            SliderDirection::LeftToRight,
        )
        .expect("interior positions should project");
        assert_close(at_quarter, 2.0);
    }

    #[test]
    fn off_lattice_targets_stay_raw_for_set_value_to_normalize() {
        // The helper takes no step: an interior pointer projects the raw
        // interpolant, and only application acceptance through
        // `SliderState::set_value` snaps it onto the lattice.
        let content = content_box(0.0, 0.0, 100.0, 10.0);
        let projection_range = range(0.0, 1.0);
        let raw = project_pointer_value(
            Vec2::new(33.0, 0.0),
            content,
            None,
            projection_range,
            SliderDirection::LeftToRight,
        )
        .expect("interior positions should project");
        assert_close(raw, 0.33);

        let mut state = SliderState::new(
            projection_range,
            0.0,
            Some(step(0.25)),
            SliderDirection::LeftToRight,
        )
        .expect("state should validate");
        assert_eq!(state.set_value(raw), Ok(true));
        assert_close(state.value(), 0.25);
    }

    #[test]
    fn projection_reads_the_border_and_padding_excluded_content_box() {
        // Synthetic slider root at (0, 0) sized 60x20 with an asymmetric
        // border (4 left, 2 right) and 3-point padding; the caller passes the
        // remaining content box.
        let root_width = 60.0;
        let border_left = 4.0;
        let border_right = 2.0;
        let padding = 3.0;
        let content_left = border_left + padding;
        let content_width = root_width - content_left - (border_right + padding);
        let content = content_box(content_left, 5.0, content_width, 10.0);
        let projection_range = range(0.0, 1.0);

        // The content box's left edge is the exact range-start endpoint; the
        // root's left edge lies outside the interval and clamps to the same
        // value.
        let at_content_edge = project_pointer_value(
            Vec2::new(content_left, 10.0),
            content,
            None,
            projection_range,
            SliderDirection::LeftToRight,
        );
        assert_eq!(
            at_content_edge.map(f32::to_bits),
            Ok(projection_range.start().to_bits()),
        );
        let at_root_edge = project_pointer_value(
            Vec2::new(0.0, 10.0),
            content,
            None,
            projection_range,
            SliderDirection::LeftToRight,
        );
        assert_eq!(
            at_root_edge.map(f32::to_bits),
            Ok(projection_range.start().to_bits()),
        );

        // The interval midpoint is the content box's center, not the root
        // box's center: the asymmetric border shifts them apart.
        let (content_center_x, _) = content.center();
        let at_content_center = project_pointer_value(
            Vec2::new(content_center_x, 10.0),
            content,
            None,
            projection_range,
            SliderDirection::LeftToRight,
        )
        .expect("interior positions should project");
        assert_close(at_content_center, 0.5);
        let at_root_center = project_pointer_value(
            Vec2::new(root_width * 0.5, 10.0),
            content,
            None,
            projection_range,
            SliderDirection::LeftToRight,
        )
        .expect("interior positions should project");
        assert_close(
            at_root_center,
            (root_width * 0.5 - content_left) / content_width,
        );
    }

    #[test]
    fn empty_directed_travel_is_unavailable() {
        let projection_range = range(0.0, 1.0);

        // Zero active-axis content extent for a headless slider.
        let zero_width = content_box(10.0, 20.0, 0.0, 16.0);
        assert_eq!(
            project_pointer_value(
                Vec2::new(10.0, 0.0),
                zero_width,
                None,
                projection_range,
                SliderDirection::LeftToRight,
            ),
            Err(ProjectionUnavailable),
        );
        let zero_height = content_box(10.0, 20.0, 40.0, 0.0);
        assert_eq!(
            project_pointer_value(
                Vec2::new(0.0, 20.0),
                zero_height,
                None,
                projection_range,
                SliderDirection::TopToBottom,
            ),
            Err(ProjectionUnavailable),
        );

        // A marked thumb meeting or exceeding the content extent has no
        // visible travel.
        let content = content_box(10.0, 20.0, 40.0, 16.0);
        for oversized_thumb in [40.0, 64.0] {
            assert_eq!(
                project_pointer_value(
                    Vec2::new(30.0, 0.0),
                    content,
                    Some(oversized_thumb),
                    projection_range,
                    SliderDirection::LeftToRight,
                ),
                Err(ProjectionUnavailable),
            );
        }
        assert_eq!(
            project_pointer_value(
                Vec2::new(0.0, 28.0),
                content,
                Some(16.0),
                projection_range,
                SliderDirection::BottomToTop,
            ),
            Err(ProjectionUnavailable),
        );
    }

    #[test]
    fn non_finite_pointer_projects_no_raw_target() {
        let content = content_box(0.0, 0.0, 100.0, 10.0);
        let projection_range = range(0.0, 1.0);
        assert_eq!(
            project_pointer_value(
                Vec2::new(f32::NAN, 0.0),
                content,
                None,
                projection_range,
                SliderDirection::LeftToRight,
            ),
            Err(ProjectionUnavailable),
        );
    }

    #[test]
    fn negative_or_non_finite_thumb_extent_is_unavailable() {
        let content = content_box(10.0, 20.0, 40.0, 16.0);
        let projection_range = range(0.0, 1.0);
        for thumb_extent in [-8.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                project_pointer_value(
                    Vec2::new(30.0, 0.0),
                    content,
                    Some(thumb_extent),
                    projection_range,
                    SliderDirection::LeftToRight,
                ),
                Err(ProjectionUnavailable),
                "thumb extent {thumb_extent}",
            );
        }
    }

    #[test]
    fn non_finite_active_axis_geometry_is_unavailable() {
        let projection_range = range(0.0, 1.0);
        let cases = [
            (
                content_box(f32::NAN, 20.0, 40.0, 16.0),
                SliderDirection::LeftToRight,
            ),
            (
                content_box(10.0, 20.0, f32::INFINITY, 16.0),
                SliderDirection::LeftToRight,
            ),
            (
                content_box(10.0, f32::NEG_INFINITY, 40.0, 16.0),
                SliderDirection::TopToBottom,
            ),
            (
                content_box(10.0, 20.0, 40.0, f32::NAN),
                SliderDirection::BottomToTop,
            ),
        ];
        for (content, direction) in cases {
            assert_eq!(
                project_pointer_value(
                    Vec2::new(30.0, 25.0),
                    content,
                    None,
                    projection_range,
                    direction,
                ),
                Err(ProjectionUnavailable),
            );
        }
    }

    // ---- pointer capture lifecycle ----

    const CAPTURE_VIEWPORT: Vec2 = Vec2::new(800.0, 600.0);
    const CAPTURE_POINTER: PointerId = PointerId::Touch(11);

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Lifecycle {
        Grabbed,
        Proposal { is_final: bool },
        Released,
        Canceled(SliderCancelCause),
    }

    #[derive(Default, Resource)]
    struct RecordedLifecycle(Vec<Lifecycle>);

    fn record_grabbed(_: On<SliderGrabbed>, mut log: ResMut<RecordedLifecycle>) {
        log.0.push(Lifecycle::Grabbed);
    }

    fn record_lifecycle_proposal(
        change: On<SliderChangeRequested>,
        mut log: ResMut<RecordedLifecycle>,
    ) {
        log.0.push(Lifecycle::Proposal {
            is_final: change.is_final,
        });
    }

    fn record_released(_: On<SliderReleased>, mut log: ResMut<RecordedLifecycle>) {
        log.0.push(Lifecycle::Released);
    }

    fn record_canceled(event: On<SliderCanceled>, mut log: ResMut<RecordedLifecycle>) {
        log.0.push(Lifecycle::Canceled(event.cause));
    }

    struct Scene {
        app:    App,
        widget: Entity,
        panel:  Entity,
        camera: Entity,
        target: NormalizedRenderTarget,
    }

    fn seeded_camera() -> Camera {
        let mut camera = Camera::default();
        camera.computed.target_info = Some(RenderTargetInfo {
            physical_size: CAPTURE_VIEWPORT.as_uvec2(),
            scale_factor:  1.0,
        });
        let mut projection = Projection::Perspective(PerspectiveProjection::default());
        projection.update(CAPTURE_VIEWPORT.x, CAPTURE_VIEWPORT.y);
        camera.computed.clip_from_view = projection.get_clip_from_view();
        camera
    }

    fn capture_app() -> App {
        let mut app = test_app();
        app.init_resource::<RecordedLifecycle>()
            .add_observer(record_grabbed)
            .add_observer(record_lifecycle_proposal)
            .add_observer(record_released)
            .add_observer(record_canceled);
        app
    }

    fn projecting_panel(app: &mut App, tree: LayoutTree) -> Entity {
        let panel = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .world_height(0.5)
            .with_tree(tree)
            .build()
            .expect("panel should build");
        app.world_mut().spawn(panel).id()
    }

    fn projecting_scene(tree: LayoutTree) -> Scene {
        let mut app = capture_app();
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        let camera = app
            .world_mut()
            .spawn((
                seeded_camera(),
                GlobalTransform::from(
                    Transform::from_xyz(0.0, 0.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
                ),
                RenderTarget::Window(WindowRef::Primary),
            ))
            .id();
        let panel = projecting_panel(&mut app, tree);
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");
        let target = RenderTarget::Window(WindowRef::Primary)
            .normalize(Some(window))
            .expect("primary window target normalizes");
        Scene {
            app,
            widget,
            panel,
            camera,
            target,
        }
    }

    fn ltr_scene() -> Scene {
        let slider = Slider::new(range(0.0, 1.0), 0.5).expect("slider should validate");
        projecting_scene(slider_tree("level", slider))
    }

    /// A projecting scene whose panel is center-anchored, so a press at the
    /// viewport center projects to the panel's layout center (authored x = 50).
    fn center_anchored_scene(tree: LayoutTree) -> Scene {
        let mut app = capture_app();
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        let camera = app
            .world_mut()
            .spawn((
                seeded_camera(),
                GlobalTransform::from(
                    Transform::from_xyz(0.0, 0.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
                ),
                RenderTarget::Window(WindowRef::Primary),
            ))
            .id();
        let panel = DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .world_height(0.5)
            .anchor(crate::Anchor::Center)
            .with_tree(tree)
            .build()
            .expect("panel should build");
        let panel = app.world_mut().spawn(panel).id();
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");
        let target = RenderTarget::Window(WindowRef::Primary)
            .normalize(Some(window))
            .expect("primary window target normalizes");
        Scene {
            app,
            widget,
            panel,
            camera,
            target,
        }
    }

    fn press_at(scene: &mut Scene, camera: Entity, position: Vec2) {
        scene.app.world_mut().trigger(Pointer::new(
            CAPTURE_POINTER,
            Location {
                target: scene.target.clone(),
                position,
            },
            Press {
                button: PointerButton::Primary,
                hit:    HitData::new(camera, 0.0, None, None),
                count:  1,
            },
            scene.widget,
        ));
        scene.app.world_mut().flush();
    }

    fn press(scene: &mut Scene, position: Vec2) {
        let camera = scene.camera;
        press_at(scene, camera, position);
    }

    fn drag(scene: &mut Scene, position: Vec2) {
        scene.app.world_mut().trigger(Pointer::new(
            CAPTURE_POINTER,
            Location {
                target: scene.target.clone(),
                position,
            },
            Drag {
                button:   PointerButton::Primary,
                distance: Vec2::ZERO,
                delta:    Vec2::ZERO,
            },
            scene.widget,
        ));
        scene.app.world_mut().flush();
    }

    fn release(scene: &mut Scene, position: Vec2) {
        scene.app.world_mut().trigger(Pointer::new(
            CAPTURE_POINTER,
            Location {
                target: scene.target.clone(),
                position,
            },
            Release {
                button: PointerButton::Primary,
                hit:    HitData::new(scene.camera, 0.0, None, None),
            },
            scene.widget,
        ));
        scene.app.world_mut().flush();
    }

    fn pointer_cancel(scene: &mut Scene) {
        scene.app.world_mut().trigger(Pointer::new(
            CAPTURE_POINTER,
            Location {
                target:   scene.target.clone(),
                position: CAPTURE_VIEWPORT * 0.5,
            },
            Cancel {
                hit: HitData::new(scene.camera, 0.0, None, None),
            },
            scene.widget,
        ));
        scene.app.world_mut().flush();
    }

    fn lifecycle(scene: &Scene) -> Vec<Lifecycle> {
        scene.app.world().resource::<RecordedLifecycle>().0.clone()
    }

    fn is_dragging(scene: &Scene) -> bool {
        scene.app.world().get::<SliderDrag>(scene.widget).is_some()
    }

    fn captures_empty(scene: &Scene) -> bool {
        scene.app.world().resource::<SliderCaptures>().is_empty()
    }

    fn applied_value(scene: &Scene) -> f32 { slider_value(&scene.app, scene.widget) }

    fn latest_target(scene: &Scene) -> f32 {
        scene
            .app
            .world()
            .resource::<SliderCaptures>()
            .latest_raw_target(scene.widget)
            .expect("a live drag stores a raw target")
    }

    fn last_proposal_value(scene: &Scene) -> f32 {
        scene
            .app
            .world()
            .resource::<RecordedProposals>()
            .0
            .last()
            .expect("a proposal is recorded")
            .1
    }

    #[test]
    fn press_projects_grabs_and_proposes_once() {
        let mut scene = ltr_scene();
        press(&mut scene, CAPTURE_VIEWPORT * 0.5);

        assert_eq!(
            lifecycle(&scene),
            [Lifecycle::Grabbed, Lifecycle::Proposal { is_final: false },]
        );
        assert!(is_dragging(&scene));
        assert!(!captures_empty(&scene));
        assert!(
            scene
                .app
                .world()
                .resource::<SliderCaptures>()
                .latest_raw_target(scene.widget)
                .is_some_and(f32::is_finite)
        );
    }

    #[test]
    fn press_without_projectable_camera_claims_nothing() {
        let mut scene = ltr_scene();
        let bogus_camera = scene.app.world_mut().spawn_empty().id();
        press_at(&mut scene, bogus_camera, CAPTURE_VIEWPORT * 0.5);

        assert!(lifecycle(&scene).is_empty());
        assert!(!is_dragging(&scene));
        assert!(captures_empty(&scene));
    }

    #[test]
    fn click_without_drag_proposes_release_value_before_released() {
        let mut scene = ltr_scene();
        press(&mut scene, CAPTURE_VIEWPORT * 0.5);
        release(&mut scene, CAPTURE_VIEWPORT * 0.5);

        assert_eq!(
            lifecycle(&scene),
            [
                Lifecycle::Grabbed,
                Lifecycle::Proposal { is_final: false },
                Lifecycle::Proposal { is_final: true },
                Lifecycle::Released,
            ]
        );
        assert!(!is_dragging(&scene));
        assert!(captures_empty(&scene));
    }

    #[test]
    fn each_drag_reprojects_and_re_stores_the_raw_target() {
        let mut scene = ltr_scene();
        press(&mut scene, CAPTURE_VIEWPORT * 0.5);
        drag(
            &mut scene,
            CAPTURE_VIEWPORT.mul_add(Vec2::splat(0.5), Vec2::new(400.0, 0.0)),
        );
        let after_right = latest_target(&scene);
        drag(
            &mut scene,
            CAPTURE_VIEWPORT.mul_add(Vec2::splat(0.5), Vec2::new(-400.0, 0.0)),
        );
        let after_left = latest_target(&scene);

        // The grab plus two drags each emit exactly one non-final proposal.
        assert_eq!(
            lifecycle(&scene),
            [
                Lifecycle::Grabbed,
                Lifecycle::Proposal { is_final: false },
                Lifecycle::Proposal { is_final: false },
                Lifecycle::Proposal { is_final: false },
            ]
        );
        // Two opposite drags replace the stored raw target with different
        // values: dragging right past the travel reprojects to the range end,
        // dragging left reprojects to the range start.
        assert_close(after_right, 1.0);
        assert_close(after_left, 0.0);
        assert!(after_right > after_left);
        // The last stored raw target matches the last proposal that drag
        // emitted.
        assert_close(after_left, last_proposal_value(&scene));
    }

    #[test]
    fn projection_loss_after_capture_cancels_exactly_once() {
        let mut scene = ltr_scene();
        press(&mut scene, CAPTURE_VIEWPORT * 0.5);
        scene.app.world_mut().entity_mut(scene.camera).despawn();
        drag(&mut scene, CAPTURE_VIEWPORT * 0.5);

        assert_eq!(
            lifecycle(&scene),
            [
                Lifecycle::Grabbed,
                Lifecycle::Proposal { is_final: false },
                Lifecycle::Canceled(SliderCancelCause::ProjectionUnavailable),
            ]
        );
        assert!(!is_dragging(&scene));
        assert!(captures_empty(&scene));

        // A later drag on the freed widget proposes and cancels nothing.
        drag(&mut scene, CAPTURE_VIEWPORT * 0.5);
        assert_eq!(lifecycle(&scene).len(), 3);
    }

    #[test]
    fn release_away_still_proposes_the_release_value() {
        let mut scene = ltr_scene();
        press(&mut scene, CAPTURE_VIEWPORT * 0.5);
        release(
            &mut scene,
            CAPTURE_VIEWPORT.mul_add(Vec2::splat(0.5), Vec2::new(400.0, 0.0)),
        );

        assert_eq!(
            lifecycle(&scene),
            [
                Lifecycle::Grabbed,
                Lifecycle::Proposal { is_final: false },
                Lifecycle::Proposal { is_final: true },
                Lifecycle::Released,
            ]
        );
        assert!(captures_empty(&scene));
    }

    #[test]
    fn rejecting_every_proposal_leaves_applied_state_unchanged() {
        let mut scene = ltr_scene();
        let before = applied_value(&scene);
        press(&mut scene, CAPTURE_VIEWPORT * 0.5);
        // Release past the right edge of the travel so the final proposal
        // carries the clamped raw release-position value: the range end.
        release(
            &mut scene,
            CAPTURE_VIEWPORT.mul_add(Vec2::splat(0.5), Vec2::new(400.0, 0.0)),
        );

        assert!(matches!(
            lifecycle(&scene).last(),
            Some(Lifecycle::Released)
        ));
        // No proposal was accepted, so the applied value never left its
        // authored initial.
        assert_close(applied_value(&scene), before);
        // The final proposal still carries the raw release-position value,
        // independent of the untouched applied state.
        let proposals = &scene.app.world().resource::<RecordedProposals>().0;
        let (_, final_value, is_final, _) =
            *proposals.last().expect("a final proposal is recorded");
        assert!(is_final, "the last proposal completes the interaction");
        assert_close(final_value, 1.0);
    }

    #[test]
    fn opt_in_self_update_applies_each_proposal_once() {
        let mut scene = ltr_scene();
        scene.app.add_observer(slider_self_update);

        // Each proposal the interaction emits is applied exactly once, so the
        // applied value tracks every proposal in turn rather than only the
        // final one.
        press(&mut scene, CAPTURE_VIEWPORT * 0.5);
        assert_close(applied_value(&scene), last_proposal_value(&scene));
        drag(
            &mut scene,
            CAPTURE_VIEWPORT.mul_add(Vec2::splat(0.5), Vec2::new(400.0, 0.0)),
        );
        assert_close(applied_value(&scene), 1.0);
        assert_close(applied_value(&scene), last_proposal_value(&scene));
        drag(
            &mut scene,
            CAPTURE_VIEWPORT.mul_add(Vec2::splat(0.5), Vec2::new(-400.0, 0.0)),
        );
        assert_close(applied_value(&scene), 0.0);
        assert_close(applied_value(&scene), last_proposal_value(&scene));
        release(&mut scene, CAPTURE_VIEWPORT * 0.5);
        assert_close(applied_value(&scene), last_proposal_value(&scene));

        // The grab, two drags, and the final release each contributed exactly
        // one accepted proposal.
        let proposals = &scene.app.world().resource::<RecordedProposals>().0;
        assert_eq!(proposals.len(), 4);
        assert!(
            proposals[3].2,
            "the last proposal completes the interaction"
        );
    }

    #[test]
    fn disable_cancels_an_active_drag() {
        let mut scene = ltr_scene();
        press(&mut scene, CAPTURE_VIEWPORT * 0.5);
        let panel = scene.panel;
        // Re-author the same slider as disabled: the same-kind refresh keeps
        // the capture, and the derived `WidgetDisabled` marker cancels it.
        let slider = Slider::new(range(0.0, 1.0), 0.5).expect("slider should validate");
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(
            El::new()
                .size(20.0, 10.0)
                .slider("level", slider)
                .widget_interactivity(WidgetInteractivity::Disabled),
            |_| {},
        );
        scene
            .app
            .world_mut()
            .commands()
            .set_tree(panel, builder.build())
            .expect("disabled tree should be accepted");
        scene.app.update();

        assert_eq!(
            lifecycle(&scene).last(),
            Some(&Lifecycle::Canceled(SliderCancelCause::Disabled))
        );
        assert!(!is_dragging(&scene));
        assert!(captures_empty(&scene));
    }

    #[test]
    fn pointer_cancel_terminates_the_drag() {
        let mut scene = ltr_scene();
        press(&mut scene, CAPTURE_VIEWPORT * 0.5);
        pointer_cancel(&mut scene);

        assert_eq!(
            lifecycle(&scene).last(),
            Some(&Lifecycle::Canceled(SliderCancelCause::PointerCanceled))
        );
        assert!(captures_empty(&scene));
    }

    #[test]
    fn semantic_cancel_terminates_and_is_a_noop_without_capture() {
        let mut scene = ltr_scene();
        let widget = scene.widget;
        scene
            .app
            .world_mut()
            .trigger(SemanticWidgetIntent::Cancel { entity: widget });
        scene.app.world_mut().flush();
        assert!(lifecycle(&scene).is_empty());

        press(&mut scene, CAPTURE_VIEWPORT * 0.5);
        scene
            .app
            .world_mut()
            .trigger(SemanticWidgetIntent::Cancel { entity: widget });
        scene.app.world_mut().flush();

        assert_eq!(
            lifecycle(&scene).last(),
            Some(&Lifecycle::Canceled(SliderCancelCause::Explicit))
        );
        assert!(captures_empty(&scene));
    }

    #[test]
    fn tree_removal_cancels_the_active_drag() {
        let mut scene = ltr_scene();
        press(&mut scene, CAPTURE_VIEWPORT * 0.5);
        let panel = scene.panel;
        scene
            .app
            .world_mut()
            .commands()
            .set_tree(panel, LayoutBuilder::new(100.0, 50.0).build())
            .expect("empty tree should be accepted");
        scene.app.update();

        assert_eq!(
            lifecycle(&scene).last(),
            Some(&Lifecycle::Canceled(SliderCancelCause::WidgetRemoved))
        );
        assert!(captures_empty(&scene));
    }

    #[test]
    fn kind_change_cancels_the_active_drag() {
        let mut scene = ltr_scene();
        press(&mut scene, CAPTURE_VIEWPORT * 0.5);
        let panel = scene.panel;
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(
            El::new()
                .size(20.0, 10.0)
                .button("level", crate::Button::new()),
            |_| {},
        );
        scene
            .app
            .world_mut()
            .commands()
            .set_tree(panel, builder.build())
            .expect("button tree should be accepted");
        scene.app.update();

        assert_eq!(
            lifecycle(&scene).last(),
            Some(&Lifecycle::Canceled(SliderCancelCause::WidgetKindChanged))
        );
        assert_eq!(
            scene.app.world().get::<WidgetKind>(scene.widget),
            Some(&WidgetKind::Button)
        );
        assert!(captures_empty(&scene));
    }

    #[test]
    fn panel_role_removal_finalizes_the_active_drag() {
        let mut scene = ltr_scene();
        press(&mut scene, CAPTURE_VIEWPORT * 0.5);
        let panel = scene.panel;
        scene
            .app
            .world_mut()
            .entity_mut(panel)
            .remove::<DiegeticPanel>();
        scene.app.world_mut().flush();

        assert_eq!(
            lifecycle(&scene).last(),
            Some(&Lifecycle::Canceled(SliderCancelCause::WidgetRemoved))
        );
        assert!(captures_empty(&scene));
    }

    #[test]
    fn full_panel_despawn_finalizes_the_active_drag() {
        let mut scene = ltr_scene();
        press(&mut scene, CAPTURE_VIEWPORT * 0.5);
        let panel = scene.panel;
        scene.app.world_mut().entity_mut(panel).despawn();
        scene.app.world_mut().flush();

        assert_eq!(
            lifecycle(&scene).last(),
            Some(&Lifecycle::Canceled(SliderCancelCause::WidgetRemoved))
        );
        assert!(captures_empty(&scene));
    }

    #[test]
    fn slider_root_content_box_excludes_border_and_padding_through_reify() {
        let slider = Slider::new(range(0.0, 1.0), 0.5).expect("slider should validate");
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(
            El::new()
                .size(40.0, 20.0)
                .border(crate::Border::all(2.0, Color::WHITE))
                .padding(crate::Padding::all(3.0))
                .slider("level", slider),
            |_| {},
        );
        let mut app = capture_app();
        let panel = projecting_panel(&mut app, builder.build());
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");

        let slots = app
            .world()
            .get::<WidgetVisualSlots>(widget)
            .expect("reified slider carries its visual slots");
        let content = slots
            .content_box(VisualSlotId::SLIDER_ROOT)
            .expect("the slider root slot records a content box");
        let border = slots
            .border_box(VisualSlotId::SLIDER_ROOT)
            .expect("the slider root slot records a border box");
        // The 2-point border and 3-point padding inset every content edge, so
        // the content box sits strictly inside the border box on both axes.
        // The panel scales the authored tree, so the inset is compared in the
        // solved space rather than against the authored 40x20 size.
        assert!(content.width > 0.0 && content.height > 0.0);
        assert!(content.width < border.width);
        assert!(content.height < border.height);
        assert!(content.x > border.x);
        assert!(content.y > border.y);
        // Border (2) plus padding (3) inset each edge equally, so the left and
        // right insets match and the content box stays centered. The scaled
        // layout carries float rounding, so compare with a small tolerance.
        let left_inset = content.x - border.x;
        let right_inset = (border.x + border.width) - (content.x + content.width);
        assert!(left_inset > 0.0);
        assert!((left_inset - right_inset).abs() < 1e-3);
    }

    #[test]
    fn projected_interior_value_reads_authored_border_and_padding() {
        // Authored panel-wide slider root with a 2-point border and 3-point
        // padding, mapped to a range whose interior midpoint (2.0) is an
        // independently known value that no clamp to 0.0 or 1.0 can produce.
        // The root spans the 100-point authored width so the panel's layout
        // center (authored x = 50) falls inside its content box.
        let slider = Slider::new(range(0.0, 4.0), 2.0).expect("slider should validate");
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(
            El::new()
                .size(100.0, 20.0)
                .border(crate::Border::all(2.0, Color::WHITE))
                .padding(crate::Padding::all(3.0))
                .slider("level", slider),
            |_| {},
        );
        let mut scene = center_anchored_scene(builder.build());

        let (content, border) = {
            let slots = scene
                .app
                .world()
                .get::<WidgetVisualSlots>(scene.widget)
                .expect("reified slider carries slots");
            (
                slots
                    .content_box(VisualSlotId::SLIDER_ROOT)
                    .expect("content box"),
                slots
                    .border_box(VisualSlotId::SLIDER_ROOT)
                    .expect("border box"),
            )
        };

        // The solved layout scales the authored tree uniformly; the border box
        // (authored width 100) fixes points-per-authored-unit independently of
        // the content-box computation under test.
        let scale = border.width / 100.0;
        // The content box excludes exactly the authored 2-point border and
        // 3-point padding on every side: width 100 - 2*2 - 2*3 = 90, left inset
        // 2 + 3 = 5.
        assert_close(content.width, 90.0 * scale);
        assert_close(content.x, 5.0f32.mul_add(scale, border.x));

        // A press at the viewport center projects to the panel's layout center
        // (authored x = 50). Through the live camera → panel-ray → content-box
        // chain, that interior travel position must map to the range value the
        // reified content box places there — not a clamp to an endpoint.
        let panel_center = 50.0 * scale;
        let fraction = (panel_center - content.x) / content.width;
        let expected = 4.0 * fraction;
        assert!(
            expected > 0.1 && expected < 3.9,
            "expected an interior value, got {expected}"
        );
        press(&mut scene, CAPTURE_VIEWPORT * 0.5);
        assert_close(last_proposal_value(&scene), expected);
    }

    // ---- real Bevy dispatcher integration ----
    //
    // These fixtures drive the shipped hover and `pointer_events` systems with
    // synthetic `PointerHits` and raw `PointerInput`, then reconcile through the
    // shared `reconcile_pointer_input` dispatcher. They never move or simulate
    // an operating-system pointer; every hit and action is authored directly.

    const DISPATCH_POINTER: PointerId = PointerId::Touch(70);
    const SECOND_POINTER: PointerId = PointerId::Touch(71);

    fn center() -> Vec2 { CAPTURE_VIEWPORT * 0.5 }

    fn past_right() -> Vec2 { CAPTURE_VIEWPORT.mul_add(Vec2::splat(0.5), Vec2::new(400.0, 0.0)) }

    /// A projecting panel wired to the shipped hover and pointer-event systems
    /// plus the shared raw dispatcher.
    struct DispatchScene {
        app:    App,
        panel:  Entity,
        camera: Entity,
        target: NormalizedRenderTarget,
    }

    #[derive(Default, Resource)]
    struct ButtonLog(Vec<&'static str>);

    fn log_button_pressed(_: On<ButtonPressed>, mut log: ResMut<ButtonLog>) {
        log.0.push("pressed");
    }

    fn log_button_released(_: On<ButtonReleased>, mut log: ResMut<ButtonLog>) {
        log.0.push("released");
    }

    fn log_button_clicked(_: On<ButtonClicked>, mut log: ResMut<ButtonLog>) {
        log.0.push("clicked");
    }

    fn dispatch_scene(tree: LayoutTree) -> DispatchScene {
        let mut app = capture_app();
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        let camera = app
            .world_mut()
            .spawn((
                seeded_camera(),
                GlobalTransform::from(
                    Transform::from_xyz(0.0, 0.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
                ),
                RenderTarget::Window(WindowRef::Primary),
            ))
            .id();
        app.add_plugins(InteractionPlugin)
            .add_message::<PointerInput>()
            .add_message::<PointerHits>()
            .init_resource::<PointerMap>()
            .configure_sets(
                PreUpdate,
                (PickingSystems::Hover, PickingSystems::Last).chain(),
            );
        let panel = projecting_panel(&mut app, tree);
        app.update();
        let target = RenderTarget::Window(WindowRef::Primary)
            .normalize(Some(window))
            .expect("primary window target normalizes");
        DispatchScene {
            app,
            panel,
            camera,
            target,
        }
    }

    fn record_buttons(scene: &mut DispatchScene) {
        scene
            .app
            .init_resource::<ButtonLog>()
            .add_observer(log_button_pressed)
            .add_observer(log_button_released)
            .add_observer(log_button_clicked);
    }

    fn add_pointer(scene: &mut DispatchScene, pointer_id: PointerId) -> Entity {
        let location = Location {
            target:   scene.target.clone(),
            position: center(),
        };
        let entity = scene
            .app
            .world_mut()
            .spawn((pointer_id, PointerLocation::new(location)))
            .id();
        let result = scene.app.world_mut().run_system_cached(update_pointer_map);
        assert!(result.is_ok());
        entity
    }

    /// Feeds one synthetic backend hit so the shipped hover systems place
    /// `widget` under `pointer_id` with `camera` as the hit camera.
    fn feed_hit(scene: &mut DispatchScene, pointer_id: PointerId, camera: Entity, widget: Entity) {
        scene.app.world_mut().write_message(PointerHits::new(
            pointer_id,
            vec![(widget, HitData::new(camera, 0.0, None, None))],
            0.0,
        ));
    }

    fn feed_input(
        scene: &mut DispatchScene,
        pointer_id: PointerId,
        position: Vec2,
        action: PointerAction,
    ) {
        let location = Location {
            target: scene.target.clone(),
            position,
        };
        scene
            .app
            .world_mut()
            .write_message(PointerInput::new(pointer_id, location, action));
    }

    /// Sets the current and previous hover maps directly so raw-batch ordering
    /// is exact.
    fn set_hover(
        scene: &mut DispatchScene,
        pointer_id: PointerId,
        previous: &[Entity],
        current: &[Entity],
        camera: Entity,
    ) {
        let entries = |entities: &[Entity]| {
            entities
                .iter()
                .copied()
                .map(|entity| (entity, HitData::new(camera, 0.0, None, None)))
                .collect()
        };
        scene
            .app
            .world_mut()
            .resource_mut::<PreviousHoverMap>()
            .insert(pointer_id, entries(previous));
        scene
            .app
            .world_mut()
            .resource_mut::<HoverMap>()
            .insert(pointer_id, entries(current));
    }

    /// Drives one raw batch through the shipped `pointer_events` system and the
    /// shared reconciler, with the hover maps left exactly as `set_hover` fixed
    /// them.
    fn run_raw(scene: &mut DispatchScene, inputs: &[(PointerId, Vec2, PointerAction)]) {
        for &(pointer_id, position, action) in inputs {
            feed_input(scene, pointer_id, position, action);
        }
        let result = scene.app.world_mut().run_system_cached(pointer_events);
        assert!(result.is_ok());
        let result = scene
            .app
            .world_mut()
            .run_system_cached(reconcile_pointer_input);
        assert!(result.is_ok());
    }

    fn dispatch_lifecycle(scene: &DispatchScene) -> Vec<Lifecycle> {
        scene.app.world().resource::<RecordedLifecycle>().0.clone()
    }

    fn owner_of(scene: &DispatchScene, widget: Entity) -> Option<PointerId> {
        scene
            .app
            .world()
            .resource::<WidgetCaptures>()
            .pointer(widget)
    }

    fn dragging(scene: &DispatchScene, widget: Entity) -> bool {
        scene.app.world().get::<SliderDrag>(widget).is_some()
    }

    fn dispatch_empty(scene: &DispatchScene) -> bool {
        scene.app.world().resource::<SliderCaptures>().is_empty()
            && scene.app.world().resource::<WidgetCaptures>().is_empty()
    }

    fn slider0() -> Slider { Slider::new(range(0.0, 1.0), 0.5).expect("slider should validate") }

    fn button_and_slider_tree() -> LayoutTree {
        let slider = slider0()
            .hovered_background(STATE_HOVER_FILL)
            .pressed_background(STATE_PRESS_FILL)
            .focused_border_color(STATE_FOCUS_BORDER);
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(
            El::new().size(20.0, 10.0).button("act", Button::new()),
            |_| {},
        );
        // The slider's state layers override background and border color, so the
        // element authors both a normal background and a normal border for the
        // surface those overrides patch.
        builder.with(
            El::new()
                .size(20.0, 10.0)
                .background(Color::WHITE)
                .border(Border::all(1.0, Color::WHITE))
                .slider("level", slider),
            |_| {},
        );
        builder.build()
    }

    /// The same unrelated button paired with a valid styled slider that marks
    /// one thumb, so a peer test can watch both the slider's root and thumb
    /// records react while the button stays untouched.
    fn button_and_thumb_slider_tree() -> LayoutTree {
        let slider = plain_slider(0.5)
            .hovered_background(STATE_HOVER_FILL)
            .pressed_background(STATE_PRESS_FILL)
            .focused_border_color(STATE_FOCUS_BORDER);
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(
            El::new().size(20.0, 10.0).button("act", Button::new()),
            |_| {},
        );
        builder.with(
            El::overlay()
                .size(40.0, 16.0)
                .background(Color::WHITE)
                .border(Border::all(1.0, Color::WHITE))
                .alignment(AlignX::Left, AlignY::Center)
                .slider("level", slider),
            |builder| {
                builder.with(El::new().size(8.0, 8.0).id(THUMB_ID).slider_thumb(), |_| {});
            },
        );
        builder.build()
    }

    fn two_slider_tree() -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(
            El::new().size(20.0, 10.0).slider("first", slider0()),
            |_| {},
        );
        builder.with(
            El::new().size(20.0, 10.0).slider("second", slider0()),
            |_| {},
        );
        builder.build()
    }

    #[test]
    fn dispatched_press_grabs_and_proposes_once() {
        let mut scene = dispatch_scene(slider_tree("level", slider0()));
        let widget = resolve_widget(&mut scene.app, scene.panel, "level");
        let camera = scene.camera;
        add_pointer(&mut scene, DISPATCH_POINTER);

        feed_hit(&mut scene, DISPATCH_POINTER, camera, widget);
        feed_input(
            &mut scene,
            DISPATCH_POINTER,
            center(),
            PointerAction::Press(PointerButton::Primary),
        );
        scene.app.update();

        assert_eq!(
            dispatch_lifecycle(&scene),
            [Lifecycle::Grabbed, Lifecycle::Proposal { is_final: false }]
        );
        assert!(dragging(&scene, widget));
        assert_eq!(owner_of(&scene, widget), Some(DISPATCH_POINTER));
        assert!(
            scene
                .app
                .world()
                .resource::<SliderCaptures>()
                .latest_raw_target(widget)
                .is_some_and(f32::is_finite)
        );
    }

    #[test]
    fn dispatched_press_without_projection_claims_nothing() {
        let mut scene = dispatch_scene(slider_tree("level", slider0()));
        let widget = resolve_widget(&mut scene.app, scene.panel, "level");
        add_pointer(&mut scene, DISPATCH_POINTER);
        let bogus_camera = scene.app.world_mut().spawn_empty().id();

        // The hit names a camera that cannot project, so the press claims and
        // emits nothing even though the dispatcher observed it.
        feed_hit(&mut scene, DISPATCH_POINTER, bogus_camera, widget);
        feed_input(
            &mut scene,
            DISPATCH_POINTER,
            center(),
            PointerAction::Press(PointerButton::Primary),
        );
        scene.app.update();

        assert!(dispatch_lifecycle(&scene).is_empty());
        assert!(!dragging(&scene, widget));
        assert!(dispatch_empty(&scene));
    }

    #[test]
    fn dispatched_click_terminalizes_release_exactly_once() {
        let mut scene = dispatch_scene(slider_tree("level", slider0()));
        let widget = resolve_widget(&mut scene.app, scene.panel, "level");
        let camera = scene.camera;
        add_pointer(&mut scene, DISPATCH_POINTER);

        feed_hit(&mut scene, DISPATCH_POINTER, camera, widget);
        feed_input(
            &mut scene,
            DISPATCH_POINTER,
            center(),
            PointerAction::Press(PointerButton::Primary),
        );
        scene.app.update();
        feed_hit(&mut scene, DISPATCH_POINTER, camera, widget);
        feed_input(
            &mut scene,
            DISPATCH_POINTER,
            center(),
            PointerAction::Release(PointerButton::Primary),
        );
        scene.app.update();

        // The final release-position proposal precedes `SliderReleased`, and the
        // capture frees exactly once.
        assert_eq!(
            dispatch_lifecycle(&scene),
            [
                Lifecycle::Grabbed,
                Lifecycle::Proposal { is_final: false },
                Lifecycle::Proposal { is_final: true },
                Lifecycle::Released,
            ]
        );
        assert!(!dragging(&scene, widget));
        assert!(dispatch_empty(&scene));

        // A further quiet update emits nothing more.
        scene.app.update();
        assert_eq!(dispatch_lifecycle(&scene).len(), 4);
    }

    #[test]
    fn dispatched_release_reconciled_after_hover_lost() {
        let mut scene = dispatch_scene(slider_tree("level", slider0()));
        let widget = resolve_widget(&mut scene.app, scene.panel, "level");
        let camera = scene.camera;
        add_pointer(&mut scene, DISPATCH_POINTER);

        feed_hit(&mut scene, DISPATCH_POINTER, camera, widget);
        feed_input(
            &mut scene,
            DISPATCH_POINTER,
            center(),
            PointerAction::Press(PointerButton::Primary),
        );
        scene.app.update();
        // Two frames without a hit leave the previous hover empty, so no
        // targeted release fires and the reconciler must resolve the raw
        // release itself.
        scene.app.update();
        feed_input(
            &mut scene,
            DISPATCH_POINTER,
            past_right(),
            PointerAction::Release(PointerButton::Primary),
        );
        scene.app.update();

        assert_eq!(
            dispatch_lifecycle(&scene),
            [
                Lifecycle::Grabbed,
                Lifecycle::Proposal { is_final: false },
                Lifecycle::Proposal { is_final: true },
                Lifecycle::Released,
            ]
        );
        assert!(!dragging(&scene, widget));
        assert!(dispatch_empty(&scene));
    }

    #[test]
    fn dispatched_pointer_removal_cancels_the_active_drag() {
        let mut scene = dispatch_scene(slider_tree("level", slider0()));
        let widget = resolve_widget(&mut scene.app, scene.panel, "level");
        let camera = scene.camera;
        let pointer_entity = add_pointer(&mut scene, DISPATCH_POINTER);

        feed_hit(&mut scene, DISPATCH_POINTER, camera, widget);
        feed_input(
            &mut scene,
            DISPATCH_POINTER,
            center(),
            PointerAction::Press(PointerButton::Primary),
        );
        scene.app.update();
        assert!(dragging(&scene, widget));

        scene.app.world_mut().entity_mut(pointer_entity).despawn();
        scene.app.world_mut().flush();

        assert_eq!(
            dispatch_lifecycle(&scene).last(),
            Some(&Lifecycle::Canceled(SliderCancelCause::PointerRemoved))
        );
        assert!(dispatch_empty(&scene));
    }

    #[test]
    fn raw_same_slider_release_then_press_reuses_capture() {
        let mut scene = dispatch_scene(slider_tree("level", slider0()));
        let widget = resolve_widget(&mut scene.app, scene.panel, "level");
        let camera = scene.camera;
        add_pointer(&mut scene, DISPATCH_POINTER);

        set_hover(&mut scene, DISPATCH_POINTER, &[widget], &[widget], camera);
        run_raw(
            &mut scene,
            &[(
                DISPATCH_POINTER,
                center(),
                PointerAction::Press(PointerButton::Primary),
            )],
        );
        set_hover(&mut scene, DISPATCH_POINTER, &[widget], &[widget], camera);
        run_raw(
            &mut scene,
            &[
                (
                    DISPATCH_POINTER,
                    center(),
                    PointerAction::Release(PointerButton::Primary),
                ),
                (
                    DISPATCH_POINTER,
                    center(),
                    PointerAction::Press(PointerButton::Primary),
                ),
            ],
        );

        // The terminal frees occupancy first, then the reprojected press
        // recaptures the same slider for the same pointer within the batch.
        assert_eq!(
            dispatch_lifecycle(&scene),
            [
                Lifecycle::Grabbed,
                Lifecycle::Proposal { is_final: false },
                Lifecycle::Proposal { is_final: true },
                Lifecycle::Released,
                Lifecycle::Grabbed,
                Lifecycle::Proposal { is_final: false },
            ]
        );
        assert!(dragging(&scene, widget));
        assert_eq!(owner_of(&scene, widget), Some(DISPATCH_POINTER));
        assert!(!scene.app.world().resource::<SliderCaptures>().has_pending());
    }

    #[test]
    fn raw_release_then_second_pointer_claims_freed_slider() {
        let mut scene = dispatch_scene(slider_tree("level", slider0()));
        let widget = resolve_widget(&mut scene.app, scene.panel, "level");
        let camera = scene.camera;
        add_pointer(&mut scene, DISPATCH_POINTER);
        add_pointer(&mut scene, SECOND_POINTER);

        set_hover(&mut scene, DISPATCH_POINTER, &[widget], &[widget], camera);
        run_raw(
            &mut scene,
            &[(
                DISPATCH_POINTER,
                center(),
                PointerAction::Press(PointerButton::Primary),
            )],
        );
        set_hover(&mut scene, DISPATCH_POINTER, &[widget], &[widget], camera);
        set_hover(&mut scene, SECOND_POINTER, &[], &[widget], camera);
        run_raw(
            &mut scene,
            &[
                (
                    DISPATCH_POINTER,
                    center(),
                    PointerAction::Release(PointerButton::Primary),
                ),
                (
                    SECOND_POINTER,
                    center(),
                    PointerAction::Press(PointerButton::Primary),
                ),
            ],
        );

        assert_eq!(
            dispatch_lifecycle(&scene),
            [
                Lifecycle::Grabbed,
                Lifecycle::Proposal { is_final: false },
                Lifecycle::Proposal { is_final: true },
                Lifecycle::Released,
                Lifecycle::Grabbed,
                Lifecycle::Proposal { is_final: false },
            ]
        );
        assert!(dragging(&scene, widget));
        assert_eq!(owner_of(&scene, widget), Some(SECOND_POINTER));
    }

    #[test]
    fn raw_other_pointer_press_before_release_stays_rejected() {
        let mut scene = dispatch_scene(slider_tree("level", slider0()));
        let widget = resolve_widget(&mut scene.app, scene.panel, "level");
        let camera = scene.camera;
        add_pointer(&mut scene, DISPATCH_POINTER);
        add_pointer(&mut scene, SECOND_POINTER);

        set_hover(&mut scene, DISPATCH_POINTER, &[widget], &[widget], camera);
        run_raw(
            &mut scene,
            &[(
                DISPATCH_POINTER,
                center(),
                PointerAction::Press(PointerButton::Primary),
            )],
        );
        // The owner's previous hover is cleared, so its release routes through
        // the reconciler and frees the widget only after the batch is scanned.
        set_hover(&mut scene, DISPATCH_POINTER, &[], &[widget], camera);
        set_hover(&mut scene, SECOND_POINTER, &[], &[widget], camera);
        run_raw(
            &mut scene,
            &[
                (
                    SECOND_POINTER,
                    center(),
                    PointerAction::Press(PointerButton::Primary),
                ),
                (
                    DISPATCH_POINTER,
                    center(),
                    PointerAction::Release(PointerButton::Primary),
                ),
            ],
        );

        // The competing press arrived before the terminal that freed the
        // widget, so it stays rejected and the owner still terminalizes once.
        assert_eq!(
            dispatch_lifecycle(&scene),
            [
                Lifecycle::Grabbed,
                Lifecycle::Proposal { is_final: false },
                Lifecycle::Proposal { is_final: true },
                Lifecycle::Released,
            ]
        );
        assert!(!dragging(&scene, widget));
        assert!(dispatch_empty(&scene));
    }

    #[test]
    fn raw_button_to_slider_handoff() {
        let mut scene = dispatch_scene(button_and_slider_tree());
        record_buttons(&mut scene);
        let button = resolve_widget(&mut scene.app, scene.panel, "act");
        let slider = resolve_widget(&mut scene.app, scene.panel, "level");
        let camera = scene.camera;
        add_pointer(&mut scene, DISPATCH_POINTER);

        set_hover(&mut scene, DISPATCH_POINTER, &[button], &[button], camera);
        run_raw(
            &mut scene,
            &[(
                DISPATCH_POINTER,
                center(),
                PointerAction::Press(PointerButton::Primary),
            )],
        );
        set_hover(&mut scene, DISPATCH_POINTER, &[button], &[slider], camera);
        run_raw(
            &mut scene,
            &[
                (
                    DISPATCH_POINTER,
                    center(),
                    PointerAction::Release(PointerButton::Primary),
                ),
                (
                    DISPATCH_POINTER,
                    center(),
                    PointerAction::Press(PointerButton::Primary),
                ),
            ],
        );

        assert!(scene.app.world().get::<ButtonPress>(button).is_none());
        assert!(dragging(&scene, slider));
        assert_eq!(owner_of(&scene, slider), Some(DISPATCH_POINTER));
        assert_eq!(owner_of(&scene, button), None);
        assert_eq!(
            scene.app.world().resource::<ButtonLog>().0,
            ["pressed", "released", "clicked"]
        );
        assert!(dispatch_lifecycle(&scene).contains(&Lifecycle::Grabbed));
    }

    #[test]
    fn raw_slider_to_button_handoff() {
        let mut scene = dispatch_scene(button_and_slider_tree());
        record_buttons(&mut scene);
        let button = resolve_widget(&mut scene.app, scene.panel, "act");
        let slider = resolve_widget(&mut scene.app, scene.panel, "level");
        let camera = scene.camera;
        add_pointer(&mut scene, DISPATCH_POINTER);

        set_hover(&mut scene, DISPATCH_POINTER, &[slider], &[slider], camera);
        run_raw(
            &mut scene,
            &[(
                DISPATCH_POINTER,
                center(),
                PointerAction::Press(PointerButton::Primary),
            )],
        );
        set_hover(&mut scene, DISPATCH_POINTER, &[slider], &[button], camera);
        run_raw(
            &mut scene,
            &[
                (
                    DISPATCH_POINTER,
                    center(),
                    PointerAction::Release(PointerButton::Primary),
                ),
                (
                    DISPATCH_POINTER,
                    center(),
                    PointerAction::Press(PointerButton::Primary),
                ),
            ],
        );

        assert!(!dragging(&scene, slider));
        assert!(scene.app.world().get::<ButtonPress>(button).is_some());
        assert_eq!(owner_of(&scene, button), Some(DISPATCH_POINTER));
        assert_eq!(owner_of(&scene, slider), None);
        assert_eq!(scene.app.world().resource::<ButtonLog>().0, ["pressed"]);
        assert_eq!(
            dispatch_lifecycle(&scene).last(),
            Some(&Lifecycle::Released)
        );
    }

    #[test]
    fn raw_cancel_then_press_ignores_the_later_action() {
        let mut scene = dispatch_scene(slider_tree("level", slider0()));
        let widget = resolve_widget(&mut scene.app, scene.panel, "level");
        let camera = scene.camera;
        add_pointer(&mut scene, DISPATCH_POINTER);

        set_hover(&mut scene, DISPATCH_POINTER, &[widget], &[widget], camera);
        run_raw(
            &mut scene,
            &[(
                DISPATCH_POINTER,
                center(),
                PointerAction::Press(PointerButton::Primary),
            )],
        );
        // Clear the hover so the reconciler owns the terminal, then a raw cancel
        // followed by a press: the cancel is terminal and the press is ignored.
        set_hover(&mut scene, DISPATCH_POINTER, &[], &[], camera);
        run_raw(
            &mut scene,
            &[
                (DISPATCH_POINTER, center(), PointerAction::Cancel),
                (
                    DISPATCH_POINTER,
                    center(),
                    PointerAction::Press(PointerButton::Primary),
                ),
            ],
        );

        assert_eq!(
            dispatch_lifecycle(&scene),
            [
                Lifecycle::Grabbed,
                Lifecycle::Proposal { is_final: false },
                Lifecycle::Canceled(SliderCancelCause::PointerCanceled),
            ]
        );
        assert!(!dragging(&scene, widget));
        assert!(dispatch_empty(&scene));
    }

    #[test]
    fn raw_exhausted_sequence_preserves_the_owner() {
        let mut scene = dispatch_scene(two_slider_tree());
        let first = resolve_widget(&mut scene.app, scene.panel, "first");
        let second = resolve_widget(&mut scene.app, scene.panel, "second");
        let camera = scene.camera;
        add_pointer(&mut scene, DISPATCH_POINTER);
        add_pointer(&mut scene, SECOND_POINTER);

        set_hover(&mut scene, DISPATCH_POINTER, &[first], &[first], camera);
        run_raw(
            &mut scene,
            &[(
                DISPATCH_POINTER,
                center(),
                PointerAction::Press(PointerButton::Primary),
            )],
        );
        scene
            .app
            .world_mut()
            .resource_mut::<WidgetCaptures>()
            .saturate_sequence();
        set_hover(&mut scene, SECOND_POINTER, &[second], &[second], camera);
        run_raw(
            &mut scene,
            &[(
                SECOND_POINTER,
                center(),
                PointerAction::Press(PointerButton::Primary),
            )],
        );

        // The exhausted sequence rejects the new press and leaves the earlier
        // owner untouched.
        assert_eq!(owner_of(&scene, first), Some(DISPATCH_POINTER));
        assert!(dragging(&scene, first));
        assert_eq!(owner_of(&scene, second), None);
        assert!(!dragging(&scene, second));
    }

    fn past_left() -> Vec2 { CAPTURE_VIEWPORT.mul_add(Vec2::splat(0.5), Vec2::new(-400.0, 0.0)) }

    #[test]
    fn raw_release_terminalizes_despite_later_unobserved_press() {
        let mut scene = dispatch_scene(slider_tree("level", slider0()));
        let widget = resolve_widget(&mut scene.app, scene.panel, "level");
        let camera = scene.camera;
        add_pointer(&mut scene, DISPATCH_POINTER);
        let bogus_camera = scene.app.world_mut().spawn_empty().id();

        // Accepted press through the real dispatcher: observed, captured,
        // dragging. Only `pointer_events` runs, so the reconciler has not yet
        // drained this observation.
        set_hover(&mut scene, DISPATCH_POINTER, &[widget], &[widget], camera);
        feed_input(
            &mut scene,
            DISPATCH_POINTER,
            center(),
            PointerAction::Press(PointerButton::Primary),
        );
        let result = scene.app.world_mut().run_system_cached(pointer_events);
        assert!(result.is_ok());
        assert!(dragging(&scene, widget));
        assert_eq!(owner_of(&scene, widget), Some(DISPATCH_POINTER));

        // The same retained unread window carries the real release and then a
        // later primary raw press. The empty previous hover keeps the release
        // out of Bevy's targeted path so the reconciler owns it; the current
        // hover names a camera that cannot project, so when the shipped
        // `pointer_events` delivers the later press, `grab_from_pointer`
        // receives it and rejects it on projection failure before `observe_press`
        // — a real unobserved press that raised the raw count without entering
        // the widget capture-order path.
        set_hover(&mut scene, DISPATCH_POINTER, &[], &[widget], bogus_camera);
        feed_input(
            &mut scene,
            DISPATCH_POINTER,
            center(),
            PointerAction::Release(PointerButton::Primary),
        );
        feed_input(
            &mut scene,
            DISPATCH_POINTER,
            center(),
            PointerAction::Press(PointerButton::Primary),
        );
        let result = scene.app.world_mut().run_system_cached(pointer_events);
        assert!(result.is_ok());
        let result = scene
            .app
            .world_mut()
            .run_system_cached(reconcile_pointer_input);
        assert!(result.is_ok());
        scene.app.world_mut().flush();

        // The later press reached `grab_from_pointer` yet claimed and observed
        // nothing, so no pending recapture is recorded.
        assert!(!scene.app.world().resource::<SliderCaptures>().has_pending());

        // The accepted drag terminalizes after its real release and frees
        // occupancy; the later unobserved press does not defer it.
        assert_eq!(
            dispatch_lifecycle(&scene),
            [
                Lifecycle::Grabbed,
                Lifecycle::Proposal { is_final: false },
                Lifecycle::Proposal { is_final: true },
                Lifecycle::Released,
            ]
        );
        assert!(!dragging(&scene, widget));
        assert!(dispatch_empty(&scene));
    }

    #[test]
    fn raw_release_finalizes_widget_with_missing_lookup_components() {
        let mut scene = dispatch_scene(slider_tree("level", slider0()));
        let widget = resolve_widget(&mut scene.app, scene.panel, "level");
        let camera = scene.camera;
        add_pointer(&mut scene, DISPATCH_POINTER);

        // Live capture through the real dispatcher.
        set_hover(&mut scene, DISPATCH_POINTER, &[widget], &[widget], camera);
        run_raw(
            &mut scene,
            &[(
                DISPATCH_POINTER,
                center(),
                PointerAction::Press(PointerButton::Primary),
            )],
        );
        assert!(dragging(&scene, widget));
        assert_eq!(owner_of(&scene, widget), Some(DISPATCH_POINTER));

        // The widget loses its lookup components while its shared occupancy and
        // typed slider payload survive, so the reconciler's `WidgetKind` lookup
        // returns `None`.
        scene
            .app
            .world_mut()
            .entity_mut(widget)
            .remove::<WidgetOf>();
        scene.app.world_mut().flush();

        // A raw release the reconciler must resolve, followed by a later press
        // in the same window.
        set_hover(&mut scene, DISPATCH_POINTER, &[], &[], camera);
        run_raw(
            &mut scene,
            &[
                (
                    DISPATCH_POINTER,
                    center(),
                    PointerAction::Release(PointerButton::Primary),
                ),
                (
                    DISPATCH_POINTER,
                    center(),
                    PointerAction::Press(PointerButton::Primary),
                ),
            ],
        );

        // The lost widget is finalized through its surviving typed payload
        // exactly once and shared occupancy frees; the reconciler never treated
        // the pointer as freed while no terminal ran.
        assert_eq!(
            dispatch_lifecycle(&scene).last(),
            Some(&Lifecycle::Canceled(SliderCancelCause::CaptureLost))
        );
        assert!(!dragging(&scene, widget));
        assert!(dispatch_empty(&scene));
    }

    #[test]
    fn raw_cancel_finalizes_lost_widget_with_pointer_canceled_cause() {
        let mut scene = dispatch_scene(slider_tree("level", slider0()));
        let widget = resolve_widget(&mut scene.app, scene.panel, "level");
        let camera = scene.camera;
        add_pointer(&mut scene, DISPATCH_POINTER);

        set_hover(&mut scene, DISPATCH_POINTER, &[widget], &[widget], camera);
        run_raw(
            &mut scene,
            &[(
                DISPATCH_POINTER,
                center(),
                PointerAction::Press(PointerButton::Primary),
            )],
        );
        assert!(dragging(&scene, widget));

        // The widget loses its lookup components so the reconciler's `WidgetKind`
        // lookup returns `None`, while its shared occupancy and typed slider
        // payload survive.
        scene
            .app
            .world_mut()
            .entity_mut(widget)
            .remove::<WidgetOf>();
        scene.app.world_mut().flush();

        // A raw cancel the reconciler must resolve. The cleared hover keeps the
        // targeted `Pointer<Cancel>` from firing, so the lost-widget fallback
        // owns it and must preserve the concrete `PointerCanceled` cause rather
        // than degrading a cancel to `CaptureLost`.
        set_hover(&mut scene, DISPATCH_POINTER, &[], &[], camera);
        run_raw(
            &mut scene,
            &[(DISPATCH_POINTER, center(), PointerAction::Cancel)],
        );

        assert_eq!(
            dispatch_lifecycle(&scene).last(),
            Some(&Lifecycle::Canceled(SliderCancelCause::PointerCanceled))
        );
        assert!(!dragging(&scene, widget));
        assert!(dispatch_empty(&scene));
    }

    #[test]
    fn raw_release_with_present_kind_and_missing_payload_frees_nothing() {
        let mut scene = dispatch_scene(slider_tree("level", slider0()));
        let widget = resolve_widget(&mut scene.app, scene.panel, "level");
        let camera = scene.camera;
        add_pointer(&mut scene, DISPATCH_POINTER);

        set_hover(&mut scene, DISPATCH_POINTER, &[widget], &[widget], camera);
        run_raw(
            &mut scene,
            &[(
                DISPATCH_POINTER,
                center(),
                PointerAction::Press(PointerButton::Primary),
            )],
        );
        assert!(dragging(&scene, widget));
        assert_eq!(owner_of(&scene, widget), Some(DISPATCH_POINTER));
        let baseline = dispatch_lifecycle(&scene);

        // Drop the typed slider payload while the `WidgetKind` lookup and shared
        // occupancy remain: `resolve_release` finds no captured payload to
        // reproject.
        assert!(
            scene
                .app
                .world_mut()
                .resource_mut::<SliderCaptures>()
                .take(widget)
                .is_some()
        );

        // A raw release the reconciler owns (empty previous hover) followed by a
        // later press delivered to the still-occupied widget. Because the release
        // records no terminal, the reconciler must not mark the pointer freed, so
        // the later press cannot claim the widget.
        set_hover(&mut scene, DISPATCH_POINTER, &[], &[widget], camera);
        run_raw(
            &mut scene,
            &[
                (
                    DISPATCH_POINTER,
                    center(),
                    PointerAction::Release(PointerButton::Primary),
                ),
                (
                    DISPATCH_POINTER,
                    center(),
                    PointerAction::Press(PointerButton::Primary),
                ),
            ],
        );

        // No terminal ran, occupancy is retained, and the later press claimed
        // nothing.
        assert_eq!(dispatch_lifecycle(&scene), baseline);
        assert!(dragging(&scene, widget));
        assert_eq!(owner_of(&scene, widget), Some(DISPATCH_POINTER));
        assert!(!scene.app.world().resource::<SliderCaptures>().has_pending());
    }

    #[test]
    fn dispatched_moves_reproject_and_replace_raw_target() {
        let mut scene = dispatch_scene(slider_tree("level", slider0()));
        let widget = resolve_widget(&mut scene.app, scene.panel, "level");
        let camera = scene.camera;
        add_pointer(&mut scene, DISPATCH_POINTER);

        // Press to capture, then feed raw `Move` actions so Bevy's dispatcher
        // delivers real `DragStart` / `Drag` and the slider reprojects each
        // position through `pointer_events`.
        feed_hit(&mut scene, DISPATCH_POINTER, camera, widget);
        feed_input(
            &mut scene,
            DISPATCH_POINTER,
            center(),
            PointerAction::Press(PointerButton::Primary),
        );
        scene.app.update();
        assert!(dragging(&scene, widget));

        feed_input(
            &mut scene,
            DISPATCH_POINTER,
            past_right(),
            PointerAction::Move {
                delta: past_right() - center(),
            },
        );
        scene.app.update();
        let after_right = scene
            .app
            .world()
            .resource::<SliderCaptures>()
            .latest_raw_target(widget)
            .expect("a live drag stores a raw target");

        feed_input(
            &mut scene,
            DISPATCH_POINTER,
            past_left(),
            PointerAction::Move {
                delta: past_left() - past_right(),
            },
        );
        scene.app.update();
        let after_left = scene
            .app
            .world()
            .resource::<SliderCaptures>()
            .latest_raw_target(widget)
            .expect("a live drag stores a raw target");

        // The grab plus two real drags each emit exactly one non-final proposal.
        assert_eq!(
            dispatch_lifecycle(&scene),
            [
                Lifecycle::Grabbed,
                Lifecycle::Proposal { is_final: false },
                Lifecycle::Proposal { is_final: false },
                Lifecycle::Proposal { is_final: false },
            ]
        );
        // Two opposite drags replace the stored raw target with different
        // values.
        assert_close(after_right, 1.0);
        assert_close(after_left, 0.0);
        assert!(after_right > after_left);
        assert!(dragging(&scene, widget));
    }

    #[test]
    fn dispatched_move_projection_loss_cancels_once_with_later_dragend() {
        let mut scene = dispatch_scene(slider_tree("level", slider0()));
        let widget = resolve_widget(&mut scene.app, scene.panel, "level");
        let camera = scene.camera;
        add_pointer(&mut scene, DISPATCH_POINTER);

        feed_hit(&mut scene, DISPATCH_POINTER, camera, widget);
        feed_input(
            &mut scene,
            DISPATCH_POINTER,
            center(),
            PointerAction::Press(PointerButton::Primary),
        );
        scene.app.update();
        assert!(dragging(&scene, widget));

        // The captured camera is lost, so the next real drag cannot project.
        // The drag cancels exactly once even though the release still delivers a
        // later `DragEnd` for the same pointer.
        scene.app.world_mut().entity_mut(camera).despawn();
        scene.app.world_mut().flush();

        feed_input(
            &mut scene,
            DISPATCH_POINTER,
            past_right(),
            PointerAction::Move {
                delta: past_right() - center(),
            },
        );
        feed_input(
            &mut scene,
            DISPATCH_POINTER,
            past_right(),
            PointerAction::Release(PointerButton::Primary),
        );
        scene.app.update();

        let canceled = dispatch_lifecycle(&scene)
            .iter()
            .filter(|event| matches!(event, Lifecycle::Canceled(_)))
            .count();
        assert_eq!(canceled, 1);
        assert_eq!(
            dispatch_lifecycle(&scene).last(),
            Some(&Lifecycle::Canceled(
                SliderCancelCause::ProjectionUnavailable
            ))
        );
        assert!(!dragging(&scene, widget));
        assert!(dispatch_empty(&scene));
    }

    #[test]
    fn picking_without_interaction_skips_slider_reconciliation() {
        let mut app = test_app();
        app.set_error_handler(bevy::ecs::error::panic)
            .insert_resource(PickingSettings {
                is_enabled: false,
                ..default()
            })
            .add_plugins(PickingPlugin);
        let _panel = spawn_panel(&mut app, slider_tree("level", slider0()));

        app.update();

        // Without `InteractionPlugin` the hover state never exists, so the
        // reconciler's run condition is never met.
        assert!(!app.world().contains_resource::<PointerState>());
        assert!(!app.world().contains_resource::<HoverMap>());
    }

    #[test]
    fn same_id_refresh_preserves_live_drag_and_applied_value() {
        let mut scene = ltr_scene();
        press(&mut scene, CAPTURE_VIEWPORT * 0.5);
        assert!(is_dragging(&scene));
        let applied = scene
            .app
            .world_mut()
            .get_mut::<SliderState>(scene.widget)
            .expect("widget should carry slider state")
            .set_value(0.75);
        assert_eq!(applied, Ok(true));

        // Re-author the identical slider: same panel, id, and kind. The refresh
        // preserves the capture and the live applied value.
        let panel = scene.panel;
        scene
            .app
            .world_mut()
            .commands()
            .set_tree(panel, slider_tree("level", slider0()))
            .expect("same-id refresh should be accepted");
        scene.app.update();

        assert_eq!(resolve_widget(&mut scene.app, panel, "level"), scene.widget);
        assert!(is_dragging(&scene), "the refresh preserves the live drag");
        assert_close(applied_value(&scene), 0.75);
        assert!(!captures_empty(&scene));
        assert_eq!(
            lifecycle(&scene),
            [Lifecycle::Grabbed, Lifecycle::Proposal { is_final: false }]
        );
    }

    #[test]
    fn built_in_escape_cancels_the_active_slider_drag() {
        let mut app = capture_app();
        app.add_plugins((InputPlugin, WidgetInputPlugin));
        let window = app
            .world_mut()
            .spawn((
                Window {
                    focused: true,
                    ..default()
                },
                PrimaryWindow,
            ))
            .id();
        let camera = app
            .world_mut()
            .spawn((
                seeded_camera(),
                GlobalTransform::from(
                    Transform::from_xyz(0.0, 0.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
                ),
                RenderTarget::Window(WindowRef::Primary),
            ))
            .id();
        app.finish();
        let panel = projecting_panel(&mut app, slider_tree("level", slider0()));
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");
        let target = RenderTarget::Window(WindowRef::Primary)
            .normalize(Some(window))
            .expect("primary window target normalizes");

        app.world_mut()
            .trigger(RequestWidgetFocus { window, widget });
        app.world_mut().flush();
        app.world_mut().trigger(Pointer::new(
            CAPTURE_POINTER,
            Location {
                target,
                position: CAPTURE_VIEWPORT * 0.5,
            },
            Press {
                button: PointerButton::Primary,
                hit:    HitData::new(camera, 0.0, None, None),
                count:  1,
            },
            widget,
        ));
        app.world_mut().flush();
        assert!(app.world().get::<SliderDrag>(widget).is_some());

        // The built-in Escape binding reaches the same explicit-cancel path as
        // `SemanticWidgetIntent::Cancel`.
        app.world_mut().write_message(KeyboardInput {
            key_code: KeyCode::Escape,
            logical_key: Key::Unidentified(NativeKey::Unidentified),
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window,
        });
        app.update();

        assert_eq!(
            app.world().resource::<RecordedLifecycle>().0.last(),
            Some(&Lifecycle::Canceled(SliderCancelCause::Explicit))
        );
        assert!(app.world().resource::<SliderCaptures>().is_empty());
        assert!(app.world().resource::<WidgetCaptures>().is_empty());
    }

    const THUMB_ID: &str = "thumb";
    const STATE_HOVER_FILL: Color = Color::srgb(0.2, 0.4, 0.6);
    const STATE_FOCUS_BORDER: Color = Color::srgb(0.9, 0.8, 0.2);

    fn plain_slider(value: f32) -> Slider {
        Slider::new(range(0.0, 1.0), value)
            .expect("slider should validate")
            .direction(SliderDirection::LeftToRight)
    }

    fn thumb_slider_tree(id: &str, slider: Slider, thumb_size: f32) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(
            El::overlay()
                .size(40.0, 16.0)
                .background(Color::WHITE)
                .border(Border::all(1.0, Color::WHITE))
                .alignment(AlignX::Left, AlignY::Center)
                .slider(id, slider),
            |builder| {
                builder.with(
                    El::new().size(thumb_size, 8.0).id(THUMB_ID).slider_thumb(),
                    |_| {},
                );
            },
        );
        builder.build()
    }

    fn build_world_panel(tree: LayoutTree) -> Result<DiegeticPanel, PanelBuildError> {
        DiegeticPanel::world()
            .size(Mm(100.0), Mm(50.0))
            .with_tree(tree)
            .build()
    }

    fn slots_of(app: &App, widget: Entity) -> WidgetVisualSlots {
        app.world()
            .get::<WidgetVisualSlots>(widget)
            .cloned()
            .expect("widget should carry visual slots")
    }

    fn root_override(app: &App, widget: Entity) -> Option<VisualSlotOverride> {
        app.world()
            .get::<WidgetVisualOverrides>(widget)
            .and_then(|overrides| overrides.get(VisualSlotId::SLIDER_ROOT).cloned())
    }

    fn thumb_offset(app: &App, widget: Entity) -> Option<Vec2> {
        app.world()
            .get::<WidgetVisualOverrides>(widget)
            .and_then(|overrides| overrides.get(VisualSlotId::SLIDER_THUMB).cloned())
            .and_then(|value| value.offset)
    }

    fn set_slider_value(app: &mut App, widget: Entity, value: f32) {
        let mut state = app
            .world_mut()
            .get_mut::<SliderState>(widget)
            .expect("widget should carry slider state");
        state.set_value(value).expect("value should apply");
    }

    #[test]
    fn thumb_translation_tracks_applied_value_without_relayout() {
        let mut app = test_app();
        let panel = spawn_panel(&mut app, thumb_slider_tree("level", plain_slider(0.0), 8.0));
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");

        // At the directed range start the thumb sits at its authored center.
        let start = thumb_offset(&app, widget).expect("thumb override present");
        assert_close(start.x, 0.0);
        assert_close(start.y, 0.0);

        let computed_before = app
            .world()
            .entity(panel)
            .get_ref::<ComputedDiegeticPanel>()
            .map(|computed| computed.last_changed());

        // Applying the range end moves the thumb across the full directed
        // travel: content extent minus the thumb extent.
        set_slider_value(&mut app, widget, 1.0);
        app.update();
        let end = thumb_offset(&app, widget).expect("thumb override present");
        let slots = slots_of(&app, widget);
        let content = slots
            .content_box(VisualSlotId::SLIDER_ROOT)
            .expect("root content box");
        let thumb = slots
            .border_box(VisualSlotId::SLIDER_THUMB)
            .expect("thumb border box");
        let scale = panel_points_to_world(&app, panel);
        let expected_end = to_render_offset(Vec2::new(content.width - thumb.width, 0.0), scale);
        assert_close(end.x, expected_end.x);
        assert_close(end.y, expected_end.y);

        // No relayout ran for the value change: the panel's computed output
        // kept its change tick.
        let computed_after = app
            .world()
            .entity(panel)
            .get_ref::<ComputedDiegeticPanel>()
            .map(|computed| computed.last_changed());
        assert_eq!(computed_after, computed_before);
    }

    #[test]
    fn root_state_override_follows_hover_and_clears() {
        let mut app = test_app();
        let slider = plain_slider(0.5)
            .hovered_background(STATE_HOVER_FILL)
            .focused_border_color(STATE_FOCUS_BORDER);
        let panel = spawn_panel(&mut app, thumb_slider_tree("level", slider, 8.0));
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");
        assert_eq!(root_override(&app, widget), None);

        app.world_mut()
            .entity_mut(widget)
            .insert(PickingInteraction::Hovered);
        app.update();
        assert_eq!(
            root_override(&app, widget),
            Some(VisualSlotOverride {
                fill_color: Some(STATE_HOVER_FILL),
                ..VisualSlotOverride::default()
            }),
        );

        app.world_mut()
            .entity_mut(widget)
            .insert(PickingInteraction::None);
        app.update();
        assert_eq!(root_override(&app, widget), None);
    }

    #[test]
    fn oversized_thumb_centers_on_content_and_blocks_projection() {
        let mut app = test_app();
        // A 60-unit thumb inside a 38-unit content box exceeds the content.
        let panel = spawn_panel(
            &mut app,
            thumb_slider_tree("level", plain_slider(0.5), 60.0),
        );
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");
        let slots = slots_of(&app, widget);
        let content = slots
            .content_box(VisualSlotId::SLIDER_ROOT)
            .expect("root content box");
        let thumb = slots
            .border_box(VisualSlotId::SLIDER_THUMB)
            .expect("thumb border box");
        assert!(
            thumb.width >= content.width,
            "the thumb must exceed the content for this case",
        );

        // The thumb centers on the content box's active axis.
        let offset = thumb_offset(&app, widget).expect("thumb override present");
        let scale = panel_points_to_world(&app, panel);
        let expected =
            to_render_offset(Vec2::new(content.center().0 - thumb.center().0, 0.0), scale);
        assert_close(offset.x, expected.x);
        assert_close(offset.y, expected.y);

        // The same geometry leaves pointer projection unavailable.
        assert_eq!(
            project_pointer_value(
                Vec2::new(content.center().0, content.center().1),
                content,
                Some(thumb.width),
                range(0.0, 1.0),
                SliderDirection::LeftToRight,
            ),
            Err(ProjectionUnavailable),
        );
    }

    #[test]
    fn orphan_thumb_is_rejected_with_its_id() {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(El::new().size(8.0, 8.0).id("stray").slider_thumb(), |_| {});
        assert!(matches!(
            build_world_panel(builder.build()),
            Err(PanelBuildError::SliderThumbOutsideSlider(id))
                if id == PanelElementId::named("stray")
        ));
    }

    #[test]
    fn second_thumb_is_rejected_with_the_slider_id() {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(
            El::overlay()
                .size(40.0, 16.0)
                .slider("level", plain_slider(0.5)),
            |builder| {
                builder.with(El::new().size(8.0, 8.0).id("a").slider_thumb(), |_| {});
                builder.with(El::new().size(8.0, 8.0).id("b").slider_thumb(), |_| {});
            },
        );
        assert!(matches!(
            build_world_panel(builder.build()),
            Err(PanelBuildError::SliderHasMultipleThumbs(id))
                if id == PanelElementId::named("level")
        ));
    }

    #[test]
    fn slider_state_builders_require_authored_surfaces() {
        let mut background = LayoutBuilder::new(100.0, 50.0);
        background.with(
            El::new()
                .size(40.0, 16.0)
                .slider("level", plain_slider(0.5).hovered_background(Color::WHITE)),
            |_| {},
        );
        assert!(matches!(
            build_world_panel(background.build()),
            Err(PanelBuildError::SliderStateBackgroundRequiresBackground(id))
                if id == PanelElementId::named("level")
        ));

        let mut border = LayoutBuilder::new(100.0, 50.0);
        border.with(
            El::new().size(40.0, 16.0).background(Color::WHITE).slider(
                "level",
                plain_slider(0.5).hovered_border_color(Color::WHITE),
            ),
            |_| {},
        );
        assert!(matches!(
            build_world_panel(border.build()),
            Err(PanelBuildError::SliderStateBorderColorRequiresBorder(id))
                if id == PanelElementId::named("level")
        ));

        let mut material = LayoutBuilder::new(100.0, 50.0);
        material.with(
            El::new().size(40.0, 16.0).slider(
                "level",
                plain_slider(0.5).hovered_material(Handle::<StandardMaterial>::default()),
            ),
            |_| {},
        );
        assert!(matches!(
            build_world_panel(material.build()),
            Err(PanelBuildError::SliderStateMaterialRequiresSurface(id))
                if id == PanelElementId::named("level")
        ));
    }

    const STATE_FOCUS_FILL: Color = Color::srgb(0.15, 0.15, 0.4);
    const STATE_PRESS_FILL: Color = Color::srgb(0.6, 0.3, 0.1);
    const STATE_DISABLED_BORDER: Color = Color::srgb(0.35, 0.35, 0.4);

    fn directed_slider(direction: SliderDirection, value: f32) -> Slider {
        Slider::new(range(0.0, 1.0), value)
            .expect("slider should validate")
            .direction(direction)
    }

    fn state_slider() -> Slider {
        plain_slider(0.5)
            .focused_background(STATE_FOCUS_FILL)
            .focused_border_color(STATE_FOCUS_BORDER)
            .hovered_background(STATE_HOVER_FILL)
            .pressed_background(STATE_PRESS_FILL)
            .disabled_border_color(STATE_DISABLED_BORDER)
    }

    /// A slider whose marked thumb is authored at the content-box center — away
    /// from every directed range start — so each expected presentation
    /// translation is genuinely computed from the solved authored center rather
    /// than reading back a zero offset. The root is intentionally non-square so
    /// an active-axis swap yields a wrong translation.
    fn centered_thumb_tree(slider: Slider, thumb_width: f32, thumb_height: f32) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(
            El::overlay()
                .size(40.0, 24.0)
                .background(Color::WHITE)
                .border(Border::all(1.0, Color::WHITE))
                .alignment(AlignX::Center, AlignY::Center)
                .slider("level", slider),
            |builder| {
                builder.with(
                    El::new()
                        .size(thumb_width, thumb_height)
                        .id(THUMB_ID)
                        .slider_thumb(),
                    |_| {},
                );
            },
        );
        builder.build()
    }

    /// Like [`centered_thumb_tree`] but authors the thumb at the top-left corner
    /// so an oversized thumb keeps a non-zero centering delta on both axes.
    fn corner_thumb_tree(slider: Slider, thumb_width: f32, thumb_height: f32) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(
            El::overlay()
                .size(40.0, 24.0)
                .background(Color::WHITE)
                .border(Border::all(1.0, Color::WHITE))
                .alignment(AlignX::Left, AlignY::Top)
                .slider("level", slider),
            |builder| {
                builder.with(
                    El::new()
                        .size(thumb_width, thumb_height)
                        .id(THUMB_ID)
                        .slider_thumb(),
                    |_| {},
                );
            },
        );
        builder.build()
    }

    fn headless_slider_tree(id: &str, slider: Slider) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.with(
            El::new()
                .size(40.0, 16.0)
                .background(Color::WHITE)
                .border(Border::all(1.0, Color::WHITE))
                .slider(id, slider),
            |_| {},
        );
        builder.build()
    }

    /// Active-axis start, extent, thumb extent, and authored thumb center for
    /// `direction`, read from solved slot boxes.
    fn active_axis_metrics(
        content: BoundingBox,
        thumb: BoundingBox,
        direction: SliderDirection,
    ) -> (f32, f32, f32, f32) {
        match direction {
            SliderDirection::LeftToRight | SliderDirection::RightToLeft => {
                (content.x, content.width, thumb.width, thumb.center().0)
            },
            SliderDirection::BottomToTop | SliderDirection::TopToBottom => {
                (content.y, content.height, thumb.height, thumb.center().1)
            },
        }
    }

    /// First-principles expected thumb translation for a normalized value,
    /// independent of the production endpoint math under test.
    fn expected_thumb_offset(
        content: BoundingBox,
        thumb: BoundingBox,
        direction: SliderDirection,
        fraction: f32,
    ) -> Vec2 {
        let (content_start, content_extent, thumb_extent, authored_center) =
            active_axis_metrics(content, thumb, direction);
        let start_coordinate = thumb_extent.mul_add(0.5, content_start);
        let end_coordinate = thumb_extent.mul_add(-0.5, content_start + content_extent);
        let (travel_start, travel_end) = match direction {
            SliderDirection::LeftToRight | SliderDirection::TopToBottom => {
                (start_coordinate, end_coordinate)
            },
            SliderDirection::RightToLeft | SliderDirection::BottomToTop => {
                (end_coordinate, start_coordinate)
            },
        };
        let desired_center = fraction.mul_add(travel_end - travel_start, travel_start);
        let delta = desired_center - authored_center;
        match direction {
            SliderDirection::LeftToRight | SliderDirection::RightToLeft => Vec2::new(delta, 0.0),
            SliderDirection::BottomToTop | SliderDirection::TopToBottom => Vec2::new(0.0, delta),
        }
    }

    fn active_offset(offset: Vec2, direction: SliderDirection) -> f32 {
        match direction {
            SliderDirection::LeftToRight | SliderDirection::RightToLeft => offset.x,
            SliderDirection::BottomToTop | SliderDirection::TopToBottom => offset.y,
        }
    }

    fn cross_offset(offset: Vec2, direction: SliderDirection) -> f32 {
        match direction {
            SliderDirection::LeftToRight | SliderDirection::RightToLeft => offset.y,
            SliderDirection::BottomToTop | SliderDirection::TopToBottom => offset.x,
        }
    }

    fn thumb_override(app: &App, widget: Entity) -> Option<VisualSlotOverride> {
        app.world()
            .get::<WidgetVisualOverrides>(widget)
            .and_then(|overrides| overrides.get(VisualSlotId::SLIDER_THUMB).cloned())
    }

    /// The owning panel's live layout-points-to-world scale, the same factor
    /// the presentation writer converts thumb deltas through.
    fn panel_points_to_world(app: &App, panel: Entity) -> f32 {
        app.world()
            .get::<DiegeticPanel>(panel)
            .expect("panel should carry its role component")
            .points_to_world()
    }

    /// Converts a layout-frame thumb delta into the panel-local render frame
    /// independently of the production conversion: X scales, Y scales and
    /// inverts.
    fn to_render_offset(layout_delta: Vec2, points_to_world: f32) -> Vec2 {
        Vec2::new(
            layout_delta.x * points_to_world,
            -layout_delta.y * points_to_world,
        )
    }

    fn indexed_offset(app: &App, panel: Entity, element_index: usize) -> Option<Vec2> {
        app.world()
            .resource::<crate::widgets::VisualOverrideIndex>()
            .get(panel, element_index)
            .and_then(|value| value.offset)
    }

    fn indexed_root(app: &App, panel: Entity, element_index: usize) -> Option<VisualSlotOverride> {
        app.world()
            .resource::<crate::widgets::VisualOverrideIndex>()
            .get(panel, element_index)
            .cloned()
    }

    fn computed_tick(app: &App, panel: Entity) -> Option<bevy::ecs::change_detection::Tick> {
        app.world()
            .entity(panel)
            .get_ref::<ComputedDiegeticPanel>()
            .map(|computed| computed.last_changed())
    }

    const ALL_DIRECTIONS: [SliderDirection; 4] = [
        SliderDirection::LeftToRight,
        SliderDirection::RightToLeft,
        SliderDirection::TopToBottom,
        SliderDirection::BottomToTop,
    ];

    #[test]
    fn thumb_tracks_applied_value_at_both_endpoints_in_all_directions() {
        for direction in ALL_DIRECTIONS {
            let mut app = test_app();
            let panel = spawn_panel(
                &mut app,
                centered_thumb_tree(directed_slider(direction, 0.0), 8.0, 8.0),
            );
            app.update();
            let widget = resolve_widget(&mut app, panel, "level");
            let slots = slots_of(&app, widget);
            let content = slots
                .content_box(VisualSlotId::SLIDER_ROOT)
                .expect("root content box");
            let thumb = slots
                .border_box(VisualSlotId::SLIDER_THUMB)
                .expect("thumb border box");
            let thumb_index = slots
                .element_index(VisualSlotId::SLIDER_THUMB)
                .expect("thumb element index");
            let root_box_before = slots
                .border_box(VisualSlotId::SLIDER_ROOT)
                .expect("root border box");
            let transform_before = *app
                .world()
                .get::<Transform>(widget)
                .expect("widget transform");
            let computed_before = computed_tick(&app, panel);

            // The authored center is not the directed start, so value 0 already
            // computes a non-zero translation back to the range start.
            let at_start = thumb_offset(&app, widget).expect("thumb override present");
            let scale = panel_points_to_world(&app, panel);
            let expected_start =
                to_render_offset(expected_thumb_offset(content, thumb, direction, 0.0), scale);
            assert_close(at_start.x, expected_start.x);
            assert_close(at_start.y, expected_start.y);
            assert!(
                active_offset(at_start, direction).abs() > f32::EPSILON,
                "{direction:?} value-0 translation must be computed from the authored center",
            );

            set_slider_value(&mut app, widget, 1.0);
            app.update();

            let at_end = thumb_offset(&app, widget).expect("thumb override present");
            let expected_end =
                to_render_offset(expected_thumb_offset(content, thumb, direction, 1.0), scale);
            assert_close(at_end.x, expected_end.x);
            assert_close(at_end.y, expected_end.y);
            assert_close(cross_offset(at_end, direction), 0.0);
            assert_eq!(
                indexed_offset(&app, panel, thumb_index),
                Some(at_end),
                "{direction:?} thumb translation must reach the visual override index",
            );

            // The value change moved only the thumb record: slider hit geometry
            // and the panel's computed layout stayed put.
            let slots_after = slots_of(&app, widget);
            assert_eq!(
                slots_after.border_box(VisualSlotId::SLIDER_ROOT),
                Some(root_box_before),
                "{direction:?} slider hit geometry must not move",
            );
            assert_eq!(
                *app.world()
                    .get::<Transform>(widget)
                    .expect("widget transform"),
                transform_before,
            );
            assert_eq!(
                computed_tick(&app, panel),
                computed_before,
                "{direction:?} value change must not relayout",
            );
        }
    }

    #[test]
    fn same_id_reauthor_adopts_new_direction_without_recreating_entity() {
        let mut app = test_app();
        let panel = spawn_panel(
            &mut app,
            centered_thumb_tree(directed_slider(SliderDirection::LeftToRight, 0.0), 8.0, 8.0),
        );
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");
        assert_eq!(
            app.world()
                .get::<SliderState>(widget)
                .map(SliderState::direction),
            Some(SliderDirection::LeftToRight),
        );
        let ltr_offset = thumb_offset(&app, widget).expect("thumb override present");

        // Re-author the same id with a reversed direction; the live value (0.0)
        // is preserved and the retained entity adopts the new direction.
        app.world_mut()
            .commands()
            .set_tree(
                panel,
                centered_thumb_tree(directed_slider(SliderDirection::RightToLeft, 0.0), 8.0, 8.0),
            )
            .expect("reversed tree should be accepted");
        app.update();
        assert_eq!(
            resolve_widget(&mut app, panel, "level"),
            widget,
            "the retained widget keeps its entity across the direction change",
        );
        assert_eq!(
            app.world()
                .get::<SliderState>(widget)
                .map(SliderState::direction),
            Some(SliderDirection::RightToLeft),
        );

        let slots = slots_of(&app, widget);
        let content = slots
            .content_box(VisualSlotId::SLIDER_ROOT)
            .expect("root content box");
        let thumb = slots
            .border_box(VisualSlotId::SLIDER_THUMB)
            .expect("thumb border box");
        let rtl_offset = thumb_offset(&app, widget).expect("thumb override present");
        let scale = panel_points_to_world(&app, panel);
        let expected_rtl = to_render_offset(
            expected_thumb_offset(content, thumb, SliderDirection::RightToLeft, 0.0),
            scale,
        );
        assert_close(rtl_offset.x, expected_rtl.x);
        assert_close(rtl_offset.y, expected_rtl.y);
        assert!(
            (rtl_offset.x - ltr_offset.x).abs() > f32::EPSILON,
            "the reversed direction must move the value-0 thumb to the opposite end",
        );
    }

    #[test]
    fn oversized_thumb_centers_in_all_directions_preserving_geometry_and_projection() {
        for direction in ALL_DIRECTIONS {
            let mut app = test_app();
            // A 50x40 thumb exceeds the 38x22 content box on both axes.
            let panel = spawn_panel(
                &mut app,
                corner_thumb_tree(directed_slider(direction, 0.3), 50.0, 40.0),
            );
            app.update();
            let widget = resolve_widget(&mut app, panel, "level");
            let slots = slots_of(&app, widget);
            let content = slots
                .content_box(VisualSlotId::SLIDER_ROOT)
                .expect("root content box");
            let thumb = slots
                .border_box(VisualSlotId::SLIDER_THUMB)
                .expect("thumb border box");
            let (content_start, content_extent, thumb_extent, authored_center) =
                active_axis_metrics(content, thumb, direction);
            assert!(
                thumb_extent >= content_extent,
                "{direction:?} thumb must meet or exceed the content on the active axis",
            );
            let expected_delta = content_extent.mul_add(0.5, content_start) - authored_center;
            let scale = panel_points_to_world(&app, panel);
            let expected_layout = match direction {
                SliderDirection::LeftToRight | SliderDirection::RightToLeft => {
                    Vec2::new(expected_delta, 0.0)
                },
                SliderDirection::BottomToTop | SliderDirection::TopToBottom => {
                    Vec2::new(0.0, expected_delta)
                },
            };
            let expected_active =
                active_offset(to_render_offset(expected_layout, scale), direction);

            // Active-axis centering with the cross axis preserved, and only an
            // offset is written so the authored draw depth and material survive.
            let value = thumb_override(&app, widget).expect("thumb override present");
            let offset = value.offset.expect("offset present");
            assert_close(active_offset(offset, direction), expected_active);
            assert_close(cross_offset(offset, direction), 0.0);
            assert_eq!(value.color, None);
            assert_eq!(value.fill_color, None);
            assert_eq!(value.border_color, None);
            assert!(value.material.is_none());
            assert!(value.texture.is_none());

            // The same geometry leaves pointer projection unavailable.
            assert_eq!(
                project_pointer_value(
                    Vec2::new(content.center().0, content.center().1),
                    content,
                    Some(thumb_extent),
                    range(0.0, 1.0),
                    direction,
                ),
                Err(ProjectionUnavailable),
                "{direction:?} oversized thumb must block projection",
            );

            // A value change keeps the thumb centered, the hit geometry fixed,
            // and runs no relayout.
            let root_box_before = slots
                .border_box(VisualSlotId::SLIDER_ROOT)
                .expect("root border box");
            let computed_before = computed_tick(&app, panel);
            set_slider_value(&mut app, widget, 0.9);
            app.update();
            let after = thumb_offset(&app, widget).expect("thumb override present");
            assert_close(active_offset(after, direction), expected_active);
            assert_close(cross_offset(after, direction), 0.0);
            assert_eq!(
                slots_of(&app, widget).border_box(VisualSlotId::SLIDER_ROOT),
                Some(root_box_before),
            );
            assert_eq!(
                computed_tick(&app, panel),
                computed_before,
                "{direction:?} value change on an oversized thumb must not relayout",
            );
        }
    }

    #[test]
    fn reauthoring_from_thumb_to_no_thumb_clears_the_stale_thumb_override() {
        let mut app = test_app();
        let panel = spawn_panel(&mut app, thumb_slider_tree("level", plain_slider(0.5), 8.0));
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");
        assert!(thumb_offset(&app, widget).is_some());
        let thumb_index = slots_of(&app, widget).element_index(VisualSlotId::SLIDER_THUMB);
        assert!(thumb_index.is_some());

        // Re-author the same id slider without a marked thumb; the widget is
        // retained and its stale thumb translation is cleared through the shared
        // `write_slot_override` clear path.
        app.world_mut()
            .commands()
            .set_tree(panel, headless_slider_tree("level", plain_slider(0.5)))
            .expect("headless tree should be accepted");
        app.update();
        app.update();
        assert_eq!(resolve_widget(&mut app, panel, "level"), widget);
        assert_eq!(
            thumb_offset(&app, widget),
            None,
            "the stale thumb override must be cleared",
        );
        assert_eq!(thumb_override(&app, widget), None);
        if let Some(index) = thumb_index {
            assert!(indexed_root(&app, panel, index).is_none());
        }

        // A later quiet frame leaves the cleared override untouched.
        let tick_after_clear = app
            .world()
            .entity(widget)
            .get_ref::<WidgetVisualOverrides>()
            .map(|overrides| overrides.last_changed());
        app.update();
        assert_eq!(thumb_offset(&app, widget), None);
        assert_eq!(
            app.world()
                .entity(widget)
                .get_ref::<WidgetVisualOverrides>()
                .map(|overrides| overrides.last_changed()),
            tick_after_clear,
            "a quiet frame must not re-touch the cleared override",
        );
    }

    fn slider_gate(app: &mut App) -> Option<bool> {
        app.world_mut()
            .run_system_cached(super::presentation_inputs_changed)
            .ok()
    }

    fn assert_interactivity_rearms_slider_gate(app: &mut App, slider: Entity) {
        const MAX_REMOVAL_UPDATES: usize = 8;

        // A widget-local interactivity edit inserts `WidgetDisabled`; the marker
        // re-arms the gate.
        let result =
            app.world_mut()
                .run_system_once(move |mut writer: crate::PanelWidgetWriter| {
                    writer.override_interactivity(slider, WidgetInteractivity::Disabled)
                });
        assert_eq!(result.ok(), Some(true));
        app.update();
        assert!(app.world().get::<WidgetDisabled>(slider).is_some());
        assert_eq!(slider_gate(app), Some(true));
        assert_eq!(slider_gate(app), Some(false));

        // Restoring inherited interactivity removes `WidgetDisabled`; the marker
        // removal re-arms the gate exactly once, then a quiet frame returns false.
        let result =
            app.world_mut()
                .run_system_once(move |mut writer: crate::PanelWidgetWriter| {
                    writer.inherit_interactivity(slider)
                });
        assert_eq!(result.ok(), Some(true));
        // The removal lands within a small bounded number of updates; stop on
        // that update so the next `slider_gate` read observes the removal edge.
        let disabled_removed = (0..MAX_REMOVAL_UPDATES).any(|_| {
            app.update();
            app.world().get::<WidgetDisabled>(slider).is_none()
        });
        assert!(
            disabled_removed,
            "inherited interactivity must remove the disabled marker",
        );
        assert_eq!(
            slider_gate(app),
            Some(true),
            "removing the disabled marker re-arms the slider walk",
        );
        assert_eq!(slider_gate(app), Some(false));
    }

    #[test]
    fn slider_quiet_frames_skip_the_presentation_walk() {
        let mut app = test_app();
        let window = app.world_mut().spawn(Window::default()).id();
        let panel = spawn_panel(&mut app, button_and_slider_tree());
        app.update();
        let slider = resolve_widget(&mut app, panel, "level");
        let button = resolve_widget(&mut app, panel, "act");

        // The first probe consumes the reify-time authored changes; a quiet
        // frame then skips the all-slider walk.
        assert_eq!(slider_gate(&mut app), Some(true));
        assert_eq!(
            slider_gate(&mut app),
            Some(false),
            "a quiet frame must not run the all-slider presentation walk",
        );

        // Hover aggregate insertion, change, and removal each re-arm once.
        app.world_mut()
            .entity_mut(slider)
            .insert(PickingInteraction::Hovered);
        assert_eq!(slider_gate(&mut app), Some(true));
        assert_eq!(slider_gate(&mut app), Some(false));
        app.world_mut()
            .entity_mut(slider)
            .insert(PickingInteraction::Pressed);
        assert_eq!(slider_gate(&mut app), Some(true));
        assert_eq!(slider_gate(&mut app), Some(false));
        app.world_mut()
            .entity_mut(slider)
            .remove::<PickingInteraction>();
        assert_eq!(
            slider_gate(&mut app),
            Some(true),
            "an aggregate removal re-arms the edge back to normal",
        );
        assert_eq!(slider_gate(&mut app), Some(false));

        // The private drag marker re-arms on insertion and removal.
        app.world_mut().entity_mut(slider).insert(SliderDrag);
        assert_eq!(slider_gate(&mut app), Some(true));
        assert_eq!(slider_gate(&mut app), Some(false));
        app.world_mut().entity_mut(slider).remove::<SliderDrag>();
        assert_eq!(slider_gate(&mut app), Some(true));
        assert_eq!(slider_gate(&mut app), Some(false));

        // An applied-value change re-arms.
        set_slider_value(&mut app, slider, 0.75);
        assert_eq!(slider_gate(&mut app), Some(true));
        assert_eq!(slider_gate(&mut app), Some(false));

        // Re-authoring the widget spec or the visual slots re-arms.
        let spec = app
            .world()
            .get::<crate::widgets::WidgetSpec>(slider)
            .cloned()
            .expect("slider spec");
        app.world_mut().entity_mut(slider).insert(spec);
        assert_eq!(slider_gate(&mut app), Some(true));
        assert_eq!(slider_gate(&mut app), Some(false));
        let visual_slots = slots_of(&app, slider);
        app.world_mut().entity_mut(slider).insert(visual_slots);
        assert_eq!(slider_gate(&mut app), Some(true));
        assert_eq!(slider_gate(&mut app), Some(false));

        // Focus request and clear re-arm through their marker commands.
        app.world_mut().trigger(RequestWidgetFocus {
            window,
            widget: slider,
        });
        app.world_mut().flush();
        assert_eq!(slider_gate(&mut app), Some(true));
        assert_eq!(slider_gate(&mut app), Some(false));
        app.world_mut().trigger(crate::ClearWidgetFocus { window });
        app.world_mut().flush();
        assert_eq!(slider_gate(&mut app), Some(true));
        assert_eq!(slider_gate(&mut app), Some(false));

        assert_interactivity_rearms_slider_gate(&mut app, slider);

        // An unrelated live button change or removal does not wake the slider
        // walk.
        app.world_mut()
            .entity_mut(button)
            .insert(PickingInteraction::Hovered);
        assert_eq!(
            slider_gate(&mut app),
            Some(false),
            "an unrelated button change must not wake the slider walk",
        );
        app.world_mut()
            .entity_mut(button)
            .remove::<PickingInteraction>();
        assert_eq!(
            slider_gate(&mut app),
            Some(false),
            "an unrelated button removal must not wake the slider walk",
        );
    }

    #[test]
    fn slider_state_change_leaves_the_peer_button_untouched() {
        let mut app = test_app();
        let panel = spawn_panel(&mut app, button_and_thumb_slider_tree());
        app.update();
        let slider = resolve_widget(&mut app, panel, "level");
        let button = resolve_widget(&mut app, panel, "act");

        let slider_root = slots_of(&app, slider)
            .element_index(VisualSlotId::SLIDER_ROOT)
            .expect("slider root element index");
        let thumb_index = slots_of(&app, slider)
            .element_index(VisualSlotId::SLIDER_THUMB)
            .expect("slider thumb element index");
        let button_root = slots_of(&app, button)
            .element_index(VisualSlotId::BUTTON_ROOT)
            .expect("button root element index");

        // The idle slider owns no root override; its marked thumb already carries
        // a translation override for the authored initial value.
        assert_eq!(root_override(&app, slider), None);
        assert_eq!(indexed_root(&app, panel, slider_root), None);
        let initial_thumb_offset = thumb_offset(&app, slider).expect("initial thumb offset");
        assert_eq!(
            indexed_offset(&app, panel, thumb_index),
            Some(initial_thumb_offset)
        );

        // The untouched peer button owns no override records.
        assert!(app.world().get::<WidgetVisualOverrides>(button).is_none());
        assert_eq!(indexed_root(&app, panel, button_root), None);

        // Change the slider's value and drive it pressed through the private drag
        // marker; only the slider's own records react.
        set_slider_value(&mut app, slider, 1.0);
        app.world_mut().entity_mut(slider).insert(SliderDrag);
        app.update();

        assert!(
            root_override(&app, slider).is_some(),
            "the slider's pressed state writes its root override",
        );
        assert!(
            indexed_root(&app, panel, slider_root).is_some(),
            "the slider's root override reaches the retained index",
        );
        let moved_thumb_offset = thumb_offset(&app, slider).expect("moved thumb offset");
        assert_ne!(
            moved_thumb_offset, initial_thumb_offset,
            "the applied-value change moves the thumb override to the new value",
        );
        assert_eq!(
            indexed_offset(&app, panel, thumb_index),
            Some(moved_thumb_offset),
            "the moved thumb override reaches the retained index",
        );

        // The peer button gains no override component and no retained index entry.
        assert!(
            app.world().get::<WidgetVisualOverrides>(button).is_none(),
            "the peer button must gain no visual override component",
        );
        assert_eq!(
            indexed_root(&app, panel, button_root),
            None,
            "the peer button must retain no visual override index entry",
        );
    }

    #[test]
    fn slider_state_precedence_layers_independently_per_property() {
        let mut app = test_app();
        let window = app.world_mut().spawn(Window::default()).id();
        let panel = spawn_panel(&mut app, thumb_slider_tree("level", state_slider(), 8.0));
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");

        // Keyboard focus alone: focused fill plus focused border.
        app.world_mut()
            .trigger(RequestWidgetFocus { window, widget });
        app.world_mut().flush();
        app.update();
        assert_eq!(
            root_override(&app, widget),
            Some(VisualSlotOverride {
                fill_color: Some(STATE_FOCUS_FILL),
                border_color: Some(STATE_FOCUS_BORDER),
                ..VisualSlotOverride::default()
            }),
        );

        // Hover replaces the fill; the border falls through to focus.
        app.world_mut()
            .entity_mut(widget)
            .insert(PickingInteraction::Hovered);
        app.update();
        assert_eq!(
            root_override(&app, widget),
            Some(VisualSlotOverride {
                fill_color: Some(STATE_HOVER_FILL),
                border_color: Some(STATE_FOCUS_BORDER),
                ..VisualSlotOverride::default()
            }),
            "hover replaces the focused fill while the border falls through to focus",
        );

        // Pressed is driven by `SliderDrag` and replaces the fill; the border
        // still falls through to focus.
        app.world_mut().entity_mut(widget).insert(SliderDrag);
        app.update();
        assert_eq!(
            root_override(&app, widget),
            Some(VisualSlotOverride {
                fill_color: Some(STATE_PRESS_FILL),
                border_color: Some(STATE_FOCUS_BORDER),
                ..VisualSlotOverride::default()
            }),
            "the private drag marker drives the pressed fill",
        );

        // Disabled replaces the border; the fill falls through to press.
        let result =
            app.world_mut()
                .run_system_once(move |mut writer: crate::PanelWidgetWriter| {
                    writer.override_interactivity(widget, WidgetInteractivity::Disabled)
                });
        assert_eq!(result.ok(), Some(true));
        app.update();
        app.update();
        assert!(app.world().get::<WidgetDisabled>(widget).is_some());
        assert_eq!(
            root_override(&app, widget),
            Some(VisualSlotOverride {
                fill_color: Some(STATE_PRESS_FILL),
                border_color: Some(STATE_DISABLED_BORDER),
                ..VisualSlotOverride::default()
            }),
            "disabled replaces the border while the fill falls through to press",
        );
    }

    #[test]
    fn slider_pressed_presentation_is_driven_by_slider_drag() {
        let mut app = test_app();
        let panel = spawn_panel(
            &mut app,
            thumb_slider_tree(
                "level",
                plain_slider(0.5).pressed_background(STATE_PRESS_FILL),
                8.0,
            ),
        );
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");
        assert_eq!(root_override(&app, widget), None);

        // The all-pointer aggregate reporting `Pressed` does not drive the
        // slider's pressed layer; only the private `SliderDrag` marker does.
        app.world_mut()
            .entity_mut(widget)
            .insert(PickingInteraction::Pressed);
        app.update();
        assert_eq!(
            root_override(&app, widget),
            None,
            "the pointer-pressed aggregate alone must not show pressed state",
        );

        app.world_mut().entity_mut(widget).insert(SliderDrag);
        app.update();
        assert_eq!(
            root_override(&app, widget),
            Some(VisualSlotOverride {
                fill_color: Some(STATE_PRESS_FILL),
                ..VisualSlotOverride::default()
            }),
        );

        app.world_mut().entity_mut(widget).remove::<SliderDrag>();
        app.update();
        assert_eq!(
            root_override(&app, widget),
            None,
            "removing the drag returns to normal",
        );
    }

    #[test]
    fn slider_state_removal_edges_return_to_prior_and_normal() {
        let mut app = test_app();
        let panel = spawn_panel(
            &mut app,
            thumb_slider_tree(
                "level",
                plain_slider(0.5)
                    .hovered_background(STATE_HOVER_FILL)
                    .pressed_background(STATE_PRESS_FILL),
                8.0,
            ),
        );
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");

        app.world_mut()
            .entity_mut(widget)
            .insert(PickingInteraction::Hovered);
        app.update();
        assert_eq!(
            root_override(&app, widget).and_then(|value| value.fill_color),
            Some(STATE_HOVER_FILL),
        );
        app.world_mut().entity_mut(widget).insert(SliderDrag);
        app.update();
        assert_eq!(
            root_override(&app, widget).and_then(|value| value.fill_color),
            Some(STATE_PRESS_FILL),
        );

        // Removing the drag returns to the prior hovered fill.
        app.world_mut().entity_mut(widget).remove::<SliderDrag>();
        app.update();
        assert_eq!(
            root_override(&app, widget).and_then(|value| value.fill_color),
            Some(STATE_HOVER_FILL),
            "removing the drag returns to the prior hover state",
        );

        // Removing the hover aggregate returns to normal.
        app.world_mut()
            .entity_mut(widget)
            .remove::<PickingInteraction>();
        app.update();
        assert_eq!(
            root_override(&app, widget),
            None,
            "removing the hover aggregate returns to normal",
        );
    }

    #[test]
    fn slider_first_override_insertion_reaches_dispatch_through_the_fence() {
        let mut app = test_app();
        let panel = spawn_panel(
            &mut app,
            headless_slider_tree(
                "level",
                plain_slider(0.5).hovered_background(STATE_HOVER_FILL),
            ),
        );
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");
        // A headless slider writes no thumb translation, so the hover override is
        // genuinely the first insertion onto a widget without the component.
        assert!(app.world().get::<WidgetVisualOverrides>(widget).is_none());
        let root_index = slots_of(&app, widget)
            .element_index(VisualSlotId::SLIDER_ROOT)
            .expect("root element index");
        assert!(indexed_root(&app, panel, root_index).is_none());

        app.world_mut()
            .entity_mut(widget)
            .insert(PickingInteraction::Hovered);
        app.update();

        let expected = VisualSlotOverride {
            fill_color: Some(STATE_HOVER_FILL),
            ..VisualSlotOverride::default()
        };
        assert_eq!(root_override(&app, widget), Some(expected.clone()));
        assert_eq!(
            indexed_root(&app, panel, root_index),
            Some(expected),
            "the first override insertion reaches dispatch through the presentation fence",
        );
    }

    #[test]
    fn slider_repeated_identical_value_and_state_leave_the_override_untouched() {
        let mut app = test_app();
        let panel = spawn_panel(
            &mut app,
            thumb_slider_tree(
                "level",
                plain_slider(0.5).hovered_background(STATE_HOVER_FILL),
                8.0,
            ),
        );
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");
        app.world_mut()
            .entity_mut(widget)
            .insert(PickingInteraction::Hovered);
        app.update();
        let tick = app
            .world()
            .entity(widget)
            .get_ref::<WidgetVisualOverrides>()
            .map(|overrides| overrides.last_changed());
        assert!(tick.is_some());

        // Re-touching `SliderState` and re-inserting the same hovered state both
        // re-arm the walk, but the requested overrides are unchanged, so the
        // component change tick and its retained records stay put.
        set_slider_value(&mut app, widget, 0.5);
        app.world_mut()
            .entity_mut(widget)
            .insert(PickingInteraction::Hovered);
        app.update();
        app.update();
        assert_eq!(
            app.world()
                .entity(widget)
                .get_ref::<WidgetVisualOverrides>()
                .map(|overrides| overrides.last_changed()),
            tick,
            "an unchanged value and state must not dirty the override component",
        );
    }

    #[test]
    fn set_tree_rejects_invalid_slider_thumbs_and_surfaces_preserving_the_live_tree() {
        let mut app = test_app();
        let panel = spawn_panel(&mut app, thumb_slider_tree("level", plain_slider(0.5), 8.0));
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");

        let mut orphan = LayoutBuilder::new(100.0, 50.0);
        orphan.with(El::new().size(8.0, 8.0).id("stray").slider_thumb(), |_| {});
        assert!(matches!(
            app.world_mut().commands().set_tree(panel, orphan.build()),
            Err(PanelBuildError::SliderThumbOutsideSlider(id))
                if id == PanelElementId::named("stray")
        ));

        let mut duplicate = LayoutBuilder::new(100.0, 50.0);
        duplicate.with(
            El::overlay()
                .size(40.0, 16.0)
                .slider("level", plain_slider(0.5)),
            |builder| {
                builder.with(El::new().size(8.0, 8.0).id("a").slider_thumb(), |_| {});
                builder.with(El::new().size(8.0, 8.0).id("b").slider_thumb(), |_| {});
            },
        );
        assert!(matches!(
            app.world_mut().commands().set_tree(panel, duplicate.build()),
            Err(PanelBuildError::SliderHasMultipleThumbs(id))
                if id == PanelElementId::named("level")
        ));

        let mut background = LayoutBuilder::new(100.0, 50.0);
        background.with(
            El::new()
                .size(40.0, 16.0)
                .slider("level", plain_slider(0.5).hovered_background(Color::WHITE)),
            |_| {},
        );
        assert!(matches!(
            app.world_mut().commands().set_tree(panel, background.build()),
            Err(PanelBuildError::SliderStateBackgroundRequiresBackground(id))
                if id == PanelElementId::named("level")
        ));

        let mut border = LayoutBuilder::new(100.0, 50.0);
        border.with(
            El::new().size(40.0, 16.0).background(Color::WHITE).slider(
                "level",
                plain_slider(0.5).hovered_border_color(Color::WHITE),
            ),
            |_| {},
        );
        assert!(matches!(
            app.world_mut().commands().set_tree(panel, border.build()),
            Err(PanelBuildError::SliderStateBorderColorRequiresBorder(id))
                if id == PanelElementId::named("level")
        ));

        let mut material = LayoutBuilder::new(100.0, 50.0);
        material.with(
            El::new().size(40.0, 16.0).slider(
                "level",
                plain_slider(0.5).hovered_material(Handle::<StandardMaterial>::default()),
            ),
            |_| {},
        );
        assert!(matches!(
            app.world_mut().commands().set_tree(panel, material.build()),
            Err(PanelBuildError::SliderStateMaterialRequiresSurface(id))
                if id == PanelElementId::named("level")
        ));

        // Every rejected replacement left the current tree and widget live.
        app.update();
        assert_eq!(
            resolve_widget(&mut app, panel, "level"),
            widget,
            "rejected trees must leave the current widget live",
        );
        assert!(app.world().get::<SliderState>(widget).is_some());
    }

    #[test]
    fn zero_thumb_headless_slider_reifies_with_a_distinct_current_root_slot() {
        let mut app = test_app();
        let panel = spawn_panel(&mut app, headless_slider_tree("level", plain_slider(0.5)));
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");
        let slots = slots_of(&app, widget);
        assert!(slots.element_index(VisualSlotId::SLIDER_ROOT).is_some());
        assert!(slots.content_box(VisualSlotId::SLIDER_ROOT).is_some());
        assert!(slots.border_box(VisualSlotId::SLIDER_ROOT).is_some());
        assert!(slots.element_index(VisualSlotId::SLIDER_THUMB).is_none());
        assert!(slots.border_box(VisualSlotId::SLIDER_THUMB).is_none());
        assert_eq!(
            thumb_offset(&app, widget),
            None,
            "a headless slider writes no thumb translation",
        );
    }

    #[test]
    fn slider_root_and_thumb_slots_are_distinct_and_carry_current_bounds() {
        let mut app = test_app();
        let panel = spawn_panel(&mut app, thumb_slider_tree("level", plain_slider(0.5), 8.0));
        app.update();
        let widget = resolve_widget(&mut app, panel, "level");
        let slots = slots_of(&app, widget);
        let root_index = slots
            .element_index(VisualSlotId::SLIDER_ROOT)
            .expect("root slot");
        let thumb_index = slots
            .element_index(VisualSlotId::SLIDER_THUMB)
            .expect("thumb slot");
        assert_ne!(
            root_index, thumb_index,
            "the root and thumb slots resolve to distinct elements",
        );
        let root = slots
            .border_box(VisualSlotId::SLIDER_ROOT)
            .expect("root border box");
        let thumb = slots
            .border_box(VisualSlotId::SLIDER_THUMB)
            .expect("thumb border box");
        let content = slots
            .content_box(VisualSlotId::SLIDER_ROOT)
            .expect("root content box");
        assert!(thumb.width > 0.0 && thumb.height > 0.0);
        assert!(thumb.width < root.width && thumb.height < root.height);
        assert!(content.width < root.width && content.height < root.height);
    }
}
