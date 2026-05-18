//! Synthesizes adapter-side action values from raw `bevy::input` resources.
//!
//! Each frame [`inject_adapter_actions`] reads `AccumulatedMouseScroll`,
//! `AccumulatedMouseMotion`, `PinchGesture` events, the keyboard / mouse-button state, and
//! the touch tracker, then writes mocked [`ActionValue`]s into the camera's adapter actions
//! (`OrbitCamAdapterOrbitAction`, `OrbitCamAdapterPanAction`, the two adapter zoom actions).
//!
//! Types (`pub(super)` so siblings in `adapter/` can read them):
//! - [`OrbitCamAdapterFrameSources`] — component tracking which source masks contributed to each
//!   action this frame; written here, read by `resolve.rs`.
//! - [`AdapterContributions`] — file-local accumulator passed between the `apply_*` helpers.
//! - [`TrackpadScrollTarget`] / [`TrackpadScrollCandidate`] — trackpad-scroll dispatch enum and
//!   per-binding candidate.
//! - [`OrbitCamTouchAdapterOverride`] — test-only resource that forces the touch gesture stream.
//!
//! Helpers handle the five contribution sources (mouse wheel, trackpad scroll, pinch,
//! touch, button-drag-zoom) plus the trackpad-binding selection and modifier-suppression
//! logic for pinch.

use bevy::input::gestures::PinchGesture;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::input::mouse::MouseScrollUnit;
use bevy::prelude::*;
use bevy_enhanced_input::prelude::ActionMock;
use bevy_enhanced_input::prelude::ActionValue;
use bevy_enhanced_input::prelude::MockSpan;
use bevy_enhanced_input::prelude::ModKeys;
use bevy_enhanced_input::prelude::TriggerState;

use super::install::OrbitCamInputActionEntities;
use super::install::OrbitCamInstalledBindings;
use crate::constants::BUTTON_ZOOM_SCALE;
use crate::constants::PINCH_GESTURE_AMPLIFICATION;
use crate::constants::PIXEL_SCROLL_SCALE;
use crate::constants::TOUCH_PINCH_SCALE;
use crate::input;
use crate::input::CameraInteractionSources;
use crate::input::OrbitCamBindings;
use crate::input::OrbitCamButtonDragZoomAxis;
use crate::input::OrbitCamInputContextGated;
use crate::input::OrbitCamTouchBinding;
use crate::input::OrbitCamTrackpadScroll;
use crate::input::PinchGestureZoom;
use crate::input::ResolvedOrbitCamInputRoute;
use crate::input::WheelZoomPolarity;
use crate::input::ZoomDirection;
use crate::touch::TouchGestures;
use crate::touch::TouchTracker;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct OrbitCamAdapterFrameSources {
    pub(super) orbit:       CameraInteractionSources,
    pub(super) pan:         CameraInteractionSources,
    pub(super) zoom_coarse: CameraInteractionSources,
    pub(super) zoom_smooth: CameraInteractionSources,
}

pub(super) fn inject_adapter_actions(
    route: Res<ResolvedOrbitCamInputRoute>,
    scroll: Option<Res<AccumulatedMouseScroll>>,
    motion: Option<Res<AccumulatedMouseMotion>>,
    keyboard: Option<Res<ButtonInput<KeyCode>>>,
    mouse_buttons: Option<Res<ButtonInput<MouseButton>>>,
    mut pinch_events: MessageReader<PinchGesture>,
    touch_tracker: Option<Res<TouchTracker>>,
    mut cameras: Query<(
        Entity,
        &OrbitCamInstalledBindings,
        &OrbitCamInputActionEntities,
        Option<&OrbitCamInputContextGated>,
        &mut OrbitCamAdapterFrameSources,
    )>,
    mut mocks: Query<&mut ActionMock>,
    #[cfg(test)] touch_override: Option<Res<OrbitCamTouchAdapterOverride>>,
) {
    let scroll = scroll.as_deref().copied().unwrap_or_default();
    let motion = motion
        .as_deref()
        .map(|motion| motion.delta)
        .unwrap_or_default();
    let pinch = pinch_events.read().map(|event| event.0).sum::<f32>();
    let touch_gestures = {
        #[cfg(test)]
        if let Some(override_gestures) = touch_override {
            override_gestures.0.clone()
        } else if let Some(touch_tracker) = touch_tracker {
            touch_tracker.get_touch_gestures()
        } else {
            TouchGestures::None
        }
        #[cfg(not(test))]
        if let Some(touch_tracker) = touch_tracker {
            touch_tracker.get_touch_gestures()
        } else {
            TouchGestures::None
        }
    };

    for (camera, bindings, actions, gated, mut frame_sources) in &mut cameras {
        *frame_sources = OrbitCamAdapterFrameSources::default();
        if route.routed_camera() != Some(camera)
            || route.metrics_for(camera).is_none()
            || route
                .blockers_for(camera)
                .is_some_and(crate::input::OrbitCamInputBlockers::is_blocked)
            || gated.is_some_and(|gated| !gated.context_gate.is_allowed())
        {
            clear_adapter_mocks(actions, &mut mocks);
            continue;
        }

        let contributions = adapter_contributions(
            &bindings.0,
            scroll,
            motion,
            pinch,
            &touch_gestures,
            keyboard.as_deref(),
            mouse_buttons.as_deref(),
        );
        *frame_sources = contributions.sources;
        mock_adapter_actions(actions, contributions, &mut mocks);
    }
}

