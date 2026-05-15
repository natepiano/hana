//! Capability: simple studio lighting for example scenes.

use bevy::light::CascadeShadowConfigBuilder;
use bevy::light::DirectionalLightShadowMap;
use bevy::light::GlobalAmbientLight;
use bevy::prelude::*;

use crate::constants::AMBIENT_BRIGHTNESS;
use crate::constants::AMBIENT_COLOR;
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

pub(crate) fn install(app: &mut App) {
    app.insert_resource(ClearColor(CLEAR_COLOR))
        .insert_resource(DirectionalLightShadowMap {
            size: SHADOW_MAP_SIZE,
        })
        .insert_resource(GlobalAmbientLight {
            color:                      AMBIENT_COLOR,
            brightness:                 AMBIENT_BRIGHTNESS,
            affects_lightmapped_meshes: false,
        })
        .add_systems(Startup, spawn_studio_lights);
}

fn spawn_studio_lights(mut commands: Commands) {
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
        Transform::from_translation(KEY_LIGHT_POS).looking_at(TARGET, Vec3::Y),
    ));

    commands.spawn((
        FairyDustStudioLight,
        DirectionalLight {
            illuminance: FILL_LIGHT_ILLUMINANCE,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_translation(FILL_LIGHT_POS).looking_at(TARGET, Vec3::Y),
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
