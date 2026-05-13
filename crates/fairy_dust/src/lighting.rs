//! Capability: simple studio lighting for example scenes.

use bevy::light::CascadeShadowConfigBuilder;
use bevy::light::DirectionalLightShadowMap;
use bevy::light::GlobalAmbientLight;
use bevy::prelude::*;

const SHADOW_MAP_SIZE: usize = 4096;
const TARGET: Vec3 = Vec3::new(0.0, 0.45, 0.0);
const GROUND_ACCENT_POS: Vec3 = Vec3::new(-2.0, 1.15, 1.85);

const KEY_LIGHT_POS: Vec3 = Vec3::new(-3.5, 7.0, 4.8);
const FILL_LIGHT_POS: Vec3 = Vec3::new(4.5, 4.0, -3.5);

#[derive(Component)]
struct FairyDustStudioLight;

pub(crate) fn install(app: &mut App) {
    app.insert_resource(ClearColor(Color::srgb(0.012, 0.014, 0.018)))
        .insert_resource(DirectionalLightShadowMap {
            size: SHADOW_MAP_SIZE,
        })
        .insert_resource(GlobalAmbientLight {
            color:                      Color::srgb(0.55, 0.62, 0.76),
            brightness:                 95.0,
            affects_lightmapped_meshes: false,
        })
        .add_systems(Startup, spawn_studio_lights);
}

fn spawn_studio_lights(mut commands: Commands) {
    commands.spawn((
        FairyDustStudioLight,
        DirectionalLight {
            illuminance: 13_500.0,
            shadows_enabled: true,
            shadow_depth_bias: 0.03,
            shadow_normal_bias: 0.7,
            ..default()
        },
        CascadeShadowConfigBuilder {
            minimum_distance: 0.1,
            maximum_distance: 18.0,
            first_cascade_far_bound: 6.0,
            ..default()
        }
        .build(),
        Transform::from_translation(KEY_LIGHT_POS).looking_at(TARGET, Vec3::Y),
    ));

    commands.spawn((
        FairyDustStudioLight,
        DirectionalLight {
            illuminance: 1_400.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_translation(FILL_LIGHT_POS).looking_at(TARGET, Vec3::Y),
    ));

    commands.spawn((
        FairyDustStudioLight,
        PointLight {
            color: Color::srgb(0.45, 0.68, 1.0),
            intensity: 1_900.0,
            range: 6.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_translation(GROUND_ACCENT_POS),
    ));
}
