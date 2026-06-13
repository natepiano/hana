//! Progressive cable playground with catenary, routed cable, cap style,
//! alignment, and detach demo sections.
//!
//! `SECTION_COUNT` sections navigated with arrow buttons or Left/Right keys.
//! `OrbitCam` animates to frame each section.
//!
//! **Known limitation**: Cable-to-cable attachment (chaining cables by attaching one
//! endpoint to another cable's endpoint entity) does not work yet because `CableEndpoint`
//! entities lack a `GlobalTransform` that tracks their resolved world position.
//!
//! Controls:
//! - Left/Right arrow keys or on-screen arrows: navigate sections
//! - Orbit: Middle-mouse drag (or two-finger trackpad)
//! - Pan: Shift + middle-mouse
//! - Zoom: Scroll wheel (or pinch)
//! - D: Toggle debug gizmos
//! - H: Home -zoom to fit entire scene
//! - R: Reset detach demo (section 6)

mod animation;
mod connector;
mod constants;
mod detach_demo;
mod entities;
mod input;
mod navigation;
mod scene;
mod sections;
mod ui;

use animation::LightAnimation;
use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_catenary::CatenaryPlugin;
use bevy_lagrange::LagrangePlugin;
use constants::CATENARY_SECTION_INDEX;
use constants::PLAYGROUND_WINDOW_TITLE;
use sections::CurrentSection;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: PLAYGROUND_WINDOW_TITLE.into(),
                    ..default()
                }),
                ..default()
            }),
            LagrangePlugin,
            MeshPickingPlugin,
            BrpExtrasPlugin::default(),
            CatenaryPlugin,
        ))
        .init_resource::<input::DragState>()
        .init_resource::<LightAnimation>()
        .insert_resource(CurrentSection(CATENARY_SECTION_INDEX))
        .add_systems(
            Startup,
            (
                scene::setup_camera,
                (scene::setup_scene, scene::setup_sections, ui::setup_ui),
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                sections::update_current_section_from_camera,
                sections::update_section_info_visibility,
                input::handle_keyboard,
                navigation::handle_navigation_buttons,
                input::handle_drag,
                animation::animate_tube_light,
            ),
        )
        .add_observer(input::on_cable_mesh_child_added)
        .run();
}
