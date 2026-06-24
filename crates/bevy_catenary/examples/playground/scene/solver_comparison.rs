use bevy::prelude::*;
use bevy_catenary::CatenarySolver;
use bevy_catenary::CurveKind;
use bevy_catenary::PathStrategy;
use bevy_catenary::Solver;

use super::constants::NODE_CUBE_DIMENSION;
use super::constants::SOLVER_COMPARISON_CATENARY_Z;
use super::constants::SOLVER_COMPARISON_LINEAR_Z;
use super::constants::SOLVER_COMPARISON_ROUTED_END_Y_OFFSET;
use super::constants::SOLVER_COMPARISON_ROUTED_START_Y_OFFSET;
use super::constants::SOLVER_COMPARISON_ROUTED_Z;
use crate::constants::BOX_LABEL_EMISSIVE_COLOR;
use crate::constants::DEFAULT_CABLE_RESOLUTION;
use crate::constants::NODE_Y;
use crate::constants::SECTION_X;
use crate::constants::SLACK_NORMAL;
use crate::constants::SOLVER_COMPARISON_SECTION_INDEX;
use crate::constants::SOLVER_FACE_LABEL_SIZE;
use crate::constants::SOLVER_FACE_LABELS;
use crate::constants::SPAN_HALF_X;
use crate::entities;

/// Section 2: Catenary, linear, and orthogonal solvers side by side.
pub(super) fn setup_section_solver_comparison(
    commands: &mut Commands,
    node_mesh: &Handle<Mesh>,
    node_material: &Handle<StandardMaterial>,
    cable_material: &Handle<StandardMaterial>,
) {
    let section_center_x = SECTION_X[SOLVER_COMPARISON_SECTION_INDEX];

    let start = Vec3::new(
        section_center_x - SPAN_HALF_X,
        NODE_Y,
        SOLVER_COMPARISON_CATENARY_Z,
    );
    let end = Vec3::new(
        section_center_x + SPAN_HALF_X,
        NODE_Y,
        SOLVER_COMPARISON_CATENARY_Z,
    );
    spawn_labeled_node_pair(
        commands,
        node_mesh,
        node_material,
        start,
        end,
        SOLVER_FACE_LABELS[0],
    );
    entities::spawn_cable(
        commands,
        start,
        end,
        Solver::Catenary(CatenarySolver::new().with_slack(SLACK_NORMAL)),
        vec![],
        cable_material,
    );

    let start = Vec3::new(
        section_center_x - SPAN_HALF_X,
        NODE_Y,
        SOLVER_COMPARISON_LINEAR_Z,
    );
    let end = Vec3::new(
        section_center_x + SPAN_HALF_X,
        NODE_Y,
        SOLVER_COMPARISON_LINEAR_Z,
    );
    spawn_labeled_node_pair(
        commands,
        node_mesh,
        node_material,
        start,
        end,
        SOLVER_FACE_LABELS[1],
    );
    entities::spawn_cable(commands, start, end, Solver::Linear, vec![], cable_material);

    let start = Vec3::new(
        section_center_x - SPAN_HALF_X,
        NODE_Y + SOLVER_COMPARISON_ROUTED_START_Y_OFFSET,
        SOLVER_COMPARISON_ROUTED_Z,
    );
    let end = Vec3::new(
        section_center_x + SPAN_HALF_X,
        NODE_Y + SOLVER_COMPARISON_ROUTED_END_Y_OFFSET,
        SOLVER_COMPARISON_ROUTED_Z,
    );
    spawn_labeled_node_pair(
        commands,
        node_mesh,
        node_material,
        start,
        end,
        SOLVER_FACE_LABELS[2],
    );
    entities::spawn_cable(
        commands,
        start,
        end,
        Solver::Routed {
            path_strategy: PathStrategy::Orthogonal,
            curve_kind:    CurveKind::Linear,
            resolution:    DEFAULT_CABLE_RESOLUTION,
        },
        vec![],
        cable_material,
    );
}

/// Spawns a node cube at each endpoint with `word` labeling every cube face in
/// the cable color.
fn spawn_labeled_node_pair(
    commands: &mut Commands,
    node_mesh: &Handle<Mesh>,
    node_material: &Handle<StandardMaterial>,
    start: Vec3,
    end: Vec3,
    word: &str,
) {
    for position in [start, end] {
        let mut node = entities::spawn_node_cube(commands, node_mesh, node_material, position);
        entities::add_cube_face_labels(
            &mut node,
            word,
            NODE_CUBE_DIMENSION,
            SOLVER_FACE_LABEL_SIZE,
            BOX_LABEL_EMISSIVE_COLOR,
        );
    }
}
