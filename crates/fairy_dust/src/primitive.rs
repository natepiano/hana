//! Capability: simple scene primitives for examples.

use std::borrow::Cow;
use std::f32::consts::FRAC_PI_2;
use std::f32::consts::PI;
use std::sync::Mutex;
use std::sync::PoisonError;

use bevy::ecs::system::EntityCommands;
use bevy::prelude::*;
use bevy_diegetic::AlignX;
use bevy_diegetic::AlignY;
use bevy_diegetic::Anchor;
use bevy_diegetic::DiegeticPanel;
use bevy_diegetic::DiegeticPanelCommands;
use bevy_diegetic::Direction;
use bevy_diegetic::El;
use bevy_diegetic::GlyphShadowMode;
use bevy_diegetic::GlyphSidedness;
use bevy_diegetic::InvalidSize;
use bevy_diegetic::LayoutBuilder;
use bevy_diegetic::LayoutTextStyle;
use bevy_diegetic::LayoutTree;
use bevy_diegetic::Padding;
use bevy_diegetic::Sizing;
use bevy_diegetic::TextAlign;
use bevy_diegetic::Unit;
use bevy_diegetic::WorldText;
use bevy_diegetic::WorldTextStyle;
use bevy_diegetic::default_panel_material;

use crate::constants::CUBE_DEFAULT_COLOR;
use crate::constants::CUBE_DEFAULT_SIZE;
use crate::constants::CUBE_FACE_LABEL_SIZE;
use crate::constants::CUBE_FACE_PANEL_BLUE;
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
        CubeFaceLabel,
        WorldText::new(text),
        WorldTextStyle::new(text_size)
            .with_color(color)
            .with_sidedness(GlyphSidedness::OneSided),
        face.local_transform(cube_size * 0.5),
    )
}

/// Canonical single-line blue label centered on a cube face.
#[must_use]
pub fn cube_face_label(face: Face, text: impl Into<String>, cube_size: f32) -> impl Bundle {
    cube_face_text(
        face,
        text,
        cube_size,
        CUBE_FACE_LABEL_SIZE,
        CUBE_FACE_PANEL_BLUE,
    )
}

/// Local transform for a cube-mounted panel centered on `face`.
#[must_use]
pub fn cube_face_transform(face: Face, cube_size: f32) -> Transform {
    face.local_transform(cube_size * 0.5)
}

/// Transparent unlit material for cube-mounted face panels.
#[must_use]
pub fn cube_face_panel_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::NONE,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default_panel_material()
    }
}

/// Activity state for cube face panel content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CubeFacePanelActivity {
    /// Render with the active body size.
    Active,
    /// Render with the idle body size.
    Idle,
}

/// Text content for a canonical cube face panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CubeFacePanelContent {
    /// Panel title.
    pub title:    Cow<'static, str>,
    /// Body rows.
    pub rows:     Vec<Cow<'static, str>>,
    /// Activity state.
    pub activity: CubeFacePanelActivity,
}

impl CubeFacePanelContent {
    /// Creates idle panel content.
    #[must_use]
    pub fn idle(
        title: impl Into<Cow<'static, str>>,
        rows: impl IntoIterator<Item = impl Into<Cow<'static, str>>>,
    ) -> Self {
        Self {
            title:    title.into(),
            rows:     rows.into_iter().map(Into::into).collect(),
            activity: CubeFacePanelActivity::Idle,
        }
    }

    /// Creates active panel content.
    #[must_use]
    pub fn active(
        title: impl Into<Cow<'static, str>>,
        rows: impl IntoIterator<Item = impl Into<Cow<'static, str>>>,
    ) -> Self {
        Self {
            title:    title.into(),
            rows:     rows.into_iter().map(Into::into).collect(),
            activity: CubeFacePanelActivity::Active,
        }
    }
}

/// Visual sizing for canonical cube face panels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubeFacePanelStyle {
    /// Square panel edge length.
    pub size:             f32,
    /// Panel inner padding.
    pub padding:          f32,
    /// Gap between title/body rows.
    pub row_gap:          f32,
    /// Title text size.
    pub title_size:       f32,
    /// Idle body text size.
    pub body_size:        f32,
    /// Active body text size.
    pub active_body_size: f32,
    /// Text color.
    pub color:            Color,
}

