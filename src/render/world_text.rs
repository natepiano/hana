//! Standalone world-space text component and rendering system.

use std::collections::HashMap;
use std::time::Instant;

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::glyph_quad::GlyphQuadData;
use super::glyph_quad::build_glyph_mesh;
use super::glyph_quad::clip_overlapping_quads;
use super::msdf_material::MsdfTextMaterial;
use super::text_renderer::ShapedTextCache;
use super::text_renderer::TextBuildStats;
use super::text_renderer::TextShapingContext;
use super::text_renderer::shape_text_cached;
use crate::layout::GlyphLoadingPolicy;
use crate::layout::GlyphRenderMode;
use crate::layout::GlyphShadowMode;
use crate::layout::TextStyle;
use crate::text::Font;
use crate::text::FontId;
use crate::text::FontRegistry;
use crate::text::GlyphKey;
use crate::text::GlyphLookup;
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
    pub anchor_x:      f32,
    /// Anchor offset Y in layout units (matches the renderer's anchor).
    pub anchor_y:      f32,
    /// Per-glyph ink bounding boxes `[x, y, width, height]` in world
    /// units. Derived from the font's glyph bbox, positioned using the
    /// same coordinate system as the renderer.
    pub glyph_rects:   Vec<[f32; 4]>,
    /// Advance width of the first glyph in world units.
    pub first_advance: f32,
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
    let total_start = Instant::now();
    let mut text_count = 0_usize;
    let mut text_stats = TextBuildStats::default();
    let mut mesh_ms_total = 0.0_f32;

    for (entity, world_text, style) in texts_iter {
        text_count += 1;
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
        let (quads, anchor_x, anchor_y, glyph_rects, first_advance, build_stats) = shape_world_text(
            &world_text.0,
            style,
            &font_registry,
            &mut atlas,
            &shaping_cx,
            &mut cache,
        );
        text_stats.accumulate(&build_stats);

        // Store computed layout data for the typography overlay.
        #[cfg(feature = "typography_overlay")]
        commands.entity(entity).insert(ComputedWorldText {
            anchor_x,
            anchor_y,
            glyph_rects,
            first_advance,
        });
        #[cfg(not(feature = "typography_overlay"))]
        {
            let _ = (anchor_x, anchor_y, glyph_rects, first_advance);
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

            let mesh_start = Instant::now();
            let mesh = build_glyph_mesh(pq);
            let mesh_handle = meshes.add(mesh);

            // Spawn visible mesh (skip for Invisible render mode).
            if !is_invisible {
                let render_mode_u32 = style.render_mode() as u32;

                #[allow(clippy::cast_possible_truncation)]
                let mat = super::msdf_material::msdf_text_material(
                    atlas.sdf_range() as f32,
                    atlas.width(),
                    atlas.height(),
                    page_image.clone(),
                    0.0,
                    render_mode_u32,
                );

                let material_handle = materials.add(mat);

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
            mesh_ms_total += mesh_start.elapsed().as_secs_f32() * 1000.0;
        }
    }

    let total_ms = total_start.elapsed().as_secs_f32() * 1000.0;
    if total_ms > 5.0
        || rebuild_all
        || text_stats.queued_glyphs > 0
        || text_stats.pending_glyphs > 0
    {
        bevy::log::info!(
            "render_world_text: total={total_ms:.1}ms texts={} shape={:.1}ms atlas={:.1}ms mesh={mesh_ms_total:.1}ms glyphs={} ready={} queued={} pending={} quads={} rebuild_all={}",
            text_count,
            text_stats.shape_ms,
            text_stats.atlas_ms,
            text_stats.glyphs,
            text_stats.ready_glyphs,
            text_stats.queued_glyphs,
            text_stats.pending_glyphs,
            text_stats.emitted_quads,
            rebuild_all,
        );
    }
}

