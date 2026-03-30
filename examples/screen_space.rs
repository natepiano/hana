//! Screen-space HUD overlay example.
//!
//! Demonstrates a [`ScreenSpace`] panel rendered as a 2D overlay on top
//! of a 3D scene. The HUD panel stays fixed in screen space regardless
//! of camera movement — text is sized in logical pixels.

use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_brp_extras::PortDisplay;
use bevy_diegetic::Anchor;
use bevy_diegetic::Border;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TrackpadBehavior;
use bevy_window_manager::WindowManagerPlugin;

const PANEL_WIDTH: f32 = 480.0;
const PANEL_HEIGHT: f32 = 310.0;
const TITLE_SIZE: f32 = 22.0;
const HEADER_SIZE: f32 = 16.0;
const BODY_SIZE: f32 = 15.0;
const PANEL_PADDING: f32 = 12.0;

const PANEL_BACKGROUND: Color = Color::srgba(0.08, 0.08, 0.12, 0.85);
const BORDER_COLOR: Color = Color::srgba(0.3, 0.5, 0.9, 0.6);
const TITLE_COLOR: Color = Color::srgb(1.0, 1.0, 1.0);
const HEADER_COLOR: Color = Color::srgb(0.8, 0.85, 1.0);
const BODY_COLOR: Color = Color::srgb(0.85, 0.85, 0.9);
const VALUE_COLOR: Color = Color::srgb(0.6, 1.0, 0.7);
const WARN_COLOR: Color = Color::srgb(1.0, 0.8, 0.35);

/// Marker for the spinning cube.
#[derive(Component)]
struct SpinCube;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            DiegeticUiPlugin,
            LagrangePlugin,
            BrpExtrasPlugin::default().port_in_title(PortDisplay::NonDefault),
            WindowManagerPlugin,
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, spin_cube)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    spawn_scene(&mut commands, &mut meshes, &mut materials);
    spawn_hud(&mut commands);
    spawn_camera(&mut commands);
}

fn spawn_scene(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    // Spinning cube.
    commands.spawn((
        SpinCube,
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.5, 0.9),
            ..default()
        })),
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));

    // Ground plane.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(3.0)))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.15, 0.15, 0.15),
            ..default()
        })),
    ));

    // Lighting.
    commands.spawn((
        DirectionalLight {
            illuminance: 5000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::XYZ,
            -std::f32::consts::FRAC_PI_4,
            std::f32::consts::FRAC_PI_4,
            0.0,
        )),
    ));

    commands.spawn(AmbientLight {
        color:                      Color::WHITE,
        brightness:                 300.0,
        affects_lightmapped_meshes: false,
    });
}

fn spawn_hud(commands: &mut Commands) {
    commands.spawn((
        DiegeticPanel::builder()
            .size_px(PANEL_WIDTH, PANEL_HEIGHT)
            .anchor(Anchor::Center)
            .layout(|b| {
                // Outer frame.
                b.with(
                    El::new()
                        .width(Sizing::GROW)
                        .height(Sizing::GROW)
                        .padding(Padding::all(3.0))
                        .background(Color::srgba(0.05, 0.05, 0.08, 0.9))
                        .border(Border::all(2.0, BORDER_COLOR)),
                    |b| {
                        b.with(
                            El::new()
                                .width(Sizing::GROW)
                                .height(Sizing::GROW)
                                .direction(Direction::TopToBottom)
                                .padding(Padding::all(PANEL_PADDING))
                                .child_gap(6.0)
                                .background(PANEL_BACKGROUND)
                                .border(Border::all(1.0, Color::srgba(0.2, 0.3, 0.6, 0.4))),
                            |b| {
                                // Title.
                                b.text(
                                    "Mission Control",
                                    LayoutTextStyle::new(TITLE_SIZE).with_color(TITLE_COLOR),
                                );
                                divider(b);

                                // Two-column layout.
                                b.with(
                                    El::new()
                                        .width(Sizing::GROW)
                                        .height(Sizing::GROW)
                                        .direction(Direction::LeftToRight)
                                        .child_gap(12.0),
                                    |b| {
                                        // Left column — Ship Status.
                                        column(
                                            b,
                                            "Ship Status",
                                            &[
                                                ("Hull", "98%", VALUE_COLOR),
                                                ("Shields", "74%", WARN_COLOR),
                                                ("Fuel", "1,247 kg", VALUE_COLOR),
                                                ("Velocity", "342 m/s", VALUE_COLOR),
                                                ("Heading", "045.2\u{00b0}", BODY_COLOR),
                                                ("Altitude", "12.4 km", VALUE_COLOR),
                                            ],
                                        );

                                        // Vertical divider.
                                        b.with(
                                            El::new()
                                                .width(Sizing::fixed(1.0))
                                                .height(Sizing::GROW)
                                                .background(BORDER_COLOR),
                                            |_| {},
                                        );

                                        // Right column — Environment.
                                        column(
                                            b,
                                            "Environment",
                                            &[
                                                ("Sector", "Gamma-7", BODY_COLOR),
                                                ("Hostiles", "3", WARN_COLOR),
                                                ("Friendlies", "12", VALUE_COLOR),
                                                ("Comms", "Online", VALUE_COLOR),
                                                ("Gravity", "0.38 g", BODY_COLOR),
                                                ("Temp", "-142 C", BODY_COLOR),
                                            ],
                                        );
                                    },
                                );
                            },
                        );
                    },
                );
            })
            .build_screen_space(),
        Transform::from_xyz(-250.0, 150.0, 0.0),
    ));
}

fn divider(b: &mut LayoutBuilder) {
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::fixed(1.0))
            .background(BORDER_COLOR),
        |_| {},
    );
}

fn column(b: &mut LayoutBuilder, title: &str, rows: &[(&str, &str, Color)]) {
    b.with(
        El::new()
            .width(Sizing::GROW)
            .height(Sizing::FIT)
            .direction(Direction::TopToBottom)
            .child_gap(4.0),
        |b| {
            b.text(
                title,
                LayoutTextStyle::new(HEADER_SIZE).with_color(HEADER_COLOR),
            );
            for &(label, value, color) in rows {
                b.with(
                    El::new()
                        .width(Sizing::GROW)
                        .height(Sizing::FIT)
                        .direction(Direction::LeftToRight)
                        .child_gap(4.0),
                    |b| {
                        b.text(
                            label,
                            LayoutTextStyle::new(BODY_SIZE).with_color(BODY_COLOR),
                        );
                        b.with(
                            El::new().width(Sizing::GROW).height(Sizing::fixed(1.0)),
                            |_| {},
                        );
                        b.text(value, LayoutTextStyle::new(BODY_SIZE).with_color(color));
                    },
                );
            }
        },
    );
}

fn spawn_camera(commands: &mut Commands) {
    commands.spawn((
        Transform {
            translation: Vec3::new(3.27, 2.24, 3.29),
            rotation:    Quat::from_xyzw(-0.1476, 0.4041, 0.0663, 0.9003),
            scale:       Vec3::ONE,
        },
        OrbitCam {
            trackpad_behavior: TrackpadBehavior::BlenderLike {
                modifier_pan:  Some(KeyCode::ShiftLeft),
                modifier_zoom: Some(KeyCode::ControlLeft),
            },
            ..default()
        },
    ));
}

fn spin_cube(time: Res<Time>, mut cubes: Query<&mut Transform, With<SpinCube>>) {
    for mut transform in &mut cubes {
        transform.rotate_y(0.5 * time.delta_secs());
    }
}
