//! Capability: simple studio lighting for example scenes.

use bevy::light::CascadeShadowConfigBuilder;
use bevy::light::DirectionalLightShadowMap;
use bevy::light::GlobalAmbientLight;
use bevy::prelude::*;

// ambient
const AMBIENT_BRIGHTNESS: f32 = 95.0;
const AMBIENT_COLOR: Color = Color::srgb(0.55, 0.62, 0.76);

// cascade shadow
const CASCADE_FIRST_FAR_BOUND: f32 = 6.0;
const CASCADE_MAX_DISTANCE: f32 = 18.0;
const CASCADE_MIN_DISTANCE: f32 = 0.1;

// clear color
const CLEAR_COLOR: Color = Color::srgb(0.012, 0.014, 0.018);

// fill light
const FILL_LIGHT_ILLUMINANCE: f32 = 1_400.0;
const FILL_LIGHT_POS: Vec3 = Vec3::new(4.5, 4.0, -3.5);

// key light
const KEY_LIGHT_ILLUMINANCE: f32 = 13_500.0;
const KEY_LIGHT_POS: Vec3 = Vec3::new(-3.5, 7.0, 4.8);
const KEY_SHADOW_DEPTH_BIAS: f32 = 0.03;
const KEY_SHADOW_NORMAL_BIAS: f32 = 0.7;

// point light
const POINT_LIGHT_COLOR: Color = Color::srgb(0.45, 0.68, 1.0);
const POINT_LIGHT_INTENSITY: f32 = 1_900.0;
const POINT_LIGHT_POS: Vec3 = Vec3::new(-2.0, 1.15, 1.85);
const POINT_LIGHT_RANGE: f32 = 6.0;

// shadow map
const SHADOW_MAP_SIZE: usize = 4096;

// target
const TARGET: Vec3 = Vec3::new(0.0, 0.45, 0.0);

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
