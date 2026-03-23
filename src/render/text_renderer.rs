//! Text rendering system — extracts text from layout results and builds glyph meshes.

use std::collections::HashMap;
use std::hash::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::time::Instant;

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::glyph_quad::GlyphQuadData;
use super::glyph_quad::build_glyph_mesh;
use super::msdf_material::MsdfTextMaterial;
use crate::layout::GlyphRenderMode;
use crate::layout::GlyphShadowMode;
use crate::layout::RenderCommandKind;
use crate::layout::TextConfig;
use crate::layout::TextMeasure;
use crate::plugin::ComputedDiegeticPanel;
use crate::plugin::DiegeticPanel;
use crate::plugin::DiegeticPerfStats;
use crate::plugin::HueOffset;
use crate::text::Font;
use crate::text::FontId;
use crate::text::FontRegistry;
use crate::text::GlyphKey;
use crate::text::MsdfAtlas;

/// Z offset for text layer (above rectangles, below borders).
const TEXT_Z_OFFSET: f32 = 0.001;

// ── Shaped text cache ────────────────────────────────────────────────────────

/// A single shaped glyph from parley — glyph ID plus its position relative to
/// the text origin. This is the parley shaping output, independent of layout
/// bounds or panel scale.
#[derive(Clone, Debug)]
pub struct ShapedGlyph {
    /// Glyph index within the font.
    pub glyph_id: u16,
    /// X position relative to the text origin (accumulated advance + fine offset).
    pub x:        f32,
    /// Y position relative to the text origin (baseline-relative).
    pub y:        f32,
    /// Baseline of the line this glyph belongs to.
    pub baseline: f32,
}

/// Snapshot of parley's per-line metrics, captured during text shaping.
///
/// All values are in layout units (Y-down coordinate system).
#[derive(Clone, Copy, Debug)]
pub struct LineMetricsSnapshot {
    /// Typographic ascent for this line.
    pub ascent:   f32,
    /// Typographic descent for this line.
    pub descent:  f32,
    /// Offset to the baseline from the top of the layout.
    pub baseline: f32,
    /// Top of the line box (parley `min_coord`).
    pub top:      f32,
    /// Bottom of the line box (parley `max_coord`).
    pub bottom:   f32,
}

/// Cached shaping result for a text string at a specific font configuration.
#[derive(Clone, Debug)]
pub struct ShapedTextRun {
    /// The shaped glyphs in order.
    pub glyphs:       Vec<ShapedGlyph>,
    /// Per-line metrics from parley, captured during shaping.
    pub line_metrics: Vec<LineMetricsSnapshot>,
}

/// Cache key: hash of the text string + the full `TextMeasure` identity.
#[derive(Clone, Eq, PartialEq, Hash)]
struct ShapedCacheKey {
    text_hash: u64,
    font_id:   u16,
    /// Size quantized to avoid floating-point hash issues (size * 100 as u32).
    size_q:    u32,
    weight_q:  u32,
    slant:     u8,
    lh_q:      u32,
    ls_q:      i32,
    ws_q:      i32,
}

impl ShapedCacheKey {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn new(text: &str, m: &TextMeasure) -> Self {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        Self {
            text_hash: hasher.finish(),
            font_id:   m.font_id,
            size_q:    (m.size * 100.0) as u32,
            weight_q:  (m.weight.0 * 10.0) as u32,
            slant:     match m.slant {
                crate::layout::FontSlant::Normal => 0,
                crate::layout::FontSlant::Italic => 1,
                crate::layout::FontSlant::Oblique => 2,
            },
            lh_q:      (m.line_height * 100.0) as u32,
            ls_q:      (m.letter_spacing * 100.0) as i32,
            ws_q:      (m.word_spacing * 100.0) as i32,
        }
    }
}

/// Caches shaped text runs and measurement results to avoid redundant parley
/// shaping.
///
/// Keyed by `(text_content, TextMeasure)`. Stores both the shaped glyph
/// positions (for rendering) and the `TextDimensions` (for layout measurement).
/// Shared between the layout engine's `MeasureTextFn` and the renderer's
/// `shape_text_to_quads` via `Arc<Mutex<>>`.
#[derive(Resource, Clone, Default)]
pub struct ShapedTextCache {
    entries:      HashMap<ShapedCacheKey, ShapedTextRun>,
    measurements: HashMap<ShapedCacheKey, crate::layout::TextDimensions>,
}

