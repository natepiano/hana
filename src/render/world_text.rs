//! Standalone world-space text component and rendering system.

use std::collections::HashMap;
use std::time::Instant;

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::Face;
use bevy_kana::ToF32;

use super::constants;
use super::glyph_quad;
use super::glyph_quad::GlyphQuadData;
use super::msdf_material::MsdfTextMaterial;
use super::text_renderer;
use super::text_renderer::TextBuildStats;
use super::text_renderer::TextShapingContext;
use super::transparency::TextAlphaModeDefault;
use crate::constants::MILLISECONDS_PER_SECOND;
use crate::layout::BoundingBox;
use crate::layout::GlyphLoadingPolicy;
use crate::layout::GlyphRenderMode;
use crate::layout::GlyphShadowMode;
use crate::layout::GlyphSidedness;
use crate::layout::ShapedGlyph;
use crate::layout::ShapedTextCache;
use crate::layout::Unit;
use crate::layout::UnitConfig;
use crate::layout::WorldTextStyle;
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
///     WorldTextStyle::new(24.0).with_color(Color::RED),
///     Transform::from_xyz(0.0, 2.0, 0.0),
/// ));
/// ```
#[derive(Component, Clone, Debug, Reflect)]
#[require(WorldTextStyle, Transform, Visibility)]
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

/// Marker on a [`WorldText`] entity whose glyphs are not yet fully
/// rasterized in the atlas. Removed automatically when all glyphs
/// become ready.
#[derive(Component)]
pub struct PendingGlyphs;

/// Internal marker: glyphs are ready and meshes are spawned, but we
/// wait for Bevy's transform propagation before firing [`WorldTextReady`].
#[derive(Component)]
pub(super) struct AwaitingReady;

/// Marker on a [`WorldText`] entity spawned as a child of a
/// [`DiegeticPanel`](crate::DiegeticPanel).
///
/// Stores the layout-computed bounding box and panel scale factors
/// needed to build panel-local quads.
#[derive(Component, Clone, Debug)]
pub struct PanelTextChild {
    /// Index of the source element in the layout tree.
    pub element_idx:   usize,
    /// Index of the render command that produced this text child.
    /// Used for Z-offset layering in Geometry mode.
    pub command_index: usize,
    /// Layout-computed position and size in layout coordinates.
    pub bounds:        BoundingBox,
    /// X scale: points to meters.
    pub scale_x:       f32,
    /// Y scale: points to meters.
    pub scale_y:       f32,
    /// Anchor X offset in world units.
    pub anchor_x:      f32,
    /// Anchor Y offset in world units.
    pub anchor_y:      f32,
    /// Active clip rect in layout coordinates, or `None` if unclipped.
    pub clip_rect:     Option<BoundingBox>,
}

/// Fired on a [`WorldText`] entity when all its glyphs are rasterized
/// and the text is fully rendered for the first time (or after a
/// text/style change).
///
/// Observe per-entity:
/// ```ignore
/// commands.spawn((WorldText::new("Hello"), ...))
///     .observe(|trigger: On<WorldTextReady>| {
///         info!("Text ready on {:?}", trigger.entity());
///     });
/// ```
#[derive(EntityEvent)]
pub struct WorldTextReady {
    /// The [`WorldText`] entity that is now fully rendered.
    pub entity: Entity,
}

