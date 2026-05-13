use std::f32::consts::PI;
use std::f32::consts::TAU;

use bevy::prelude::*;

use super::CameraOrientation;
use super::InitializationState;
use super::OrbitCam;
use super::OrbitCamUpdateRequest;
use super::OrbitDragState;
use super::TimeSource;
use super::UpsideDownPolicy;
use crate::constants::SCROLL_ZOOM_FACTOR;
use crate::input::CameraInputSurfaceMetrics;
use crate::input::OrbitCamInput;
use crate::input::ResolvedOrbitCamInputRoute;
use crate::orbital_math;

/// Aggregated camera input for a single frame.
struct CameraInput {
    orbit:        Vec2,
    pan:          Vec2,
    scroll_line:  f32,
    scroll_pixel: f32,
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

/// Converts finalized semantic input into controller movement values.
fn collect_camera_input(orbit_cam: &OrbitCam, input: &OrbitCamInput) -> CameraInput {
    let mut camera_input = CameraInput {
        orbit:        Vec2::ZERO,
        pan:          Vec2::ZERO,
        scroll_line:  0.0,
        scroll_pixel: 0.0,
    };

    if input.has_orbit() {
        camera_input.orbit = input.orbit().pixels() * orbit_cam.orbit_sensitivity;
    }
    if input.has_pan() {
        camera_input.pan = input.pan().pixels() * orbit_cam.pan_sensitivity;
    }
    if input.has_zoom() {
        camera_input.scroll_line = input.zoom_coarse().amount() * orbit_cam.zoom_sensitivity;
        camera_input.scroll_pixel = input.zoom_smooth().amount() * orbit_cam.zoom_sensitivity;
    }

    camera_input
}

fn merged_surface_metrics(
    routed: Option<CameraInputSurfaceMetrics>,
    explicit: Option<CameraInputSurfaceMetrics>,
) -> CameraInputSurfaceMetrics {
    let mut metrics = routed.unwrap_or_default();
    if let Some(explicit) = explicit {
        if explicit.camera_view_size.is_some() {
            metrics.camera_view_size = explicit.camera_view_size;
        }
        if explicit.input_surface_size.is_some() {
            metrics.input_surface_size = explicit.input_surface_size;
        }
    }
    metrics
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
}

/// Main system for processing input and converting to transformations
pub fn orbit_cam(
    route: Res<ResolvedOrbitCamInputRoute>,
    mut orbit_cameras: Query<(
        Entity,
        &mut OrbitCam,
        &mut OrbitDragState,
        &OrbitCamInput,
        Option<&CameraInputSurfaceMetrics>,
        &mut Transform,
        &mut Projection,
    )>,
    time_real: Res<Time<Real>>,
    time_virt: Res<Time<Virtual>>,
) {
    for (
        entity,
        mut orbit_cam,
        mut drag_state,
        input,
        explicit_metrics,
        mut transform,
        mut projection,
    ) in &mut orbit_cameras
    {
        if orbit_cam.initialization == InitializationState::Pending {
            initialize_orbit_cam(&mut orbit_cam, &mut transform, &mut projection);
        }

        let input = collect_camera_input(&orbit_cam, input);
        let metrics = merged_surface_metrics(route.metrics_for(entity), explicit_metrics.copied());

        // Only check for upside down when orbiting started or ended this frame,
        // so we don't reverse the yaw direction while the user is still dragging
        let orbit_active = input.orbit != Vec2::ZERO;
        if orbit_active != drag_state.orbit_active {
            let world_up = orbit_cam.axis[1];
            drag_state.orientation = if transform.up().dot(world_up) < 0.0 {
                CameraOrientation::UpsideDown
            } else {
                CameraOrientation::Normal
            };
            drag_state.orbit_active = orbit_active;
        }

        let mut has_moved = apply_orbit_input(
            input.orbit,
            &mut orbit_cam,
            *drag_state,
            metrics.input_surface_size,
        );
        has_moved |= apply_pan_input(
            input.pan,
            &mut orbit_cam,
            metrics.camera_view_size,
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

        let update_request = orbit_cam.consume_update_request();
        #[allow(
            clippy::float_cmp,
            reason = "lerp_and_snap produces bitwise-identical values on convergence"
        )]
        let needs_update = has_moved
            || update_request == OrbitCamUpdateRequest::ForceUpdate
            || orbit_cam.target_yaw != yaw
            || orbit_cam.target_pitch != pitch
            || orbit_cam.target_radius != radius
            || orbit_cam.target_focus != orbit_cam.focus;

        if needs_update {
            smooth_and_update_transform(&mut orbit_cam, &mut transform, &mut projection, delta);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::CameraInteractionSources;

    #[test]
    fn collect_camera_input_scales_finalized_intent() {
        let orbit_cam = OrbitCam {
            orbit_sensitivity: 2.0,
            pan_sensitivity: 3.0,
            zoom_sensitivity: 4.0,
            ..default()
        };
        let mut input = OrbitCamInput::default();
        input
            .orbit_pixels_with_sources(Vec2::new(1.0, 2.0), CameraInteractionSources::MOUSE)
            .pan_pixels_with_sources(Vec2::new(3.0, 4.0), CameraInteractionSources::MOUSE)
            .zoom_coarse_with_sources(5.0, CameraInteractionSources::WHEEL)
            .zoom_smooth_with_sources(6.0, CameraInteractionSources::SMOOTH_SCROLL);

        let input = collect_camera_input(&orbit_cam, &input);

        assert_eq!(input.orbit, Vec2::new(2.0, 4.0));
        assert_eq!(input.pan, Vec2::new(9.0, 12.0));
        assert!((input.scroll_line - 20.0).abs() <= f32::EPSILON);
        assert!((input.scroll_pixel - 24.0).abs() <= f32::EPSILON);
    }

    #[test]
    fn explicit_surface_metrics_override_routed_metrics() {
        let routed = CameraInputSurfaceMetrics::camera_view_and_input_surface(
            Vec2::new(100.0, 200.0),
            Vec2::new(300.0, 400.0),
        );
        let explicit = CameraInputSurfaceMetrics::camera_view(Vec2::new(500.0, 600.0));

        let metrics = merged_surface_metrics(Some(routed), Some(explicit));

        assert_eq!(metrics.camera_view_size, Some(Vec2::new(500.0, 600.0)));
        assert_eq!(metrics.input_surface_size, Some(Vec2::new(300.0, 400.0)));
    }
}
