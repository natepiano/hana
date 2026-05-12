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

impl PrimitiveConfig {
    pub(crate) const fn ground_plane() -> Self {
        Self {
            kind:      PrimitiveKind::GroundPlane,
            size:      8.0,
            color:     Color::srgb(0.28, 0.42, 0.34),
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
    let material = config.material.unwrap_or_else(|| StandardMaterial {
        base_color: config.color,
        ..default()
    });
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
