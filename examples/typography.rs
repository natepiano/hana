//! Typography overlay demo — visualizes font-level metric lines and
//! per-glyph bounding boxes on a `WorldText` entity using the library's
//! built-in `TypographyOverlay` debug component.
//!
//! Requires the `typography_overlay` feature:
//! ```sh
//! cargo run --example typography --features typography_overlay
//! ```

use std::time::Duration;

use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_brp_extras::PortDisplay;
use bevy_diegetic::Border;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextConfig;
use bevy_diegetic::TextStyle;
use bevy_diegetic::TypographyOverlay;
use bevy_diegetic::WorldText;
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_panorbit_camera::PanOrbitCameraPlugin;
use bevy_panorbit_camera::TrackpadBehavior;
use bevy_panorbit_camera_ext::CameraMove;
use bevy_panorbit_camera_ext::PanOrbitCameraExtPlugin;
use bevy_panorbit_camera_ext::PlayAnimation;
use bevy_panorbit_camera_ext::ZoomToFit;
use bevy_window_manager::WindowManagerPlugin;

const DISPLAY_SIZE: f32 = 48.0;
const ZOOM_MARGIN_SCENE: f32 = 0.08;
const ZOOM_DURATION_MS: u64 = 1000;

const HOME_FOCUS: Vec3 = Vec3::new(-0.001, 0.461, 2.002);
const HOME_RADIUS: f32 = 2.84;
const HOME_YAW: f32 = 0.0;
const HOME_PITCH: f32 = 0.055;

const CONTROLS_LAYOUT_W: f32 = 100.0;
const CONTROLS_LAYOUT_H: f32 = 60.0;
const CONTROLS_WORLD_W: f32 = 0.6;
const CONTROLS_WORLD_H: f32 = 0.36;
const CONTROLS_FONT_SIZE: f32 = 9.0;
const CONTROLS_TITLE_SIZE: f32 = 10.5;
const CONTROLS_TITLE_COLOR: Color = Color::srgb(0.42, 0.5, 0.72);

#[derive(Resource)]
struct SceneBounds(Entity);

#[derive(Component)]
struct ControlsPanel;

/// Marker for the main display text that the overlay toggle targets.
#[derive(Component)]
struct DisplayText;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            PanOrbitCameraPlugin,
            PanOrbitCameraExtPlugin,
            BrpExtrasPlugin::default().port_in_title(PortDisplay::NonDefault),
            WindowManagerPlugin,
            MeshPickingPlugin,
            DiegeticUiPlugin,
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, (toggle_overlay, home_camera))
        .add_observer(on_world_text_added)
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
            Mesh3d(meshes.add(Plane3d::default().mesh().size(5.4, 5.4))),
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
    commands.spawn((
        DisplayText,
        WorldText::new("TypogrÂphy"),
        TextStyle::new()
            .with_size(DISPLAY_SIZE)
            .with_color(Color::srgb(0.9, 0.9, 0.9)),
        TypographyOverlay::default(),
        Transform::from_xyz(0.0, 0.5, 2.0),
    ));

    // Hint text
    commands.spawn((
        WorldText::new("Click text to zoom in · Click plane to zoom out"),
        TextStyle::new()
            .with_size(2.0)
            .with_color(Color::srgba(0.6, 0.6, 0.6, 0.8)),
        Transform::from_xyz(0.0, 0.0, 3.45),
    ));

    // Light
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(0.0, 1.5, 3.0).looking_at(Vec3::new(0.0, 0.0, -6.0), Vec3::Y),
    ));

    // Controls panel — upper left.
    commands
        .spawn((
            ControlsPanel,
            DiegeticPanel {
                tree:          build_controls_panel(),
                layout_width:  CONTROLS_LAYOUT_W,
                layout_height: CONTROLS_LAYOUT_H,
                world_width:   CONTROLS_WORLD_W,
                world_height:  CONTROLS_WORLD_H,
            },
            Transform::from_xyz(-1.2, 1.5, 0.5),
        ))
        .observe(on_panel_clicked);

    // Camera
    commands.spawn((PanOrbitCamera {
        focus: HOME_FOCUS,
        radius: Some(HOME_RADIUS),
        yaw: Some(HOME_YAW),
        pitch: Some(HOME_PITCH),
        button_orbit: MouseButton::Middle,
        button_pan: MouseButton::Middle,
        modifier_pan: Some(KeyCode::ShiftLeft),
        trackpad_behavior: TrackpadBehavior::BlenderLike {
            modifier_pan:  Some(KeyCode::ShiftLeft),
            modifier_zoom: Some(KeyCode::ControlLeft),
        },
        trackpad_sensitivity: 0.5,
        trackpad_pinch_to_zoom_enabled: true,
        ..default()
    },));
}

