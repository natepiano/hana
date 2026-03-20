//! Text rendering system — extracts text from layout results and builds glyph meshes.

use std::collections::HashMap;
use std::hash::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::time::Instant;

use bevy::prelude::*;

use super::glyph_quad::GlyphQuadData;
use super::glyph_quad::build_glyph_mesh;
use super::msdf_material::MsdfTextMaterial;
use crate::layout::RenderCommandKind;
use crate::layout::TextConfig;
use crate::layout::TextMeasure;
use crate::plugin::ComputedDiegeticPanel;
use crate::plugin::DiegeticPanel;
use crate::plugin::DiegeticPerfStats;
use crate::text::DEFAULT_CANONICAL_SIZE;
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

/// Cached shaping result for a text string at a specific font configuration.
#[derive(Clone, Debug)]
pub struct ShapedTextRun {
    /// The shaped glyphs in order.
    pub glyphs: Vec<ShapedGlyph>,
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
}

/// Reusable parley shaping buffers.
///
/// Avoids reallocating `LayoutContext` and `Layout` on every
/// `shape_text_to_quads` call. Wrapped in `Mutex` for `Send + Sync`.
#[derive(Resource)]
pub struct TextShapingContext {
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
        app.add_systems(
            PostUpdate,
            (
                extract_text_meshes.after(crate::plugin::compute_panel_layouts),
                super::world_text::render_world_text,
            ),
        );
    }
}