impl ShapedTextCache {
    /// Returns cached measurement dimensions for the given text + config,
    /// or `None` if not yet cached.
    #[must_use]
    pub fn get_measurement(
        &self,
        text: &str,
        measure: &TextMeasure,
    ) -> Option<crate::layout::TextDimensions> {
        let key = ShapedCacheKey::new(text, measure);
        self.measurements.get(&key).copied()
    }

    /// Returns the cached shaped text run for the given text + config,
    /// or `None` if not yet cached.
    #[must_use]
    pub fn get_shaped(&self, text: &str, measure: &TextMeasure) -> Option<&ShapedTextRun> {
        let key = ShapedCacheKey::new(text, measure);
        self.entries.get(&key)
    }
}

/// Reusable parley shaping buffers.
///
/// Avoids reallocating `LayoutContext` and `Layout` on every
/// `shape_text_to_quads` call. Wrapped in `Mutex` for `Send + Sync`.
#[derive(Resource)]
pub(super) struct TextShapingContext {
    layout_cx: Mutex<parley::LayoutContext<()>>,
    layout:    Mutex<parley::Layout<()>>,
}

impl Default for TextShapingContext {
    fn default() -> Self {
        Self {
            layout_cx: Mutex::new(parley::LayoutContext::default()),
            layout:    Mutex::new(parley::Layout::new()),
        }
    }
}

/// Marker component for text mesh entities spawned by the renderer.
#[derive(Component)]
struct DiegeticTextMesh;

/// Marker component for shadow proxy mesh entities.
#[derive(Component)]
struct DiegeticShadowProxy;

/// Set by [`poll_atlas_glyphs`] when new glyphs are inserted into the
/// atlas. Text systems check this flag and rebuild all meshes when true.
///
/// **Optimization opportunity:** when this flag is set, ALL panels and
/// `WorldText` entities are rebuilt — even those whose glyphs were
/// already complete. A `HashSet<Entity>` tracking only panels/texts
/// with missing glyphs (where `get_or_insert` returned `None`) would
/// limit rebuilds to just the entities that need them. In practice the
/// flag only fires 1–3 times during startup so the cost is low.
#[derive(Resource, Default)]
pub(super) struct GlyphsReady(pub(super) bool);

/// Key for grouping text quads that share the same material configuration.
/// Quads with different page indices produce separate meshes because each
/// page has its own atlas texture.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct TextBatchKey {
    render_mode: GlyphRenderMode,
    shadow_mode: GlyphShadowMode,
    page_index:  u32,
}

/// Cached default material handles shared across panels without a
/// [`HueOffset`] component. Keyed by atlas page index — each page
/// has its own texture and needs its own material.
#[derive(Resource, Default)]
struct SharedMsdfMaterials {
    handles: HashMap<u32, Handle<MsdfTextMaterial>>,
}

/// Plugin that adds MSDF text rendering for diegetic panels.
///
/// Registers the [`MsdfTextMaterial`], adds the text extraction system,
/// and sets up rendering.
pub struct TextRenderPlugin;

impl Plugin for TextRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<MsdfTextMaterial>::default());
        app.init_resource::<TextShapingContext>();
        app.init_resource::<ShapedTextCache>();
        app.init_resource::<SharedMsdfMaterials>();
        app.init_resource::<GlyphsReady>();
        app.add_systems(
            PostUpdate,
            (
                poll_atlas_glyphs.before(extract_text_meshes),
                extract_text_meshes,
                sync_panel_hue_offset.after(extract_text_meshes),
                super::world_text::render_world_text.after(poll_atlas_glyphs),
            ),
        );
    }
}

/// Polls completed async glyph rasterizations, inserts them into the
/// atlas, syncs to GPU, and marks all panels/text as changed so the
/// existing text systems re-extract meshes.
fn poll_atlas_glyphs(
    mut atlas: ResMut<MsdfAtlas>,
    mut images: ResMut<Assets<Image>>,
    mut ready: ResMut<GlyphsReady>,
    mut shared_mats: ResMut<SharedMsdfMaterials>,
) {
    // Clear last frame's flag before polling new results.
    ready.0 = false;

    if atlas.poll_async_glyphs() {
        atlas.sync_to_gpu(&mut images);
        ready.0 = true;
        // Invalidate all shared materials so they get recreated with
        // updated atlas textures on the next extract_text_meshes run.
        shared_mats.handles.clear();
    }
}

