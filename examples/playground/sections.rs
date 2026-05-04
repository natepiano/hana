//! Section navigation state: bounds, current section, per-section info text.

use bevy::picking::Pickable;
use bevy::prelude::*;
use bevy_lagrange::OrbitCam;

use super::constants::NODE_Y;
use super::constants::SECTION_X;
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
                SPAN_HALF_X.mul_add(2.0, 2.0),
                NODE_Y + 2.0,
                5.0,
            ))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.0, 0.0, 0.0, 0.0),
                alpha_mode: AlphaMode::Blend,
                ..default()
            })),
            Transform::from_translation(Vec3::new(center_x, NODE_Y * 0.5, 0.0)),
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
    let Ok(cam) = cameras.single() else {
        return;
    };
    let cam_x = cam.focus.x;
    let nearest = SECTION_X
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (cam_x - *a)
                .abs()
                .partial_cmp(&(cam_x - *b).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map_or(0, |(i, _)| i);

    if nearest != current_section.0 {
        current_section.0 = nearest;
        navigation::update_nav_label(&mut label_query, nearest);
    }
}
