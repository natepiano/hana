//! Demonstrates explicit camera input routing with
//! `CameraInputRoutingConfig::explicit(camera_entity)` plus
//! `CameraInputSurfaceMetrics::camera_view_and_input_surface(view, surface)`.
//! Input is delivered to an offscreen `OrbitCam` that renders into a 512x512
//! image; the window camera shows that image on a rotating cube. The input
//! surface metrics convert window-pixel mouse deltas into the 512x512
//! texture-space deltas the offscreen camera expects.
//!
//! Adapted from Bevy's `render_to_texture` example.
//!
//! Controls:
//!   Drag - orbit the offscreen camera (the texture on the outer cube
//!          reframes as you do)
//!   H    - return to the camera home pose

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
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::Fit;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::Px;
use bevy_diegetic::Sizing;
use bevy_kana::ToF32;
use bevy_lagrange::CameraInputRoutingConfig;
use bevy_lagrange::CameraInputSurfaceMetrics;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamPreset;
use fairy_dust::Anchor;
use fairy_dust::CameraHomeTarget;
use fairy_dust::DEFAULT_PANEL_BACKGROUND;
use fairy_dust::FairyDustOrbitCam;
use fairy_dust::LABEL_SIZE;
use fairy_dust::TITLE_COLOR;
use fairy_dust::TITLE_SIZE;
use fairy_dust::TitleBar;
use fairy_dust::screen_panel_frame;
use fairy_dust::screen_panel_material;

fn main() {
    fairy_dust::sprinkle_example()
        .with_brp_extras()
        .with_save_window_position()
        .with_studio_lighting()
        .with_camera_home()
        .yaw(HOME_YAW)
        .pitch(HOME_PITCH)
        .margin(HOME_MARGIN)
        .with_title_bar(
            TitleBar::new()
                .with_title(EXAMPLE_TITLE)
                .with_anchor(Anchor::TopLeft),
        )
        .with_camera_control_panel()
        .add_systems(Startup, (setup, spawn_info_panel))
        .add_systems(Update, cube_rotator_system)
        .run();
}

// ═════════════════════════════════════════════════════════════════════════════
// INPUT ROUTING — CameraInputRoutingConfig::explicit + CameraInputSurfaceMetrics.
//
// How it works (all wired in `setup`, Startup):
//   1. Build a 512x512 `Image` with `RENDER_ATTACHMENT | TEXTURE_BINDING | COPY_DST` usages and add
//      it to `Assets<Image>`.
//   2. Spawn the first-pass cube + a point light on `RenderLayers::layer(FIRST_PASS_LAYER_INDEX)`.
//      The first-pass cube carries `CameraHomeTarget` so Fairy Dust's home logic frames it.
//   3. Spawn the offscreen OrbitCam with `RenderTarget::Image(image_handle)`, the same
//      `RenderLayers`, `OrbitCamInputMode::Preset(BlenderLike)`, `FairyDustOrbitCam` (so the camera
//      control panel finds it), and `CameraInputSurfaceMetrics::camera_view_and_input_surface(view,
//      surface)` where `view` is the 512x512 texture extent and `surface` is the primary window
//      size — i.e. mouse deltas arrive in window pixels, the OrbitCam's camera view is the texture.
//   4. Spawn the main-pass (outer) cube with a `StandardMaterial` whose `base_color_texture` is
//      that same image handle, plus the main-pass window camera looking at it.
//   5. Insert `CameraInputRoutingConfig::explicit(orbit_cam_id)`. Without this resource, the
//      offscreen camera wouldn't receive input — it has no window of its own to dispatch from.
// ═════════════════════════════════════════════════════════════════════════════

// app / title bar
const EXAMPLE_TITLE: &str = "Render to Texture";

// camera home
const HOME_MARGIN: f32 = 0.1;
const HOME_PITCH: f32 = 0.45;
const HOME_YAW: f32 = 0.75;

