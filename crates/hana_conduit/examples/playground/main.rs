//! Progressive cable playground with catenary, routed cable, cap style,
//! alignment, and detach demo sections.
//!
//! `SECTION_COUNT` sections navigated with Left/Right arrow keys or number
//! keys 1-9. `OrbitCam` animates to frame each section, and a bottom-left
//! Fairy Dust panel highlights the current section and last-used direction.
//!
//! **Known limitation**: Cable-to-cable attachment (chaining cables by attaching one
//! endpoint to another cable's endpoint entity) does not work yet because `CableEndpoint`
//! entities lack a `GlobalTransform` that tracks their resolved world position.
//!
//! Controls:
//! - Left/Right arrows: step between sections
//! - 1-9: jump to a section
//! - H: Home — frame the first section
//! - F: Overview — frame the whole scene
//! - Orbit: Middle-mouse drag (or two-finger trackpad)
//! - Pan: Shift + middle-mouse
//! - Zoom: Scroll wheel (or pinch)
//! - +/-: Adjust catenary slack
//! - R: Reset detach demo (Detach Policy section)
//! - Esc: Pause tube light animation
//! - Ctrl+Shift+R: Hot-restart

mod animation;
mod connector;
mod constants;
mod detach_demo;
mod entities;
mod input;
mod labels;
mod nav_panel;
mod scene;
mod sections;
mod ui;

use animation::LightAnimation;
use bevy::prelude::*;
use bevy_lagrange::OrbitCamPreset;
use constants::CATENARY_SECTION_INDEX;
use constants::EXAMPLE_TITLE;
use constants::GROUND_DEPTH;
use constants::GROUND_WIDTH;
use constants::HOME_PITCH;
use constants::HOME_YAW;
use constants::OVERVIEW_CONTROL;
use constants::SLACK_HINT;
use constants::SLACK_MINUS_LABEL;
use constants::SLACK_MINUS_SEGMENT_ID;
use constants::SLACK_PLUS_LABEL;
use constants::SLACK_PLUS_SEGMENT_ID;
use constants::ZOOM_MARGIN_NAVIGATION;
use entities::FullSceneTarget;
use fairy_dust::Anchor;
use fairy_dust::TitleBar;
use fairy_dust::TitleBarControl;
use fairy_dust::TitleBarSegment;
use hana_conduit::CatenaryPlugin;
use input::SlackMinusPulseBegin;
use input::SlackMinusPulseEnd;
use input::SlackPlusPulseBegin;
use input::SlackPlusPulseEnd;
use nav_panel::NavSelection;
use nav_panel::RequestedNavigation;
use sections::CurrentSection;

fn main() {
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_hdr()
        .with_studio_lighting()
        .with_ground_plane()
        .size(1.0)
        .transform(Transform::from_scale(Vec3::new(
            GROUND_WIDTH,
            1.0,
            GROUND_DEPTH,
        )))
        .insert(FullSceneTarget)
        .with_orbit_cam_preset(|_| {}, OrbitCamPreset::blender_like())
        .unclamped()
        .with_bloom()
        .with_camera_home()
        .yaw(HOME_YAW)
        .pitch(HOME_PITCH)
        .margin(ZOOM_MARGIN_NAVIGATION)
        .with_title_bar(
            TitleBar::new()
                .with_title(EXAMPLE_TITLE)
                .with_anchor(Anchor::TopLeft)
                .control(OVERVIEW_CONTROL)
                .control(TitleBarControl::segmented(
                    SLACK_HINT,
                    [
                        TitleBarSegment::new(SLACK_PLUS_SEGMENT_ID, SLACK_PLUS_LABEL),
                        TitleBarSegment::new(SLACK_MINUS_SEGMENT_ID, SLACK_MINUS_LABEL),
                    ],
                )),
        )
        .wire_chip_to_fit_target::<FullSceneTarget>(OVERVIEW_CONTROL)
        .wire_chip_to_events::<SlackPlusPulseBegin, SlackPlusPulseEnd>(SLACK_PLUS_SEGMENT_ID)
        .wire_chip_to_events::<SlackMinusPulseBegin, SlackMinusPulseEnd>(SLACK_MINUS_SEGMENT_ID)
        .with_camera_control_panel()
        .add_plugins(CatenaryPlugin)
        .init_resource::<input::DragState>()
        .init_resource::<input::SlackPulse>()
        .init_resource::<labels::RResetFlash>()
        .init_resource::<LightAnimation>()
        .init_resource::<NavSelection>()
        .init_resource::<RequestedNavigation>()
        .insert_resource(CurrentSection(CATENARY_SECTION_INDEX))
        .add_systems(
            Startup,
            (
                (scene::setup_sections, ui::setup_ui).chain(),
                nav_panel::spawn_nav_panel,
                labels::spawn_section_labels,
                labels::spawn_cap_styles_labels,
                labels::spawn_connector_labels,
            ),
        )
        .add_systems(
            Update,
            (
                sections::update_current_section_from_camera,
                sections::update_section_info_visibility,
                nav_panel::apply_navigation_request,
                nav_panel::refresh_nav_panel,
                input::clear_slack_pulses,
                labels::update_esc_pause_label,
                labels::update_r_reset_label,
                (input::handle_drag, scene::sync_movable_obstacles).chain(),
                labels::billboard_camera_facing_labels,
                animation::animate_tube_light,
            ),
        )
        .add_observer(input::on_cable_mesh_child_added)
        .with_shortcut(KeyCode::ArrowLeft, nav_panel::request_previous_section)
        .with_shortcut(KeyCode::ArrowRight, nav_panel::request_next_section)
        .with_shortcut(KeyCode::Digit1, nav_panel::request_catenary_section)
        .with_shortcut(KeyCode::Digit2, nav_panel::request_cap_styles_section)
        .with_shortcut(
            KeyCode::Digit3,
            nav_panel::request_solver_comparison_section,
        )
        .with_shortcut(
            KeyCode::Digit4,
            nav_panel::request_entity_attachment_section,
        )
        .with_shortcut(KeyCode::Digit5, nav_panel::request_shared_hub_section)
        .with_shortcut(
            KeyCode::Digit6,
            nav_panel::request_orthogonal_routing_section,
        )
        .with_shortcut(KeyCode::Digit7, nav_panel::request_detach_demo_section)
        .with_shortcut(KeyCode::Digit8, nav_panel::request_inside_view_section)
        .with_shortcut(KeyCode::Digit9, nav_panel::request_connector_section)
        .with_shortcut(KeyCode::KeyF, input::frame_full_scene)
        .with_shortcut(KeyCode::KeyR, input::reset_detach_demo)
        .with_shortcut(KeyCode::Escape, animation::toggle_light_animation)
        .with_shortcut(KeyCode::Equal, input::begin_plus_slack_pulse)
        .with_shortcut(KeyCode::Minus, input::begin_minus_slack_pulse)
        .with_held_shortcut(KeyCode::Equal, input::increase_slack)
        .with_held_shortcut(KeyCode::Minus, input::decrease_slack)
        .run();
}
