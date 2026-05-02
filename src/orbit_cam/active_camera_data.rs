use bevy::camera::RenderTarget;
use bevy::input::gestures::PinchGesture;
use bevy::input::mouse::MouseWheel;
use bevy::input::touch::Touch;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::window::WindowRef;

use super::OrbitCam;
#[cfg(feature = "bevy_egui")]
use crate::egui::BlockOnEguiFocus;
#[cfg(feature = "bevy_egui")]
use crate::egui::EguiWantsFocus;
use crate::input;

/// Base system set to allow ordering of `OrbitCam`
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct OrbitCamSystemSet;

/// Tracks which `OrbitCam` is active (should handle input events).
///
/// Also stores the window and viewport dimensions, which are used for scaling mouse motion.
/// `LagrangePlugin` manages this resource automatically, in order to support multiple
/// viewports/windows. However, if this doesn't work for you, you can take over and manage it
/// yourself, e.g. when you want to control a camera that is rendering to a texture.
#[derive(Resource, Default, Debug, PartialEq)]
pub struct ActiveCameraData {
    /// ID of the entity with `OrbitCam` that will handle user input. In other words, this
    /// is the camera that will move when you orbit/pan/zoom.
    pub entity:        Option<Entity>,
    /// The viewport size. This is only used to scale the panning mouse motion. I recommend setting
    /// this to the actual render target dimensions (e.g. the image or viewport), and changing
    /// `OrbitCam::pan_sensitivity` to adjust the sensitivity if required.
    pub viewport_size: Option<Vec2>,
    /// The size of the window. This is only used to scale the orbit mouse motion. I recommend
    /// setting this to actual dimensions of the window that you want to control the camera from,
    /// and changing `OrbitCam::orbit_sensitivity` to adjust the sensitivity if required.
    pub window_size:   Option<Vec2>,
    /// Controls whether `LagrangePlugin` auto-detects the active camera from cursor position.
    /// Set to `CameraInputDetection::Manual` when you populate this resource yourself
    /// (e.g. render-to-texture scenarios where there is no on-screen viewport to hit-test).
    /// Note that `Manual` disables multi-viewport/window support unless you reimplement it.
    pub detection:     CameraInputDetection,
}

/// Whether the plugin auto-detects which camera should receive input.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub enum CameraInputDetection {
    /// The plugin hit-tests cursor position against viewport rects each frame.
    #[default]
    Automatic,
    /// The user populates `ActiveCameraData` directly; the detection system is skipped.
    Manual,
}

/// Gather data about the active viewport, i.e. the viewport the user is interacting with.
/// Enables multiple viewports/windows.
pub fn active_viewport_data(
    mut active_cam: ResMut<ActiveCameraData>,
    mouse_input: Res<ButtonInput<MouseButton>>,
    key_input: Res<ButtonInput<KeyCode>>,
    pinch_events: MessageReader<PinchGesture>,
    scroll_events: MessageReader<MouseWheel>,
    touches: Res<Touches>,
    primary_windows: Query<&Window, With<PrimaryWindow>>,
    other_windows: Query<&Window, Without<PrimaryWindow>>,
    orbit_cameras: Query<(Entity, &Camera, &RenderTarget, &OrbitCam)>,
    #[cfg(feature = "bevy_egui")] egui_wants_focus: Res<EguiWantsFocus>,
    #[cfg(feature = "bevy_egui")] block_on_egui_query: Query<&BlockOnEguiFocus>,
) {
    let mut new_active_camera_data = ActiveCameraData::default();
    let mut max_camera_order = 0;

    let mut has_input = false;
    for (entity, camera, target, orbit_cam) in &orbit_cameras {
        let input_just_activated = input::orbit_just_pressed(orbit_cam, &mouse_input, &key_input)
            || input::pan_just_pressed(orbit_cam, &mouse_input, &key_input)
            || !pinch_events.is_empty()
            || !scroll_events.is_empty()
            || input::button_zoom_just_pressed(orbit_cam, &mouse_input)
            || (touches.iter_just_pressed().count() > 0
                && touches.iter_just_pressed().count() == touches.iter().count());

        if input_just_activated && orbit_cam.input_control.is_some() {
            has_input = true;
            let should_get_input = {
                #[cfg(feature = "bevy_egui")]
                {
                    if block_on_egui_query.contains(entity) {
                        !egui_wants_focus.prev && !egui_wants_focus.curr
                    } else {
                        true
                    }
                }
                #[cfg(not(feature = "bevy_egui"))]
                {
                    true
                }
            };
            if should_get_input && let RenderTarget::Window(win_ref) = target {
                let Some(window) = (match win_ref {
                    WindowRef::Primary => primary_windows.single().ok(),
                    WindowRef::Entity(entity) => other_windows.get(*entity).ok(),
                }) else {
                    // Window does not exist - maybe it was closed and the camera not cleaned up
                    continue;
                };

                // Is the cursor/touch in this window?
                // Note: there's a bug in winit that causes `window.cursor_position()` to
                // return a `Some` value even if the cursor is not in this window, in very
                // specific cases.
                // See: https://github.com/natepiano/bevy_lagrange/issues/22
                if let Some(input_position) = window.cursor_position().or_else(|| {
                    touches
                        .iter_just_pressed()
                        .collect::<Vec<_>>()
                        .first()
                        .copied()
                        .map(Touch::position)
                }) && let Some(Rect { min, max }) = camera.logical_viewport_rect()
                {
                    // Window coordinates have Y starting at the bottom, so we need to
                    // reverse the y component before comparing
                    // with the viewport rect
                    let cursor_in_viewport = input_position.x > min.x
                        && input_position.x < max.x
                        && input_position.y > min.y
                        && input_position.y < max.y;

                    // Only set if camera order is higher. This may overwrite a previous
                    // value in the case the viewport is
                    // overlapping another viewport.
                    if cursor_in_viewport && camera.order >= max_camera_order {
                        new_active_camera_data = ActiveCameraData {
                            entity:        Some(entity),
                            viewport_size: camera.logical_viewport_size(),
                            window_size:   Some(Vec2::new(window.width(), window.height())),
                            detection:     CameraInputDetection::Automatic,
                        };
                        max_camera_order = camera.order;
                    }
                }
            }
        }
    }

    if has_input {
        active_cam.set_if_neq(new_active_camera_data);
    }
}
