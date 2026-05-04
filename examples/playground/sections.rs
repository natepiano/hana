//! Section navigation state: bounds, current section, per-section info text.

use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy_lagrange::OrbitCam;

use super::constants::NODE_Y;
use super::constants::SECTION_BOUNDS_CENTER_Y_MULTIPLIER;
use super::constants::SECTION_BOUNDS_COLOR;
use super::constants::SECTION_BOUNDS_DEPTH;
use super::constants::SECTION_BOUNDS_HEIGHT_PADDING;
use super::constants::SECTION_BOUNDS_SPAN_MULTIPLIER;
use super::constants::SECTION_BOUNDS_WIDTH_PADDING;
use super::constants::SECTION_X;
use super::constants::SECTION_Z;
use super::constants::SPAN_HALF_X;
use super::navigation;
use super::navigation::NavLabel;

#[derive(Resource)]
pub(crate) struct CurrentSection(pub(crate) usize);

#[derive(Resource)]
pub(crate) struct SectionBounds(pub(crate) Vec<Entity>);

/// UI text that is only visible when viewing a specific section.
#[derive(Component)]
pub(crate) struct SectionInfo(pub(crate) usize);

pub(crate) fn spawn_section_bounds(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    center_x: f32,
) -> Entity {
    commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(
                SPAN_HALF_X.mul_add(SECTION_BOUNDS_SPAN_MULTIPLIER, SECTION_BOUNDS_WIDTH_PADDING),
                NODE_Y + SECTION_BOUNDS_HEIGHT_PADDING,
                SECTION_BOUNDS_DEPTH,
            ))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: SECTION_BOUNDS_COLOR,
                alpha_mode: AlphaMode::Blend,
                ..default()
            })),
            Transform::from_translation(Vec3::new(
                center_x,
                NODE_Y * SECTION_BOUNDS_CENTER_Y_MULTIPLIER,
                SECTION_Z,
            )),
            Pickable::IGNORE,
        ))
        .id()
}

pub(crate) fn update_section_info_visibility(
    current_section: Res<CurrentSection>,
    mut infos: Query<(&SectionInfo, &mut Visibility)>,
) {
    for (info, mut visibility) in &mut infos {
        *visibility = if info.0 == current_section.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

pub(crate) fn update_current_section_from_camera(
    cameras: Query<&OrbitCam>,
    mut current_section: ResMut<CurrentSection>,
    mut label_query: Query<&mut Text, With<NavLabel>>,
) {
    let Ok(camera) = cameras.single() else {
        return;
    };
    let camera_x = camera.focus.x;
    let nearest = SECTION_X
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (camera_x - *a)
                .abs()
                .partial_cmp(&(camera_x - *b).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map_or(0, |(i, _)| i);

    if nearest != current_section.0 {
        current_section.0 = nearest;
        navigation::update_nav_label(&mut label_query, nearest);
    }
}