/// Extracts `RenderCommandKind::Text` entries from computed panels and
/// builds glyph mesh entities with [`MsdfTextMaterial`].
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn extract_text_meshes(
    changed_panels: Query<
        (
            Entity,
            &DiegeticPanel,
            &ComputedDiegeticPanel,
            Option<&HueOffset>,
        ),
        Changed<ComputedDiegeticPanel>,
    >,
    all_panels: Query<(
        Entity,
        &DiegeticPanel,
        &ComputedDiegeticPanel,
        Option<&HueOffset>,
    )>,
    old_text: Query<(Entity, &ChildOf), Or<(With<DiegeticTextMesh>, With<DiegeticShadowProxy>)>>,
    mut atlas: ResMut<MsdfAtlas>,
    font_registry: Res<FontRegistry>,
    shaping_cx: Res<TextShapingContext>,
    mut cache: ResMut<ShapedTextCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MsdfTextMaterial>>,
    mut shared_mats: ResMut<SharedMsdfMaterials>,
    mut commands: Commands,
    mut perf: ResMut<DiegeticPerfStats>,
    ready: Res<GlyphsReady>,
) {
    let rebuild_all = ready.0;

    if !rebuild_all && changed_panels.is_empty() {
        perf.last_text_extract_ms = 0.0;
        perf.last_text_extract_panels = 0;
        return;
    }

    let start = Instant::now();
    let mut panel_count = 0_usize;

    // Ensure at least page 0 has a GPU image.
    if atlas.image_handle(0).is_none() {
        return;
    }

    let panels_iter: Vec<_> = if rebuild_all {
        all_panels.iter().collect()
    } else {
        changed_panels.iter().collect()
    };

    for (panel_entity, panel, computed, hue_offset) in panels_iter {
        panel_count += 1;

        let Some(result) = computed.result() else {
            continue;
        };

        let scale_x = panel.world_width / panel.layout_width;
        let scale_y = panel.world_height / panel.layout_height;
        let half_w = panel.world_width * 0.5;
        let half_h = panel.world_height * 0.5;
        let hue = hue_offset.map_or(0.0, |h| h.0);

        // Group quads by (render_mode, shadow_mode, page_index) — each
        // unique combo becomes its own mesh entity with the appropriate
        // material bound to that atlas page's texture.
        let mut batches: HashMap<TextBatchKey, Vec<GlyphQuadData>> = HashMap::new();
        for cmd in &result.commands {
            let (text, config) = match &cmd.kind {
                RenderCommandKind::Text { text, config, .. } => (text.as_str(), config.clone()),
                _ => continue,
            };

            let tagged_quads = shape_text_to_quads(
                text,
                &config,
                &cmd.bounds,
                &font_registry,
                &mut atlas,
                &shaping_cx,
                &mut cache,
                scale_x,
                scale_y,
                half_w,
                half_h,
            );

            for (page_index, quad) in tagged_quads {
                let key = TextBatchKey {
                    render_mode: config.render_mode(),
                    shadow_mode: config.shadow_mode(),
                    page_index,
                };
                batches.entry(key).or_default().push(quad);
            }
        }

        let total_quads: usize = batches.values().map(Vec::len).sum();

        // Only replace old meshes if we have content. Keeps partial
        // text visible until a more complete replacement is ready.
        if total_quads == 0 {
            continue;
        }

        // Despawn previous text mesh children — safe because we have a
        // non-empty replacement ready.
        for (entity, child_of) in &old_text {
            if child_of.parent() == panel_entity {
                commands.entity(entity).despawn();
            }
        }

        spawn_batch_meshes(
            &batches,
            panel_entity,
            hue,
            &atlas,
            &mut meshes,
            &mut materials,
            &mut shared_mats,
            &mut commands,
        );
    }

    perf.last_text_extract_ms = start.elapsed().as_secs_f32() * 1000.0;
    perf.last_text_extract_panels = panel_count;
}