impl CubeFacePanelStyle {
    /// Canonical panel style scaled for a cube of `cube_size`.
    #[must_use]
    pub const fn for_cube(cube_size: f32) -> Self {
        Self {
            size:             cube_size * 0.88,
            padding:          cube_size * 0.06,
            row_gap:          cube_size * 0.02,
            title_size:       72.0,
            body_size:        44.0,
            active_body_size: 52.0,
            color:            CUBE_FACE_PANEL_BLUE,
        }
    }
}

/// Builds a cube face panel with the canonical transparent material and tree.
///
/// # Errors
///
/// Returns [`InvalidSize`] when `style.size` is not a positive, finite value.
pub fn cube_face_panel(
    style: CubeFacePanelStyle,
    content: CubeFacePanelContent,
) -> Result<DiegeticPanel, InvalidSize> {
    cube_face_panel_with_tree(style.size, cube_face_panel_tree(style, content))
}

/// Builds a transparent cube face panel from a caller-authored layout tree.
///
/// # Errors
///
/// Returns [`InvalidSize`] when `size` is not a positive, finite value.
pub fn cube_face_panel_with_tree(
    size: f32,
    tree: LayoutTree,
) -> Result<DiegeticPanel, InvalidSize> {
    let transparent = cube_face_panel_material();
    DiegeticPanel::world()
        .size(size, size)
        .font_unit(Unit::Millimeters)
        .anchor(Anchor::Center)
        .material(transparent.clone())
        .text_material(transparent)
        .with_tree(tree)
        .build()
}

/// Builds the layout tree for a cube face panel.
#[must_use]
pub fn cube_face_panel_tree(
    style: CubeFacePanelStyle,
    content: CubeFacePanelContent,
) -> LayoutTree {
    let mut builder = LayoutBuilder::with_root(
        El::new()
            .width(Sizing::fixed(style.size))
            .height(Sizing::fixed(style.size))
            .direction(Direction::TopToBottom)
            .child_alignment(AlignX::Center, AlignY::Center)
            .child_gap(style.row_gap)
            .padding(Padding::all(style.padding))
            .clip(),
    );

    let title = LayoutTextStyle::new(style.title_size)
        .with_color(style.color)
        .with_align(TextAlign::Center)
        .with_shadow_mode(GlyphShadowMode::None);
    builder.text(content.title, title);

    let body_size = match content.activity {
        CubeFacePanelActivity::Active => style.active_body_size,
        CubeFacePanelActivity::Idle => style.body_size,
    };
    let body = LayoutTextStyle::new(body_size)
        .with_color(style.color)
        .with_align(TextAlign::Center)
        .with_shadow_mode(GlyphShadowMode::None);
    for row in content.rows {
        builder.text(row, body.clone());
    }

    builder.build()
}

/// Replaces a cube face panel's layout tree.
pub fn set_cube_face_panel_tree(commands: &mut Commands, entity: Entity, tree: LayoutTree) {
    commands.set_tree(entity, tree);
}

/// Mesh kind spawned by [`crate::SprinkleBuilder`] scene helpers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrimitiveKind {
    /// A square ground plane in the XZ plane.
    GroundPlane,
    /// A cube centered on its transform.
    Cube,
}

/// Identity marker inserted on cubes spawned by Fairy Dust primitive builders.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct FairyDustCube;

/// Marker inserted on the per-face [`WorldText`] labels of a cube.
///
/// Lets a caller retarget only the cube's face labels (e.g. `Query<&mut
/// WorldText, With<CubeFaceLabel>>`) without also matching panel-child or other
/// `WorldText`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct CubeFaceLabel;

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
type PrimitiveInsert = Box<dyn FnOnce(&mut EntityCommands) + Send + Sync>;

pub(crate) fn install(app: &mut App, config: PrimitiveConfig, inserts: Vec<PrimitiveInsert>) {
    let inserts_cell: Mutex<Option<Vec<PrimitiveInsert>>> = Mutex::new(Some(inserts));
    app.add_systems(
        Startup,
        move |mut commands: Commands,
              mut meshes: ResMut<Assets<Mesh>>,
              mut materials: ResMut<Assets<StandardMaterial>>| {
            let inserts = inserts_cell
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
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
    if matches!(config.kind, PrimitiveKind::Cube) {
        entity.insert(FairyDustCube);
    }
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
