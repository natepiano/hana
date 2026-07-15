//! World-space ground text beneath each example, plus the shared camera-facing
//! billboard system.
//!
//! Every section gets its name laid flat on the ground. Cap Styles additionally
//! gets a per-cylinder cap name (Round / Flat / None) under each tube and an
//! `Esc - Pause lights` line below its title that turns yellow while the
//! tube-light animation is paused.

use std::f32::consts::FRAC_PI_2;

use bevy::picking::Pickable;
use bevy::prelude::*;
use fairy_dust::FairyDustOrbitCam;
use hana_diegetic::Anchor;
use hana_diegetic::DiegeticText;
use hana_diegetic::TextAlign;

use super::animation::LightAnimation;
use super::constants::CAP_STYLE_ESC_ACTIVE_COLOR;
use super::constants::CAP_STYLE_ESC_TEXT;
use super::constants::CAP_STYLE_INFO_Z;
use super::constants::CAP_STYLE_LABEL_NAMES;
use super::constants::CAP_STYLE_LABEL_SIZE;
use super::constants::CAP_STYLE_LABEL_X_MULTIPLIERS;
use super::constants::CAP_STYLE_LABEL_Z;
use super::constants::CAP_STYLE_TITLE_Z;
use super::constants::CAP_STYLE_TUBE_SPACING;
use super::constants::CAP_STYLES_SECTION_INDEX;
use super::constants::CONNECTOR_DRAG_HINT_TEXT;
use super::constants::CONNECTOR_DRAG_HINT_Z_OFFSET;
use super::constants::CONNECTOR_LANE_DESC_SIZE;
use super::constants::CONNECTOR_LANE_DESC_WRAP_WIDTH;
use super::constants::CONNECTOR_LANE_DESCRIPTIONS;
use super::constants::CONNECTOR_LANE_FIXED_INDEX;
use super::constants::CONNECTOR_LANE_LABEL_COLOR;
use super::constants::CONNECTOR_LANE_LABEL_GAP;
use super::constants::CONNECTOR_LANE_LABEL_Y_OFFSETS;
use super::constants::CONNECTOR_LANE_LABELS;
use super::constants::CONNECTOR_LANE_NAME_DESC_GAP;
use super::constants::CONNECTOR_LANE_NAME_SIZE;
use super::constants::CONNECTOR_LANE_Z;
use super::constants::CONNECTOR_SECTION_INDEX;
use super::constants::DETACH_DEMO_SECTION_INDEX;
use super::constants::DETACH_R_RESET_ACTIVE_COLOR;
use super::constants::DETACH_R_RESET_TEXT;
use super::constants::DETACH_R_RESET_Z;
use super::constants::GROUND_LABEL_COLOR;
use super::constants::GROUND_LABEL_SIZE;
use super::constants::GROUND_LABEL_Y;
use super::constants::NODE_Y;
use super::constants::SECTION_GROUND_LABEL_Z;
use super::constants::SECTION_TITLES;
use super::constants::SECTION_X;
use super::constants::SECTION_Z;
use super::constants::SPAN_HALF_X;

/// The recoloring `Esc - Pause lights` line under the Cap Styles title.
#[derive(Component)]
pub(crate) struct EscPauseLabel;

/// World text that rotates each frame so its front faces the orbit camera. Used
/// for the hub "Drag Me" label and the Inside View label.
#[derive(Component)]
pub(crate) struct CameraFacingLabel;

/// The "R - Reset" ground line below the Detach Policy title.
#[derive(Component)]
pub(crate) struct RResetLabel;

/// Elapsed-seconds deadline until which the "R - Reset" line flashes yellow.
/// Set when `R` is pressed; cleared once the deadline passes.
#[derive(Resource, Default)]
pub(crate) struct RResetFlash {
    pub(crate) flash_until_secs: Option<f32>,
}