/// Renders [`WorldText`] entities as MSDF glyph meshes.
///
/// Processes entities in two cases:
/// - **Changed**: `WorldText` or `TextStyle` was modified — re-shape and check glyphs.
/// - **Pending**: entity has [`PendingGlyphs`] — re-check atlas each frame.
///
/// When all glyphs are ready, builds meshes and fires [`WorldTextReady`].
/// When glyphs are still missing, adds/keeps [`PendingGlyphs`].
pub(super) fn render_world_text(
    changed_texts: Query<
        Entity,
        (
            With<WorldText>,
            Without<PanelTextChild>,
            Or<(Changed<WorldText>, Changed<WorldTextStyle>)>,
        ),
    >,
    pending_texts: Query<
        Entity,
        (
            With<WorldText>,
            With<PendingGlyphs>,
            Without<PanelTextChild>,
        ),
    >,
    texts: Query<(&WorldText, &WorldTextStyle), Without<PanelTextChild>>,
    old_meshes: Query<(Entity, &ChildOf), Or<(With<WorldTextMesh>, With<WorldTextShadowProxy>)>>,
    mut atlas: ResMut<MsdfAtlas>,
    font_registry: Res<FontRegistry>,
    shaping_cx: Res<TextShapingContext>,
    mut cache: ResMut<ShapedTextCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MsdfTextMaterial>>,
    alpha_default: Res<TextAlphaModeDefault>,
    mut commands: Commands,
    unit_config: Res<UnitConfig>,
) {
    // Collect entities that need processing: changed texts + pending texts.
    let mut to_process = Vec::new();
    for entity in &changed_texts {
        to_process.push(entity);
    }
    for entity in &pending_texts {
        if !to_process.contains(&entity) {
            to_process.push(entity);
        }
    }

    if to_process.is_empty() {
        return;
    }

    let total_start = Instant::now();
    let mut text_count = 0_usize;
    let mut text_stats = TextBuildStats::default();
    let mut mesh_ms_total = 0.0_f32;

    for entity in to_process {
        let Ok((world_text, style)) = texts.get(entity) else {
            continue;
        };
        text_count += 1;

        if world_text.0.is_empty() {
            for (mesh_entity, child_of) in &old_meshes {
                if child_of.parent() == entity {
                    commands.entity(mesh_entity).despawn();
                }
            }
            commands.entity(entity).remove::<PendingGlyphs>();
            continue;
        }

        let scale = style
            .world_scale()
            .or_else(|| style.unit().map(Unit::meters_per_unit))
            .unwrap_or_else(|| unit_config.world_font.meters_per_unit());

        // Shape text and build quads in entity-local coordinates.
        let shaped = shape_world_text(
            &world_text.0,
            style,
            &font_registry,
            &mut atlas,
            &shaping_cx,
            &mut cache,
            scale,
        );
        text_stats.accumulate(&shaped.stats);

        let all_ready = shaped.stats.glyphs > 0 && shaped.stats.ready_glyphs == shaped.stats.glyphs;
        let has_pending = shaped.stats.pending_glyphs > 0 || shaped.stats.queued_glyphs > 0;

        // Store computed layout data for the typography overlay.
        // Only inserted when all glyphs are ready so the overlay
        // appears atomically with the complete text.
        #[cfg(feature = "typography_overlay")]
        if all_ready {
            commands.entity(entity).insert(ComputedWorldText {
                anchor_x:      shaped.anchor_x,
                anchor_y:      shaped.anchor_y,
                glyph_rects:   shaped.glyph_rects,
                first_advance: shaped.first_advance,
            });
        }

        // Group quads by page.
        let mut page_quads: HashMap<u32, Vec<GlyphQuadData>> = HashMap::new();
        for (page_index, quad) in shaped.quads {
            page_quads.entry(page_index).or_default().push(quad);
        }

        let total_quads: usize = page_quads.values().map(Vec::len).sum();

        if total_quads > 0 {
            // Despawn previous mesh children and rebuild.
            for (mesh_entity, child_of) in &old_meshes {
                if child_of.parent() == entity {
                    commands.entity(mesh_entity).despawn();
                }
            }

            let resolved_alpha = style.alpha_mode().unwrap_or(alpha_default.0);
            mesh_ms_total += spawn_world_text_meshes(
                &page_quads,
                entity,
                style,
                &atlas,
                &mut meshes,
                &mut materials,
                resolved_alpha,
                &mut commands,
            );
        }

        // Track per-entity glyph readiness.
        if has_pending {
            commands.entity(entity).insert_if_new(PendingGlyphs);
        } else if all_ready {
            commands.entity(entity).remove::<PendingGlyphs>();
            commands.entity(entity).insert(AwaitingReady);
        }
    }

    let total_ms = total_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    if total_ms > 5.0 || text_stats.queued_glyphs > 0 || text_stats.pending_glyphs > 0 {
        bevy::log::debug!(
            "render_world_text: total={total_ms:.1}ms texts={text_count} shape={:.1}ms atlas={:.1}ms mesh={mesh_ms_total:.1}ms glyphs={} ready={} queued={} pending={} quads={}",
            text_stats.shape_ms,
            text_stats.atlas_ms,
            text_stats.glyphs,
            text_stats.ready_glyphs,
            text_stats.queued_glyphs,
            text_stats.pending_glyphs,
            text_stats.emitted_quads,
        );
    }
}

