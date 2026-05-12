//! Demonstrates explicit camera input routing for an `OrbitCam` that renders to a texture/image
//! instead of a window viewport.
//!
//! In this example, Bevy input is routed to the camera that renders the texture applied to the
//! cube, rather than the main window camera. The camera still uses normal preset/custom input;
//! explicit routing only chooses which camera receives that input.
//!
//! This example is based off Bevy's `render_to_texture` example.

use std::f32::consts::PI;

use bevy::camera::ImageRenderTarget;
use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::render_resource::Extent3d;
use bevy::render::render_resource::TextureDescriptor;
use bevy::render::render_resource::TextureDimension;
use bevy::render::render_resource::TextureFormat;
use bevy::render::render_resource::TextureUsages;
use bevy::window::PrimaryWindow;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_kana::ToF32;
use bevy_lagrange::CameraInputRoutingConfig;
use bevy_lagrange::CameraInputSurfaceMetrics;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;
use bevy_window_manager::WindowManagerPlugin;

// camera
const FIRST_PASS_CAMERA_ORDER: isize = -1;
const FIRST_PASS_CAMERA_TRANSLATION: Vec3 = Vec3::new(0.0, 0.0, 15.0);
const FIRST_PASS_LAYER_INDEX: usize = 1;
const MAIN_PASS_CAMERA_TRANSLATION: Vec3 = Vec3::new(0.0, 0.0, 15.0);

// cube
const CUBE_COLOR: Color = Color::srgb(0.8, 0.7, 0.6);
const CUBE_REFLECTANCE: f32 = 0.02;
const CUBE_SIZE: f32 = 4.0;
const FIRST_PASS_CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, 0.0, 1.0);
const MAIN_PASS_CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, 0.0, 1.5);
const OUTER_CUBE_ROTATION_SPEED_X: f32 = 1.0;
const OUTER_CUBE_ROTATION_SPEED_Y: f32 = 0.7;
const OUTER_CUBE_ROTATION_X: f32 = -PI / 5.0;

// render target
const RENDER_TARGET_MIP_LEVEL_COUNT: u32 = 1;
const RENDER_TARGET_SAMPLE_COUNT: u32 = 1;
const RENDER_TARGET_HEIGHT: u32 = 512;
const RENDER_TARGET_WIDTH: u32 = 512;

// scene
const LIGHT_TRANSLATION: Vec3 = Vec3::new(0.0, 0.0, 10.0);

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(LagrangePlugin)
        .add_plugins(BrpExtrasPlugin::default())
        .add_plugins(WindowManagerPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, cube_rotator_system)
        .run();
}

// Marks the first pass cube (rendered to a texture.)
#[derive(Component)]
struct FirstPassCube;

// Marks the main pass cube, to which the texture is applied.
#[derive(Component)]
struct MainPassCube;

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    primary_window: Single<&Window, With<PrimaryWindow>>,
) {
    let size = Extent3d {
        width: RENDER_TARGET_WIDTH,
        height: RENDER_TARGET_HEIGHT,
        ..default()
    };

    // This is the texture that will be rendered to.
    let mut image = Image {
        texture_descriptor: TextureDescriptor {
            label: None,
            size,
            dimension: TextureDimension::D2,
            format: TextureFormat::Bgra8UnormSrgb,
            mip_level_count: RENDER_TARGET_MIP_LEVEL_COUNT,
            sample_count: RENDER_TARGET_SAMPLE_COUNT,
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_DST
                | TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        },
        ..default()
    };

    // fill image.data with zeroes
    image.resize(size);

    let image_handle = images.add(image);

    let cube_handle = meshes.add(Cuboid::new(CUBE_SIZE, CUBE_SIZE, CUBE_SIZE));
    let cube_material_handle = materials.add(StandardMaterial {
        base_color: CUBE_COLOR,
        reflectance: CUBE_REFLECTANCE,
        unlit: false,
        ..default()
    });

    // This specifies the layer used for the first pass, which will be attached to the first pass
    // camera and cube.
    let first_pass_layer = RenderLayers::layer(FIRST_PASS_LAYER_INDEX);

    // The cube that will be rendered to the texture.
    commands.spawn((
        Mesh3d(cube_handle),
        MeshMaterial3d(cube_material_handle),
        Transform::from_translation(FIRST_PASS_CUBE_TRANSLATION),
        FirstPassCube,
        first_pass_layer.clone(),
    ));

    // Light
    // NOTE: Currently lights are shared between passes - see https://github.com/bevyengine/bevy/issues/3462
    commands.spawn((
        PointLight::default(),
        Transform::from_translation(LIGHT_TRANSLATION),
    ));

    // The camera for the first pass cube that will be rendered to the texture. This is the camera
    // that is controlled by `OrbitCam`.
    let orbit_cam_id = commands
        .spawn((
            Camera {
                // render before the "main pass" camera
                clear_color: ClearColorConfig::Custom(Color::WHITE),
                order: FIRST_PASS_CAMERA_ORDER,
                ..default()
            },
            RenderTarget::Image(ImageRenderTarget::from(image_handle.clone())),
            Transform::from_translation(FIRST_PASS_CAMERA_TRANSLATION)
                .looking_at(Vec3::ZERO, Vec3::Y),
            OrbitCam::default(),
            // Logical camera-view and input-surface sizes replace the old active-camera
            // bookkeeping path. Use image texel dimensions only when the image grid is the
            // interaction surface.
            CameraInputSurfaceMetrics::camera_view_and_input_surface(
                Vec2::new(size.width.to_f32(), size.height.to_f32()),
                Vec2::new(primary_window.width(), primary_window.height()),
            ),
            first_pass_layer,
        ))
        .id();

    let cube_handle = meshes.add(Cuboid::new(CUBE_SIZE, CUBE_SIZE, CUBE_SIZE));

    // This material has the texture that has been rendered.
    let material_handle = materials.add(StandardMaterial {
        base_color_texture: Some(image_handle),
        reflectance: CUBE_REFLECTANCE,
        unlit: false,
        ..default()
    });

    // Main pass cube, with material containing the rendered first pass texture.
    commands.spawn((
        Mesh3d(cube_handle),
        MeshMaterial3d(material_handle),
        Transform::from_translation(MAIN_PASS_CUBE_TRANSLATION)
            .with_rotation(Quat::from_rotation_x(OUTER_CUBE_ROTATION_X)),
        MainPassCube,
    ));

    // The main pass camera.
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(MAIN_PASS_CAMERA_TRANSLATION).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.insert_resource(CameraInputRoutingConfig::explicit(orbit_cam_id));
}

/// Rotates the outer cube (main pass)
fn cube_rotator_system(time: Res<Time>, mut query: Query<&mut Transform, With<MainPassCube>>) {
    for mut transform in &mut query {
        transform.rotate_x(OUTER_CUBE_ROTATION_SPEED_X * time.delta_secs());
        transform.rotate_y(OUTER_CUBE_ROTATION_SPEED_Y * time.delta_secs());
    }
}
