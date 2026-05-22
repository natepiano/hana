//! Progressive cable playground — showcases `bevy_catenary` features from simple to complex.
//!
//! 7 sections navigated with arrow buttons or Left/Right keys. Camera animates
//! to frame each section.
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
//! - I: Toggle inspector
//! - R: Reset detach demo (section 6)

use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_catenary::CatenaryPlugin;
use bevy_inspector_egui::bevy_egui::EguiPlugin;
use bevy_inspector_egui::quick::ResourceInspectorPlugin;
use bevy_lagrange::LagrangePlugin;

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
use constants::CATENARY_SECTION_INDEX;
use constants::PLAYGROUND_WINDOW_TITLE;
use sections::CurrentSection;
use ui::CableSettings;
use ui::InspectorVisibility;

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
            EguiPlugin::default(),
            LagrangePlugin,
            MeshPickingPlugin,
            BrpExtrasPlugin::default(),
            CatenaryPlugin,
            ResourceInspectorPlugin::<CableSettings>::default().run_if(
                |visible: Res<InspectorVisibility>| {
                    matches!(*visible, InspectorVisibility::Visible)
                },
            ),
        ))
        .init_resource::<CableSettings>()
        .init_resource::<InspectorVisibility>()
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
                ui::sync_cable_settings.run_if(resource_changed::<CableSettings>),
                animation::animate_tube_light,
            ),
        )
        .add_observer(input::on_cable_mesh_child_added)
        .run();
}