// offscreen (first-pass) camera + cube + light
const FIRST_PASS_CAMERA_NAME: &str = "Texture camera";
const FIRST_PASS_CAMERA_ORDER: isize = -1;
const FIRST_PASS_CAMERA_TRANSLATION: Vec3 = Vec3::new(0.0, 0.0, 15.0);
const FIRST_PASS_LAYER_INDEX: usize = 1;
const FIRST_PASS_CUBE_COLOR: Color = Color::srgb(1.0, 0.48, 0.12);
const FIRST_PASS_CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, 0.0, 1.0);
const FIRST_PASS_LIGHT_INTENSITY: f32 = 220_000.0;
const FIRST_PASS_LIGHT_RANGE: f32 = 40.0;
const FIRST_PASS_LIGHT_TRANSLATION: Vec3 = Vec3::new(-3.0, 4.0, 8.0);

// main-pass (window) camera + cube
const MAIN_PASS_CAMERA_NAME: &str = "Window camera";
const MAIN_PASS_CAMERA_TRANSLATION: Vec3 = Vec3::new(0.0, 0.0, 15.0);
const MAIN_PASS_CUBE_TRANSLATION: Vec3 = Vec3::new(0.0, 0.0, 1.5);

// shared cube material
const CUBE_REFLECTANCE: f32 = 0.02;
const CUBE_SIZE: f32 = 4.0;

// render target
const RENDER_TARGET_HEIGHT: u32 = 512;
const RENDER_TARGET_MIP_LEVEL_COUNT: u32 = 1;
const RENDER_TARGET_SAMPLE_COUNT: u32 = 1;
const RENDER_TARGET_WIDTH: u32 = 512;

// Marks the first-pass cube — rendered into the texture by the offscreen camera.
#[derive(Component)]
struct FirstPassCube;

