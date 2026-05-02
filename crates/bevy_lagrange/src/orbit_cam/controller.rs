use std::f32::consts::PI;
use std::f32::consts::TAU;

use bevy::prelude::*;

use super::ActiveCameraData;
use super::CameraOrientation;
use super::ForceUpdate;
use super::InitializationState;
use super::OrbitCam;
use super::OrbitDragState;
use super::TimeSource;
use super::UpsideDownPolicy;
use crate::constants::SCROLL_ZOOM_FACTOR;
use crate::constants::TOUCH_PINCH_SCALE;
use crate::input::MouseKeyTracker;
use crate::input::OrbitButtonChange;
use crate::input::ZoomDirection;
use crate::orbital_math;
use crate::touch::TouchGestures;
use crate::touch::TouchInput;
use crate::touch::TouchTracker;

/// Aggregated camera input for a single frame.
struct CameraInput {
    orbit:               Vec2,
    pan:                 Vec2,
    scroll_line:         f32,
    scroll_pixel:        f32,
    orbit_button_change: OrbitButtonChange,
}

/// Initializes `OrbitCam` from the camera's current transform, applying all limits.
fn initialize_orbit_cam(
    orbit_cam: &mut OrbitCam,
    transform: &mut Transform,
    projection: &mut Projection,
) {
    let (yaw, pitch, radius) = orbital_math::calculate_from_translation_and_focus(
        transform.translation,
        orbit_cam.focus,
        orbit_cam.axis,
    );
    let &mut mut yaw = orbit_cam.yaw.get_or_insert(yaw);
    let &mut mut pitch = orbit_cam.pitch.get_or_insert(pitch);
    let &mut mut radius = orbit_cam.radius.get_or_insert(radius);
    let mut focus = orbit_cam.focus;

    yaw = orbit_cam.clamp_yaw(yaw);
    pitch = orbit_cam.clamp_pitch(pitch);
    radius = orbit_cam.clamp_zoom(radius);
    focus = orbit_cam.clamp_focus(focus);

    orbit_cam.yaw = Some(yaw);
    orbit_cam.pitch = Some(pitch);
    orbit_cam.radius = Some(radius);
    orbit_cam.target_yaw = yaw;
    orbit_cam.target_pitch = pitch;
    orbit_cam.target_radius = radius;
    orbit_cam.target_focus = focus;

    orbital_math::update_orbit_transform(
        yaw,
        pitch,
        radius,
        focus,
        transform,
        projection,
        orbit_cam.axis,
    );

    orbit_cam.initialization = InitializationState::Complete;
}

/// Collects mouse, keyboard, and touch input into a single `CameraInput`.
fn collect_camera_input(
    entity: Entity,
    orbit_cam: &OrbitCam,
    active_cam: &ActiveCameraData,
    mouse_key_tracker: &MouseKeyTracker,
    touch_tracker: &TouchTracker,
) -> CameraInput {
    let mut orbit = Vec2::ZERO;
    let mut pan = Vec2::ZERO;
    let mut scroll_line = 0.0;
    let mut scroll_pixel = 0.0;
    let mut orbit_button_change = OrbitButtonChange::Unchanged;

    // Only skip getting input if the camera is inactive/disabled — it might still
    // be lerping towards target values when the user is not actively controlling it.
    if let Some(input_control) = orbit_cam.input_control
        && active_cam.entity == Some(entity)
    {
        let zoom_sign = match input_control.zoom {
            ZoomDirection::Normal => 1.0,
            ZoomDirection::Reversed => -1.0,
        };

        orbit = mouse_key_tracker.orbit * orbit_cam.orbit_sensitivity;
        pan = mouse_key_tracker.pan * orbit_cam.pan_sensitivity;
        scroll_line = mouse_key_tracker.scroll_line * zoom_sign * orbit_cam.zoom_sensitivity;
        scroll_pixel = mouse_key_tracker.scroll_pixel * zoom_sign * orbit_cam.zoom_sensitivity;
        orbit_button_change = mouse_key_tracker.orbit_button_change;

        if let Some(touch_input) = input_control.touch {
            let (touch_orbit, touch_pan, touch_zoom_pixel) = match touch_input {
                TouchInput::OneFingerOrbit => match touch_tracker.get_touch_gestures() {
                    TouchGestures::None => (Vec2::ZERO, Vec2::ZERO, 0.0),
                    TouchGestures::OneFinger(one_finger_gestures) => {
                        (one_finger_gestures.motion, Vec2::ZERO, 0.0)
                    },
                    TouchGestures::TwoFinger(two_finger_gestures) => (
                        Vec2::ZERO,
                        two_finger_gestures.motion,
                        two_finger_gestures.pinch * TOUCH_PINCH_SCALE,
                    ),
                },
                TouchInput::TwoFingerOrbit => match touch_tracker.get_touch_gestures() {
                    TouchGestures::None => (Vec2::ZERO, Vec2::ZERO, 0.0),
                    TouchGestures::OneFinger(one_finger_gestures) => {
                        (Vec2::ZERO, one_finger_gestures.motion, 0.0)
                    },
                    TouchGestures::TwoFinger(two_finger_gestures) => (
                        two_finger_gestures.motion,
                        Vec2::ZERO,
                        two_finger_gestures.pinch * TOUCH_PINCH_SCALE,
                    ),
                },
            };

            orbit += touch_orbit * orbit_cam.orbit_sensitivity;
            pan += touch_pan * orbit_cam.pan_sensitivity;
            scroll_pixel += touch_zoom_pixel * zoom_sign * orbit_cam.zoom_sensitivity;
        }
    }

    CameraInput {
        orbit,
        pan,
        scroll_line,
        scroll_pixel,
        orbit_button_change,
    }
}

