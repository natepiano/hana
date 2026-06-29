use bevy::prelude::*;
use hana_conduit::CatenarySolver;
use hana_conduit::Solver;

use crate::constants::CATENARY_SECTION_INDEX;
use crate::constants::NODE_Y;
use crate::constants::SECTION_X;
use crate::constants::SECTION_Z;
use crate::constants::SLACK_NORMAL;
use crate::constants::SPAN_HALF_X;
use crate::entities;

/// Section 0: Simple catenary cable between two nodes.
pub(super) fn setup_section_catenary(
    commands: &mut Commands,
    node_mesh: &Handle<Mesh>,
    node_material: &Handle<StandardMaterial>,
    cable_material: &Handle<StandardMaterial>,
) {
    let section_center_x = SECTION_X[CATENARY_SECTION_INDEX];
    let start = Vec3::new(section_center_x - SPAN_HALF_X, NODE_Y, SECTION_Z);
    let end = Vec3::new(section_center_x + SPAN_HALF_X, NODE_Y, SECTION_Z);
    entities::spawn_node_pair(commands, node_mesh, node_material, start, end);
    entities::spawn_cable(
        commands,
        start,
        end,
        Solver::Catenary(CatenarySolver::new().with_slack(SLACK_NORMAL)),
        vec![],
        cable_material,
    );
}
