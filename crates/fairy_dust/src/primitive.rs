//! Capability: simple scene primitives for examples.

use std::f32::consts::FRAC_PI_2;
use std::f32::consts::PI;
use std::sync::Mutex;

use bevy::ecs::system::EntityCommands;
use bevy::prelude::*;
use bevy_diegetic::GlyphSidedness;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;

use crate::constants::CUBE_DEFAULT_COLOR;
use crate::constants::CUBE_DEFAULT_SIZE;
use crate::constants::FACE_TEXT_Z_OFFSET;
use crate::constants::GROUND_PLANE_ALPHA;
use crate::constants::GROUND_PLANE_DEFAULT_COLOR;
use crate::constants::GROUND_PLANE_DEFAULT_SIZE;
use crate::constants::GROUND_PLANE_METALLIC;
use crate::constants::GROUND_PLANE_REFLECTANCE;
use crate::constants::GROUND_PLANE_ROUGHNESS;

/// Names a single face of an axis-aligned cube.
///
/// Used by [`PrimitiveBuilder::face_text`](crate::PrimitiveBuilder::face_text)
/// and [`cube_face_text`] to place a centered `WorldText` label on one face
/// of a cube.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    /// +Z face.
    Front,
    /// -Z face.
    Back,
    /// -X face.
    Left,
    /// +X face.
    Right,
    /// +Y face.
    Top,
    /// -Y face.
    Bottom,
}

impl Face {
    /// Local transform for a label centered on this face of a cube whose
    /// half-extent (size / 2) is `half_extent`. The text is offset slightly
    /// outward to avoid z-fighting with the cube surface.
    fn local_transform(self, half_extent: f32) -> Transform {
        let face_pos = half_extent + FACE_TEXT_Z_OFFSET;
        match self {
            Self::Front => Transform::from_xyz(0.0, 0.0, face_pos),
            Self::Back => {
                Transform::from_xyz(0.0, 0.0, -face_pos).with_rotation(Quat::from_rotation_y(PI))
            },
            Self::Right => Transform::from_xyz(face_pos, 0.0, 0.0)
                .with_rotation(Quat::from_rotation_y(FRAC_PI_2)),
            Self::Left => Transform::from_xyz(-face_pos, 0.0, 0.0)
                .with_rotation(Quat::from_rotation_y(-FRAC_PI_2)),
            Self::Top => Transform::from_xyz(0.0, face_pos, 0.0)
                .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
            Self::Bottom => Transform::from_xyz(0.0, -face_pos, 0.0)
                .with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
        }
    }
}

/// Bundle that renders a single line of [`WorldText`] centered on one face of
/// a cube. Spawn as a child of the cube entity (or independently using the
/// cube's transform as a parent).
///
/// Use this when you spawn a cube manually with `commands.spawn`. For cubes
/// built through [`PrimitiveBuilder`](crate::PrimitiveBuilder), prefer
/// [`PrimitiveBuilder::face_text`](crate::PrimitiveBuilder::face_text).
#[must_use]
pub fn cube_face_text(
    face: Face,
    text: impl Into<String>,
    cube_size: f32,
    text_size: f32,
    color: Color,
) -> impl Bundle {
    (
        WorldText::new(text),
        WorldTextStyle::new(text_size)
            .with_color(color)
            .with_sidedness(GlyphSidedness::OneSided),
        face.local_transform(cube_size * 0.5),
    )
}

/// Mesh kind spawned by [`crate::SprinkleBuilder`] scene helpers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PrimitiveKind {
    /// A square ground plane in the XZ plane.
    GroundPlane,
    /// A cube centered on its transform.
    Cube,
}

#[derive(Clone, Debug)]
pub(crate) struct FaceTextSpec {
    pub(crate) face:      Face,
    pub(crate) text:      String,
    pub(crate) text_size: f32,
    pub(crate) color:     Color,
}

/// Configuration shared by all simple scene primitives.
#[derive(Clone, Debug)]
pub(crate) struct PrimitiveConfig {
    kind:       PrimitiveKind,
    size:       f32,
    color:      Color,
    material:   Option<StandardMaterial>,
    transform:  Option<Transform>,
    face_texts: Vec<FaceTextSpec>,
}

impl PrimitiveConfig {
    pub(crate) const fn ground_plane() -> Self {
        Self {
            kind:       PrimitiveKind::GroundPlane,
            size:       GROUND_PLANE_DEFAULT_SIZE,
            color:      GROUND_PLANE_DEFAULT_COLOR,
            material:   None,
            transform:  None,
            face_texts: Vec::new(),
        }
    }

    pub(crate) const fn cube() -> Self {
        Self {
            kind:       PrimitiveKind::Cube,
            size:       CUBE_DEFAULT_SIZE,
            color:      CUBE_DEFAULT_COLOR,
            material:   None,
            transform:  None,
            face_texts: Vec::new(),
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

    pub(crate) fn push_face_text(&mut self, spec: FaceTextSpec) { self.face_texts.push(spec); }
}

/// Boxed deferred-insert closure applied to a primitive entity at spawn time.
pub(crate) type PrimitiveInsert = Box<dyn FnOnce(&mut EntityCommands) + Send + Sync>;

pub(crate) fn install(app: &mut App, config: PrimitiveConfig, inserts: Vec<PrimitiveInsert>) {
    let inserts_cell: Mutex<Option<Vec<PrimitiveInsert>>> = Mutex::new(Some(inserts));
    app.add_systems(
        Startup,
        move |mut commands: Commands,
              mut meshes: ResMut<Assets<Mesh>>,
              mut materials: ResMut<Assets<StandardMaterial>>| {
            let inserts = inserts_cell
                .lock()
                .expect("primitive install Startup runs once")
                .take()
                .unwrap_or_default();
            spawn_primitive(
                &mut commands,
                &mut meshes,
                &mut materials,
                config.clone(),
                inserts,
            );
        },
    );
}

fn spawn_primitive(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    config: PrimitiveConfig,
    inserts: Vec<PrimitiveInsert>,
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

    let cube_size = config.size;
    let face_texts = config.face_texts;
    let mut entity = commands.spawn((
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(material)),
        transform,
    ));
    for insert in inserts {
        insert(&mut entity);
    }
    if !face_texts.is_empty() {
        entity.with_children(|parent| {
            for spec in face_texts {
                parent.spawn(cube_face_text(
                    spec.face,
                    spec.text,
                    cube_size,
                    spec.text_size,
                    spec.color,
                ));
            }
        });
    }
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
