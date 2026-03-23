//! Ink rect diagnostic — renders a single glyph "I" with its MSDF quad
//! rect (white) and computed ink rect (yellow) to isolate bounding box
//! alignment issues.

use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_brp_extras::PortDisplay;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::TextStyle;
use bevy_diegetic::WorldText;
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_panorbit_camera::PanOrbitCameraPlugin;
use bevy_panorbit_camera::TrackpadBehavior;
use bevy_panorbit_camera_ext::PanOrbitCameraExtPlugin;
use bevy_window_manager::WindowManagerPlugin;

const DISPLAY_SIZE: f32 = 48.0;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            PanOrbitCameraPlugin,
            PanOrbitCameraExtPlugin,
            BrpExtrasPlugin::default().port_in_title(PortDisplay::NonDefault),
            WindowManagerPlugin,
            DiegeticUiPlugin,
        ))
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    // Single glyph "I" — simplest possible rectangle glyph.
    commands.spawn((
        WorldText::new("grÂ"),
        TextStyle::new()
            .with_size(DISPLAY_SIZE)
            .with_color(Color::srgb(0.9, 0.9, 0.9)),
        bevy_diegetic::TypographyOverlay {
            show_font_metrics: false,
            show_glyph_metrics: true,
            show_labels: false,
            color: Color::srgb(1.0, 1.0, 0.0),
            line_width: 2.0,
            ..default()
        },
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));

    // Light
    commands.spawn((
        DirectionalLight {
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Camera — close up, looking straight at the glyph
    commands.spawn((
        PanOrbitCamera {
            focus: Vec3::new(0.0, 0.5, 0.0),
            radius: Some(0.5),
            yaw: Some(0.0),
            pitch: Some(0.0),
            button_orbit: MouseButton::Middle,
            button_pan: MouseButton::Middle,
            modifier_pan: Some(KeyCode::ShiftLeft),
            trackpad_behavior: TrackpadBehavior::BlenderLike {
                modifier_pan: Some(KeyCode::ShiftLeft),
                modifier_zoom: Some(KeyCode::ControlLeft),
            },
            trackpad_pinch_to_zoom_enabled: true,
            ..default()
        },
    ));
}
