use bevy::prelude::*;
use bevy_catenary::Cable;
use bevy_catenary::CableEnd;
use bevy_catenary::CableEndpoint;
use bevy_catenary::CableMeshConfig;
use bevy_catenary::Capping;
use bevy_catenary::Faces;
use bevy_catenary::Solver;
use bevy_catenary::TubeConfig;

use super::RadiusMultiplier;
use super::constants::INSIDE_VIEW_END_Y_OFFSET;
use super::constants::INSIDE_VIEW_ENDPOINT_X_OFFSET;
use super::constants::INSIDE_VIEW_START_Y_OFFSET;
use super::constants::INSIDE_VIEW_TUBE_SIDES;
use super::constants::INSIDE_VIEW_Z_EXTENT;
use crate::constants::DEFAULT_CABLE_RESOLUTION;
use crate::constants::INSIDE_VIEW_RADIUS_MULTIPLIER;
use crate::constants::INSIDE_VIEW_SECTION_INDEX;
use crate::constants::NODE_Y;
use crate::constants::SECTION_X;
use crate::constants::TUBE_RADIUS;

/// Section 7: Inside view — large tube rendered inside-only.
pub(super) fn setup_section_inside_view(
    commands: &mut Commands,
    cable_material: &Handle<StandardMaterial>,
) {
    let section_center_x = SECTION_X[INSIDE_VIEW_SECTION_INDEX];
    let start = Vec3::new(
        section_center_x + INSIDE_VIEW_ENDPOINT_X_OFFSET,
        NODE_Y + INSIDE_VIEW_START_Y_OFFSET,
        INSIDE_VIEW_Z_EXTENT,
    );
    let end = Vec3::new(
        section_center_x - INSIDE_VIEW_ENDPOINT_X_OFFSET,
        NODE_Y + INSIDE_VIEW_END_Y_OFFSET,
        -INSIDE_VIEW_Z_EXTENT,
    );
    commands
        .spawn((
            Cable {
                solver:     Solver::Linear,
                obstacles:  vec![],
                resolution: DEFAULT_CABLE_RESOLUTION,
            },
            CableMeshConfig {
                tube: TubeConfig {
                    radius: TUBE_RADIUS * INSIDE_VIEW_RADIUS_MULTIPLIER,
                    sides:  INSIDE_VIEW_TUBE_SIDES,
                    faces:  Faces::Both,
                },
                material: Some(cable_material.clone()),
                ..default()
            },
            RadiusMultiplier(INSIDE_VIEW_RADIUS_MULTIPLIER),
        ))
        .with_children(|parent| {
            parent.spawn(CableEndpoint::new(CableEnd::Start, start).with_cap(Capping::None));
            parent.spawn(CableEndpoint::new(CableEnd::End, end).with_cap(Capping::None));
        });
}
