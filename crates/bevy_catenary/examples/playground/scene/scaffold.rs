use bevy::prelude::*;
use fairy_dust::CameraHomeTarget;

use super::cap_styles;
use super::catenary;
use super::constants::NODE_CUBE_DIMENSION;
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
use crate::constants::ENTITY_ATTACHMENT_SECTION_INDEX;
use crate::constants::INSIDE_VIEW_SECTION_INDEX;
use crate::constants::NODE_COLOR;
use crate::constants::ORTHOGONAL_ROUTING_SECTION_INDEX;
use crate::constants::SECTION_X;
use crate::constants::SHARED_HUB_SECTION_INDEX;
use crate::constants::SOLVER_COMPARISON_SECTION_INDEX;
use crate::detach_demo;
use crate::sections;
use crate::sections::SectionBounds;

/// Shared cable material handle for all cable meshes.
#[derive(Resource)]
pub(crate) struct SharedCableMaterial(pub(crate) Handle<StandardMaterial>);

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

    let catenary_bounds = sections::spawn_section_bounds(
        commands,
        meshes,
        materials,
        SECTION_X[CATENARY_SECTION_INDEX],
    );
    // The first section's bounds define the camera home region (`H` frames it).
    commands.entity(catenary_bounds).insert(CameraHomeTarget);
    bounds.push(catenary_bounds);
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