/// Shapes text and produces glyph quads in entity-local coordinates.
///
/// Unlike panel text, standalone text has no layout bounds or panel scale.
/// Glyphs are positioned relative to the origin, offset by the anchor point,
/// with a fixed scale (1 layout unit = 0.01 world units by default).
/// Returns `(quads, anchor_x, anchor_y, glyph_rects, first_advance, stats)`.
#[allow(clippy::type_complexity)]
fn shape_world_text(
    text: &str,
    style: &TextStyle,
    font_registry: &FontRegistry,
    atlas: &mut MsdfAtlas,
    shaping_cx: &TextShapingContext,
    cache: &mut ShapedTextCache,
) -> (
    Vec<(u32, GlyphQuadData)>,
    f32,
    f32,
    Vec<[f32; 4]>,
    f32,
    TextBuildStats,
) {
    // Convert TextStyle to TextConfig for shaping (same underlying fields).
    let config = style.as_layout_config();

    let mut stats = TextBuildStats {
        texts: 1,
        ..Default::default()
    };
    let shape_start = Instant::now();
    let shaped = shape_text_cached(text, &config, font_registry, shaping_cx, cache);
    stats.shape_ms = shape_start.elapsed().as_secs_f32() * 1000.0;
    stats.glyphs = shaped.glyphs.len();

    let font_data = font_registry
        .font(FontId(style.font_id()))
        .map_or(crate::text::EMBEDDED_FONT, Font::data);

    let atlas_start = Instant::now();
    // Under `WhenReady`, trigger async rasterization for every glyph but
    // emit nothing until the entire string is cached in the atlas.
    if style.loading_policy() == GlyphLoadingPolicy::WhenReady {
        let mut all_ready = true;
        for sg in &shaped.glyphs {
            let glyph_key = GlyphKey {
                font_id:     style.font_id(),
                glyph_index: sg.glyph_id,
            };
            match atlas.lookup_or_queue(glyph_key, font_data) {
                GlyphLookup::Ready(_) => {},
                GlyphLookup::Pending => {
                    stats.pending_glyphs += 1;
                    all_ready = false;
                },
                GlyphLookup::Queued => {
                    stats.queued_glyphs += 1;
                    all_ready = false;
                },
            }
        }
        if !all_ready {
            stats.atlas_ms = atlas_start.elapsed().as_secs_f32() * 1000.0;
            return (Vec::new(), 0.0, 0.0, Vec::new(), 0.0, stats);
        }
    }

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
    let natural_line_height = if style.line_height_raw() > 0.0 {
        style.line_height_raw()
    } else {
        font_registry
            .font(FontId(style.font_id()))
            .map_or(style.size(), |f| f.metrics(style.size()).line_height)
    };
    let max_y = natural_line_height * line_count as f32;

    let scale = 0.01_f32;
    let (anchor_x, anchor_y) = anchor_offset(style.anchor(), max_x, max_y);

    let mut quads = Vec::with_capacity(shaped.glyphs.len());
    let mut glyph_rects = Vec::with_capacity(shaped.glyphs.len());
    for sg in &shaped.glyphs {
        let glyph_key = GlyphKey {
            font_id:     style.font_id(),
            glyph_index: sg.glyph_id,
        };

        let metrics = match atlas.lookup_or_queue(glyph_key, font_data) {
            GlyphLookup::Ready(metrics) => {
                stats.ready_glyphs += 1;
                metrics
            },
            GlyphLookup::Pending => {
                stats.pending_glyphs += 1;
                continue;
            },
            GlyphLookup::Queued => {
                stats.queued_glyphs += 1;
                continue;
            },
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

        if let Some(rect) = ink_rect(
            font_data,
            sg.glyph_id,
            style.size(),
            sg.x,
            sg.baseline - sg.y,
            anchor_x,
            anchor_y,
            scale,
        ) {
            glyph_rects.push(rect);
        }
    }

    clip_overlapping_quads(&mut quads);

    let first_advance = shaped.glyphs.first().map_or(0.0, |sg| {
        glyph_advance(font_data, sg.glyph_id, style.size(), scale)
    });

    stats.atlas_ms = atlas_start.elapsed().as_secs_f32() * 1000.0;
    stats.emitted_quads = quads.len();

    (quads, anchor_x, anchor_y, glyph_rects, first_advance, stats)
}

/// Computes the ink bounding box for a single glyph, returned as `[x, y, w, h]`
/// in world units, or `None` if the font face or glyph bbox is unavailable.
#[allow(clippy::too_many_arguments)]
fn ink_rect(
    font_data: &[u8],
    glyph_id: u16,
    font_size: f32,
    glyph_x: f32,
    baseline_offset: f32,
    anchor_x: f32,
    anchor_y: f32,
    scale: f32,
) -> Option<[f32; 4]> {
    let face = ttf_parser::Face::parse(font_data, 0).ok()?;
    let bbox = face.glyph_bounding_box(ttf_parser::GlyphId(glyph_id))?;
    let upm = f32::from(face.units_per_em());
    let font_scale = font_size / upm;

    let ink_w = f32::from(bbox.x_max - bbox.x_min) * font_scale;
    let ink_h = f32::from(bbox.y_max - bbox.y_min) * font_scale;
    let ink_x = f32::from(bbox.x_min).mul_add(font_scale, glyph_x) - anchor_x;
    let ink_top = f32::from(bbox.y_max).mul_add(-font_scale, baseline_offset) - anchor_y;

    Some([
        ink_x * scale,
        -ink_top * scale,
        ink_w * scale,
        ink_h * scale,
    ])
}

/// Returns a single glyph's horizontal advance in world units.
fn glyph_advance(font_data: &[u8], glyph_id: u16, font_size: f32, scale: f32) -> f32 {
    ttf_parser::Face::parse(font_data, 0)
        .ok()
        .and_then(|f| {
            let gid = ttf_parser::GlyphId(glyph_id);
            f.glyph_hor_advance(gid).map(|adv| {
                let upm = f32::from(f.units_per_em());
                f32::from(adv) * font_size / upm * scale
            })
        })
        .unwrap_or(0.0)
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