/// Applies orbit input to target yaw/pitch. Returns `true` if the camera moved.
fn apply_orbit_input(
    orbit: Vec2,
    orbit_cam: &mut OrbitCam,
    drag_state: OrbitDragState,
    window_size: Option<Vec2>,
) -> bool {
    if orbit.length_squared() > 0.0 {
        // Use window size for rotation otherwise the sensitivity is far too high for small
        // viewports
        if let Some(window_size) = window_size {
            let delta_x = {
                let delta = orbit.x / window_size.x * TAU;
                match drag_state.orientation {
                    CameraOrientation::UpsideDown => -delta,
                    CameraOrientation::Normal => delta,
                }
            };
            let delta_y = orbit.y / window_size.y * PI;
            orbit_cam.target_yaw -= delta_x;
            orbit_cam.target_pitch += delta_y;
            return true;
        }
    }
    false
}

/// Applies pan input to target focus. Returns `true` if the camera moved.
fn apply_pan_input(
    mut pan: Vec2,
    orbit_cam: &mut OrbitCam,
    viewport_size: Option<Vec2>,
    transform: &Transform,
    projection: &Projection,
) -> bool {
    if pan.length_squared() > 0.0 {
        // Make panning distance independent of resolution and FOV
        if let Some(viewport_size) = viewport_size {
            let mut multiplier = 1.0;
            match *projection {
                Projection::Perspective(ref perspective_projection) => {
                    pan *= Vec2::new(
                        perspective_projection.fov * perspective_projection.aspect_ratio,
                        perspective_projection.fov,
                    ) / viewport_size;
                    // Make panning proportional to distance away from focus point
                    if let Some(radius) = orbit_cam.radius {
                        multiplier = radius;
                    }
                },
                Projection::Orthographic(ref orthographic_projection) => {
                    pan *= Vec2::new(
                        orthographic_projection.area.width(),
                        orthographic_projection.area.height(),
                    ) / viewport_size;
                },
                Projection::Custom(_) => todo!(),
            }
            // Translate by local axes
            let right = transform.rotation * orbit_cam.axis[0] * -pan.x;
            let up = transform.rotation * orbit_cam.axis[1] * pan.y;
            let translation = (right + up) * multiplier;
            orbit_cam.target_focus += translation;
            return true;
        }
    }
    false
}

/// Applies scroll/zoom input to target radius. Returns `true` if the camera moved.
fn apply_scroll_input(scroll_line: f32, scroll_pixel: f32, orbit_cam: &mut OrbitCam) -> bool {
    if (scroll_line + scroll_pixel).abs() > 0.0 {
        let line_delta = -scroll_line * orbit_cam.target_radius * SCROLL_ZOOM_FACTOR;
        let pixel_delta = -scroll_pixel * orbit_cam.target_radius * SCROLL_ZOOM_FACTOR;

        orbit_cam.target_radius += line_delta + pixel_delta;

        // Pixel-based scrolling is added directly to the current value (already smooth)
        orbit_cam.radius = orbit_cam
            .radius
            .map(|value| orbit_cam.clamp_zoom(value + pixel_delta));

        return true;
    }
    false
}