pub(crate) fn spawn_section_labels(mut commands: Commands) {
    for (section, title) in SECTION_TITLES.iter().enumerate() {
        // Cap Styles and Connector place their own titles relative to geometry.
        if section == CAP_STYLES_SECTION_INDEX || section == CONNECTOR_SECTION_INDEX {
            continue;
        }
        commands.spawn(ground_text(
            title,
            SECTION_X[section],
            SECTION_Z + SECTION_GROUND_LABEL_Z[section],
            GROUND_LABEL_SIZE,
            GROUND_LABEL_COLOR,
        ));
    }
}

/// Connector section labels: the "Connector Model" title and its "Drag the plugs
/// to compare" hint centered on the ground under the forward (fixed) plug, plus a
/// standing orange `Front`/`Middle`/`Back` label left of each lane's fixed
/// endpoint (the end that does not move).
pub(crate) fn spawn_connector_labels(mut commands: Commands) {
    let center_x = SECTION_X[CONNECTOR_SECTION_INDEX];
    let plug_x = center_x + SPAN_HALF_X;
    let forward_z = CONNECTOR_LANE_Z[CONNECTOR_LANE_FIXED_INDEX];

    commands.spawn(ground_text(
        SECTION_TITLES[CONNECTOR_SECTION_INDEX],
        plug_x,
        forward_z,
        GROUND_LABEL_SIZE,
        GROUND_LABEL_COLOR,
    ));
    commands.spawn(ground_text(
        CONNECTOR_DRAG_HINT_TEXT,
        plug_x,
        forward_z + CONNECTOR_DRAG_HINT_Z_OFFSET,
        CAP_STYLE_LABEL_SIZE,
        GROUND_LABEL_COLOR,
    ));

    let label_x = center_x - SPAN_HALF_X - CONNECTOR_LANE_LABEL_GAP;
    for (((name, description), lane_z), y_offset) in CONNECTOR_LANE_LABELS
        .iter()
        .zip(CONNECTOR_LANE_DESCRIPTIONS)
        .zip(CONNECTOR_LANE_Z)
        .zip(CONNECTOR_LANE_LABEL_Y_OFFSETS)
    {
        let name_y = NODE_Y + y_offset;
        // Name above, smaller description centered just below it.
        commands.spawn((
            CameraFacingLabel,
            Pickable::IGNORE,
            DiegeticText::world(*name)
                .size(CONNECTOR_LANE_NAME_SIZE)
                .anchor(Anchor::BottomCenter)
                .color(CONNECTOR_LANE_LABEL_COLOR)
                .unlit()
                .transform(Transform::from_xyz(label_x, name_y, lane_z))
                .build(),
        ));
        commands.spawn((
            CameraFacingLabel,
            Pickable::IGNORE,
            DiegeticText::world(description)
                .size(CONNECTOR_LANE_DESC_SIZE)
                .width(CONNECTOR_LANE_DESC_WRAP_WIDTH)
                .align(TextAlign::Center)
                .anchor(Anchor::TopCenter)
                .color(CONNECTOR_LANE_LABEL_COLOR)
                .unlit()
                .transform(Transform::from_xyz(
                    label_x,
                    name_y - CONNECTOR_LANE_NAME_DESC_GAP,
                    lane_z,
                ))
                .build(),
        ));
    }
}

pub(crate) fn spawn_cap_styles_labels(mut commands: Commands) {
    let center_x = SECTION_X[CAP_STYLES_SECTION_INDEX];

    commands.spawn(ground_text(
        SECTION_TITLES[CAP_STYLES_SECTION_INDEX],
        center_x,
        SECTION_Z + CAP_STYLE_TITLE_Z,
        GROUND_LABEL_SIZE,
        GROUND_LABEL_COLOR,
    ));

    for (name, multiplier) in CAP_STYLE_LABEL_NAMES
        .iter()
        .copied()
        .zip(CAP_STYLE_LABEL_X_MULTIPLIERS)
    {
        commands.spawn(ground_text(
            name,
            CAP_STYLE_TUBE_SPACING.mul_add(multiplier, center_x),
            SECTION_Z + CAP_STYLE_LABEL_Z,
            CAP_STYLE_LABEL_SIZE,
            GROUND_LABEL_COLOR,
        ));
    }
}

