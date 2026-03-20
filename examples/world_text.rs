//! `WorldText` example — standalone MSDF text in world space.
//!
//! Demonstrates `WorldText` on a ground plane and on the front face of a cube.

use std::time::Duration;

use bevy::core_pipeline::oit::OrderIndependentTransparencySettings;
use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_diegetic::DiegeticUiPlugin;
use bevy_diegetic::MsdfTextMaterial;
use bevy_diegetic::TextAnchor;
use bevy_diegetic::TextStyle;
use bevy_diegetic::WorldText;
use bevy_diegetic::msdf_text_material;
use bevy_inspector_egui::bevy_egui::EguiPlugin;
use bevy_inspector_egui::inspector_options::std_options::NumberDisplay;
use bevy_inspector_egui::prelude::*;
use bevy_inspector_egui::quick::ResourceInspectorPlugin;
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_panorbit_camera::PanOrbitCameraPlugin;
use bevy_panorbit_camera::TrackpadBehavior;
use bevy_panorbit_camera_ext::PanOrbitCameraExtPlugin;
use bevy_panorbit_camera_ext::ZoomToFit;
use bevy_window_manager::WindowManagerPlugin;

#[derive(Resource, Reflect, InspectorOptions)]
#[reflect(Resource, InspectorOptions)]
struct TextMaterialSettings {
    #[inspector(min = 0.0, max = 1.0, speed = 0.01, display = NumberDisplay::Slider)]
    text_alpha:  f32,
    #[inspector(min = 0.0, max = 1.0, speed = 0.01, display = NumberDisplay::Slider)]
    plane_alpha: f32,
    unlit:       bool,
}

impl Default for TextMaterialSettings {
    fn default() -> Self {
        Self {
            text_alpha:  1.0,
            plane_alpha: 1.0,
            unlit:       false,
        }
    }
}

const ZOOM_MARGIN_MESH: f32 = 0.15;
const ZOOM_MARGIN_SCENE: f32 = 0.08;
const ZOOM_DURATION_MS: u64 = 1000;

#[derive(Resource)]
struct SceneBounds(Entity);