/// Interpolates current values toward targets and updates the camera transform.
fn smooth_and_update_transform(
    orbit_cam: &mut OrbitCam,
    transform: &mut Transform,
    projection: &mut Projection,
    delta: f32,
) {
    let (Some(yaw), Some(pitch), Some(radius)) = (orbit_cam.yaw, orbit_cam.pitch, orbit_cam.radius)
    else {
        return;
    };

    let new_yaw = orbital_math::lerp_and_snap_f32(
        yaw,
        orbit_cam.target_yaw,
        orbit_cam.orbit_smoothness,
        delta,
    );
    let new_pitch = orbital_math::lerp_and_snap_f32(
        pitch,
        orbit_cam.target_pitch,
        orbit_cam.orbit_smoothness,
        delta,
    );
    let new_radius = orbital_math::lerp_and_snap_f32(
        radius,
        orbit_cam.target_radius,
        orbit_cam.zoom_smoothness,
        delta,
    );
    let new_focus = orbital_math::lerp_and_snap_position(
        orbit_cam.focus,
        orbit_cam.target_focus,
        orbit_cam.pan_smoothness,
        delta,
    );

    orbital_math::update_orbit_transform(
        new_yaw,
        new_pitch,
        new_radius,
        new_focus,
        transform,
        projection,
        orbit_cam.axis,
    );

    orbit_cam.yaw = Some(new_yaw);
    orbit_cam.pitch = Some(new_pitch);
    orbit_cam.radius = Some(new_radius);
    orbit_cam.focus = *new_focus;
    orbit_cam.force_update = ForceUpdate::Idle;
}

/// Main system for processing input and converting to transformations
pub fn orbit_cam(
    active_cam: Res<ActiveCameraData>,
    mouse_key_tracker: Res<MouseKeyTracker>,
    touch_tracker: Res<TouchTracker>,
    mut orbit_cameras: Query<(
        Entity,
        &mut OrbitCam,
        &mut OrbitDragState,
        &mut Transform,
        &mut Projection,
    )>,
    time_real: Res<Time<Real>>,
    time_virt: Res<Time<Virtual>>,
) {
    for (entity, mut orbit_cam, mut drag_state, mut transform, mut projection) in &mut orbit_cameras
    {
        if orbit_cam.initialization == InitializationState::Pending {
            initialize_orbit_cam(&mut orbit_cam, &mut transform, &mut projection);
        }

        let input = collect_camera_input(
            entity,
            &orbit_cam,
            &active_cam,
            &mouse_key_tracker,
            &touch_tracker,
        );

        // Only check for upside down when orbiting started or ended this frame,
        // so we don't reverse the yaw direction while the user is still dragging
        if input.orbit_button_change == OrbitButtonChange::Changed {
            let world_up = orbit_cam.axis[1];
            drag_state.orientation = if transform.up().dot(world_up) < 0.0 {
                CameraOrientation::UpsideDown
            } else {
                CameraOrientation::Normal
            };
        }

        let mut has_moved = apply_orbit_input(
            input.orbit,
            &mut orbit_cam,
            *drag_state,
            active_cam.window_size,
        );
        has_moved |= apply_pan_input(
            input.pan,
            &mut orbit_cam,
            active_cam.viewport_size,
            &transform,
            &projection,
        );
        has_moved |= apply_scroll_input(input.scroll_line, input.scroll_pixel, &mut orbit_cam);

        // Apply constraints
        orbit_cam.target_yaw = orbit_cam.clamp_yaw(orbit_cam.target_yaw);
        orbit_cam.target_pitch = orbit_cam.clamp_pitch(orbit_cam.target_pitch);
        orbit_cam.target_radius = orbit_cam.clamp_zoom(orbit_cam.target_radius);
        orbit_cam.target_focus = orbit_cam.clamp_focus(orbit_cam.target_focus);
        if orbit_cam.upside_down_policy == UpsideDownPolicy::Prevent {
            orbit_cam.target_pitch = orbit_cam.target_pitch.clamp(-PI / 2.0, PI / 2.0);
        }

        let delta = match orbit_cam.time_source {
            TimeSource::Real => time_real.delta_secs(),
            TimeSource::Virtual => time_virt.delta_secs(),
        };

        // Only pass `&mut transform` when something actually changed.
        // Passing it unconditionally triggers Bevy's `DerefMut` change detection,
        // marking `Transform` (and therefore `GlobalTransform`) as changed every
        // frame — even when the camera is idle.
        let (Some(yaw), Some(pitch), Some(radius)) =
            (orbit_cam.yaw, orbit_cam.pitch, orbit_cam.radius)
        else {
            continue;
        };

        #[allow(
            clippy::float_cmp,
            reason = "lerp_and_snap produces bitwise-identical values on convergence"
        )]
        let needs_update = has_moved
            || orbit_cam.force_update != ForceUpdate::Idle
            || orbit_cam.target_yaw != yaw
            || orbit_cam.target_pitch != pitch
            || orbit_cam.target_radius != radius
            || orbit_cam.target_focus != orbit_cam.focus;

        if needs_update {
            smooth_and_update_transform(&mut orbit_cam, &mut transform, &mut projection, delta);
        }
    }
}
