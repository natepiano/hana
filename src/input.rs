use bevy::input::gestures::PinchGesture;
use bevy::input::mouse::MouseMotion;
use bevy::input::mouse::MouseScrollUnit;
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;

use super::ActiveCameraData;
use super::ButtonZoomAxis;
use super::OrbitCam;
use super::TrackpadBehavior;
use super::ZoomDirection;
use super::constants::BUTTON_ZOOM_SCALE;
use super::constants::PINCH_GESTURE_AMPLIFICATION;
use super::constants::PIXEL_SCROLL_SCALE;

#[derive(Resource, Default, Debug)]
pub(crate) struct MouseKeyTracker {
    pub orbit:                Vec2,
    pub pan:                  Vec2,
    pub scroll_line:          f32,
    pub scroll_pixel:         f32,
    pub orbit_button_changed: bool,
}

pub(crate) fn mouse_key_tracker(
    mut camera_movement: ResMut<MouseKeyTracker>,
    mouse_input: Res<ButtonInput<MouseButton>>,
    key_input: Res<ButtonInput<KeyCode>>,
    mut mouse_motion: MessageReader<MouseMotion>,
    mut pinch_events: MessageReader<PinchGesture>,
    mut scroll_events: MessageReader<MouseWheel>,
    active_cam: Res<ActiveCameraData>,
    orbit_cameras: Query<&OrbitCam>,
) {
    let Some(active_entity) = active_cam.entity else {
        return;
    };

    let Ok(pan_orbit) = orbit_cameras.get(active_entity) else {
        return;
    };

    // Collect input deltas
    let mouse_delta = mouse_motion.read().map(|event| event.delta).sum::<Vec2>();

    // Collect scroll events
    let scroll_events_vec: Vec<MouseWheel> = scroll_events.read().copied().collect();

    // Scroll processing needs to account for mouse and trackpad. In `BlenderLike` mode, pixel
    // scrolling may produce orbit or pan instead of zoom.
    let scroll_processing_result = process_scroll_events(&scroll_events_vec, pan_orbit, &key_input);
    // Initialize orbit and pan with trackpad contributions
    let mut orbit = scroll_processing_result.trackpad_orbit;
    let mut pan = scroll_processing_result.trackpad_pan;

    // Handle pinch gestures separately
    // Process pinch events
    let pinch_zoom = process_pinch_events(&mut pinch_events, pan_orbit, &key_input);

    // If zoom button set, apply zoom based on mouse movement
    let mouse_zoom = if button_zoom_pressed(pan_orbit, &mouse_input) {
        let mut delta = match pan_orbit.button_zoom_axis {
            ButtonZoomAxis::X => mouse_delta.x,
            ButtonZoomAxis::Y => -mouse_delta.y,
            ButtonZoomAxis::XY => mouse_delta.x + -mouse_delta.y,
        };
        if let Some(input_control) = pan_orbit.input_control
            && input_control.zoom == ZoomDirection::Reversed
        {
            delta *= -1.0;
        }
        delta * BUTTON_ZOOM_SCALE
    } else {
        0.0
    };

    // Handle mouse movement for orbiting and panning
    if orbit_pressed(pan_orbit, &mouse_input, &key_input) {
        orbit += mouse_delta;
    } else if pan_pressed(pan_orbit, &mouse_input, &key_input) {
        pan += mouse_delta;
    }

    // Track button state changes
    let orbit_button_changed = orbit_just_pressed(pan_orbit, &mouse_input, &key_input)
        || orbit_just_released(pan_orbit, &mouse_input, &key_input);

    // Update the movement resource
    camera_movement.orbit = orbit;
    camera_movement.pan = pan;
    camera_movement.scroll_line = scroll_processing_result.scroll_line;
    camera_movement.scroll_pixel = scroll_processing_result.scroll_pixel + pinch_zoom + mouse_zoom;
    camera_movement.orbit_button_changed = orbit_button_changed;
}

#[derive(Default)]
struct ScrollProcessingResult {
    trackpad_orbit: Vec2,
    trackpad_pan:   Vec2,
    scroll_line:    f32,
    scroll_pixel:   f32,
}

