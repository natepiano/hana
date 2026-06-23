use bevy::prelude::*;
use bevy_lagrange::OrbitCam;
use bevy_lagrange::OrbitCamInputMode;
use bevy_lagrange::OrbitCamPreset;

use super::cap_styles;
use super::catenary;
use super::constants::CAMERA_FOCUS_Y_MULTIPLIER;
use super::constants::CAMERA_PITCH;
use super::constants::CAMERA_RADIUS;
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
use super::orthogonal_routing;
use super::shared_hub;
use super::solver_comparison;
use crate::connector;
use crate::constants::CABLE_COLOR;
use crate::constants::CAP_STYLES_SECTION_INDEX;
use crate::constants::CATENARY_SECTION_INDEX;
use crate::constants::CONNECTOR_SECTION_INDEX;
use crate::constants::DETACH_DEMO_SECTION_INDEX;
use crate::constants::DIRECTIONAL_LIGHT_ILLUMINANCE;
use crate::constants::ENTITY_ATTACHMENT_SECTION_INDEX;
use crate::constants::GROUND_DEPTH;
use crate::constants::GROUND_WIDTH;
use crate::constants::INSIDE_VIEW_SECTION_INDEX;
use crate::constants::NODE_COLOR;
use crate::constants::NODE_Y;
use crate::constants::ORTHOGONAL_ROUTING_SECTION_INDEX;
use crate::constants::SECTION_X;
use crate::constants::SECTION_Z;
use crate::constants::SHARED_HUB_SECTION_INDEX;
use crate::constants::SOLVER_COMPARISON_SECTION_INDEX;
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

pub(crate) fn setup_camera(mut commands: Commands) {
    let focus = Vec3::new(
        SECTION_X[CATENARY_SECTION_INDEX],
        NODE_Y * CAMERA_FOCUS_Y_MULTIPLIER,
        SECTION_Z,
    );
    let camera = commands
        .spawn((
            OrbitCam {
                focus,
                target_focus: focus,
                yaw: Some(CAMERA_YAW),
                pitch: Some(CAMERA_PITCH),
                radius: Some(CAMERA_RADIUS),
                ..default()
            },
            OrbitCamInputMode::with_preset(OrbitCamPreset::blender_like()),
        ))
        .with_child(SpotLight {
            intensity: SCENE_SPOTLIGHT_INTENSITY,
            range: SCENE_SPOTLIGHT_RANGE,
            outer_angle: SCENE_SPOTLIGHT_OUTER_ANGLE,
            inner_angle: SCENE_SPOTLIGHT_INNER_ANGLE,
            shadow_maps_enabled: false,
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
            shadow_maps_enabled: true,
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
    let cable_material = materials.add(StandardMaterial {
        base_color: CABLE_COLOR,
        ..default()
    });
    commands.insert_resource(SharedCableMaterial(cable_material.clone()));

    let node_mesh = meshes.add(Cuboid::new(
        NODE_CUBE_DIMENSION,
        NODE_CUBE_DIMENSION,
        NODE_CUBE_DIMENSION,
    ));
    let node_material = materials.add(StandardMaterial {
        base_color: NODE_COLOR,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    let bounds = spawn_all_sections(
        &mut commands,
        &mut meshes,
        &mut materials,
        &node_mesh,
        &node_material,
        &cable_material,
        &asset_server,
    );
    commands.insert_resource(SectionBounds(bounds));
}

fn spawn_all_sections(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    node_mesh: &Handle<Mesh>,
    node_material: &Handle<StandardMaterial>,
    cable_material: &Handle<StandardMaterial>,
    asset_server: &AssetServer,
) -> Vec<Entity> {
    let mut bounds = Vec::new();

    bounds.push(sections::spawn_section_bounds(
        commands,
        meshes,
        materials,
        SECTION_X[CATENARY_SECTION_INDEX],
    ));
    catenary::setup_section_catenary(commands, node_mesh, node_material, cable_material);

    bounds.push(sections::spawn_section_bounds(
        commands,
        meshes,
        materials,
        SECTION_X[CAP_STYLES_SECTION_INDEX],
    ));
    cap_styles::setup_section_cap_styles(commands, materials, cable_material);

    bounds.push(sections::spawn_section_bounds(
        commands,
        meshes,
        materials,
        SECTION_X[SOLVER_COMPARISON_SECTION_INDEX],
    ));
    solver_comparison::setup_section_solver_comparison(
        commands,
        node_mesh,
        node_material,
        cable_material,
    );

    bounds.push(sections::spawn_section_bounds(
        commands,
        meshes,
        materials,
        SECTION_X[ENTITY_ATTACHMENT_SECTION_INDEX],
    ));
    entity_attachment::setup_section_entity_attachment(commands, meshes, materials, cable_material);

    bounds.push(sections::spawn_section_bounds(
        commands,
        meshes,
        materials,
        SECTION_X[SHARED_HUB_SECTION_INDEX],
    ));
    shared_hub::setup_section_shared_hub(
        commands,
        meshes,
        materials,
        node_mesh,
        node_material,
        cable_material,
    );

    bounds.push(sections::spawn_section_bounds(
        commands,
        meshes,
        materials,
        SECTION_X[ORTHOGONAL_ROUTING_SECTION_INDEX],
    ));
    orthogonal_routing::setup_section_orthogonal_routing(
        commands,
        meshes,
        materials,
        node_mesh,
        node_material,
        cable_material,
    );

    bounds.push(sections::spawn_section_bounds(
        commands,
        meshes,
        materials,
        SECTION_X[DETACH_DEMO_SECTION_INDEX],
    ));
    detach_demo::spawn_detach_demo(
        commands,
        meshes,
        materials,
        node_mesh,
        node_material,
        cable_material,
    );

    bounds.push(sections::spawn_section_bounds(
        commands,
        meshes,
        materials,
        SECTION_X[INSIDE_VIEW_SECTION_INDEX],
    ));
    inside_view::setup_section_inside_view(commands, cable_material);

    bounds.push(sections::spawn_section_bounds(
        commands,
        meshes,
        materials,
        SECTION_X[CONNECTOR_SECTION_INDEX],
    ));
    connector::setup_section_connector(commands, cable_material, asset_server);

    bounds
}
