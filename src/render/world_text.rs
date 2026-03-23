//! Standalone world-space text component and rendering system.

use std::collections::HashMap;

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::glyph_quad::GlyphQuadData;
use super::glyph_quad::build_glyph_mesh;
use super::glyph_quad::clip_overlapping_quads;
use super::msdf_material::MsdfTextMaterial;
use super::text_renderer::ShapedTextCache;
use super::text_renderer::TextShapingContext;
use super::text_renderer::shape_text_cached;
use crate::layout::GlyphRenderMode;
use crate::layout::GlyphShadowMode;
use crate::layout::TextStyle;
use crate::text::Font;
use crate::text::FontId;
use crate::text::FontRegistry;
use crate::text::GlyphKey;
use crate::text::MsdfAtlas;

/// Computed layout data for a [`WorldText`] entity, populated by the
/// renderer. Used by the typography debug overlay to draw glyph bounding
/// boxes and metric lines aligned with the rendered text.
///
/// Only available when the `typography_overlay` feature is enabled.
#[cfg(feature = "typography_overlay")]
#[derive(Component, Clone, Debug)]
pub struct ComputedWorldText {
    /// Anchor offset X in layout units (matches the renderer's anchor).
    pub anchor_x:    f32,
    /// Anchor offset Y in layout units (matches the renderer's anchor).
    pub anchor_y:    f32,
    /// Total text width in layout units (rightmost MSDF quad extent).
    pub text_width:  f32,
    /// Per-glyph rendered quad rects in world units (after anchor, scale,
    /// and overlap clipping). Each `[f32; 4]` is `[x, y, width, height]`
    /// where `(x, y)` is the top-left corner. These are the exact rects
    /// that the MSDF renderer draws — the overlay uses them directly.
    pub glyph_rects: Vec<[f32; 4]>,
}

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
#[derive(Component, Clone, Debug, Reflect)]
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

/// Marker for shadow proxy entities spawned by the world text renderer.
#[derive(Component)]
pub(super) struct WorldTextShadowProxy;