/// Spawns visible mesh and optional shadow proxy entities for each batch
/// of glyph quads under the given `panel_entity`.
#[allow(clippy::too_many_arguments)]
fn spawn_batch_meshes(
    batches: &HashMap<TextBatchKey, Vec<GlyphQuadData>>,
    panel_entity: Entity,
    hue: f32,
    atlas: &MsdfAtlas,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<MsdfTextMaterial>,
    shared_mats: &mut SharedMsdfMaterials,
    commands: &mut Commands,
) {
    for (key, quads) in batches {
        if quads.is_empty() {
            continue;
        }

        let Some(page_image) = atlas.image_handle(key.page_index).cloned() else {
            continue;
        };

        let mesh = build_glyph_mesh(quads);
        let mesh_handle = meshes.add(mesh);

        let is_invisible = key.render_mode == GlyphRenderMode::Invisible;

        let needs_proxy = if is_invisible {
            key.shadow_mode != GlyphShadowMode::None
        } else {
            matches!(
                key.shadow_mode,
                GlyphShadowMode::Text | GlyphShadowMode::PunchOut
            )
        };
        let suppress_shadow =
            is_invisible || needs_proxy || key.shadow_mode == GlyphShadowMode::None;

        if !is_invisible {
            let render_mode_u32 = key.render_mode as u32;

            #[allow(clippy::cast_possible_truncation)]
            let material_handle =
                if hue.abs() < f32::EPSILON && key.render_mode == GlyphRenderMode::Text {
                    shared_mats
                        .handles
                        .entry(key.page_index)
                        .or_insert_with(|| {
                            materials.add(super::msdf_material::msdf_text_material(
                                atlas.sdf_range() as f32,
                                atlas.width(),
                                atlas.height(),
                                page_image.clone(),
                                0.0,
                                GlyphRenderMode::Text as u32,
                            ))
                        })
                        .clone()
                } else {
                    materials.add(super::msdf_material::msdf_text_material(
                        atlas.sdf_range() as f32,
                        atlas.width(),
                        atlas.height(),
                        page_image.clone(),
                        hue,
                        render_mode_u32,
                    ))
                };

            if suppress_shadow {
                commands.entity(panel_entity).with_child((
                    DiegeticTextMesh,
                    NotShadowCaster,
                    Mesh3d(mesh_handle.clone()),
                    MeshMaterial3d(material_handle),
                    Transform::IDENTITY,
                ));
            } else {
                commands.entity(panel_entity).with_child((
                    DiegeticTextMesh,
                    Mesh3d(mesh_handle.clone()),
                    MeshMaterial3d(material_handle),
                    Transform::IDENTITY,
                ));
            }
        }

        if needs_proxy {
            let shadow_render_mode = match key.shadow_mode {
                GlyphShadowMode::SolidQuad => GlyphRenderMode::SolidQuad as u32,
                GlyphShadowMode::PunchOut => GlyphRenderMode::PunchOut as u32,
                GlyphShadowMode::None | GlyphShadowMode::Text => GlyphRenderMode::Text as u32,
            };

            #[allow(clippy::cast_possible_truncation)]
            let proxy_material = materials.add(super::msdf_material::msdf_shadow_proxy_material(
                atlas.sdf_range() as f32,
                atlas.width(),
                atlas.height(),
                page_image,
                hue,
                shadow_render_mode,
            ));

            commands.entity(panel_entity).with_child((
                DiegeticShadowProxy,
                Mesh3d(mesh_handle),
                MeshMaterial3d(proxy_material),
                Transform::IDENTITY,
            ));
        }
    }
}

