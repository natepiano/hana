//! Standalone world-space text component and rendering system.

use bevy::prelude::*;

use super::glyph_quad::GlyphQuadData;
use super::glyph_quad::build_glyph_mesh;
use super::msdf_material::MsdfTextMaterial;
use super::text_renderer::ShapedTextCache;
use super::text_renderer::TextShapingContext;
use super::text_renderer::shape_text_cached;
use crate::layout::TextStyle;
use crate::text::DEFAULT_CANONICAL_SIZE;
use crate::text::FontRegistry;
use crate::text::GlyphKey;
use crate::text::MsdfAtlas;

/// Standalone MSDF text rendered in world space.
///
/// Attach to any entity with a [`Transform`] to place text in the 3D scene.
/// Style is controlled by the required [`TextStyle`] component (added
/// automatically with defaults if not specified).
///
/// ```ignore
/// commands.spawn((
///     WorldText::new("Hello, world!"),
///     Transform::from_xyz(0.0, 2.0, 0.0),
/// ));
///
/// // With custom style:
/// commands.spawn((
///     WorldText::new("Styled"),
///     TextStyle::new().with_size(24.0).with_color(Color::RED),
///     Transform::from_xyz(0.0, 2.0, 0.0),
/// ));
/// ```
#[derive(Component, Clone, Debug)]
#[require(TextStyle, Transform, Visibility)]
pub struct WorldText(pub String);

impl WorldText {
    /// Creates a new world text with the given string.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self { Self(text.into()) }
}

/// Marker for mesh entities spawned by the world text renderer.
#[derive(Component)]
pub(super) struct WorldTextMesh;

/// Renders [`WorldText`] entities as MSDF glyph meshes.
///
/// Rebuilds the text mesh whenever the [`WorldText`] or [`TextStyle`]
/// component changes.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_world_text(
    texts: Query<(Entity, &WorldText, &TextStyle), Or<(Changed<WorldText>, Changed<TextStyle>)>>,
    old_meshes: Query<(Entity, &ChildOf), With<WorldTextMesh>>,
    mut atlas: ResMut<MsdfAtlas>,
    font_registry: Res<FontRegistry>,
    shaping_cx: Res<TextShapingContext>,
    mut cache: ResMut<ShapedTextCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MsdfTextMaterial>>,
    mut commands: Commands,
) {
    for (entity, world_text, style) in &texts {
        // Despawn previous mesh children.
        for (mesh_entity, child_of) in &old_meshes {
            if child_of.parent() == entity {
                commands.entity(mesh_entity).despawn();
            }
        }

        if world_text.0.is_empty() {
            continue;
        }

        // Shape text and build quads in entity-local coordinates.
        let quads = shape_world_text(
            &world_text.0,
            style,
            &font_registry,
            &mut atlas,
            &shaping_cx,
            &mut cache,
        );

        if quads.is_empty() {
            continue;
        }

        let Some(atlas_image) = atlas.image_handle().cloned() else {
            continue;
        };

        #[allow(clippy::cast_possible_truncation)]
        let material_handle = materials.add(MsdfTextMaterial::new(
            LinearRgba::WHITE,
            atlas.sdf_range() as f32,
            atlas.width(),
            atlas.height(),
            atlas_image,
        ));

        let mesh = build_glyph_mesh(&quads);
        let mesh_handle = meshes.add(mesh);

        commands.entity(entity).with_child((
            WorldTextMesh,
            Mesh3d(mesh_handle),
            MeshMaterial3d(material_handle),
            Transform::IDENTITY,
        ));
    }
}

/// Shapes text and produces glyph quads in entity-local coordinates.
///
/// Unlike panel text, standalone text has no layout bounds or panel scale.
/// Glyphs are positioned relative to the origin, offset by the anchor point,
/// with a fixed scale (1 layout unit = 0.01 world units by default).
fn shape_world_text(
    text: &str,
    style: &TextStyle,
    font_registry: &FontRegistry,
    atlas: &mut MsdfAtlas,
    shaping_cx: &TextShapingContext,
    cache: &mut ShapedTextCache,
) -> Vec<GlyphQuadData> {
    // Convert TextStyle to TextConfig for shaping (same underlying fields).
    let config = style.as_layout_config();

    let shaped = shape_text_cached(text, &config, font_registry, shaping_cx, cache);

    let font_data = crate::text::EMBEDDED_FONT;
    let linear: LinearRgba = style.color().into();
    let color_arr = [linear.red, linear.green, linear.blue, linear.alpha];

    #[allow(clippy::cast_precision_loss)]
    let em_scale = style.size() / DEFAULT_CANONICAL_SIZE as f32;

    // Measure total dimensions for anchor offset.
    let mut max_x = 0.0_f32;
    let mut max_y = 0.0_f32;
    for sg in &shaped.glyphs {
        let glyph_key = GlyphKey {
            font_id:     style.font_id(),
            glyph_index: sg.glyph_id,
        };
        if let Some(metrics) = atlas.peek(glyph_key) {
            #[allow(clippy::cast_precision_loss)]
            let right =
                sg.x + metrics.bearing_x * style.size() + metrics.pixel_width as f32 * em_scale;
            #[allow(clippy::cast_precision_loss)]
            let bottom = sg.baseline - sg.y - metrics.bearing_y * style.size()
                + metrics.pixel_height as f32 * em_scale;
            max_x = max_x.max(right);
            max_y = max_y.max(bottom);
        }
    }

    // Scale: layout units to world units.
    let scale = 0.01_f32;

    // Anchor offset in layout units.
    let (anchor_x, anchor_y) = anchor_offset(style.anchor(), max_x, max_y);

    let mut quads = Vec::with_capacity(shaped.glyphs.len());
    for sg in &shaped.glyphs {
        let glyph_key = GlyphKey {
            font_id:     style.font_id(),
            glyph_index: sg.glyph_id,
        };

        let Some(metrics) = atlas.get_or_insert(glyph_key, font_data) else {
            continue;
        };

        #[allow(clippy::cast_precision_loss)]
        let quad_w = metrics.pixel_width as f32 * em_scale;
        #[allow(clippy::cast_precision_loss)]
        let quad_h = metrics.pixel_height as f32 * em_scale;

        let quad_x = sg.x + metrics.bearing_x * style.size() - anchor_x;
        let quad_y = -(sg.baseline - sg.y - metrics.bearing_y * style.size() - anchor_y);

        quads.push(GlyphQuadData {
            position: [quad_x * scale, quad_y * scale, 0.0],
            size:     [quad_w * scale, quad_h * scale],
            uv_rect:  metrics.uv_rect,
            color:    color_arr,
        });
    }

    quads
}

/// Returns the anchor offset in layout units for centering/alignment.
fn anchor_offset(anchor: crate::layout::TextAnchor, width: f32, height: f32) -> (f32, f32) {
    use crate::layout::TextAnchor;
    let x = match anchor {
        TextAnchor::TopLeft | TextAnchor::CenterLeft | TextAnchor::BottomLeft => 0.0,
        TextAnchor::TopCenter | TextAnchor::Center | TextAnchor::BottomCenter => width * 0.5,
        TextAnchor::TopRight | TextAnchor::CenterRight | TextAnchor::BottomRight => width,
    };
    let y = match anchor {
        TextAnchor::TopLeft | TextAnchor::TopCenter | TextAnchor::TopRight => 0.0,
        TextAnchor::CenterLeft | TextAnchor::Center | TextAnchor::CenterRight => height * 0.5,
        TextAnchor::BottomLeft | TextAnchor::BottomCenter | TextAnchor::BottomRight => height,
    };
    (x, y)
}