/// Renders [`WorldText`] entities as MSDF glyph meshes.
///
/// Rebuilds the text mesh whenever the [`WorldText`] or [`TextStyle`]
/// component changes.
#[allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::too_many_lines
)]
pub(super) fn render_world_text(
    changed_texts: Query<
        (Entity, &WorldText, &TextStyle),
        Or<(Changed<WorldText>, Changed<TextStyle>)>,
    >,
    all_texts: Query<(Entity, &WorldText, &TextStyle)>,
    old_meshes: Query<(Entity, &ChildOf), Or<(With<WorldTextMesh>, With<WorldTextShadowProxy>)>>,
    mut atlas: ResMut<MsdfAtlas>,
    font_registry: Res<FontRegistry>,
    shaping_cx: Res<TextShapingContext>,
    mut cache: ResMut<ShapedTextCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MsdfTextMaterial>>,
    mut commands: Commands,
    ready: Res<super::text_renderer::GlyphsReady>,
) {
    let rebuild_all = ready.0;

    let texts_iter: Vec<_> = if rebuild_all {
        all_texts.iter().collect()
    } else {
        changed_texts.iter().collect()
    };

    for (entity, world_text, style) in texts_iter {
        if world_text.0.is_empty() {
            // Despawn old meshes for empty text.
            for (mesh_entity, child_of) in &old_meshes {
                if child_of.parent() == entity {
                    commands.entity(mesh_entity).despawn();
                }
            }
            continue;
        }

        // Shape text and build quads in entity-local coordinates.
        let (quads, anchor_x, anchor_y, text_width, glyph_rects) = shape_world_text(
            &world_text.0,
            style,
            &font_registry,
            &mut atlas,
            &shaping_cx,
            &mut cache,
        );

        // Store computed layout data for the typography overlay.
        #[cfg(feature = "typography_overlay")]
        commands.entity(entity).insert(ComputedWorldText {
            anchor_x,
            anchor_y,
            text_width,
            glyph_rects,
        });
        #[cfg(not(feature = "typography_overlay"))]
        {
            let _ = (anchor_x, anchor_y, text_width, glyph_rects);
        }

        // Group quads by page.
        let mut page_quads: HashMap<u32, Vec<GlyphQuadData>> = HashMap::new();
        for (page_index, quad) in quads {
            page_quads.entry(page_index).or_default().push(quad);
        }

        let total_quads: usize = page_quads.values().map(Vec::len).sum();

        // Only replace old meshes if we have content.
        if total_quads == 0 {
            continue;
        }

        // Despawn previous mesh children — safe because we have content.
        for (mesh_entity, child_of) in &old_meshes {
            if child_of.parent() == entity {
                commands.entity(mesh_entity).despawn();
            }
        }

        let is_invisible = style.render_mode() == GlyphRenderMode::Invisible;
        let needs_proxy = if is_invisible {
            style.shadow_mode() != GlyphShadowMode::None
        } else {
            matches!(
                style.shadow_mode(),
                GlyphShadowMode::Text | GlyphShadowMode::PunchOut
            )
        };
        let suppress_shadow =
            is_invisible || needs_proxy || style.shadow_mode() == GlyphShadowMode::None;

        for (page_index, pq) in &page_quads {
            let Some(page_image) = atlas.image_handle(*page_index).cloned() else {
                continue;
            };

            let mesh = build_glyph_mesh(pq);
            let mesh_handle = meshes.add(mesh);

            // Spawn visible mesh (skip for Invisible render mode).
            if !is_invisible {
                let render_mode_u32 = style.render_mode() as u32;

                #[allow(clippy::cast_possible_truncation)]
                let material_handle = materials.add(super::msdf_material::msdf_text_material(
                    atlas.sdf_range() as f32,
                    atlas.width(),
                    atlas.height(),
                    page_image.clone(),
                    0.0,
                    render_mode_u32,
                ));

                if suppress_shadow {
                    commands.entity(entity).with_child((
                        WorldTextMesh,
                        NotShadowCaster,
                        Mesh3d(mesh_handle.clone()),
                        MeshMaterial3d(material_handle),
                        Transform::IDENTITY,
                    ));
                } else {
                    commands.entity(entity).with_child((
                        WorldTextMesh,
                        Mesh3d(mesh_handle.clone()),
                        MeshMaterial3d(material_handle),
                        Transform::IDENTITY,
                    ));
                }
            }

            // Shadow proxy for shaped shadows (or any shadow when Invisible).
            if needs_proxy {
                let shadow_render_mode = match style.shadow_mode() {
                    GlyphShadowMode::SolidQuad => GlyphRenderMode::SolidQuad as u32,
                    GlyphShadowMode::PunchOut => GlyphRenderMode::PunchOut as u32,
                    GlyphShadowMode::None | GlyphShadowMode::Text => GlyphRenderMode::Text as u32,
                };

                #[allow(clippy::cast_possible_truncation)]
                let proxy_material =
                    materials.add(super::msdf_material::msdf_shadow_proxy_material(
                        atlas.sdf_range() as f32,
                        atlas.width(),
                        atlas.height(),
                        page_image,
                        0.0,
                        shadow_render_mode,
                    ));

                commands.entity(entity).with_child((
                    WorldTextShadowProxy,
                    Mesh3d(mesh_handle),
                    MeshMaterial3d(proxy_material),
                    Transform::IDENTITY,
                ));
            }
        }
    }
}