#[derive(Component)]
struct GroundPlane;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            DiegeticUiPlugin,
            PanOrbitCameraPlugin,
            PanOrbitCameraExtPlugin,
            BrpExtrasPlugin::default(),
            WindowManagerPlugin,
            MeshPickingPlugin,
            EguiPlugin::default(),
            ResourceInspectorPlugin::<TextMaterialSettings>::default(),
        ))
        .init_resource::<TextMaterialSettings>()
        .add_systems(Startup, setup)
        .add_systems(Update, apply_material_settings)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    _msdf_materials: ResMut<Assets<MsdfTextMaterial>>,
    _atlas: Res<bevy_diegetic::MsdfAtlas>,
) {
    // Ground plane
    let ground = commands
        .spawn((
            GroundPlane,
            Mesh3d(meshes.add(Plane3d::default().mesh().size(8.0, 8.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.15, 0.15, 0.2, 1.8),
                alpha_mode: AlphaMode::Opaque,
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
        ))
        .observe(on_ground_clicked)
        .id();

    commands.insert_resource(SceneBounds(ground));

    // Cube
    commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::default())),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.8, 0.7, 0.6),
                ..default()
            })),
            Transform::from_xyz(0.0, 1.0, 0.0),
        ))
        .observe(on_mesh_clicked);

    // Text on the ground plane (lying flat, facing up).
    commands.spawn((
        WorldText::new("GROUND"),
        TextStyle::new()
            .with_size(48.0)
            .with_color(Color::srgb(0.9, 0.9, 0.1)),
        Transform::from_xyz(0.0, 0.001, 1.5)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    ));

    // Instructions on the upper-left of the plane surface.
    commands.spawn((
        WorldText::new("click the box to zoom in\nclick the plane to zoom back out"),
        TextStyle::new()
            .with_size(16.0)
            .with_color(Color::WHITE)
            .with_anchor(TextAnchor::TopLeft),
        Transform::from_xyz(-3.8, 0.001, -3.8)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    ));

    // Debug: plain StandardMaterial quad ON the transparent plane.
    commands.spawn((
        Mesh3d(meshes.add(Rectangle::new(0.3, 0.3))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.9, 0.3, 0.1, 0.5),
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_xyz(-1.5, 0.001, 1.5)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    ));

    // Debug: opaque peach quad parallel to plane at y=1.0 for comparison.
    commands.spawn((
        Mesh3d(meshes.add(Rectangle::new(0.3, 0.3))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.9, 0.3, 0.1),
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_xyz(-2.0, 1.0, 1.5)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    ));

    // Debug: plain quad as CHILD with MsdfTextMaterial.
    // Deferred to first Update frame so the atlas GPU image is ready.
    commands.queue(move |world: &mut World| {
        let atlas = world.resource::<bevy_diegetic::MsdfAtlas>();
        let Some(atlas_image) = atlas.image_handle().cloned() else {
            return;
        };
        #[allow(clippy::cast_possible_truncation)]
        let msdf_mat = msdf_text_material(
            atlas.sdf_range() as f32,
            atlas.width(),
            atlas.height(),
            atlas_image,
        );
        let mesh = world
            .resource_mut::<Assets<Mesh>>()
            .add(Rectangle::new(0.3, 0.3));
        let mat = world
            .resource_mut::<Assets<MsdfTextMaterial>>()
            .add(msdf_mat);
        world
            .spawn((Transform::from_xyz(0.8, 1.0, 0.55), Visibility::default()))
            .with_child((Mesh3d(mesh), MeshMaterial3d(mat), Transform::IDENTITY));
    });

    let face_style = TextStyle::new()
        .with_size(20.0)
        .with_color(Color::srgb(0.9, 0.3, 0.1));

    // Front face (+Z).
    commands.spawn((
        WorldText::new("FRONT"),
        face_style.clone(),
        Transform::from_xyz(0.0, 1.0, 0.55),
    ));

    // Back face (-Z).
    commands.spawn((
        WorldText::new("BACK"),
        face_style.clone(),
        Transform::from_xyz(0.0, 1.0, -0.55)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
    ));

    // Top face (+Y).
    commands.spawn((
        WorldText::new("TOP"),
        face_style.clone(),
        Transform::from_xyz(0.0, 1.55, 0.0)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    ));

    // Bottom face (-Y).
    commands.spawn((
        WorldText::new("BOTTOM"),
        face_style.clone(),
        Transform::from_xyz(0.0, 0.45, 0.0)
            .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
    ));

    // Left face (-X).
    commands.spawn((
        WorldText::new("LEFT"),
        face_style.clone(),
        Transform::from_xyz(-0.55, 1.0, 0.0)
            .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
    ));

    // Right face (+X).
    commands.spawn((
        WorldText::new("RIGHT"),
        face_style,
        Transform::from_xyz(0.51, 1.0, 0.0)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
    ));

    // Ambient light so text is always readable.
    commands.insert_resource(GlobalAmbientLight {
        color:                      Color::WHITE,
        brightness:                 5_000.0,
        affects_lightmapped_meshes: true,
    });

    // Directional light for shadows and depth.
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
        Transform::from_xyz(0.0, 3.0, 5.0).looking_at(Vec3::new(0.0, 0.8, 0.0), Vec3::Y),
    ));
}

fn on_mesh_clicked(click: On<Pointer<Click>>, mut commands: Commands) {
    let camera = click.hit.camera;
    commands.trigger(
        ZoomToFit::new(camera, click.entity)
            .margin(ZOOM_MARGIN_MESH)
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

/// Applies inspector settings to all MSDF text materials.
///
/// Runs every frame for the first few frames to catch materials created
/// after startup, then only on inspector changes.
fn apply_material_settings(
    settings: Res<TextMaterialSettings>,
    mut msdf_materials: ResMut<Assets<MsdfTextMaterial>>,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
    plane: Query<&MeshMaterial3d<StandardMaterial>, With<GroundPlane>>,
    mut frame_count: Local<u32>,
) {
    if !settings.is_changed() {
        return;
    }
    for (_, mat) in msdf_materials.iter_mut() {
        mat.base.base_color = Color::srgba(1.0, 1.0, 1.0, settings.text_alpha);
        mat.base.alpha_mode = AlphaMode::Blend;
        mat.base.unlit = settings.unlit;
    }
    // Update ground plane alpha.
    for mat_handle in &plane {
        if let Some(mat) = std_materials.get_mut(&mat_handle.0) {
            mat.base_color = mat.base_color.with_alpha(settings.plane_alpha);
            mat.alpha_mode = if settings.plane_alpha < 1.0 {
                AlphaMode::Blend
            } else {
                AlphaMode::Opaque
            };
        }
    }
}
