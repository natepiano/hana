//! Capability: simple studio lighting for example scenes.

use bevy::light::CascadeShadowConfigBuilder;
use bevy::light::DirectionalLightShadowMap;
use bevy::prelude::*;

use crate::constants::CASCADE_FIRST_FAR_BOUND;
use crate::constants::CASCADE_MAX_DISTANCE;
use crate::constants::CASCADE_MIN_DISTANCE;
use crate::constants::CLEAR_COLOR;
use crate::constants::FILL_LIGHT_ILLUMINANCE;
use crate::constants::FILL_LIGHT_POS;
use crate::constants::KEY_LIGHT_ILLUMINANCE;
use crate::constants::KEY_LIGHT_POS;
use crate::constants::KEY_SHADOW_DEPTH_BIAS;
use crate::constants::KEY_SHADOW_NORMAL_BIAS;
use crate::constants::POINT_LIGHT_COLOR;
use crate::constants::POINT_LIGHT_INTENSITY;
use crate::constants::POINT_LIGHT_POS;
use crate::constants::POINT_LIGHT_RANGE;
use crate::constants::SHADOW_MAP_SIZE;
use crate::constants::TARGET;

#[derive(Component)]
struct FairyDustStudioLight;

/// Configuration consumed by the studio-lighting startup system. Defaults
/// match the original hard-coded rig; builder methods on
/// [`crate::builder::StudioLightingBuilder`] override individual fields.
#[derive(Resource, Clone, Copy)]
pub(crate) struct StudioLightingConfig {
    pub(crate) key_light_pos: Vec3,
    pub(crate) aim_at:        Vec3,
}

impl Default for StudioLightingConfig {
    fn default() -> Self {
        Self {
            key_light_pos: KEY_LIGHT_POS,
            aim_at:        TARGET,
        }
    }
}

pub(crate) fn install(app: &mut App, config: StudioLightingConfig) {
    app.insert_resource(ClearColor(CLEAR_COLOR))
        .insert_resource(DirectionalLightShadowMap {
            size: SHADOW_MAP_SIZE,
        })
        .insert_resource(config)
        .add_systems(Startup, spawn_studio_lights);
}

fn spawn_studio_lights(mut commands: Commands, config: Res<StudioLightingConfig>) {
    commands.spawn((
        FairyDustStudioLight,
        DirectionalLight {
            illuminance: KEY_LIGHT_ILLUMINANCE,
            shadows_enabled: true,
            shadow_depth_bias: KEY_SHADOW_DEPTH_BIAS,
            shadow_normal_bias: KEY_SHADOW_NORMAL_BIAS,
            ..default()
        },
        CascadeShadowConfigBuilder {
            minimum_distance: CASCADE_MIN_DISTANCE,
            maximum_distance: CASCADE_MAX_DISTANCE,
            first_cascade_far_bound: CASCADE_FIRST_FAR_BOUND,
            ..default()
        }
        .build(),
        Transform::from_translation(config.key_light_pos).looking_at(config.aim_at, Vec3::Y),
    ));

    commands.spawn((
        FairyDustStudioLight,
        DirectionalLight {
            illuminance: FILL_LIGHT_ILLUMINANCE,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_translation(FILL_LIGHT_POS).looking_at(config.aim_at, Vec3::Y),
    ));

    commands.spawn((
        FairyDustStudioLight,
        PointLight {
            color: POINT_LIGHT_COLOR,
            intensity: POINT_LIGHT_INTENSITY,
            range: POINT_LIGHT_RANGE,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_translation(POINT_LIGHT_POS),
    ));
}