/// Shapes text and produces glyph quads in entity-local coordinates.
///
/// Unlike panel text, standalone text has no layout bounds or panel scale.
/// Glyphs are positioned relative to the origin, offset by the anchor point,
/// with a fixed scale (1 layout unit = 0.01 world units by default).
/// Returns `(quads, anchor_x, anchor_y, text_width, glyph_rects)`.
///
/// `glyph_rects` contains `[x, y, width, height]` for each rendered glyph
/// quad in world units (after anchor offset, scale, and overlap clipping).
/// Used by the typography overlay to draw bounding boxes that exactly match
/// the rendered MSDF quads.
#[allow(clippy::type_complexity)]
fn shape_world_text(
    text: &str,
    style: &TextStyle,
    font_registry: &FontRegistry,
    atlas: &mut MsdfAtlas,
    shaping_cx: &TextShapingContext,
    cache: &mut ShapedTextCache,
) -> (Vec<(u32, GlyphQuadData)>, f32, f32, f32, Vec<[f32; 4]>) {
    // Convert TextStyle to TextConfig for shaping (same underlying fields).
    let config = style.as_layout_config();

    let shaped = shape_text_cached(text, &config, font_registry, shaping_cx, cache);

    let font_data = font_registry
        .font(FontId(style.font_id()))
        .map_or(crate::text::EMBEDDED_FONT, Font::data);
    let linear: LinearRgba = style.color().into();
    let color_arr = [linear.red, linear.green, linear.blue, linear.alpha];

    #[allow(clippy::cast_precision_loss)]
    let em_scale = style.size() / atlas.canonical_size() as f32;

    // Measure total dimensions for anchor offset.
    let mut max_x = 0.0_f32;
    for sg in &shaped.glyphs {
        let glyph_key = GlyphKey {
            font_id:     style.font_id(),
            glyph_index: sg.glyph_id,
        };
        if let Some(metrics) = atlas.get_or_insert(glyph_key, font_data) {
            #[allow(clippy::cast_precision_loss)]
            let right = (metrics.pixel_width as f32)
                .mul_add(em_scale, metrics.bearing_x.mul_add(style.size(), sg.x));
            max_x = max_x.max(right);
        }
    }
    let mut baselines: Vec<f32> = shaped.glyphs.iter().map(|g| g.baseline).collect();
    baselines.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    let line_count = baselines.len().max(1);
    #[allow(clippy::cast_precision_loss)]
    let max_y = style.effective_line_height() * line_count as f32;

    let scale = 0.01_f32;
    let (anchor_x, anchor_y) = anchor_offset(style.anchor(), max_x, max_y);

    let mut quads = Vec::with_capacity(shaped.glyphs.len());
    #[cfg(feature = "typography_overlay")]
    let mut ink_rects: Vec<[f32; 4]> = Vec::with_capacity(shaped.glyphs.len());
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

        let quad_x = metrics.bearing_x.mul_add(style.size(), sg.x) - anchor_x;
        let quad_y = -(metrics.bearing_y.mul_add(-style.size(), sg.baseline - sg.y) - anchor_y);

        quads.push((
            metrics.page_index,
            GlyphQuadData {
                position: [quad_x * scale, quad_y * scale, 0.0],
                size:     [quad_w * scale, quad_h * scale],
                uv_rect:  metrics.uv_rect,
                color:    color_arr,
            },
        ));

        #[cfg(feature = "typography_overlay")]
        if let Some(ink) = compute_ink_rect(font_data, sg, style, anchor_x, anchor_y, scale) {
            ink_rects.push(ink);
        }
    }

    clip_overlapping_quads(&mut quads);

    #[cfg(not(feature = "typography_overlay"))]
    let ink_rects = Vec::new();

    (quads, anchor_x, anchor_y, max_x, ink_rects)
}

/// Computes a glyph's visible ink rect from its font bounding box.
///
/// Returns `[x, y, width, height]` in world units where `(x, y)` is the
/// top-left corner. Returns `None` if the font face can't be parsed or the
/// glyph has no bounding box (e.g. space).
#[cfg(feature = "typography_overlay")]
fn compute_ink_rect(
    font_data: &[u8],
    sg: &super::text_renderer::ShapedGlyph,
    style: &TextStyle,
    anchor_x: f32,
    anchor_y: f32,
    scale: f32,
) -> Option<[f32; 4]> {
    let face = ttf_parser::Face::parse(font_data, 0).ok()?;
    let bbox = face.glyph_bounding_box(ttf_parser::GlyphId(sg.glyph_id))?;

    let upm = f32::from(face.units_per_em());
    let font_scale = style.size() / upm;

    let ink_width = f32::from(bbox.x_max - bbox.x_min) * font_scale;
    let ink_height = f32::from(bbox.y_max - bbox.y_min) * font_scale;
    let ink_x = f32::from(bbox.x_min).mul_add(font_scale, sg.x) - anchor_x;
    let ink_top = f32::from(bbox.y_max).mul_add(-font_scale, sg.baseline - sg.y) - anchor_y;

    Some([
        ink_x * scale,
        -ink_top * scale,
        ink_width * scale,
        ink_height * scale,
    ])
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
