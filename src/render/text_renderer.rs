//! Text rendering system — extracts text from layout results and builds glyph meshes.

use std::collections::HashMap;
use std::hash::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::time::Instant;

use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::constants;
use super::glyph_quad;
use super::glyph_quad::GlyphQuadData;
use super::msdf_material::MsdfTextMaterial;
use super::panel_rtt::PanelRttRegistry;
use super::world_text::AwaitingReady;
use super::world_text::PanelTextChild;
use super::world_text::PendingGlyphs;
use super::world_text::WorldText;
use crate::layout::BoundingBox;
use crate::layout::FontFeatures;
use crate::layout::FontSlant::Italic;
use crate::layout::FontSlant::Normal;
use crate::layout::FontSlant::Oblique;
use crate::layout::GlyphLoadingPolicy;
use crate::layout::GlyphRenderMode;
use crate::layout::GlyphShadowMode;
use crate::layout::LayoutTextStyle;
use crate::layout::RenderCommandKind;
use crate::layout::TextDimensions;
use crate::layout::TextMeasure;
use crate::layout::WorldTextStyle;
use crate::plugin::ComputedDiegeticPanel;
use crate::plugin::DiegeticPanel;
use crate::plugin::DiegeticPerfStats;
use crate::plugin::HueOffset;
use crate::plugin::RenderMode;
use crate::plugin::UnitConfig;
use crate::text::Font;
use crate::text::FontId;
use crate::text::FontRegistry;
use crate::text::GlyphKey;
use crate::text::GlyphLookup;
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
    text_hash:     u64,
    font_id:       u16,
    /// Size quantized to avoid floating-point hash issues (size * 100 as u32).
    size_q:        u32,
    weight_q:      u32,
    slant:         u8,
    lh_q:          u32,
    ls_q:          i32,
    ws_q:          i32,
    font_features: FontFeatures,
}

