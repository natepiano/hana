//! Capability: simple studio lighting for example scenes.

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::RenderLayers;
use bevy::light::CascadeShadowConfig;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::light::DirectionalLightShadowMap;
use bevy::prelude::*;

use crate::constants::CASCADE_COUNT;
use crate::constants::CASCADE_FIRST_BOUND_RATIO;
use crate::constants::CASCADE_FIRST_FAR_BOUND;
use crate::constants::CASCADE_FIT_RADIUS_MULTIPLE;
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

/// Marks the key directional light whose shadow cascade is fitted to the scene
/// the first time the scene's geometry exists. Removed once the fit runs, so
/// [`fit_cascade_to_scene`] is a one-shot.
#[derive(Component)]
struct FairyDustAutoCascade;

/// Startup set that spawns Fairy Dust's studio lighting rig.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct FairyDustStudioLightingSet;

/// Configuration consumed by the studio-lighting startup system. Defaults
/// match the original hard-coded rig; builder methods on
/// [`crate::builder::StudioLightingBuilder`] override individual fields.
#[derive(Resource, Clone, Copy)]
pub(crate) struct StudioLightingConfig {
    pub(crate) key_light_pos:         Vec3,
    pub(crate) aim_at:                Vec3,
    pub(crate) key_light_illuminance: f32,
}

impl Default for StudioLightingConfig {
    fn default() -> Self {
        Self {
            key_light_pos:         KEY_LIGHT_POS,
            aim_at:                TARGET,
            key_light_illuminance: KEY_LIGHT_ILLUMINANCE,
        }
    }
}

pub(crate) fn install(app: &mut App, config: StudioLightingConfig) {
    app.insert_resource(ClearColor(CLEAR_COLOR))
        .insert_resource(DirectionalLightShadowMap {
            size: SHADOW_MAP_SIZE,
        })
        .insert_resource(config)
        .add_systems(
            Startup,
            spawn_studio_lights.in_set(FairyDustStudioLightingSet),
        )
        .add_systems(Update, fit_cascade_to_scene);
}

fn spawn_studio_lights(mut commands: Commands, config: Res<StudioLightingConfig>) {
    commands.spawn((
        FairyDustStudioLight,
        FairyDustAutoCascade,
        DirectionalLight {
            illuminance: config.key_light_illuminance,
            shadow_maps_enabled: true,
            shadow_depth_bias: KEY_SHADOW_DEPTH_BIAS,
            shadow_normal_bias: KEY_SHADOW_NORMAL_BIAS,
            ..default()
        },
        CascadeShadowConfigBuilder {
            num_cascades: CASCADE_COUNT,
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
            shadow_maps_enabled: false,
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
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_translation(POINT_LIGHT_POS),
    ));
}

/// The 8 corner sign-patterns of a unit AABB, used to walk a local [`Aabb`]
/// into world space through a [`GlobalTransform`].
const AABB_CORNER_SIGNS: [Vec3; 8] = [
    Vec3::new(-1.0, -1.0, -1.0),
    Vec3::new(1.0, -1.0, -1.0),
    Vec3::new(-1.0, 1.0, -1.0),
    Vec3::new(1.0, 1.0, -1.0),
    Vec3::new(-1.0, -1.0, 1.0),
    Vec3::new(1.0, -1.0, 1.0),
    Vec3::new(-1.0, 1.0, 1.0),
    Vec3::new(1.0, 1.0, 1.0),
];

/// Fits the key light's shadow cascade to the scene the first time its meshes
/// have an [`Aabb`], then removes [`FairyDustAutoCascade`] so it runs exactly
/// once. The cascade is sized to the bounding sphere of only the meshes the key
/// light actually shadows — those sharing its [`RenderLayers`] — so screen-space
/// UI panels (which live on other layers, far out in world space) are excluded.
/// Keeping `maximum_distance` to what the scene needs packs the shadow map over
/// less area, so shadows stay sharp instead of being either clipped (cascade too
/// small) or coarse (cascade too large). [`Aabb`]s are computed in `PostUpdate`,
/// so this can't run at startup.
fn fit_cascade_to_scene(
    mut commands: Commands,
    mut light: Query<
        (Entity, &mut CascadeShadowConfig, Option<&RenderLayers>),
        With<FairyDustAutoCascade>,
    >,
    meshes: Query<(&Aabb, &GlobalTransform, Option<&RenderLayers>), With<Mesh3d>>,
) {
    let Ok((light_entity, mut cascade, light_layers)) = light.single_mut() else {
        return;
    };
    let light_layers = light_layers.cloned().unwrap_or_default();
    let Some(radius) = scene_bounding_radius(&meshes, &light_layers) else {
        return;
    };
    let maximum_distance = radius * CASCADE_FIT_RADIUS_MULTIPLE;
    *cascade = CascadeShadowConfigBuilder {
        num_cascades: CASCADE_COUNT,
        minimum_distance: CASCADE_MIN_DISTANCE,
        maximum_distance,
        first_cascade_far_bound: maximum_distance * CASCADE_FIRST_BOUND_RATIO,
        ..default()
    }
    .build();
    commands
        .entity(light_entity)
        .remove::<FairyDustAutoCascade>();
}

/// World-space bounding-sphere radius of the meshes whose [`RenderLayers`]
/// intersect `light_layers`, or `None` when none has an [`Aabb`] yet. Each
/// mesh's local [`Aabb`] is walked into world space through its
/// [`GlobalTransform`]; the radius is the half-diagonal of the union box.
fn scene_bounding_radius(
    meshes: &Query<(&Aabb, &GlobalTransform, Option<&RenderLayers>), With<Mesh3d>>,
    light_layers: &RenderLayers,
) -> Option<f32> {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    let mut found = false;
    for (aabb, global_transform, mesh_layers) in meshes {
        let mesh_layers = mesh_layers.cloned().unwrap_or_default();
        if !light_layers.intersects(&mesh_layers) {
            continue;
        }
        let local_center = Vec3::from(aabb.center);
        let half = Vec3::from(aabb.half_extents);
        for sign in AABB_CORNER_SIGNS {
            let world = global_transform.transform_point(local_center + sign * half);
            min = min.min(world);
            max = max.max(world);
            found = true;
        }
    }
    if !found {
        return None;
    }
    Some(((max - min) * 0.5).length())
}