/// Fires [`WorldTextReady`] for entities whose meshes and transforms are
/// now fully propagated. Runs after `CalculateBounds` so that `Aabb` and
/// `GlobalTransform` are available on mesh children.
pub(super) fn emit_world_text_ready(
    awaiting: Query<Entity, With<AwaitingReady>>,
    mut commands: Commands,
) {
    for entity in &awaiting {
        commands.entity(entity).remove::<AwaitingReady>();
        commands
            .entity(entity)
            .trigger(|e| WorldTextReady { entity: e });
    }
}

/// Spawns visible mesh and optional shadow proxy entities for each atlas page
/// of glyph quads under the given `entity`. Returns accumulated mesh build time
/// in milliseconds.
/// Configures a `StandardMaterial`'s `double_sided` and `cull_mode` fields
/// from a [`GlyphSidedness`] choice.
const fn apply_sidedness(base: &mut StandardMaterial, sidedness: GlyphSidedness) {
    match sidedness {
        GlyphSidedness::DoubleSided => {
            base.double_sided = true;
            base.cull_mode = None;
        },
        GlyphSidedness::OneSided => {
            base.double_sided = false;
            base.cull_mode = Some(Face::Back);
        },
    }
}

fn spawn_world_text_meshes(
    page_quads: &HashMap<u32, Vec<GlyphQuadData>>,
    entity: Entity,
    style: &WorldTextStyle,
    atlas: &MsdfAtlas,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<MsdfTextMaterial>,
    alpha_mode: AlphaMode,
    commands: &mut Commands,
) -> f32 {
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

    let mut mesh_ms = 0.0_f32;
    for (page_index, pq) in page_quads {
        let Some(page_image) = atlas.image_handle(*page_index).cloned() else {
            continue;
        };

        let mesh_start = Instant::now();
        let mesh = glyph_quad::build_glyph_mesh(pq);
        let mesh_handle = meshes.add(mesh);

        // Spawn visible mesh (skip for Invisible render mode).
        if !is_invisible {
            let mut visible_base = StandardMaterial {
                depth_bias: -1.0,
                ..Default::default()
            };
            apply_sidedness(&mut visible_base, style.sidedness());
            let mat = super::msdf_material::msdf_text_material(
                visible_base,
                MsdfAtlas::sdf_range().to_f32(),
                atlas.width(),
                atlas.height(),
                page_image.clone(),
                0.0,
                u32::from(style.render_mode()),
                constants::UNCLIPPED_TEXT_CLIP_RECT,
                constants::OIT_DEPTH_STEP,
                alpha_mode,
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
                GlyphShadowMode::SolidQuad => u32::from(GlyphRenderMode::SolidQuad),
                GlyphShadowMode::PunchOut => u32::from(GlyphRenderMode::PunchOut),
                GlyphShadowMode::None | GlyphShadowMode::Text => u32::from(GlyphRenderMode::Text),
            };

            let mut proxy_base = StandardMaterial {
                depth_bias: -constants::LAYER_DEPTH_BIAS,
                ..Default::default()
            };
            apply_sidedness(&mut proxy_base, style.sidedness());
            let proxy_material = materials.add(super::msdf_material::msdf_shadow_proxy_material(
                proxy_base,
                MsdfAtlas::sdf_range().to_f32(),
                atlas.width(),
                atlas.height(),
                page_image,
                0.0,
                shadow_render_mode,
                constants::UNCLIPPED_TEXT_CLIP_RECT,
                0.0,
            ));

            commands.entity(entity).with_child((
                WorldTextShadowProxy,
                Mesh3d(mesh_handle),
                MeshMaterial3d(proxy_material),
                Transform::IDENTITY,
            ));
        }
        mesh_ms = mesh_start
            .elapsed()
            .as_secs_f32()
            .mul_add(MILLISECONDS_PER_SECOND, mesh_ms);
    }
    mesh_ms
}