impl ShapedCacheKey {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn new(text: &str, m: &TextMeasure) -> Self {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        Self {
            text_hash:     hasher.finish(),
            font_id:       m.font_id,
            size_q:        (m.size * 100.0) as u32,
            weight_q:      (m.weight.0 * 10.0) as u32,
            slant:         match m.slant {
                Normal => 0,
                Italic => 1,
                Oblique => 2,
            },
            lh_q:          (m.line_height * 100.0) as u32,
            ls_q:          (m.letter_spacing * 100.0) as i32,
            ws_q:          (m.word_spacing * 100.0) as i32,
            font_features: m.font_features,
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
    measurements: HashMap<ShapedCacheKey, TextDimensions>,
}

impl ShapedTextCache {
    /// Returns cached measurement dimensions for the given text + config,
    /// or `None` if not yet cached.
    #[must_use]
    pub fn get_measurement(&self, text: &str, measure: &TextMeasure) -> Option<TextDimensions> {
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

    /// Inserts a measurement result into the cache.
    pub fn insert_measurement(&mut self, text: &str, measure: &TextMeasure, dims: TextDimensions) {
        let key = ShapedCacheKey::new(text, measure);
        self.measurements.insert(key, dims);
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

/// Timing and queue diagnostics gathered while building text quads.
#[derive(Clone, Debug, Default)]
pub(super) struct TextBuildStats {
    pub texts:          usize,
    pub glyphs:         usize,
    pub ready_glyphs:   usize,
    pub queued_glyphs:  usize,
    pub pending_glyphs: usize,
    pub emitted_quads:  usize,
    pub shape_ms:       f32,
    pub atlas_ms:       f32,
}

impl TextBuildStats {
    pub(super) fn accumulate(&mut self, other: &Self) {
        self.texts += other.texts;
        self.glyphs += other.glyphs;
        self.ready_glyphs += other.ready_glyphs;
        self.queued_glyphs += other.queued_glyphs;
        self.pending_glyphs += other.pending_glyphs;
        self.emitted_quads += other.emitted_quads;
        self.shape_ms += other.shape_ms;
        self.atlas_ms += other.atlas_ms;
    }
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
        app.init_resource::<DiegeticPerfStats>();
        app.add_systems(
            PostUpdate,
            (
                super::panel_rtt::setup_panel_rtt,
                poll_atlas_glyphs,
                reconcile_panel_text_children
                    .after(poll_atlas_glyphs)
                    .after(super::panel_rtt::setup_panel_rtt),
                reconcile_panel_image_children
                    .after(poll_atlas_glyphs)
                    .after(super::panel_rtt::setup_panel_rtt),
                shape_panel_text_children
                    .after(reconcile_panel_text_children)
                    .after(poll_atlas_glyphs),
                build_panel_batched_meshes.after(shape_panel_text_children),
                sync_panel_hue_offset.after(build_panel_batched_meshes),
                super::world_text::render_world_text.after(poll_atlas_glyphs),
                super::world_text::emit_world_text_ready
                    .after(bevy::camera::visibility::VisibilitySystems::CalculateBounds),
            ),
        );
    }
}

/// Polls completed async glyph rasterizations, inserts them into the
/// atlas, and syncs to GPU. Entities with [`PendingGlyphs`] will be
/// re-checked by `shape_panel_text_children` and `render_world_text`.
fn poll_atlas_glyphs(
    mut atlas: ResMut<MsdfAtlas>,
    mut images: ResMut<Assets<Image>>,
    mut shared_mats: ResMut<SharedMsdfMaterials>,
    mut perf: ResMut<DiegeticPerfStats>,
) {
    let poll_start = Instant::now();
    let poll_stats = atlas.poll_async_glyphs_stats();
    let poll_ms = poll_start.elapsed().as_secs_f32() * 1000.0;
    let dirty_pages = atlas.dirty_page_count();
    let mut sync_ms = 0.0;

    if poll_stats.inserted > 0 || poll_stats.invisible > 0 {
        let sync_start = Instant::now();
        atlas.sync_to_gpu(&mut images);
        sync_ms = sync_start.elapsed().as_secs_f32() * 1000.0;
        // Invalidate all shared materials so they get recreated with
        // updated atlas textures on the next `build_panel_batched_meshes` run.
        shared_mats.handles.clear();
    }

    perf.last_atlas_poll_ms = poll_ms;
    perf.last_atlas_sync_ms = sync_ms;
    perf.last_atlas_completed_glyphs = poll_stats.completed;
    perf.last_atlas_inserted_glyphs = poll_stats.inserted;
    perf.last_atlas_invisible_glyphs = poll_stats.invisible;
    perf.last_atlas_pages_added = poll_stats.pages_added;
    perf.last_atlas_dirty_pages = dirty_pages;
    perf.last_atlas_in_flight_glyphs = atlas.in_flight_count();
    perf.last_atlas_active_jobs = atlas.active_job_count();
    perf.last_atlas_peak_active_jobs = atlas.peak_active_job_count();
    perf.last_atlas_worker_threads = poll_stats.worker_threads;
    perf.last_atlas_avg_raster_ms = poll_stats.avg_raster_ms;
    perf.last_atlas_max_raster_ms = poll_stats.max_raster_ms;
    perf.last_atlas_batch_max_active_jobs = poll_stats.max_active_jobs;
    perf.last_atlas_total_glyphs = atlas.glyph_count();

    if poll_stats.completed > 0 || sync_ms > 0.0 {
        bevy::log::debug!(
            "poll_atlas_glyphs: poll={poll_ms:.2}ms sync={sync_ms:.2}ms completed={} inserted={} invisible={} pages_added={} dirty_pages={} in_flight={} active_jobs={} peak_active={} workers={} avg_raster={:.2}ms max_raster={:.2}ms batch_max_active={} total_glyphs={}",
            poll_stats.completed,
            poll_stats.inserted,
            poll_stats.invisible,
            poll_stats.pages_added,
            dirty_pages,
            atlas.in_flight_count(),
            atlas.active_job_count(),
            atlas.peak_active_job_count(),
            poll_stats.worker_threads,
            poll_stats.avg_raster_ms,
            poll_stats.max_raster_ms,
            poll_stats.max_active_jobs,
            atlas.glyph_count(),
        );
    }
}

// ── Panel text quad storage ──────────────────────────────────────────────────

/// Stores shaped glyph quads for a panel [`WorldText`] child, along with its
/// render and shadow modes for batching into combined meshes.
#[derive(Component)]
pub(super) struct PanelTextQuads {
    /// Per-glyph quads keyed by atlas page index.
    pub quads:       Vec<(u32, GlyphQuadData)>,
    /// The glyph render mode for this text element.
    pub render_mode: GlyphRenderMode,
    /// The glyph shadow mode for this text element.
    pub shadow_mode: GlyphShadowMode,
}

// ── System 1: reconcile_panel_text_children ─────────────────────────────────

/// Reconciles [`WorldText`] children for each changed [`ComputedDiegeticPanel`].
///
/// For each panel whose layout changed:
/// 1. Collects all `RenderCommandKind::Text` commands.
/// 2. Diffs against existing [`PanelTextChild`] children by `element_idx`.
/// 3. Updates, spawns, or despawns children as needed.
#[allow(clippy::too_many_arguments)]
fn reconcile_panel_text_children(
    changed_panels: Query<
        (Entity, &DiegeticPanel, &ComputedDiegeticPanel),
        Changed<ComputedDiegeticPanel>,
    >,
    existing_children: Query<(Entity, &PanelTextChild, &ChildOf)>,
    mut commands: Commands,
    unit_config: Res<UnitConfig>,
) {
    for (panel_entity, panel, computed) in &changed_panels {
        let Some(result) = computed.result() else {
            continue;
        };

        // Layout output is in points. Convert to world meters
        // (incorporates world_width/world_height scaling).
        let pts_mpu = panel.points_to_world(&unit_config);
        let scale_x = pts_mpu;
        let scale_y = pts_mpu;
        let (anchor_x, anchor_y) = panel.anchor_offsets(&unit_config);

        // Collect text commands from layout result, preserving the
        // command index for Z-offset layering in Geometry mode.
        let text_commands: Vec<_> = result
            .commands
            .iter()
            .enumerate()
            .filter_map(|(cmd_index, cmd)| match &cmd.kind {
                RenderCommandKind::Text { text, config } => Some((
                    cmd.element_idx,
                    cmd_index,
                    text.clone(),
                    config.clone(),
                    cmd.bounds,
                )),
                _ => None,
            })
            .collect();

        // Build a map of existing children by `element_idx`.
        let mut existing_by_idx: HashMap<usize, Entity> = HashMap::new();
        for (entity, ptc, child_of) in &existing_children {
            if child_of.parent() == panel_entity {
                existing_by_idx.insert(ptc.element_idx, entity);
            }
        }

        // Track which existing indices we visited so we can despawn extras.
        let mut visited_indices: Vec<usize> = Vec::new();

        for (element_idx, cmd_index, text, config, bounds) in &text_commands {
            let style = config.as_standalone();
            let ptc = PanelTextChild {
                element_idx: *element_idx,
                command_index: *cmd_index,
                bounds: *bounds,
                scale_x,
                scale_y,
                anchor_x,
                anchor_y,
            };

            visited_indices.push(*element_idx);

            if let Some(&child_entity) = existing_by_idx.get(element_idx) {
                // Update existing child.
                commands
                    .entity(child_entity)
                    .insert((WorldText(text.clone()), style, ptc));
            } else {
                // Spawn new child.
                commands
                    .entity(panel_entity)
                    .with_child((WorldText(text.clone()), style, ptc));
            }
        }

        // Despawn children whose `element_idx` is no longer present.
        for (entity, ptc, child_of) in &existing_children {
            if child_of.parent() == panel_entity && !visited_indices.contains(&ptc.element_idx) {
                commands.entity(entity).despawn();
            }
        }
    }
}

// ── System 1b: reconcile_panel_image_children ───────────────────────────────

/// Marker on image child entities spawned by the panel image reconciler.
#[derive(Component, Clone, Debug)]
pub(super) struct PanelImageChild {
    /// Index of the source element in the layout tree.
    pub element_idx: usize,
}

/// Reconciles image children for each changed [`ComputedDiegeticPanel`].
///
/// For each `RenderCommandKind::Image` in the layout result, spawns or
/// updates a child entity with `Mesh3d` + `MeshMaterial3d` using the
/// image handle and tint from the command.
fn reconcile_panel_image_children(
    changed_panels: Query<
        (Entity, &DiegeticPanel, &ComputedDiegeticPanel),
        Changed<ComputedDiegeticPanel>,
    >,
    existing_children: Query<(Entity, &PanelImageChild, &ChildOf)>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    unit_config: Res<UnitConfig>,
    rtt_registry: Res<PanelRttRegistry>,
) {
    for (panel_entity, panel, computed) in &changed_panels {
        let Some(result) = computed.result() else {
            continue;
        };

        let pts_mpu = panel.points_to_world(&unit_config);
        let (anchor_x, anchor_y) = panel.anchor_offsets(&unit_config);
        let layer = rtt_registry
            .get_layer(panel_entity)
            .map_or(RenderLayers::layer(0), RenderLayers::layer);

        // Collect image commands.
        let image_commands: Vec<_> = result
            .commands
            .iter()
            .filter_map(|cmd| match &cmd.kind {
                RenderCommandKind::Image { handle, tint } => {
                    Some((cmd.element_idx, handle.clone(), *tint, cmd.bounds))
                },
                _ => None,
            })
            .collect();

        // Build a map of existing image children by `element_idx`.
        let mut existing_by_idx: HashMap<usize, Entity> = HashMap::new();
        for (entity, pic, child_of) in &existing_children {
            if child_of.parent() == panel_entity {
                existing_by_idx.insert(pic.element_idx, entity);
            }
        }

        let mut visited_indices: Vec<usize> = Vec::new();

        for (element_idx, handle, tint, bounds) in &image_commands {
            visited_indices.push(*element_idx);

            // Convert layout bounds to world-space dimensions.
            let world_w = bounds.width * pts_mpu;
            let world_h = bounds.height * pts_mpu;
            let world_x = bounds.x.mul_add(pts_mpu, world_w * 0.5) - anchor_x;
            let world_y = -(bounds.y.mul_add(pts_mpu, world_h * 0.5) - anchor_y);

            let mesh_handle = meshes.add(Rectangle::new(world_w, world_h));
            let material_handle = materials.add(StandardMaterial {
                base_color: *tint,
                base_color_texture: Some(handle.clone()),
                unlit: true,
                double_sided: true,
                cull_mode: None,
                alpha_mode: AlphaMode::Blend,
                ..default()
            });

            let transform = Transform::from_xyz(world_x, world_y, TEXT_Z_OFFSET);

            if let Some(&child_entity) = existing_by_idx.get(element_idx) {
                commands.entity(child_entity).insert((
                    PanelImageChild {
                        element_idx: *element_idx,
                    },
                    Mesh3d(mesh_handle),
                    MeshMaterial3d(material_handle),
                    transform,
                    layer.clone(),
                ));
            } else {
                commands.entity(panel_entity).with_child((
                    PanelImageChild {
                        element_idx: *element_idx,
                    },
                    Mesh3d(mesh_handle),
                    MeshMaterial3d(material_handle),
                    transform,
                    layer.clone(),
                ));
            }
        }

        // Despawn image children no longer present.
        for (entity, pic, child_of) in &existing_children {
            if child_of.parent() == panel_entity && !visited_indices.contains(&pic.element_idx) {
                commands.entity(entity).despawn();
            }
        }
    }
}

// ── System 2: shape_panel_text_children ─────────────────────────────────────

/// Shapes text for panel [`WorldText`] children that are changed or pending.
///
/// For each eligible entity:
/// 1. Calls [`shape_text_to_quads`] using the [`PanelTextChild`] scale data.
/// 2. If all glyphs are ready, stores results in [`PanelTextQuads`] and removes [`PendingGlyphs`].
/// 3. If glyphs are still pending, inserts [`PendingGlyphs`].
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn shape_panel_text_children(
    changed_texts: Query<
        Entity,
        (
            With<PanelTextChild>,
            With<WorldText>,
            Or<(
                Changed<WorldText>,
                Changed<WorldTextStyle>,
                Changed<PanelTextChild>,
            )>,
        ),
    >,
    pending_texts: Query<Entity, (With<PanelTextChild>, With<WorldText>, With<PendingGlyphs>)>,
    texts: Query<(&WorldText, &WorldTextStyle, &PanelTextChild)>,
    mut atlas: ResMut<MsdfAtlas>,
    font_registry: Res<FontRegistry>,
    shaping_cx: Res<TextShapingContext>,
    mut cache: ResMut<ShapedTextCache>,
    mut commands: Commands,
) {
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

    for entity in to_process {
        let Ok((world_text, style, ptc)) = texts.get(entity) else {
            continue;
        };

        if world_text.0.is_empty() {
            commands
                .entity(entity)
                .remove::<PendingGlyphs>()
                .remove::<PanelTextQuads>();
            continue;
        }

        let config = style.as_layout_config();

        let (quads, stats) = shape_text_to_quads(
            &world_text.0,
            &config,
            &ptc.bounds,
            &font_registry,
            &mut atlas,
            &shaping_cx,
            &mut cache,
            ptc.scale_x,
            ptc.scale_y,
            ptc.anchor_x,
            ptc.anchor_y,
        );

        let all_ready = stats.glyphs > 0 && stats.ready_glyphs == stats.glyphs;
        let has_pending = stats.pending_glyphs > 0 || stats.queued_glyphs > 0;

        if all_ready {
            commands.entity(entity).insert(PanelTextQuads {
                quads,
                render_mode: config.render_mode(),
                shadow_mode: config.shadow_mode(),
            });
            commands.entity(entity).remove::<PendingGlyphs>();
            commands.entity(entity).insert(AwaitingReady);
        } else if has_pending {
            commands.entity(entity).insert_if_new(PendingGlyphs);
        }
    }
}

// ── System 3: build_panel_batched_meshes ────────────────────────────────────

/// Builds batched meshes for panels whose [`WorldText`] children have changed
/// [`PanelTextQuads`].
///
/// For each affected panel:
/// 1. Collects all [`PanelTextQuads`] from children.
/// 2. Groups quads by [`TextBatchKey`] (render mode, shadow mode, page index).
/// 3. Despawns old [`DiegeticTextMesh`] / [`DiegeticShadowProxy`] children.
/// 4. Spawns new batched mesh entities via [`spawn_batch_meshes`].
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn build_panel_batched_meshes(
    changed_quads: Query<&ChildOf, (With<PanelTextChild>, Changed<PanelTextQuads>)>,
    panel_children: Query<(&PanelTextQuads, &PanelTextChild, &ChildOf)>,
    old_meshes: Query<(Entity, &ChildOf), Or<(With<DiegeticTextMesh>, With<DiegeticShadowProxy>)>>,
    panels: Query<(&DiegeticPanel, Option<&HueOffset>, Option<&RenderLayers>)>,
    atlas: Res<MsdfAtlas>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MsdfTextMaterial>>,
    mut shared_mats: ResMut<SharedMsdfMaterials>,
    rtt_registry: Res<PanelRttRegistry>,
    mut commands: Commands,
) {
    // Collect the set of panel entities that have at least one child with
    // changed `PanelTextQuads`.
    let mut dirty_panels: Vec<Entity> = Vec::new();
    for child_of in &changed_quads {
        let panel_entity = child_of.parent();
        if !dirty_panels.contains(&panel_entity) {
            dirty_panels.push(panel_entity);
        }
    }

    // Clear the shared materials cache so material property changes
    // (like unlit toggling) create fresh materials instead of reusing
    // stale cached ones.
    if !dirty_panels.is_empty() {
        shared_mats.handles.clear();
    }

    for panel_entity in dirty_panels {
        let Ok((panel, hue_offset, panel_layers)) = panels.get(panel_entity) else {
            continue;
        };
        let hue = hue_offset.map_or(0.0, |h| h.0);
        let is_geometry = panel.render_mode == RenderMode::Geometry;
        let scene_layer = panel_layers.cloned().unwrap_or(RenderLayers::layer(0));

        // Collect all quads from this panel's `PanelTextChild` children
        // and track the maximum command index for Z-offset layering.
        let mut batches: HashMap<TextBatchKey, Vec<GlyphQuadData>> = HashMap::new();
        let mut max_command_index: usize = 0;
        for (ptq, ptc, child_of) in &panel_children {
            if child_of.parent() != panel_entity {
                continue;
            }
            max_command_index = max_command_index.max(ptc.command_index);
            for (page_index, quad) in &ptq.quads {
                let key = TextBatchKey {
                    render_mode: ptq.render_mode,
                    shadow_mode: ptq.shadow_mode,
                    page_index:  *page_index,
                };
                batches.entry(key).or_default().push(*quad);
            }
        }

        let total_quads: usize = batches.values().map(Vec::len).sum();
        if total_quads == 0 {
            continue;
        }

        // Despawn previous mesh children.
        for (mesh_entity, child_of) in &old_meshes {
            if child_of.parent() == panel_entity {
                commands.entity(mesh_entity).despawn();
            }
        }

        let content_layer = rtt_registry
            .get_layer(panel_entity)
            .map_or_else(|| scene_layer.clone(), RenderLayers::layer);

        // Resolve the base StandardMaterial for text in this panel.
        let mut text_base = panel
            .text_material
            .clone()
            .unwrap_or_else(constants::default_panel_material);
        text_base.alpha_mode = AlphaMode::Blend;
        text_base.double_sided = true;
        text_base.cull_mode = None;
        if !is_geometry {
            text_base.unlit = true;
        }

        // Text Z offset: use the max command index so text renders
        // on top of all geometry at or below that index.
        #[allow(clippy::cast_precision_loss)]
        let text_z = if is_geometry {
            max_command_index as f32 * constants::LAYER_Z_STEP
        } else {
            0.0
        };

        spawn_batch_meshes(
            &batches,
            panel_entity,
            hue,
            &atlas,
            &mut meshes,
            &mut materials,
            &mut shared_mats,
            &content_layer,
            &scene_layer,
            &text_base,
            text_z,
            &mut commands,
        );
    }
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
    content_layer: &RenderLayers,
    scene_layer: &RenderLayers,
    text_base: &StandardMaterial,
    text_z: f32,
    commands: &mut Commands,
) {
    for (key, quads) in batches {
        if quads.is_empty() {
            continue;
        }

        let Some(page_image) = atlas.image_handle(key.page_index).cloned() else {
            continue;
        };

        let mesh = glyph_quad::build_glyph_mesh(quads);
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
                                text_base.clone(),
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
                        text_base.clone(),
                        atlas.sdf_range() as f32,
                        atlas.width(),
                        atlas.height(),
                        page_image.clone(),
                        hue,
                        render_mode_u32,
                    ))
                };