/// Recolors the `Esc - Pause lights` line yellow while paused, white while
/// running. Despawns and respawns the label because a built world label carries
/// its color in the tree, not in a mutable component.
pub(crate) fn update_esc_pause_label(
    mut commands: Commands,
    animation: Res<LightAnimation>,
    existing: Query<Entity, With<EscPauseLabel>>,
) {
    if !animation.is_changed() {
        return;
    }
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    let color = match *animation {
        LightAnimation::Paused { .. } => CAP_STYLE_ESC_ACTIVE_COLOR,
        LightAnimation::Running => GROUND_LABEL_COLOR,
    };
    commands.spawn((
        EscPauseLabel,
        ground_text(
            CAP_STYLE_ESC_TEXT,
            SECTION_X[CAP_STYLES_SECTION_INDEX],
            SECTION_Z + CAP_STYLE_INFO_Z,
            CAP_STYLE_LABEL_SIZE,
            color,
        ),
    ));
}

/// Spawns the "R - Reset" line below the Detach Policy title and recolors it
/// yellow while [`RResetFlash`] is active, white otherwise. Respawns on each
/// color change because a built world label carries its color in the tree, not
/// in a mutable component; the `Local` tracks the last color to avoid per-frame
/// churn and seeds the first spawn.
pub(crate) fn update_r_reset_label(
    time: Res<Time>,
    mut flash: ResMut<RResetFlash>,
    mut last_flashing: Local<Option<bool>>,
    mut commands: Commands,
    existing: Query<Entity, With<RResetLabel>>,
) {
    if flash
        .flash_until_secs
        .is_some_and(|end| time.elapsed_secs() >= end)
    {
        flash.flash_until_secs = None;
    }
    let flashing = flash.flash_until_secs.is_some();
    if *last_flashing == Some(flashing) {
        return;
    }
    *last_flashing = Some(flashing);

    for entity in &existing {
        commands.entity(entity).despawn();
    }
    let color = if flashing {
        DETACH_R_RESET_ACTIVE_COLOR
    } else {
        GROUND_LABEL_COLOR
    };
    commands.spawn((
        RResetLabel,
        ground_text(
            DETACH_R_RESET_TEXT,
            SECTION_X[DETACH_DEMO_SECTION_INDEX],
            SECTION_Z + DETACH_R_RESET_Z,
            CAP_STYLE_LABEL_SIZE,
            color,
        ),
    ));
}

/// Rotates every [`CameraFacingLabel`] so its front (+Z) faces the orbit camera.
/// Labels parented to a non-rotating entity get the world-facing rotation set
/// directly as their local rotation.
pub(crate) fn billboard_camera_facing_labels(
    camera: Query<&GlobalTransform, With<FairyDustOrbitCam>>,
    mut labels: Query<(&mut Transform, &GlobalTransform), With<CameraFacingLabel>>,
) {
    let Ok(camera_transform) = camera.single() else {
        return;
    };
    let camera_position = camera_transform.translation();
    for (mut transform, global_transform) in &mut labels {
        if let Some(direction) = (camera_position - global_transform.translation()).try_normalize()
        {
            transform.rotation = Quat::from_rotation_arc(Vec3::Z, direction);
        }
    }
}

/// A flat ground label centered at `(x, z)`, oriented so the orbit camera reads
/// it upright.
fn ground_text(text: &str, x: f32, z: f32, size: f32, color: Color) -> impl Bundle {
    DiegeticText::world(text)
        .size(size)
        .color(color)
        .transform(
            Transform::from_translation(Vec3::new(x, GROUND_LABEL_Y, z))
                .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
        )
        .build()
}