fn on_ground_clicked(click: On<Pointer<Click>>, mut commands: Commands, scene: Res<SceneBounds>) {
    let camera = click.hit.camera;
    commands.trigger(
        ZoomToFit::new(camera, scene.0)
            .margin(ZOOM_MARGIN_SCENE)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );
}

fn on_world_text_added(added: On<Add, WorldText>, mut commands: Commands) {
    commands.entity(added.entity).observe(on_text_clicked);
}

fn on_text_clicked(mut click: On<Pointer<Click>>, mut commands: Commands) {
    click.propagate(false);
    let camera = click.hit.camera;
    commands.trigger(
        ZoomToFit::new(camera, click.entity)
            .margin(ZOOM_MARGIN_SCENE)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );
}

fn on_panel_clicked(mut click: On<Pointer<Click>>, mut commands: Commands) {
    click.propagate(false);
    let camera = click.hit.camera;
    commands.trigger(
        ZoomToFit::new(camera, click.entity)
            .margin(ZOOM_MARGIN_SCENE)
            .duration(Duration::from_millis(ZOOM_DURATION_MS)),
    );
}

fn build_controls_panel() -> bevy_diegetic::LayoutTree {
    let border_color = Color::srgb(0.4, 0.4, 0.45);
    let divider_color = Color::srgb(0.45, 0.45, 0.5);

    let mut builder = LayoutBuilder::new(CONTROLS_LAYOUT_W, CONTROLS_LAYOUT_H);
    builder.with(
        El::new()
            .width(Sizing::FIT)
            .height(Sizing::FIT)
            .padding(Padding::all(2.5))
            .direction(Direction::TopToBottom)
            .child_gap(1.5)
            .background(Color::srgba(0.1, 0.1, 0.12, 0.85))
            .border(Border::all(1.0, border_color)),
        |b| {
            b.text(
                "controls",
                TextConfig::new(CONTROLS_TITLE_SIZE).with_color(CONTROLS_TITLE_COLOR),
            );
            // Horizontal divider.
            b.with(
                El::new()
                    .width(Sizing::GROW)
                    .height(Sizing::fixed(0.3))
                    .background(divider_color),
                |_| {},
            );
            // Three-column layout: keys | divider | descriptions.
            b.with(
                El::new()
                    .width(Sizing::FIT)
                    .height(Sizing::FIT)
                    .direction(Direction::LeftToRight)
                    .child_gap(2.0),
                |b| {
                    // Key column.
                    b.with(
                        El::new()
                            .width(Sizing::FIT)
                            .height(Sizing::FIT)
                            .direction(Direction::TopToBottom)
                            .child_gap(1.0),
                        |b| {
                            b.text("t", TextConfig::new(CONTROLS_FONT_SIZE));
                            b.text("h", TextConfig::new(CONTROLS_FONT_SIZE));
                        },
                    );
                    // Single vertical divider.
                    b.with(
                        El::new()
                            .width(Sizing::fixed(0.3))
                            .height(Sizing::GROW)
                            .background(divider_color),
                        |_| {},
                    );
                    // Description column.
                    b.with(
                        El::new()
                            .width(Sizing::FIT)
                            .height(Sizing::FIT)
                            .direction(Direction::TopToBottom)
                            .child_gap(1.0),
                        |b| {
                            b.text("toggle overlay", TextConfig::new(CONTROLS_FONT_SIZE));
                            b.text("home camera", TextConfig::new(CONTROLS_FONT_SIZE));
                        },
                    );
                },
            );
        },
    );
    builder.build()
}

fn toggle_overlay(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    with_overlay: Query<Entity, (With<DisplayText>, With<TypographyOverlay>)>,
    without_overlay: Query<Entity, (With<DisplayText>, Without<TypographyOverlay>)>,
) {
    if !keyboard.just_pressed(KeyCode::KeyT) {
        return;
    }
    if with_overlay.is_empty() {
        for entity in &without_overlay {
            commands.entity(entity).insert(TypographyOverlay::default());
        }
    } else {
        for entity in &with_overlay {
            commands.entity(entity).remove::<TypographyOverlay>();
        }
    }
}

fn home_camera(
    keyboard: Res<ButtonInput<KeyCode>>,
    cameras: Query<Entity, With<PanOrbitCamera>>,
    mut commands: Commands,
) {
    if !keyboard.just_pressed(KeyCode::KeyH) {
        return;
    }
    for camera in &cameras {
        commands.trigger(PlayAnimation::new(
            camera,
            [CameraMove::ToOrbit {
                focus:    HOME_FOCUS,
                yaw:      HOME_YAW,
                pitch:    HOME_PITCH,
                radius:   HOME_RADIUS,
                duration: Duration::from_millis(ZOOM_DURATION_MS),
                easing:   bevy::math::curve::easing::EaseFunction::CubicOut,
            }],
        ));
    }
}
