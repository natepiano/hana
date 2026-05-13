//! Capability: simple scene primitives for examples.

use bevy::prelude::*;

/// Primitive shape spawned by [`crate::SprinkleBuilder`] scene helpers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PrimitiveKind {
    /// A square ground plane in the XZ plane.
    GroundPlane,
    /// A cube centered on its transform.
    Cube,
}

/// Configuration shared by all simple scene primitives.
#[derive(Clone, Debug)]
pub(crate) struct PrimitiveConfig {
    kind:      PrimitiveKind,
    size:      f32,
    color:     Color,
    material:  Option<StandardMaterial>,
    transform: Option<Transform>,
}

const GROUND_PLANE_METALLIC: f32 = 0.0;
const GROUND_PLANE_REFLECTANCE: f32 = 0.45;
const GROUND_PLANE_ROUGHNESS: f32 = 0.40;
const GROUND_PLANE_ALPHA: f32 = 0.78;

impl PrimitiveConfig {
    pub(crate) const fn ground_plane() -> Self {
        Self {
            kind:      PrimitiveKind::GroundPlane,
            size:      8.0,
            color:     Color::srgb(0.125, 0.14, 0.16),
            material:  None,
            transform: None,
        }
    }

    pub(crate) const fn cube() -> Self {
        Self {
            kind:      PrimitiveKind::Cube,
            size:      1.0,
            color:     Color::srgb(0.8, 0.7, 0.6),
            material:  None,
            transform: None,
        }
    }

    pub(crate) const fn set_size(&mut self, size: f32) { self.size = size; }

    pub(crate) const fn set_color(&mut self, color: Color) { self.color = color; }

    pub(crate) fn with_material(mut self, material: StandardMaterial) -> Self {
        self.material = Some(material);
        self
    }

    pub(crate) const fn set_transform(&mut self, transform: Transform) {
        self.transform = Some(transform);
    }
}

pub(crate) fn install(app: &mut App, config: PrimitiveConfig) {
    app.add_systems(
        Startup,
        move |mut commands: Commands,
              mut meshes: ResMut<Assets<Mesh>>,
              mut materials: ResMut<Assets<StandardMaterial>>| {
            spawn_primitive(&mut commands, &mut meshes, &mut materials, config.clone());
        },
    );
}

fn spawn_primitive(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    config: PrimitiveConfig,
) {
    let mesh = match config.kind {
        PrimitiveKind::GroundPlane => {
            Mesh::from(Plane3d::default().mesh().size(config.size, config.size))
        },
        PrimitiveKind::Cube => Mesh::from(Cuboid::from_size(Vec3::splat(config.size))),
    };
    let material = config
        .material
        .unwrap_or_else(|| default_material(config.kind, config.color));
    let transform = config.transform.unwrap_or_else(|| match config.kind {
        PrimitiveKind::GroundPlane => Transform::default(),
        PrimitiveKind::Cube => Transform::from_xyz(0.0, config.size * 0.5, 0.0),
    });

    commands.spawn((
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(material)),
        transform,
    ));
}

fn default_material(kind: PrimitiveKind, color: Color) -> StandardMaterial {
    match kind {
        PrimitiveKind::GroundPlane => StandardMaterial {
            base_color: color.with_alpha(GROUND_PLANE_ALPHA),
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            metallic: GROUND_PLANE_METALLIC,
            reflectance: GROUND_PLANE_REFLECTANCE,
            perceptual_roughness: GROUND_PLANE_ROUGHNESS,
            ..default()
        },
        PrimitiveKind::Cube => StandardMaterial {
            base_color: color,
            ..default()
        },
    }
}