            let text_transform = Transform::from_xyz(0.0, 0.0, text_z);

            if suppress_shadow {
                commands.entity(panel_entity).with_child((
                    DiegeticTextMesh,
                    NotShadowCaster,
                    Mesh3d(mesh_handle.clone()),
                    MeshMaterial3d(material_handle),
                    text_transform,
                    content_layer.clone(),
                ));
            } else {
                commands.entity(panel_entity).with_child((
                    DiegeticTextMesh,
                    Mesh3d(mesh_handle.clone()),
                    MeshMaterial3d(material_handle),
                    text_transform,
                    content_layer.clone(),
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
                text_base.clone(),
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
                Transform::from_xyz(0.0, 0.0, text_z),
                scene_layer.clone(),
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
    config: &LayoutTextStyle,
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
    if config.line_height_raw() > 0.0 {
        builder.push_default(parley::style::StyleProperty::LineHeight(
            parley::style::LineHeight::Absolute(config.line_height_raw()),
        ));
    }

    // Push OpenType feature overrides (liga, calt, dlig, kern).
    let font_features = config.font_features();
    if !font_features.is_default() {
        let parley_features: Vec<parley::style::FontFeature> = font_features
            .to_parley_settings()
            .into_iter()
            .map(|(tag, value)| parley::swash::Setting {
                tag: parley::swash::tag_from_bytes(&tag),
                value,
            })
            .collect();
        builder.push_default(parley::style::StyleProperty::FontFeatures(
            parley::style::FontSettings::List(std::borrow::Cow::Owned(parley_features)),
        ));
    }

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
        width:       layout.full_width(),
        height:      layout.height(),
        line_height: layout
            .lines()
            .next()
            .map_or_else(|| config.size(), |l| l.metrics().line_height),
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
    config: &LayoutTextStyle,
    bounds: &BoundingBox,
    font_registry: &FontRegistry,
    atlas: &mut MsdfAtlas,
    shaping_cx: &TextShapingContext,
    cache: &mut ShapedTextCache,
    scale_x: f32,
    scale_y: f32,
    anchor_x: f32,
    anchor_y: f32,
) -> (Vec<(u32, GlyphQuadData)>, TextBuildStats) {
    let mut stats = TextBuildStats {
        texts: 1,
        ..Default::default()
    };
    let shape_start = Instant::now();
    let shaped = shape_text_cached(text, config, font_registry, shaping_cx, cache);
    stats.shape_ms = shape_start.elapsed().as_secs_f32() * 1000.0;
    stats.glyphs = shaped.glyphs.len();

    let font_data = font_registry
        .font(FontId(config.font_id()))
        .map_or(crate::text::EMBEDDED_FONT, Font::data);

    let atlas_start = Instant::now();
    // Under `WhenReady`, trigger async rasterization for every glyph but
    // emit nothing until the entire string is cached in the atlas.
    if config.loading_policy() == GlyphLoadingPolicy::WhenReady {
        let mut all_ready = true;
        for sg in &shaped.glyphs {
            let glyph_key = GlyphKey {
                font_id:     config.font_id(),
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
            return (Vec::new(), stats);
        }
    }

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

        // Convert to panel-local (anchor origin, Y-up).
        let local_x = quad_layout_x.mul_add(scale_x, -anchor_x);
        let local_y = (-quad_layout_y).mul_add(scale_y, anchor_y);

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
    stats.atlas_ms = atlas_start.elapsed().as_secs_f32() * 1000.0;
    stats.emitted_quads = quads.len();

    (quads, stats)
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
        let base = StandardMaterial::default();
        let shared_handle = materials.add(super::super::msdf_material::msdf_text_material(
            base.clone(),
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
                    base.clone(),
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
