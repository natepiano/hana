//! Typography overlay demo — visualizes font-level metric lines and
//! per-glyph bounding boxes on a `WorldText` entity using the library's
//! built-in `TypographyOverlay` debug component.
//!
//! Requires the `typography_overlay` feature:
//! ```sh
//! cargo run --example typography --features typography_overlay
//! ```

use std::time::Duration;

use bevy::color::palettes::css::WHITE;
use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::TextStyle;
use bevy_diegetic::TypographyOverlay;
use bevy_diegetic::WorldText;
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_panorbit_camera::PanOrbitCameraPlugin;
use bevy_panorbit_camera::TrackpadBehavior;
use bevy_panorbit_camera_ext::PanOrbitCameraExtPlugin;
use bevy_panorbit_camera_ext::ZoomToFit;
use bevy_window_manager::WindowManagerPlugin;

const DISPLAY_SIZE: f32 = 48.0;
const ZOOM_MARGIN_SCENE: f32 = 0.08;
const ZOOM_DURATION_MS: u64 = 1000;

#[derive(Resource)]
struct SceneBounds(Entity);

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            PanOrbitCameraPlugin,
            PanOrbitCameraExtPlugin,
            BrpExtrasPlugin::default(),
            WindowManagerPlugin,
            MeshPickingPlugin,
            DiegeticUiPlugin,
        ))
        .add_systems(Startup, setup)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Ground plane — subtle, light gray.
    let ground = commands
        .spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(12.0, 12.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.15, 0.15, 0.15),
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
        ))
        .observe(on_ground_clicked)
        .id();

    commands.insert_resource(SceneBounds(ground));

    // Display word with typography overlay.
    commands
        .spawn((
            WorldText::new("Typography"),
            TextStyle::new()
                .with_size(DISPLAY_SIZE)
                .with_color(Color::srgb(0.9, 0.9, 0.9)),
            // Overlay temporarily disabled to isolate MSDF artifacts.
            TypographyOverlay {
                show_font_metrics: false,
                show_glyph_metrics: false,
                show_labels: false,
                color: Color::from(WHITE),
                ..default()
            },
            Transform::from_xyz(0.0, 1.5, 0.0),
        ))
        .observe(on_text_clicked);

    // Light
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Camera
    commands.spawn((
        PanOrbitCamera {
            focus:  Vec3::new(0.0, 1.5, 0.0),
            radius: Some(2.93),
            yaw:    Some(0.0),
            pitch:  Some(0.05),
            button_orbit: MouseButton::Middle,
            button_pan: MouseButton::Middle,
            modifier_pan: Some(KeyCode::ShiftLeft),
            trackpad_behavior: TrackpadBehavior::BlenderLike {
                modifier_pan:  Some(KeyCode::ShiftLeft),
                modifier_zoom: Some(KeyCode::ControlLeft),
            },
            trackpad_pinch_to_zoom_enabled: true,
            ..default()
        },
    ));
}

fn on_text_clicked(click: On<Pointer<Click>>, mut commands: Commands) {
    let camera = click.hit.camera;
    commands.trigger(
        ZoomToFit::new(camera, click.entity)
            .margin(ZOOM_MARGIN_SCENE)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );
}

fn on_ground_clicked(click: On<Pointer<Click>>, mut commands: Commands, scene: Res<SceneBounds>) {
    let camera = click.hit.camera;
    commands.trigger(
        ZoomToFit::new(camera, scene.0)
            .margin(ZOOM_MARGIN_SCENE)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );
}