/// Result of shaping and building glyph quads for a [`WorldText`] entity.
struct ShapedWorldText {
    /// Per-glyph quads keyed by atlas page index.
    quads:         Vec<(u32, GlyphQuadData)>,
    /// Anchor offset X in layout units.
    anchor_x:      f32,
    /// Anchor offset Y in layout units.
    anchor_y:      f32,
    /// Per-glyph ink bounding boxes `[x, y, w, h]` in world units.
    glyph_rects:   Vec<[f32; 4]>,
    /// Advance width of the first glyph in world units.
    first_advance: f32,
    /// Timing and queue diagnostics from the build.
    stats:         TextBuildStats,
}

impl ShapedWorldText {
    const fn empty(stats: TextBuildStats) -> Self {
        Self {
            quads: Vec::new(),
            anchor_x: 0.0,
            anchor_y: 0.0,
            glyph_rects: Vec::new(),
            first_advance: 0.0,
            stats,
        }
    }
}

/// Shapes text and produces glyph quads in entity-local coordinates.
///
/// Unlike panel text, standalone text has no layout bounds or panel scale.
/// Glyphs are positioned relative to the origin, offset by the anchor point.
/// The `scale` parameter converts layout units to world units.
fn shape_world_text(
    text: &str,
    style: &WorldTextStyle,
    font_registry: &FontRegistry,
    atlas: &mut MsdfAtlas,
    shaping_cx: &TextShapingContext,
    cache: &mut ShapedTextCache,
    scale: f32,
) -> ShapedWorldText {
    // Pre-scale font size to points for shaping. Parley's quantize mode
    // rounds baselines to integers, which destroys metrics when the font
    // size is below 1.0 (e.g., 0.10 meters). We shape at the equivalent
    // point size and scale the output back down.
    let points_to_world = Unit::Points.meters_per_unit();
    let boost = if points_to_world > 0.0 {
        1.0 / points_to_world
    } else {
        1.0
    };
    let config = style.as_layout_config().scaled(boost);

    let mut stats = TextBuildStats {
        texts: 1,
        ..Default::default()
    };
    let shape_start = Instant::now();
    let shaped = text_renderer::shape_text_cached(text, &config, font_registry, shaping_cx, cache);
    stats.shape_ms = shape_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    stats.glyphs = shaped.glyphs.len();

    let font_data = font_registry
        .font(FontId(style.font_id()))
        .map_or(crate::text::EMBEDDED_FONT, Font::data);

    let atlas_start = Instant::now();
    if style.loading_policy() == GlyphLoadingPolicy::WhenReady
        && !ensure_all_glyphs_ready(&shaped.glyphs, style, atlas, font_data, &mut stats)
    {
        stats.atlas_ms = atlas_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
        return ShapedWorldText::empty(stats);
    }

    let linear: LinearRgba = style.color().into();
    let color_arr = [linear.red, linear.green, linear.blue, linear.alpha];

    // em_scale uses the boosted config size (in points) for atlas lookup,
    // then the final quad positions are multiplied by `scale` (which already
    // accounts for meters_per_unit). The boost cancels out:
    //   quad_world = (glyph_pts * em_scale_pts) * scale_meters
    // where em_scale_pts = config.size() / canonical and scale includes
    // the 1/boost factor to convert back from points to the original unit.
    let em_scale = config.size() / atlas.canonical_size().to_f32();

    // The boosted config is `ForLayout` (no anchor field). Convert to
    // standalone and restore the *original* style's anchor so the offset
    // computation uses the user's intended anchor, not the default Center.
    let (anchor_x, anchor_y) = measure_anchor_offset(
        &shaped.glyphs,
        &config.as_standalone().with_anchor(style.anchor()),
        font_registry,
        atlas,
        font_data,
        em_scale,
    );

    // Constant across all glyphs — hoist above the loop so they remain
    // in scope for the `first_advance` calculation below.
    let boosted_size = config.size();
    let world_scale = scale * points_to_world; // points → world meters

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

        let quad_w = metrics.pixel_width.to_f32() * em_scale;
        let quad_h = metrics.pixel_height.to_f32() * em_scale;

        let quad_x = metrics.bearing_x.mul_add(boosted_size, sg.x) - anchor_x;
        let quad_y = -(metrics.bearing_y.mul_add(-boosted_size, sg.baseline + sg.y) - anchor_y);

        quads.push((
            metrics.page_index,
            GlyphQuadData {
                position: [quad_x * world_scale, quad_y * world_scale, 0.0],
                size:     [quad_w * world_scale, quad_h * world_scale],
                uv_rect:  metrics.uv_rect,
                color:    color_arr,
            },
        ));

        if let Some(rect) = ink_rect(
            font_data,
            sg.glyph_id,
            boosted_size,
            sg.x,
            sg.baseline + sg.y,
            anchor_x,
            anchor_y,
            world_scale,
        ) {
            glyph_rects.push(rect);
        }
    }

    glyph_quad::clip_overlapping_quads(&mut quads);

    let first_advance = shaped.glyphs.first().map_or(0.0, |sg| {
        glyph_advance(font_data, sg.glyph_id, boosted_size, world_scale)
    });

    stats.atlas_ms = atlas_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    stats.emitted_quads = quads.len();

    // Anchor values are in boosted (points) space. Scale back to original
    // units for downstream consumers (typography overlay).
    ShapedWorldText {
        quads,
        anchor_x: anchor_x * points_to_world,
        anchor_y: anchor_y * points_to_world,
        glyph_rects,
        first_advance,
        stats,
    }
}

