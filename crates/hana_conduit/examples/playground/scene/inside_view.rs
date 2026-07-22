use bevy::picking::Pickable;
use bevy::prelude::*;
use hana_conduit::Cable;
use hana_conduit::CableEnd;
use hana_conduit::CableEndpoint;
use hana_conduit::CableMeshConfig;
use hana_conduit::CapStyle;
use hana_conduit::Faces;
use hana_conduit::Solver;
use hana_conduit::TubeConfig;
use hana_diegetic::DiegeticText;
use hana_diegetic::PanelPicking;

use super::constants::INSIDE_VIEW_END_Y_OFFSET;
use super::constants::INSIDE_VIEW_ENDPOINT_X_OFFSET;
use super::constants::INSIDE_VIEW_START_Y_OFFSET;
use super::constants::INSIDE_VIEW_TUBE_SIDES;
use super::constants::INSIDE_VIEW_Z_EXTENT;
use crate::constants::BOX_LABEL_EMISSIVE_COLOR;
use crate::constants::DEFAULT_CABLE_RESOLUTION;
use crate::constants::INSIDE_VIEW_LABEL_SIZE;
use crate::constants::INSIDE_VIEW_LABEL_TEXT;
use crate::constants::INSIDE_VIEW_RADIUS_MULTIPLIER;
use crate::constants::INSIDE_VIEW_SECTION_INDEX;
use crate::constants::NODE_Y;
use crate::constants::SECTION_X;
use crate::constants::TUBE_RADIUS;
use crate::labels::CameraFacingLabel;

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
                tube_config: TubeConfig {
                    radius: TUBE_RADIUS * INSIDE_VIEW_RADIUS_MULTIPLIER,
                    sides:  INSIDE_VIEW_TUBE_SIDES,
                    faces:  Faces::Both,
                },
                material: Some(cable_material.clone()),
                ..default()
            },
        ))
        .with_children(|parent| {
            parent.spawn(CableEndpoint::new(CableEnd::Start, start).with_cap(CapStyle::None));
            parent.spawn(CableEndpoint::new(CableEnd::End, end).with_cap(CapStyle::None));
        });

    // Emissive label at the tube's midpoint, billboarded to face the camera.
    let tube_center = (start + end) * 0.5;
    commands.spawn((
        CameraFacingLabel,
        Pickable::IGNORE,
        PanelPicking::PASS_THROUGH,
        DiegeticText::world(INSIDE_VIEW_LABEL_TEXT)
            .size(INSIDE_VIEW_LABEL_SIZE)
            .color(BOX_LABEL_EMISSIVE_COLOR)
            .unlit()
            .transform(Transform::from_translation(tube_center))
            .build(),
    ));
}