/// Mimic how Blender ignores pinch gestures while trackpad modifiers are pressed.
fn process_scroll_events(
    scroll_events: &[MouseWheel],
    pan_orbit: &OrbitCam,
    key_input: &Res<ButtonInput<KeyCode>>,
) -> ScrollProcessingResult {
    let Some(input_control) = pan_orbit.input_control else {
        return ScrollProcessingResult::default();
    };
    let Some(trackpad_input) = input_control.trackpad else {
        return ScrollProcessingResult::default();
    };

    match trackpad_input.behavior {
        TrackpadBehavior::BlenderLike {
            modifier_pan,
            modifier_zoom,
        } => {
            let is_zoom_modifier_pressed =
                modifier_zoom.is_none_or(|modifier| key_input.pressed(modifier));
            let is_pan_modifier_pressed =
                modifier_pan.is_none_or(|modifier| key_input.pressed(modifier));

            let mut scroll_processing_result = ScrollProcessingResult::default();

            for event in scroll_events {
                match event.unit {
                    MouseScrollUnit::Line => {
                        scroll_processing_result.scroll_line += event.y;
                    },
                    MouseScrollUnit::Pixel => {
                        if is_zoom_modifier_pressed {
                            scroll_processing_result.scroll_pixel += event.y * PIXEL_SCROLL_SCALE;
                        } else if is_pan_modifier_pressed {
                            scroll_processing_result.trackpad_pan +=
                                Vec2::new(event.x, event.y) * trackpad_input.sensitivity;
                        } else {
                            scroll_processing_result.trackpad_orbit +=
                                Vec2::new(event.x, event.y) * trackpad_input.sensitivity;
                        }
                    },
                }
            }

            scroll_processing_result
        },
        TrackpadBehavior::ZoomOnly => {
            // Zoom-only behavior: all scroll events contribute to zoom.
            let (scroll_line, scroll_pixel) = scroll_events
                .iter()
                .map(|event| match event.unit {
                    MouseScrollUnit::Line => (event.y, 0.0),
                    MouseScrollUnit::Pixel => (0.0, event.y * PIXEL_SCROLL_SCALE),
                })
                .fold((0.0, 0.0), |acc, item| (acc.0 + item.0, acc.1 + item.1));

            ScrollProcessingResult {
                trackpad_orbit: Vec2::ZERO,
                trackpad_pan: Vec2::ZERO,
                scroll_line,
                scroll_pixel,
            }
        },
    }
}

fn process_pinch_events(
    pinch_events: &mut MessageReader<PinchGesture>,
    pan_orbit: &OrbitCam,
    key_input: &Res<ButtonInput<KeyCode>>,
) -> f32 {
    let Some(input_control) = pan_orbit.input_control else {
        return 0.0;
    };
    let Some(trackpad_input) = input_control.trackpad else {
        return 0.0;
    };

    // Check if no modifiers are pressed (including BlenderLike modifiers if applicable)
    let no_modifiers_pressed = match trackpad_input.behavior {
        TrackpadBehavior::BlenderLike {
            modifier_pan,
            modifier_zoom,
        } => {
            // Check regular modifiers and BlenderLike modifiers
            pan_orbit
                .modifier_orbit
                .is_none_or(|modifier| !key_input.pressed(modifier))
                && pan_orbit
                    .modifier_pan
                    .is_none_or(|modifier| !key_input.pressed(modifier))
                && modifier_pan.is_none_or(|modifier| !key_input.pressed(modifier))
                && modifier_zoom.is_none_or(|modifier| !key_input.pressed(modifier))
        },
        TrackpadBehavior::ZoomOnly => {
            // Just check regular modifiers
            pan_orbit
                .modifier_orbit
                .is_none_or(|modifier| !key_input.pressed(modifier))
                && pan_orbit
                    .modifier_pan
                    .is_none_or(|modifier| !key_input.pressed(modifier))
        },
    };

    if no_modifiers_pressed {
        pinch_events
            .read()
            .map(|event| event.0 * PINCH_GESTURE_AMPLIFICATION * trackpad_input.sensitivity)
            .sum()
    } else {
        0.0
    }
}