/// Shapes text via parley, using the cache when possible.
///
/// On cache hit, returns the stored glyph run directly. On miss, shapes via
/// parley and inserts into the cache.
pub(super) fn shape_text_cached(
    text: &str,
    config: &TextConfig,
    font_registry: &FontRegistry,
    shaping_cx: &TextShapingContext,
    cache: &mut ShapedTextCache,
) -> ShapedTextRun {
    let key = ShapedCacheKey::new(text, &config.as_measure());

    if let Some(cached) = cache.entries.get(&key) {
        return cached.clone();
    }

    // Cache miss — shape via parley.
    let font_cx = font_registry.font_context();
    let mut font_cx = font_cx.lock().unwrap_or_else(PoisonError::into_inner);
    let mut layout_cx = shaping_cx
        .layout_cx
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let mut layout = shaping_cx
        .layout
        .lock()
        .unwrap_or_else(PoisonError::into_inner);

    let family_name = font_registry
        .family_name(FontId(config.font_id()))
        .unwrap_or("JetBrains Mono");

    let mut builder = layout_cx.ranged_builder(&mut font_cx, text, 1.0, true);
    builder.push_default(parley::style::StyleProperty::FontSize(config.size()));
    builder.push_default(parley::style::StyleProperty::FontStack(
        parley::style::FontStack::Single(parley::style::FontFamily::Named(family_name.into())),
    ));
    let line_height = config.effective_line_height();
    builder.push_default(parley::style::StyleProperty::LineHeight(
        parley::style::LineHeight::Absolute(line_height),
    ));
    builder.build_into(&mut layout, text);
    layout.break_all_lines(None);

    drop(font_cx);
    drop(layout_cx);

    let mut glyphs = Vec::new();
    let mut line_metrics_list = Vec::new();
    for line in layout.lines() {
        let lm = line.metrics();
        line_metrics_list.push(LineMetricsSnapshot {
            ascent:   lm.ascent,
            descent:  lm.descent,
            baseline: lm.baseline,
            top:      lm.min_coord,
            bottom:   lm.max_coord,
        });
        for item in line.items() {
            let parley::layout::PositionedLayoutItem::GlyphRun(run) = item else {
                continue;
            };
            let glyph_run = run.run();
            let mut advance_x = 0.0_f32;
            for cluster in glyph_run.clusters() {
                for glyph in cluster.glyphs() {
                    #[allow(clippy::cast_possible_truncation)]
                    glyphs.push(ShapedGlyph {
                        glyph_id: glyph.id as u16,
                        x:        run.offset() + advance_x + glyph.x,
                        y:        glyph.y,
                        baseline: run.baseline(),
                    });
                    advance_x += glyph.advance;
                }
            }
        }
    }

    // Store measurement alongside the shaped run.
    let dims = crate::layout::TextDimensions {
        width:  layout.full_width(),
        height: layout.height(),
    };
    drop(layout);
    let run = ShapedTextRun {
        glyphs,
        line_metrics: line_metrics_list,
    };
    cache.measurements.insert(key.clone(), dims);
    cache.entries.insert(key, run.clone());
    run
}

/// Shapes text and produces glyph quads in panel-local coordinates.
///
/// Uses the [`ShapedTextCache`] to avoid redundant parley shaping. Quad
/// construction from cached glyphs + atlas metrics is cheap arithmetic.
#[allow(clippy::too_many_arguments)]
fn shape_text_to_quads(
    text: &str,
    config: &TextConfig,
    bounds: &crate::layout::BoundingBox,
    font_registry: &FontRegistry,
    atlas: &mut MsdfAtlas,
    shaping_cx: &TextShapingContext,
    cache: &mut ShapedTextCache,
    scale_x: f32,
    scale_y: f32,
    half_w: f32,
    half_h: f32,
) -> Vec<(u32, GlyphQuadData)> {
    let shaped = shape_text_cached(text, config, font_registry, shaping_cx, cache);

    let font_data = font_registry
        .font(FontId(config.font_id()))
        .map_or(crate::text::EMBEDDED_FONT, Font::data);
    let linear: LinearRgba = config.color().into();
    let color_arr = [linear.red, linear.green, linear.blue, linear.alpha];

    #[allow(clippy::cast_precision_loss)]
    let em_scale = config.size() / atlas.canonical_size() as f32;

    let mut quads = Vec::with_capacity(shaped.glyphs.len());
    for sg in &shaped.glyphs {
        let glyph_key = GlyphKey {
            font_id:     config.font_id(),
            glyph_index: sg.glyph_id,
        };

        let Some(metrics) = atlas.get_or_insert(glyph_key, font_data) else {
            continue;
        };

        // Glyph position in layout coordinates (Y-down).
        let glyph_x = bounds.x + sg.x;
        let glyph_y = bounds.y + sg.baseline - sg.y;

        // Glyph quad size in layout units.
        #[allow(clippy::cast_precision_loss)]
        let quad_w = metrics.pixel_width as f32 * em_scale;
        #[allow(clippy::cast_precision_loss)]
        let quad_h = metrics.pixel_height as f32 * em_scale;

        // Quad top-left in layout coordinates.
        let quad_layout_x = metrics.bearing_x.mul_add(config.size(), glyph_x);
        let quad_layout_y = (-metrics.bearing_y).mul_add(config.size(), glyph_y);

        // Convert to panel-local (center origin, Y-up).
        let local_x = quad_layout_x.mul_add(scale_x, -half_w);
        let local_y = (-quad_layout_y).mul_add(scale_y, half_h);

        quads.push((
            metrics.page_index,
            GlyphQuadData {
                position: [local_x, local_y, TEXT_Z_OFFSET],
                size:     [quad_w * scale_x, quad_h * scale_y],
                uv_rect:  metrics.uv_rect,
                color:    color_arr,
            },
        ));
    }

    super::glyph_quad::clip_overlapping_quads(&mut quads);

    quads
}

