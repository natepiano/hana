//! Basic outline on a rotating mesh with adjustable width.

use bevy::color::palettes::css::SILVER;
use bevy::color::palettes::css::YELLOW;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use bevy_lagrange::InputControl;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TrackpadInput;
use bevy_liminal::LiminalPlugin;
use bevy_liminal::Outline;
use bevy_liminal::OutlineCamera;

// Animation
const ROTATION_X_SPEED: f32 = 1.0 / 3.0;
const ROTATION_Y_SPEED: f32 = 1.0 / 6.0;
const WIDTH_STEP: f32 = 0.1;

// Camera
const CAMERA_FOCUS: Vec3 = Vec3::new(0.0, 1.0, 0.0);
const CAMERA_POSITION: Vec3 = Vec3::new(3.0, 2.0, 3.0);

// Display formatting
const WIDTH_DISPLAY_PRECISION: usize = 1;

// Lighting
const LIGHT_INTENSITY: f32 = 10_000_000.0;
const LIGHT_POSITION: Vec3 = Vec3::new(8.0, 16.0, 8.0);
const LIGHT_RANGE: f32 = 100.0;
const LIGHT_SHADOW_DEPTH_BIAS: f32 = 0.2;

// Scene
const GROUND_SIZE: f32 = 50.0;
const GROUND_SUBDIVISIONS: u32 = 10;
const INITIAL_OUTLINE_WIDTH: f32 = 10.0;
const OUTLINED_CUBE_POSITION: Vec3 = Vec3::new(0.0, 1.0, 0.0);

// UI
const UI_FONT_SIZE: f32 = 16.0;
const UI_PADDING: f32 = 10.0;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(ImagePlugin::default_nearest()),
            LagrangePlugin,
            LiminalPlugin,
        ))
        .add_systems(Startup, (setup, setup_ui))
        .add_systems(
            FixedUpdate,
            (
                rotate,
                handle_width_input.run_if(on_message::<KeyboardInput>),
            ),
        )
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(CAMERA_POSITION).looking_at(CAMERA_FOCUS, Vec3::Y),
        OrbitCam {
            button_orbit: MouseButton::Middle,
            button_pan: MouseButton::Middle,
            modifier_pan: Some(KeyCode::ShiftLeft),
            input_control: Some(InputControl {
                trackpad: Some(TrackpadInput::blender_default()),
                ..default()
            }),
            ..default()
        },
        OutlineCamera,
    ));

    commands.spawn((
        PointLight {
            shadows_enabled: true,
            intensity: LIGHT_INTENSITY,
            range: LIGHT_RANGE,
            shadow_depth_bias: LIGHT_SHADOW_DEPTH_BIAS,
            ..default()
        },
        Transform::from_translation(LIGHT_POSITION),
    ));

    // ground plane
    commands.spawn((
        Mesh3d(
            meshes.add(
                Plane3d::default()
                    .mesh()
                    .size(GROUND_SIZE, GROUND_SIZE)
                    .subdivisions(GROUND_SUBDIVISIONS),
            ),
        ),
        MeshMaterial3d(materials.add(Color::from(SILVER))),
    ));

    commands.spawn((
        Mesh3d(meshes.add(Cuboid::default())),
        MeshMaterial3d(materials.add(Color::from(YELLOW))),
        Transform::from_translation(OUTLINED_CUBE_POSITION),
        // Add outline
        Outline::jump_flood(INITIAL_OUTLINE_WIDTH).build(),
    ));
}

fn rotate(mut outline_query: Query<&mut Transform, With<Outline>>, time: Res<Time>) {
    for mut transform in &mut outline_query {
        let rotation = Quat::from_rotation_y(time.delta_secs() * ROTATION_Y_SPEED)
            * Quat::from_rotation_x(time.delta_secs() * ROTATION_X_SPEED);

        transform.rotation *= rotation;
    }
}

fn handle_width_input(
    input: Res<ButtonInput<KeyCode>>,
    mut outline: Single<&mut Outline>,
    mut text_query: Single<&mut Text, With<WidthText>>,
) {
    let mut delta = 0.0;

    if input.pressed(KeyCode::KeyQ) {
        delta -= WIDTH_STEP;
    } else if input.pressed(KeyCode::KeyW) {
        delta += WIDTH_STEP;
    }

    if delta == 0.0 {
        return;
    }

    outline.width += delta;
    text_query.0 = width_text(outline.width);
}

#[derive(Component)]
struct WidthText;

fn setup_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(width_text(INITIAL_OUTLINE_WIDTH)),
        TextFont {
            font_size: UI_FONT_SIZE,
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(UI_PADDING),
            right: Val::Px(UI_PADDING),
            ..default()
        },
        WidthText,
    ));
}

fn width_text(width: f32) -> String {
    format!(
        "Decrease width (Q)\nIncrease width (W)\nCurrent width: {width:.WIDTH_DISPLAY_PRECISION$}"
    )
}