pub(crate) fn orbit_pressed(
    pan_orbit: &OrbitCam,
    mouse_input: &Res<ButtonInput<MouseButton>>,
    key_input: &Res<ButtonInput<KeyCode>>,
) -> bool {
    if pan_orbit.input_control.is_none() {
        return false;
    }

    let is_pressed = pan_orbit
        .modifier_orbit
        .is_none_or(|modifier| key_input.pressed(modifier))
        && mouse_input.pressed(pan_orbit.button_orbit);

    is_pressed
        && pan_orbit
            .modifier_pan
            .is_none_or(|modifier| !key_input.pressed(modifier))
}

pub(crate) fn orbit_just_pressed(
    pan_orbit: &OrbitCam,
    mouse_input: &Res<ButtonInput<MouseButton>>,
    key_input: &Res<ButtonInput<KeyCode>>,
) -> bool {
    if pan_orbit.input_control.is_none() {
        return false;
    }

    let just_pressed = pan_orbit
        .modifier_orbit
        .is_none_or(|modifier| key_input.pressed(modifier))
        && (mouse_input.just_pressed(pan_orbit.button_orbit));

    just_pressed
        && pan_orbit
            .modifier_pan
            .is_none_or(|modifier| !key_input.pressed(modifier))
}

pub(crate) fn orbit_just_released(
    pan_orbit: &OrbitCam,
    mouse_input: &Res<ButtonInput<MouseButton>>,
    key_input: &Res<ButtonInput<KeyCode>>,
) -> bool {
    if pan_orbit.input_control.is_none() {
        return false;
    }

    let just_released = pan_orbit
        .modifier_orbit
        .is_none_or(|modifier| key_input.pressed(modifier))
        && (mouse_input.just_released(pan_orbit.button_orbit));

    just_released
        && pan_orbit
            .modifier_pan
            .is_none_or(|modifier| !key_input.pressed(modifier))
}

pub(crate) fn pan_pressed(
    pan_orbit: &OrbitCam,
    mouse_input: &Res<ButtonInput<MouseButton>>,
    key_input: &Res<ButtonInput<KeyCode>>,
) -> bool {
    if pan_orbit.input_control.is_none() {
        return false;
    }

    let is_pressed = pan_orbit
        .modifier_pan
        .is_none_or(|modifier| key_input.pressed(modifier))
        && mouse_input.pressed(pan_orbit.button_pan);

    is_pressed
        && pan_orbit
            .modifier_orbit
            .is_none_or(|modifier| !key_input.pressed(modifier))
}

pub(crate) fn pan_just_pressed(
    pan_orbit: &OrbitCam,
    mouse_input: &Res<ButtonInput<MouseButton>>,
    key_input: &Res<ButtonInput<KeyCode>>,
) -> bool {
    if pan_orbit.input_control.is_none() {
        return false;
    }

    let just_pressed = pan_orbit
        .modifier_pan
        .is_none_or(|modifier| key_input.pressed(modifier))
        && (mouse_input.just_pressed(pan_orbit.button_pan));

    just_pressed
        && pan_orbit
            .modifier_orbit
            .is_none_or(|modifier| !key_input.pressed(modifier))
}

pub(crate) fn button_zoom_pressed(
    pan_orbit: &OrbitCam,
    mouse_input: &Res<ButtonInput<MouseButton>>,
) -> bool {
    if pan_orbit.input_control.is_none() {
        return false;
    }

    pan_orbit
        .button_zoom
        .is_some_and(|btn| mouse_input.pressed(btn))
}

pub(crate) fn button_zoom_just_pressed(
    pan_orbit: &OrbitCam,
    mouse_input: &Res<ButtonInput<MouseButton>>,
) -> bool {
    if pan_orbit.input_control.is_none() {
        return false;
    }

    pan_orbit
        .button_zoom
        .is_some_and(|btn| mouse_input.just_pressed(btn))
}