/// Syncs [`HueOffset`] to text materials on child meshes.
///
/// Panels using the shared material that receive a [`HueOffset`] are
/// split off onto their own private material clone. Panels already on a
/// private material are updated in place. This ensures that changing one
/// panel's hue offset never affects other panels.
fn sync_panel_hue_offset(
    panels: Query<(Entity, &HueOffset), Changed<HueOffset>>,
    mut children: Query<(&ChildOf, &mut MeshMaterial3d<MsdfTextMaterial>)>,
    shared_mats: Res<SharedMsdfMaterials>,
    mut materials: ResMut<Assets<MsdfTextMaterial>>,
) {
    for (panel_entity, hue_offset) in &panels {
        for (child_of, mut mat_handle) in &mut children {
            if child_of.parent() != panel_entity {
                continue;
            }

            let is_shared = shared_mats.handles.values().any(|h| *h == mat_handle.0);

            if is_shared {
                // Panel is on the shared material — clone it into a
                // private material with this panel's hue_offset.
                if let Some(base) = materials.get(&mat_handle.0) {
                    let mut private = base.clone();
                    private.extension.uniforms.hue_offset = hue_offset.0;
                    mat_handle.0 = materials.add(private);
                }
            } else {
                // Already private — update in place.
                if let Some(mat) = materials.get_mut(&mat_handle.0) {
                    mat.extension.uniforms.hue_offset = hue_offset.0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that the material sharing decision produces the expected
    /// handle identity: zero hue → shared handle, non-zero hue → unique handle.
    #[test]
    fn material_sharing_by_hue_offset() {
        let mut materials = Assets::<MsdfTextMaterial>::default();
        let mut images = Assets::<Image>::default();

        // Create a dummy atlas image.
        let atlas_image = images.add(Image::default());

        // Create the shared default material (hue_offset = 0).
        let shared_handle = materials.add(super::super::msdf_material::msdf_text_material(
            4.0,
            256,
            256,
            atlas_image.clone(),
            0.0,
            0,
        ));

        // Simulate the decision logic from extract_text_meshes.
        let mut decide = |hue: f32| -> Handle<MsdfTextMaterial> {
            if hue.abs() < f32::EPSILON {
                shared_handle.clone()
            } else {
                materials.add(super::super::msdf_material::msdf_text_material(
                    4.0,
                    256,
                    256,
                    atlas_image.clone(),
                    hue,
                    0,
                ))
            }
        };

        // Panels with no hue offset get the shared handle.
        let a = decide(0.0);
        let b = decide(0.0);
        assert_eq!(a.id(), b.id(), "zero-hue panels should share a handle");

        // Panel with non-zero hue gets its own handle.
        let c = decide(0.5);
        assert_ne!(
            a.id(),
            c.id(),
            "non-zero-hue panel should get a unique handle"
        );

        // Two non-zero panels get different handles (each call to
        // materials.add creates a new asset).
        let d = decide(0.5);
        assert_ne!(
            c.id(),
            d.id(),
            "each non-zero-hue panel gets its own handle"
        );
    }
}