/// Extracts `RenderCommandKind::Text` entries from computed panels and
/// builds glyph mesh entities with [`MsdfTextMaterial`].
///
/// When `ComputedDiegeticPanel::color_only` is set, takes a fast path that
/// patches vertex colors on the existing mesh instead of rebuilding from
/// scratch.
#[allow(clippy::too_many_arguments)]
fn extract_text_meshes(
    mut panels: Query<
        (Entity, &DiegeticPanel, &mut ComputedDiegeticPanel),
        Changed<ComputedDiegeticPanel>,
    >,
    old_text: Query<(Entity, &ChildOf, &DiegeticTextMesh, &Mesh3d)>,
    mut atlas: ResMut<MsdfAtlas>,
    font_registry: Res<FontRegistry>,
    shaping_cx: Res<TextShapingContext>,
    mut cache: ResMut<ShapedTextCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MsdfTextMaterial>>,
    mut commands: Commands,
    mut perf: ResMut<DiegeticPerfStats>,
) {
    if panels.is_empty() {
        perf.last_text_extract_ms = 0.0;
        perf.last_text_extract_panels = 0;
        return;
    }

    let start = Instant::now();
    let mut panel_count = 0_usize;

    for (panel_entity, panel, mut computed) in &mut panels {
        panel_count += 1;
        let Some(result) = &computed.result else {
            continue;
        };

        // ── Color-only fast path ─────────────────────────────────────────
        if computed.color_only {
            if let Some(mesh_handle) = find_text_mesh_handle(&old_text, panel_entity) {
                if let Some(mesh) = meshes.get_mut(&mesh_handle) {
                    let colors = build_color_array(result);
                    debug_assert_eq!(
                        colors.len(),
                        mesh.count_vertices(),
                        "color array must match mesh vertex count"
                    );
                    if !colors.is_empty() {
                        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
                    }
                }
            }
            continue;
        }

        // ── Full rebuild path ────────────────────────────────────────────

        // Despawn previous text mesh children for this panel.
        for (entity, child_of, _, _) in &old_text {
            if child_of.parent() == panel_entity {
                commands.entity(entity).despawn();
            }
        }

        let scale_x = panel.world_width / panel.layout_width;
        let scale_y = panel.world_height / panel.layout_height;
        let half_w = panel.world_width * 0.5;
        let half_h = panel.world_height * 0.5;

        // Get the shared material handle (create once per atlas state).
        let Some(atlas_image) = atlas.image_handle().cloned() else {
            continue;
        };
        #[allow(clippy::cast_possible_truncation)]
        let material_handle = materials.add(super::msdf_material::msdf_text_material(
            atlas.sdf_range() as f32,
            atlas.width(),
            atlas.height(),
            atlas_image,
        ));

        // Batch all text quads into a single mesh per panel, and record the
        // emitted quad count on each text command for the color-only fast path.
        let result_mut = computed.result.as_mut().unwrap();
        let mut all_quads = Vec::new();
        for cmd in &mut result_mut.commands {
            let (text, config) = match &cmd.kind {
                RenderCommandKind::Text { text, config, .. } => (text.as_str(), config.clone()),
                _ => continue,
            };

            let quads = shape_text_to_quads(
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

            if let RenderCommandKind::Text { quad_count, .. } = &mut cmd.kind {
                *quad_count = quads.len();
            }

            all_quads.extend_from_slice(&quads);
        }

        if !all_quads.is_empty() {
            let mesh = build_glyph_mesh(&all_quads);
            let mesh_handle = meshes.add(mesh);

            commands.entity(panel_entity).with_child((
                DiegeticTextMesh,
                Mesh3d(mesh_handle),
                MeshMaterial3d(material_handle.clone()),
                Transform::IDENTITY,
            ));
        }
    }

    perf.last_text_extract_ms = start.elapsed().as_secs_f32() * 1000.0;
    perf.last_text_extract_panels = panel_count;
}

/// Finds the mesh handle of the existing text mesh child for a panel entity.
fn find_text_mesh_handle(
    old_text: &Query<(Entity, &ChildOf, &DiegeticTextMesh, &Mesh3d)>,
    panel_entity: Entity,
) -> Option<Handle<Mesh>> {
    old_text
        .iter()
        .find(|(_, child_of, _, _)| child_of.parent() == panel_entity)
        .map(|(_, _, _, mesh3d)| mesh3d.0.clone())
}

/// Builds a flat vertex color array from render commands using the stored
/// `quad_count` on each text command.
///
/// Each quad produces 4 vertices, all sharing the same color. The order
/// matches the original mesh construction in [`build_glyph_mesh`].
///
/// Uses the `quad_count` recorded during the last full mesh build rather
/// than the shaped glyph count, because `shape_text_to_quads` skips glyphs
/// without atlas entries (spaces, etc.). Using the shaped count would
/// misalign colors across text commands.
fn build_color_array(result: &crate::layout::LayoutResult) -> Vec<[f32; 4]> {
    let mut colors = Vec::new();
    for cmd in &result.commands {
        let (config, quad_count) = match &cmd.kind {
            RenderCommandKind::Text {
                config, quad_count, ..
            } => (config, *quad_count),
            _ => continue,
        };
        let linear: LinearRgba = config.color().into();
        let color_arr = [linear.red, linear.green, linear.blue, linear.alpha];
        for _ in 0..quad_count {
            // 4 vertices per quad.
            colors.push(color_arr);
            colors.push(color_arr);
            colors.push(color_arr);
            colors.push(color_arr);
        }
    }
    colors
}

/// Shapes text via parley, using the cache when possible.
///
/// On cache hit, returns the stored glyph run directly. On miss, shapes via
/// parley and inserts into the cache.
pub(crate) fn shape_text_cached(
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
    for line in layout.lines() {
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
    let run = ShapedTextRun { glyphs };
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
) -> Vec<GlyphQuadData> {
    let shaped = shape_text_cached(text, config, font_registry, shaping_cx, cache);

    let font_data = crate::text::EMBEDDED_FONT;
    let linear: LinearRgba = config.color().into();
    let color_arr = [linear.red, linear.green, linear.blue, linear.alpha];

    #[allow(clippy::cast_precision_loss)]
    let em_scale = config.size() / DEFAULT_CANONICAL_SIZE as f32;

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

        quads.push(GlyphQuadData {
            position: [local_x, local_y, TEXT_Z_OFFSET],
            size:     [quad_w * scale_x, quad_h * scale_y],
            uv_rect:  metrics.uv_rect,
            color:    color_arr,
        });
    }

    quads
}
