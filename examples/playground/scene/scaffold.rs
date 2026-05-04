use bevy::prelude::*;
use bevy_lagrange::InputControl;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::TrackpadBehavior;
use bevy_lagrange::TrackpadInput;

use super::astar;
use super::cap_styles;
use super::catenary;
use super::constants::CAMERA_FOCUS_Y_MULTIPLIER;
use super::constants::CAMERA_PITCH;
use super::constants::CAMERA_RADIUS;
use super::constants::CAMERA_TRACKPAD_SENSITIVITY;
use super::constants::CAMERA_YAW;
use super::constants::DIRECTIONAL_LIGHT_ROTATION;
use super::constants::GROUND_COLOR;
use super::constants::NODE_CUBE_DIMENSION;
use super::constants::SCENE_SPOTLIGHT_INNER_ANGLE;
use super::constants::SCENE_SPOTLIGHT_INTENSITY;
use super::constants::SCENE_SPOTLIGHT_OUTER_ANGLE;
use super::constants::SCENE_SPOTLIGHT_RANGE;
use super::entity_attachment;
use super::inside_view;
use super::shared_hub;
use super::solver_comparison;
use crate::connector;
use crate::constants::CABLE_COLOR;
use crate::constants::DIRECTIONAL_LIGHT_ILLUMINANCE;
use crate::constants::GROUND_DEPTH;
use crate::constants::GROUND_WIDTH;
use crate::constants::NODE_COLOR;
use crate::constants::NODE_Y;
use crate::constants::SECTION_X;
use crate::detach_demo;
use crate::input;
use crate::sections;
use crate::sections::SectionBounds;

#[derive(Resource)]
pub(crate) struct SceneEntities {
    pub(crate) camera: Entity,
    pub(crate) ground: Entity,
}

/// Shared cable material handle for all cable meshes.
#[derive(Resource)]
pub(crate) struct SharedCableMaterial(pub(crate) Handle<StandardMaterial>);

/// Marker for cables with a radius multiplier relative to the inspector setting.
/// The `sync_cable_settings` system applies `radius * multiplier` instead of raw radius.
#[derive(Component)]
pub(crate) struct RadiusMultiplier(pub(crate) f32);

pub(crate) fn setup_camera(mut commands: Commands) {
    let focus = Vec3::new(SECTION_X[0], NODE_Y * CAMERA_FOCUS_Y_MULTIPLIER, 0.0);
    let camera = commands
        .spawn(OrbitCam {
            button_orbit: MouseButton::Middle,
            button_pan: MouseButton::Middle,
            modifier_pan: Some(KeyCode::ShiftLeft),
            input_control: Some(InputControl {
                trackpad: Some(TrackpadInput {
                    behavior:    TrackpadBehavior::BlenderLike {
                        modifier_pan:  Some(KeyCode::ShiftLeft),
                        modifier_zoom: Some(KeyCode::ControlLeft),
                    },
                    sensitivity: CAMERA_TRACKPAD_SENSITIVITY,
                }),
                ..Default::default()
            }),
            focus,
            target_focus: focus,
            yaw: Some(CAMERA_YAW),
            pitch: Some(CAMERA_PITCH),
            radius: Some(CAMERA_RADIUS),
            ..default()
        })
        .with_child(SpotLight {
            intensity: SCENE_SPOTLIGHT_INTENSITY,
            range: SCENE_SPOTLIGHT_RANGE,
            outer_angle: SCENE_SPOTLIGHT_OUTER_ANGLE,
            inner_angle: SCENE_SPOTLIGHT_INNER_ANGLE,
            shadows_enabled: false,
            ..default()
        })
        .id();

    commands.insert_resource(SceneEntities {
        camera,
        ground: Entity::PLACEHOLDER,
    });
}

pub(crate) fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut scene_entities: ResMut<SceneEntities>,
) {
    let ground = commands
        .spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(GROUND_WIDTH, GROUND_DEPTH))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: GROUND_COLOR,
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
        ))
        .observe(input::on_ground_clicked)
        .id();
    scene_entities.ground = ground;

    commands.spawn((
        DirectionalLight {
            illuminance: DIRECTIONAL_LIGHT_ILLUMINANCE,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::ZYX,
            DIRECTIONAL_LIGHT_ROTATION.0,
            DIRECTIONAL_LIGHT_ROTATION.1,
            DIRECTIONAL_LIGHT_ROTATION.2,
        )),
    ));
}

pub(crate) fn setup_sections(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    let cable_mat = materials.add(StandardMaterial {
        base_color: CABLE_COLOR,
        ..default()
    });
    commands.insert_resource(SharedCableMaterial(cable_mat.clone()));

    let node_mesh = meshes.add(Cuboid::new(
        NODE_CUBE_DIMENSION,
        NODE_CUBE_DIMENSION,
        NODE_CUBE_DIMENSION,
    ));
    let node_mat = materials.add(StandardMaterial {
        base_color: NODE_COLOR,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    let bounds = spawn_all_sections(
        &mut commands,
        &mut meshes,
        &mut materials,
        &node_mesh,
        &node_mat,
        &cable_mat,
        &asset_server,
    );
    commands.insert_resource(SectionBounds(bounds));
}

fn spawn_all_sections(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    node_mesh: &Handle<Mesh>,
    node_mat: &Handle<StandardMaterial>,
    cable_mat: &Handle<StandardMaterial>,
    asset_server: &AssetServer,
) -> Vec<Entity> {
    let mut bounds = Vec::new();

    bounds.push(sections::spawn_section_bounds(
        commands,
        meshes,
        materials,
        SECTION_X[0],
    ));
    catenary::setup_section_catenary(commands, node_mesh, node_mat, cable_mat);

    bounds.push(sections::spawn_section_bounds(
        commands,
        meshes,
        materials,
        SECTION_X[1],
    ));
    cap_styles::setup_section_cap_styles(commands, materials, cable_mat);

    bounds.push(sections::spawn_section_bounds(
        commands,
        meshes,
        materials,
        SECTION_X[2],
    ));
    solver_comparison::setup_section_solver_comparison(commands, node_mesh, node_mat, cable_mat);

    bounds.push(sections::spawn_section_bounds(
        commands,
        meshes,
        materials,
        SECTION_X[3],
    ));
    entity_attachment::setup_section_entity_attachment(commands, meshes, materials, cable_mat);

    bounds.push(sections::spawn_section_bounds(
        commands,
        meshes,
        materials,
        SECTION_X[4],
    ));
    shared_hub::setup_section_shared_hub(
        commands, meshes, materials, node_mesh, node_mat, cable_mat,
    );

    bounds.push(sections::spawn_section_bounds(
        commands,
        meshes,
        materials,
        SECTION_X[5],
    ));
    astar::setup_section_astar(commands, meshes, materials, node_mesh, node_mat, cable_mat);

    bounds.push(sections::spawn_section_bounds(
        commands,
        meshes,
        materials,
        SECTION_X[6],
    ));
    detach_demo::spawn_detach_demo(commands, meshes, materials, node_mesh, node_mat, cable_mat);

    bounds.push(sections::spawn_section_bounds(
        commands,
        meshes,
        materials,
        SECTION_X[7],
    ));
    inside_view::setup_section_inside_view(commands, cable_mat);

    bounds.push(sections::spawn_section_bounds(
        commands,
        meshes,
        materials,
        SECTION_X[8],
    ));
    connector::setup_section_connector(commands, cable_mat, asset_server);

    bounds
}
