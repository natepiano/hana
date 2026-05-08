use bevy::light::NotShadowCaster;
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
use super::constants::CAP_STYLE_ENDPOINT_X_MULTIPLIERS;
use super::constants::CAP_STYLE_LEFT_TUBE_INDEX;
use super::constants::CAP_STYLE_LIGHT_PHASES;
use super::constants::CAP_STYLE_MIDDLE_TUBE_INDEX;
use super::constants::CAP_STYLE_RIGHT_TUBE_INDEX;
use crate::animation::TubeLight;
use crate::constants::CAP_STYLE_RADIUS_MULTIPLIER;
use crate::constants::CAP_STYLE_TUBE_OFFSET;
use crate::constants::CAP_STYLE_TUBE_SPACING;
use crate::constants::CAP_STYLES_SECTION_INDEX;
use crate::constants::DEFAULT_CABLE_RESOLUTION;
use crate::constants::NODE_Y;
use crate::constants::POINT_LIGHT_COLOR;
use crate::constants::POINT_LIGHT_INTENSITY;
use crate::constants::POINT_LIGHT_RANGE;
use crate::constants::SECTION_X;
use crate::constants::TRANSPARENT_TUBE_COLOR;
use crate::constants::TUBE_RADIUS;

/// Section 1: Three cables with different cap combinations — each end is freely choosable.
/// Left: Round/Round, Middle: Round/Flat, Right: Round/None.
pub(super) fn setup_section_cap_styles(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    cable_mat: &Handle<StandardMaterial>,
) {
    let section_center_x = SECTION_X[CAP_STYLES_SECTION_INDEX];

    let transparent_mat = materials.add(StandardMaterial {
        base_color: TRANSPARENT_TUBE_COLOR,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    let (left_start, left_end) = cap_style_endpoints(section_center_x, CAP_STYLE_LEFT_TUBE_INDEX);
    spawn_cap_style_tube(
        commands,
        transparent_mat,
        left_start,
        left_end,
        Capping::Round,
        Capping::Round,
    );

    let (mid_start, mid_end) = cap_style_endpoints(section_center_x, CAP_STYLE_MIDDLE_TUBE_INDEX);
    spawn_cap_style_tube(
        commands,
        cable_mat.clone(),
        mid_start,
        mid_end,
        Capping::None,
        Capping::flat(),
    );

    let (right_start, right_end) =
        cap_style_endpoints(section_center_x, CAP_STYLE_RIGHT_TUBE_INDEX);
    spawn_cap_style_tube(
        commands,
        cable_mat.clone(),
        right_start,
        right_end,
        Capping::Round,
        Capping::None,
    );

    let tubes = [
        (
            left_start,
            left_end,
            CAP_STYLE_LIGHT_PHASES[CAP_STYLE_LEFT_TUBE_INDEX],
        ),
        (
            mid_start,
            mid_end,
            CAP_STYLE_LIGHT_PHASES[CAP_STYLE_MIDDLE_TUBE_INDEX],
        ),
        (
            right_start,
            right_end,
            CAP_STYLE_LIGHT_PHASES[CAP_STYLE_RIGHT_TUBE_INDEX],
        ),
    ];
    for (start, end, initial_t) in tubes {
        commands.spawn((
            PointLight {
                color: POINT_LIGHT_COLOR,
                intensity: POINT_LIGHT_INTENSITY,
                range: POINT_LIGHT_RANGE,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_translation(start.lerp(end, initial_t)),
            NotShadowCaster,
            TubeLight { start, end },
        ));
    }
}

fn cap_style_endpoints(section_center_x: f32, tube_index: usize) -> (Vec3, Vec3) {
    let (start_x_multiplier, end_x_multiplier) = CAP_STYLE_ENDPOINT_X_MULTIPLIERS[tube_index];
    let start = Vec3::new(
        CAP_STYLE_TUBE_SPACING.mul_add(start_x_multiplier, section_center_x),
        NODE_Y,
        -CAP_STYLE_TUBE_OFFSET,
    );
    let end = Vec3::new(
        CAP_STYLE_TUBE_SPACING.mul_add(end_x_multiplier, section_center_x),
        NODE_Y,
        CAP_STYLE_TUBE_OFFSET,
    );

    (start, end)
}

fn spawn_cap_style_tube(
    commands: &mut Commands,
    material: Handle<StandardMaterial>,
    start: Vec3,
    end: Vec3,
    start_cap: Capping,
    end_cap: Capping,
) {
    commands
        .spawn((
            Cable {
                solver:     Solver::Linear,
                obstacles:  vec![],
                resolution: DEFAULT_CABLE_RESOLUTION,
            },
            CableMeshConfig {
                tube: TubeConfig {
                    radius: TUBE_RADIUS * CAP_STYLE_RADIUS_MULTIPLIER,
                    faces: Faces::Both,
                    ..default()
                },
                material: Some(material),
                ..default()
            },
            RadiusMultiplier(CAP_STYLE_RADIUS_MULTIPLIER),
        ))
        .with_children(|parent| {
            parent.spawn(CableEndpoint::new(CableEnd::Start, start).with_cap(start_cap));
            parent.spawn(CableEndpoint::new(CableEnd::End, end).with_cap(end_cap));
        });
}
