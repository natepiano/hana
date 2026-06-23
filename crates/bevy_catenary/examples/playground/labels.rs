//! World-space section-name labels laid flat on the ground beneath each
//! example.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use bevy_diegetic::DiegeticText;

use super::constants::GROUND_LABEL_COLOR;
use super::constants::GROUND_LABEL_SIZE;
use super::constants::GROUND_LABEL_Y;
use super::constants::GROUND_LABEL_Z;
use super::constants::SECTION_TITLES;
use super::constants::SECTION_X;
use super::constants::SECTION_Z;

pub(crate) fn spawn_section_labels(mut commands: Commands) {
    for (section, title) in SECTION_TITLES.iter().enumerate() {
        let position = Vec3::new(
            SECTION_X[section],
            GROUND_LABEL_Y,
            SECTION_Z + GROUND_LABEL_Z,
        );
        commands.spawn(
            DiegeticText::world(*title)
                .size(GROUND_LABEL_SIZE)
                .color(GROUND_LABEL_COLOR)
                .transform(
                    Transform::from_translation(position)
                        .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                )
                .build(),
        );
    }
}