/// Queues all glyphs for async rasterization and returns `true` if every glyph
/// in the run is already cached in the atlas.
fn ensure_all_glyphs_ready(
    glyphs: &[ShapedGlyph],
    style: &WorldTextStyle,
    atlas: &mut MsdfAtlas,
    font_data: &[u8],
    stats: &mut TextBuildStats,
) -> bool {
    let mut all_ready = true;
    for sg in glyphs {
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
    all_ready
}

/// Measures the total text extent and returns the `(anchor_x, anchor_y)` offset
/// for the given anchor mode.
fn measure_anchor_offset(
    glyphs: &[ShapedGlyph],
    style: &WorldTextStyle,
    font_registry: &FontRegistry,
    atlas: &mut MsdfAtlas,
    font_data: &[u8],
    em_scale: f32,
) -> (f32, f32) {
    let mut max_x = 0.0_f32;
    for sg in glyphs {
        let glyph_key = GlyphKey {
            font_id:     style.font_id(),
            glyph_index: sg.glyph_id,
        };
        if let Some(metrics) = atlas.get_or_insert(glyph_key, font_data) {
            let right = (metrics.pixel_width.to_f32())
                .mul_add(em_scale, metrics.bearing_x.mul_add(style.size(), sg.x));
            max_x = max_x.max(right);
        }
    }
    let mut baselines: Vec<f32> = glyphs.iter().map(|g| g.baseline).collect();
    baselines.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    let line_count = baselines.len().max(1);
    let natural_line_height = if style.line_height_raw() > 0.0 {
        style.line_height_raw()
    } else {
        font_registry
            .font(FontId(style.font_id()))
            .map_or_else(|| style.size(), |f| f.metrics(style.size()).line_height)
    };
    let max_y = natural_line_height * line_count.to_f32();
    style.anchor().offset(max_x, max_y)
}

/// Computes the ink bounding box for a single glyph, returned as `[x, y, w, h]`
/// in world units, or `None` if the font face or glyph bbox is unavailable.
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