// Marks the main-pass (outer) cube — its material samples the rendered texture.
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

    // The Image the offscreen camera renders into and the main-pass material
    // samples from. RENDER_ATTACHMENT lets it be a render target; TEXTURE_BINDING
    // lets it be sampled in a material; COPY_DST lets `image.resize` zero it.
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
    image.resize(size);

    let image_handle = images.add(image);

    let cube_handle = meshes.add(Cuboid::new(CUBE_SIZE, CUBE_SIZE, CUBE_SIZE));
    let cube_material_handle = materials.add(StandardMaterial {
        base_color: FIRST_PASS_CUBE_COLOR,
        reflectance: CUBE_REFLECTANCE,
        unlit: false,
        ..default()
    });

    // Render layer that isolates the first-pass cube + light + camera from
    // the main pass — the offscreen camera and its scene live on this layer.
    let first_pass_layer = RenderLayers::layer(FIRST_PASS_LAYER_INDEX);

    commands.spawn((
        Mesh3d(cube_handle),
        MeshMaterial3d(cube_material_handle),
        Transform::from_translation(FIRST_PASS_CUBE_TRANSLATION),
        FirstPassCube,
        CameraHomeTarget,
        first_pass_layer.clone(),
    ));

    commands.spawn((
        PointLight {
            intensity: FIRST_PASS_LIGHT_INTENSITY,
            range: FIRST_PASS_LIGHT_RANGE,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_translation(FIRST_PASS_LIGHT_TRANSLATION),
        first_pass_layer.clone(),
    ));

    // The offscreen OrbitCam: renders the first-pass layer into `image_handle`.
    // `CameraInputSurfaceMetrics::camera_view_and_input_surface` tells the
    // OrbitCam that mouse deltas arrive in window pixels but should be applied
    // against a 512x512 camera view; without it, dragging would feel wrong.
    let orbit_cam_id = commands
        .spawn((
            Name::new(FIRST_PASS_CAMERA_NAME),
            Camera {
                clear_color: ClearColorConfig::Custom(Color::WHITE),
                order: FIRST_PASS_CAMERA_ORDER,
                ..default()
            },
            RenderTarget::Image(ImageRenderTarget::from(image_handle.clone())),
            Transform::from_translation(FIRST_PASS_CAMERA_TRANSLATION)
                .looking_at(Vec3::ZERO, Vec3::Y),
            OrbitCam::default(),
            OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike),
            CameraInputSurfaceMetrics::camera_view_and_input_surface(
                Vec2::new(size.width.to_f32(), size.height.to_f32()),
                Vec2::new(primary_window.width(), primary_window.height()),
            ),
            first_pass_layer,
            FairyDustOrbitCam,
        ))
        .id();

    let cube_handle = meshes.add(Cuboid::new(CUBE_SIZE, CUBE_SIZE, CUBE_SIZE));

    // Main-pass material samples the rendered first-pass image.
    let material_handle = materials.add(StandardMaterial {
        base_color_texture: Some(image_handle),
        reflectance: CUBE_REFLECTANCE,
        unlit: false,
        ..default()
    });

    commands.spawn((
        Mesh3d(cube_handle),
        MeshMaterial3d(material_handle),
        Transform::from_translation(MAIN_PASS_CUBE_TRANSLATION)
            .with_rotation(Quat::from_rotation_x(OUTER_CUBE_ROTATION_X)),
        MainPassCube,
    ));

    commands.spawn((
        Name::new(MAIN_PASS_CAMERA_NAME),
        Camera3d::default(),
        Transform::from_translation(MAIN_PASS_CAMERA_TRANSLATION).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Aim Bevy input at the offscreen OrbitCam. Without this, the camera has
    // no window of its own to dispatch from and would receive no input.
    commands.insert_resource(CameraInputRoutingConfig::explicit(orbit_cam_id));
}

// ═════════════════════════════════════════════════════════════════════════════
// OUTER CUBE ROTATION — decorative spin of the main-pass cube so the rendered
// texture is visible from changing angles.
// ═════════════════════════════════════════════════════════════════════════════

const OUTER_CUBE_ROTATION_SPEED_X: f32 = 0.35;
const OUTER_CUBE_ROTATION_SPEED_Y: f32 = 0.25;
const OUTER_CUBE_ROTATION_X: f32 = -PI / 5.0;

fn cube_rotator_system(time: Res<Time>, mut query: Query<&mut Transform, With<MainPassCube>>) {
    for mut transform in &mut query {
        transform.rotate_x(OUTER_CUBE_ROTATION_SPEED_X * time.delta_secs());
        transform.rotate_y(OUTER_CUBE_ROTATION_SPEED_Y * time.delta_secs());
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// INFO PANEL — top-right diegetic panel describing the demo in-app.
// ═════════════════════════════════════════════════════════════════════════════

const INFO_PANEL_GAP: Px = Px(6.0);
const INFO_PANEL_WIDTH: Px = Px(430.0);
const INFO_TEXT_COLOR: Color = Color::srgba(0.68, 0.72, 0.82, 0.9);
const INFO_PANEL_LINES: [&str; 3] = [
    "Input is routed to the offscreen OrbitCam.",
    "That camera renders the inner cube into a 512x512 image.",
    "The window camera displays that image on the rotating cube.",
];

#[derive(Component)]
struct InfoPanel;

fn spawn_info_panel(mut commands: Commands) {
    let unlit = screen_panel_material();
    let panel = DiegeticPanel::screen()
        .size(Fit, Fit)
        .anchor(Anchor::TopRight)
        .material(unlit.clone())
        .text_material(unlit)
        .layout(build_info_panel_layout)
        .build();

    match panel {
        Ok(panel) => {
            commands.spawn((InfoPanel, panel, Transform::default()));
        },
        Err(error) => {
            error!("render_to_texture: failed to build info panel: {error}");
        },
    }
}

fn build_info_panel_layout(builder: &mut LayoutBuilder) {
    let title = LayoutTextStyle::new(TITLE_SIZE)
        .with_color(TITLE_COLOR)
        .no_wrap();
    let body = LayoutTextStyle::new(LABEL_SIZE).with_color(INFO_TEXT_COLOR);

    screen_panel_frame(
        builder,
        Sizing::fixed(INFO_PANEL_WIDTH),
        Sizing::FIT,
        DEFAULT_PANEL_BACKGROUND,
        |builder| {
            builder.with(
                El::new()
                    .width(Sizing::GROW)
                    .direction(Direction::TopToBottom)
                    .child_gap(INFO_PANEL_GAP),
                |builder| {
                    builder.text("How it works", title);
                    for line in INFO_PANEL_LINES {
                        builder.text(line, body.clone());
                    }
                },
            );
        },
    );
}