fn clear_adapter_mocks(actions: &OrbitCamInputActionEntities, mocks: &mut Query<&mut ActionMock>) {
    set_mock(mocks, actions.adapter_orbit, TriggerState::None, Vec2::ZERO);
    set_mock(mocks, actions.adapter_pan, TriggerState::None, Vec2::ZERO);
    set_mock(mocks, actions.adapter_zoom_coarse, TriggerState::None, 0.0);
    set_mock(mocks, actions.adapter_zoom_smooth, TriggerState::None, 0.0);
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct AdapterContributions {
    orbit:       Vec2,
    pan:         Vec2,
    zoom_coarse: f32,
    zoom_smooth: f32,
    sources:     OrbitCamAdapterFrameSources,
}

fn adapter_contributions(
    bindings: &OrbitCamBindings,
    scroll: AccumulatedMouseScroll,
    mouse_motion: Vec2,
    pinch: f32,
    touch_gestures: &TouchGestures,
    keyboard: Option<&ButtonInput<KeyCode>>,
    mouse_buttons: Option<&ButtonInput<MouseButton>>,
) -> AdapterContributions {
    let mut contributions = AdapterContributions::default();
    apply_mouse_wheel_zoom_contribution(bindings, scroll, &mut contributions);
    apply_trackpad_scroll_contribution(bindings, scroll, keyboard, &mut contributions);
    apply_pinch_contribution(bindings, pinch, keyboard, mouse_buttons, &mut contributions);
    apply_touch_contribution(bindings, touch_gestures, &mut contributions);
    apply_button_drag_zoom_contribution(bindings, mouse_motion, mouse_buttons, &mut contributions);
    contributions
}

fn apply_mouse_wheel_zoom_contribution(
    bindings: &OrbitCamBindings,
    scroll: AccumulatedMouseScroll,
    contributions: &mut AdapterContributions,
) {
    let Some(mouse_wheel_zoom) = bindings.mouse_wheel_zoom() else {
        return;
    };
    if scroll.delta == Vec2::ZERO || scroll.unit != MouseScrollUnit::Line {
        return;
    }

    contributions.zoom_coarse += zoom_signed(scroll.delta.y, bindings, mouse_wheel_zoom.polarity);
    contributions.sources.zoom_coarse = contributions
        .sources
        .zoom_coarse
        .union(CameraInteractionSources::WHEEL);
}

fn apply_trackpad_scroll_contribution(
    bindings: &OrbitCamBindings,
    scroll: AccumulatedMouseScroll,
    keyboard: Option<&ButtonInput<KeyCode>>,
    contributions: &mut AdapterContributions,
) {
    if scroll.delta == Vec2::ZERO || scroll.unit != MouseScrollUnit::Pixel {
        return;
    }

    match selected_trackpad_binding(bindings, keyboard) {
        Some(TrackpadScrollTarget::Orbit) => {
            contributions.orbit += scroll.delta;
            contributions.sources.orbit = contributions
                .sources
                .orbit
                .union(CameraInteractionSources::SMOOTH_SCROLL);
        },
        Some(TrackpadScrollTarget::Pan) => {
            contributions.pan += scroll.delta;
            contributions.sources.pan = contributions
                .sources
                .pan
                .union(CameraInteractionSources::SMOOTH_SCROLL);
        },
        Some(TrackpadScrollTarget::Zoom) => {
            contributions.zoom_smooth += zoom_signed(
                scroll.delta.y * PIXEL_SCROLL_SCALE,
                bindings,
                WheelZoomPolarity::Normal,
            );
            contributions.sources.zoom_smooth = contributions
                .sources
                .zoom_smooth
                .union(CameraInteractionSources::SMOOTH_SCROLL);
        },
        None => {},
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrackpadScrollTarget {
    Orbit,
    Pan,
    Zoom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TrackpadScrollCandidate {
    target:   TrackpadScrollTarget,
    mod_keys: ModKeys,
}

fn selected_trackpad_binding(
    bindings: &OrbitCamBindings,
    keyboard: Option<&ButtonInput<KeyCode>>,
) -> Option<TrackpadScrollTarget> {
    let candidates = bindings
        .trackpad_orbit()
        .iter()
        .map(|binding| trackpad_candidate(TrackpadScrollTarget::Orbit, *binding))
        .chain(
            bindings
                .trackpad_pan()
                .iter()
                .map(|binding| trackpad_candidate(TrackpadScrollTarget::Pan, *binding)),
        )
        .chain(
            bindings
                .trackpad_zoom()
                .iter()
                .map(|binding| trackpad_candidate(TrackpadScrollTarget::Zoom, *binding)),
        );

    candidates
        .filter(|candidate| trackpad_mod_keys_pressed(keyboard, candidate.mod_keys))
        .max_by_key(|candidate| {
            (
                mod_key_count(candidate.mod_keys),
                trackpad_target_priority(candidate.target),
            )
        })
        .map(|candidate| candidate.target)
}

const fn trackpad_candidate(
    target: TrackpadScrollTarget,
    binding: OrbitCamTrackpadScroll,
) -> TrackpadScrollCandidate {
    TrackpadScrollCandidate {
        target,
        mod_keys: binding.mod_keys,
    }
}

fn trackpad_mod_keys_pressed(keyboard: Option<&ButtonInput<KeyCode>>, mod_keys: ModKeys) -> bool {
    if mod_keys.is_empty() {
        return true;
    }
    keyboard.is_some_and(|keyboard| input::mod_keys_pressed(keyboard, mod_keys))
}

fn mod_key_count(mod_keys: ModKeys) -> usize { mod_keys.iter_names().count() }

const fn trackpad_target_priority(target: TrackpadScrollTarget) -> usize {
    match target {
        TrackpadScrollTarget::Orbit => 0,
        TrackpadScrollTarget::Pan => 1,
        TrackpadScrollTarget::Zoom => 2,
    }
}

fn apply_pinch_contribution(
    bindings: &OrbitCamBindings,
    pinch: f32,
    keyboard: Option<&ButtonInput<KeyCode>>,
    mouse_buttons: Option<&ButtonInput<MouseButton>>,
    contributions: &mut AdapterContributions,
) {
    if !matches!(bindings.pinch_zoom(), PinchGestureZoom::Enabled)
        || pinch == 0.0
        || pinch_suppressed(bindings, keyboard, mouse_buttons)
    {
        return;
    }

    contributions.zoom_smooth += zoom_signed(
        pinch * PINCH_GESTURE_AMPLIFICATION,
        bindings,
        WheelZoomPolarity::Normal,
    );
    contributions.sources.zoom_smooth = contributions
        .sources
        .zoom_smooth
        .union(CameraInteractionSources::PINCH);
}

fn pinch_suppressed(
    bindings: &OrbitCamBindings,
    keyboard: Option<&ButtonInput<KeyCode>>,
    mouse_buttons: Option<&ButtonInput<MouseButton>>,
) -> bool {
    trackpad_modifier_active(bindings, keyboard)
        || bindings.orbit().entries().iter().any(|entry| {
            entry
                .engagement_descriptor()
                .is_active(keyboard, mouse_buttons)
        })
        || bindings.pan().entries().iter().any(|entry| {
            entry
                .engagement_descriptor()
                .is_active(keyboard, mouse_buttons)
        })
        || bindings.zoom_smooth().entries().iter().any(|entry| {
            entry
                .engagement_descriptor()
                .is_active(keyboard, mouse_buttons)
        })
}

fn trackpad_modifier_active(
    bindings: &OrbitCamBindings,
    keyboard: Option<&ButtonInput<KeyCode>>,
) -> bool {
    let Some(keyboard) = keyboard else {
        return false;
    };

    bindings
        .trackpad_orbit()
        .iter()
        .chain(bindings.trackpad_pan())
        .chain(bindings.trackpad_zoom())
        .any(|binding| {
            !binding.mod_keys.is_empty() && input::mod_keys_pressed(keyboard, binding.mod_keys)
        })
}

fn apply_touch_contribution(
    bindings: &OrbitCamBindings,
    touch_gestures: &TouchGestures,
    contributions: &mut AdapterContributions,
) {
    let Some(touch) = bindings.touch() else {
        return;
    };

    let (orbit, pan, zoom) = match (touch, touch_gestures) {
        (OrbitCamTouchBinding::OneFingerOrbit, TouchGestures::OneFinger(gesture)) => {
            (gesture.motion, Vec2::ZERO, 0.0)
        },
        (OrbitCamTouchBinding::OneFingerOrbit, TouchGestures::TwoFinger(gesture)) => (
            Vec2::ZERO,
            gesture.motion,
            gesture.pinch * TOUCH_PINCH_SCALE,
        ),
        (OrbitCamTouchBinding::TwoFingerOrbit, TouchGestures::OneFinger(gesture)) => {
            (Vec2::ZERO, gesture.motion, 0.0)
        },
        (OrbitCamTouchBinding::TwoFingerOrbit, TouchGestures::TwoFinger(gesture)) => (
            gesture.motion,
            Vec2::ZERO,
            gesture.pinch * TOUCH_PINCH_SCALE,
        ),
        (_, TouchGestures::None) => (Vec2::ZERO, Vec2::ZERO, 0.0),
    };

    if orbit != Vec2::ZERO {
        contributions.orbit += orbit;
        contributions.sources.orbit = contributions
            .sources
            .orbit
            .union(CameraInteractionSources::TOUCH);
    }
    if pan != Vec2::ZERO {
        contributions.pan += pan;
        contributions.sources.pan = contributions
            .sources
            .pan
            .union(CameraInteractionSources::TOUCH);
    }
    if zoom != 0.0 {
        contributions.zoom_smooth += zoom_signed(zoom, bindings, WheelZoomPolarity::Normal);
        contributions.sources.zoom_smooth = contributions
            .sources
            .zoom_smooth
            .union(CameraInteractionSources::TOUCH);
    }
}

fn apply_button_drag_zoom_contribution(
    bindings: &OrbitCamBindings,
    mouse_motion: Vec2,
    mouse_buttons: Option<&ButtonInput<MouseButton>>,
    contributions: &mut AdapterContributions,
) {
    let Some(button_drag_zoom) = bindings.button_drag_zoom() else {
        return;
    };
    if mouse_motion == Vec2::ZERO
        || mouse_buttons.is_none_or(|buttons| !buttons.pressed(button_drag_zoom.button))
    {
        return;
    }

    let delta = match button_drag_zoom.axis {
        OrbitCamButtonDragZoomAxis::X => mouse_motion.x,
        OrbitCamButtonDragZoomAxis::Y => -mouse_motion.y,
        OrbitCamButtonDragZoomAxis::XY => mouse_motion.x - mouse_motion.y,
    };

    contributions.zoom_smooth += zoom_signed(
        delta * BUTTON_ZOOM_SCALE,
        bindings,
        WheelZoomPolarity::Normal,
    );
    contributions.sources.zoom_smooth = contributions
        .sources
        .zoom_smooth
        .union(CameraInteractionSources::MOUSE);
}

fn zoom_signed(value: f32, bindings: &OrbitCamBindings, polarity: WheelZoomPolarity) -> f32 {
    let wheel_sign = match polarity {
        WheelZoomPolarity::Normal => 1.0,
        WheelZoomPolarity::Inverted => -1.0,
    };
    let zoom_sign = match bindings.zoom_direction() {
        ZoomDirection::Normal => 1.0,
        ZoomDirection::Reversed => -1.0,
    };
    value * wheel_sign * zoom_sign
}

fn mock_adapter_actions(
    actions: &OrbitCamInputActionEntities,
    contributions: AdapterContributions,
    mocks: &mut Query<&mut ActionMock>,
) {
    set_mock(
        mocks,
        actions.adapter_orbit,
        state_for_vec2(contributions.orbit),
        contributions.orbit,
    );
    set_mock(
        mocks,
        actions.adapter_pan,
        state_for_vec2(contributions.pan),
        contributions.pan,
    );
    set_mock(
        mocks,
        actions.adapter_zoom_coarse,
        state_for_f32(contributions.zoom_coarse),
        contributions.zoom_coarse,
    );
    set_mock(
        mocks,
        actions.adapter_zoom_smooth,
        state_for_f32(contributions.zoom_smooth),
        contributions.zoom_smooth,
    );
}

fn set_mock(
    mocks: &mut Query<&mut ActionMock>,
    action: Entity,
    state: TriggerState,
    value: impl Into<ActionValue>,
) {
    if let Ok(mut mock) = mocks.get_mut(action) {
        *mock = ActionMock::new(state, value, MockSpan::once());
    }
}

const fn state_for_vec2(value: Vec2) -> TriggerState {
    if value.x == 0.0 && value.y == 0.0 {
        TriggerState::None
    } else {
        TriggerState::Fired
    }
}

const fn state_for_f32(value: f32) -> TriggerState {
    if value == 0.0 {
        TriggerState::None
    } else {
        TriggerState::Fired
    }
}

#[cfg(test)]
#[derive(Resource, Clone, Debug)]
pub(super) struct OrbitCamTouchAdapterOverride(pub(super) TouchGestures);
