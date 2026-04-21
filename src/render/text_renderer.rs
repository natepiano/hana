//! Text rendering system — extracts text from layout results and builds glyph meshes.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::time::Instant;

use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::math::Vec4;
use bevy::prelude::*;
use bevy_kana::ToF32;
use bevy_kana::ToU16;

use super::clip;
use super::constants;
use super::constants::TEXT_Z_OFFSET;
use super::glyph_quad;
use super::glyph_quad::GlyphQuadData;
use super::msdf_material;
use super::msdf_material::MsdfTextMaterial;
use super::panel_rtt;
use super::panel_rtt::PanelRttRegistry;
use super::world_text;
use super::world_text::AwaitingReady;
use super::world_text::PanelTextChild;
use super::world_text::PendingGlyphs;
use super::world_text::WorldText;
use crate::cascade::CascadeDefaults;
use crate::cascade::CascadeEntityPlugin;
use crate::cascade::CascadePanelChild;
use crate::cascade::CascadePanelChildPlugin;
use crate::cascade::Resolved;
use crate::constants::MILLISECONDS_PER_SECOND;
use crate::layout::BoundingBox;
use crate::layout::GlyphLoadingPolicy;
use crate::layout::GlyphRenderMode;
use crate::layout::GlyphShadowMode;
use crate::layout::LayoutTextStyle;
use crate::layout::LineMetricsSnapshot;
use crate::layout::RenderCommandKind;
use crate::layout::ShapedGlyph;
use crate::layout::ShapedTextCache;
use crate::layout::ShapedTextRun;
use crate::layout::UnitConfig;
use crate::layout::WorldTextStyle;
use crate::panel::ComputedDiegeticPanel;
use crate::panel::DiegeticPanel;
use crate::panel::DiegeticPerfStats;
use crate::panel::HueOffset;
use crate::panel::RenderMode;
use crate::text::Font;
use crate::text::FontId;
use crate::text::FontRegistry;
use crate::text::GlyphKey;
use crate::text::GlyphLookup;
use crate::text::MsdfAtlas;

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
    render_mode:     GlyphRenderMode,
    shadow_mode:     GlyphShadowMode,
    page_index:      u32,
    clip_rect:       [u32; 4],
    alpha_mode_bits: u64,
}

/// Key for shared zero-hue MSDF text materials.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct SharedMsdfMaterialKey {
    page_index:      u32,
    clip_rect:       [u32; 4],
    depth_bias_bits: u32,
    alpha_mode_bits: u64,
}

/// Encodes an [`AlphaMode`] into a hashable `u64` so it can be used as a key
/// component. Covers all Bevy 0.18 variants.
#[must_use]
fn alpha_mode_bits(mode: AlphaMode) -> u64 {
    match mode {
        AlphaMode::Opaque => 1,
        AlphaMode::Mask(t) => 2 | (u64::from(t.to_bits()) << 8),
        AlphaMode::Blend => 3,
        AlphaMode::Premultiplied => 4,
        AlphaMode::AlphaToCoverage => 5,
        AlphaMode::Add => 6,
        AlphaMode::Multiply => 7,
    }
}

/// Cached default material handles shared across panels without a
/// [`HueOffset`] component. Keyed by atlas page index, clip rect, and
/// depth bias so each shared material keeps the correct ordering state.
#[derive(Resource, Default)]
struct SharedMsdfMaterials {
    handles: HashMap<SharedMsdfMaterialKey, Handle<MsdfTextMaterial>>,
}

#[must_use]
fn panel_clip_rect_local(
    clip_rect: Option<BoundingBox>,
    scale_x: f32,
    scale_y: f32,
    anchor_x: f32,
    anchor_y: f32,
) -> Vec4 {
    clip_rect.map_or(constants::UNCLIPPED_TEXT_CLIP_RECT, |clip| {
        Vec4::new(
            clip.x.mul_add(scale_x, -anchor_x),
            (clip.y + clip.height).mul_add(-scale_y, anchor_y),
            (clip.x + clip.width).mul_add(scale_x, -anchor_x),
            clip.y.mul_add(-scale_y, anchor_y),
        )
    })
}

#[must_use]
fn clip_rect_bits(clip_rect: Vec4) -> [u32; 4] {
    [
        clip_rect.x.to_bits(),
        clip_rect.y.to_bits(),
        clip_rect.z.to_bits(),
        clip_rect.w.to_bits(),
    ]
}

#[must_use]
const fn clip_rect_from_bits(bits: [u32; 4]) -> Vec4 {
    Vec4::new(
        f32::from_bits(bits[0]),
        f32::from_bits(bits[1]),
        f32::from_bits(bits[2]),
        f32::from_bits(bits[3]),
    )
}

#[must_use]
const fn glyph_render_mode_uniform(render_mode: GlyphRenderMode) -> u32 {
    match render_mode {
        GlyphRenderMode::Invisible => 0,
        GlyphRenderMode::Text => 1,
        GlyphRenderMode::PunchOut => 2,
        GlyphRenderMode::SolidQuad => 3,
    }
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
pub(super) struct TextRenderPlugin;

impl Plugin for TextRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<MsdfTextMaterial>::default());
        app.add_plugins(CascadePanelChildPlugin::<PanelTextAlpha>::default());
        app.add_plugins(CascadeEntityPlugin::<world_text::WorldTextAlpha>::default());
        app.add_plugins(CascadeEntityPlugin::<world_text::WorldFontUnit>::default());
        app.init_resource::<TextShapingContext>();
        app.init_resource::<ShapedTextCache>();
        app.init_resource::<SharedMsdfMaterials>();
        app.init_resource::<DiegeticPerfStats>();
        app.add_systems(
            PostUpdate,
            (
                panel_rtt::setup_panel_rtt,
                poll_atlas_glyphs,
                reconcile_panel_text_children
                    .after(poll_atlas_glyphs)
                    .after(panel_rtt::setup_panel_rtt),
                reconcile_panel_image_children
                    .after(poll_atlas_glyphs)
                    .after(panel_rtt::setup_panel_rtt),
                shape_panel_text_children
                    .after(reconcile_panel_text_children)
                    .after(poll_atlas_glyphs),
                build_panel_batched_meshes.after(shape_panel_text_children),
                sync_panel_hue_offset.after(build_panel_batched_meshes),
                world_text::render_world_text.after(poll_atlas_glyphs),
                world_text::emit_world_text_ready
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
    let poll_ms = poll_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    let dirty_pages = atlas.dirty_page_count();
    let mut sync_ms = 0.0;

    if poll_stats.inserted > 0 || poll_stats.invisible > 0 {
        let sync_start = Instant::now();
        atlas.sync_to_gpu(&mut images);
        sync_ms = sync_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
        // Invalidate all shared materials so they get recreated with
        // updated atlas textures on the next `build_panel_batched_meshes` run.
        shared_mats.handles.clear();
    }

    perf.atlas.poll_ms = poll_ms;
    perf.atlas.sync_ms = sync_ms;
    perf.atlas.completed_glyphs = poll_stats.completed;
    perf.atlas.inserted_glyphs = poll_stats.inserted;
    perf.atlas.invisible_glyphs = poll_stats.invisible;
    perf.atlas.pages_added = poll_stats.pages_added;
    perf.atlas.dirty_pages = dirty_pages;
    perf.atlas.in_flight_glyphs = atlas.in_flight_count();
    perf.atlas.active_jobs = atlas.active_job_count();
    perf.atlas.peak_active_jobs = atlas.peak_active_job_count();
    perf.atlas.worker_threads = poll_stats.worker_threads;
    perf.atlas.avg_raster_ms = poll_stats.avg_raster_ms;
    perf.atlas.max_raster_ms = poll_stats.max_raster_ms;
    perf.atlas.batch_max_active_jobs = poll_stats.max_active_jobs;
    perf.atlas.total_glyphs = atlas.glyph_count();

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
    /// Per-style alpha-mode override (from `LayoutTextStyle`). `None` means
    /// the entity inherits from its parent panel, which in turn inherits from
    /// [`CascadeDefaults::text_alpha`]. Resolution is cached in
    /// [`Resolved<PanelTextAlpha>`].
    pub alpha_mode:  Option<AlphaMode>,
}

/// Cascading attribute for panel-text alpha mode.
///
/// 3-tier cascade: [`PanelTextQuads::alpha_mode`] (entity) →
/// [`DiegeticPanel::text_alpha_mode`] (panel) →
/// [`CascadeDefaults::text_alpha`] (global). The final resolved value is
/// cached in [`Resolved<PanelTextAlpha>`] on each panel and each text child;
/// [`build_panel_meshes_for_entity`] reads it to key material batches by
/// alpha mode.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub(super) struct PanelTextAlpha(pub AlphaMode);

impl CascadePanelChild for PanelTextAlpha {
    type EntityOverride = PanelTextQuads;
    type PanelOverride = DiegeticPanel;

    fn entity_value(entity_override: &PanelTextQuads) -> Option<Self> {
        entity_override.alpha_mode.map(Self)
    }

    fn panel_value(panel_override: &DiegeticPanel) -> Option<Self> {
        panel_override.text_alpha_mode().map(Self)
    }

    fn global_default(defaults: &CascadeDefaults) -> Self { Self(defaults.text_alpha) }
}

// ── System 1: reconcile_panel_text_children ─────────────────────────────────

/// Reconciles [`WorldText`] children for each changed [`ComputedDiegeticPanel`].
///
/// For each panel whose layout changed:
/// 1. Collects all `RenderCommandKind::Text` commands.
/// 2. Diffs against existing [`PanelTextChild`] children by `element_idx`.
/// 3. Updates, spawns, or despawns children as needed.
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
        let points_to_world = panel.points_to_world(&unit_config);
        let scale_x = points_to_world;
        let scale_y = points_to_world;
        let (anchor_x, anchor_y) = panel.anchor_offsets(&unit_config);

        // Collect text commands from layout result, preserving the
        // command index for Z-offset layering in Geometry mode.
        let clip_rects = clip::compute_clip_rects(&result.commands);
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
                    clip_rects[cmd_index],
                )),
                _ => None,
            })
            .collect();

        // Build a map of existing children by (element_idx, command_index).
        // Wrapped text produces multiple commands with the same element_idx but
        // distinct command_index (one per wrapped line); keying by element_idx
        // alone collapses them and leaks stale entities.
        let mut existing_by_key: HashMap<(usize, usize), Entity> = HashMap::new();
        for (entity, ptc, child_of) in &existing_children {
            if child_of.parent() == panel_entity {
                existing_by_key.insert((ptc.element_idx, ptc.command_index), entity);
            }
        }

        // Track which existing (idx, cmd) pairs we visited so we can despawn extras.
        let mut visited_keys: Vec<(usize, usize)> = Vec::new();

        for (element_idx, cmd_index, text, config, bounds, clip) in &text_commands {
            let style = config.as_standalone();
            let ptc = PanelTextChild {
                element_idx: *element_idx,
                command_index: *cmd_index,
                bounds: *bounds,
                scale_x,
                scale_y,
                anchor_x,
                anchor_y,
                clip_rect: *clip,
            };

            let key = (*element_idx, *cmd_index);
            visited_keys.push(key);

            if let Some(&child_entity) = existing_by_key.get(&key) {
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

        // Despawn children whose (element_idx, command_index) is no longer present.
        for (entity, ptc, child_of) in &existing_children {
            if child_of.parent() == panel_entity
                && !visited_keys.contains(&(ptc.element_idx, ptc.command_index))
            {
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

        let points_to_world = panel.points_to_world(&unit_config);
        let (anchor_x, anchor_y) = panel.anchor_offsets(&unit_config);
        let layer = rtt_registry
            .get_layer(panel_entity)
            .map_or(RenderLayers::layer(0), RenderLayers::layer);
        let is_geometry = panel.render_mode() == RenderMode::Geometry;

        // Collect image commands, skipping those entirely outside their clip rect.
        let clip_rects = clip::compute_clip_rects(&result.commands);
        let image_commands: Vec<_> = result
            .commands
            .iter()
            .enumerate()
            .filter_map(|(cmd_index, cmd)| match &cmd.kind {
                RenderCommandKind::Image { handle, tint } => {
                    let clip = clip_rects[cmd_index];
                    if clip.is_some_and(|c| cmd.bounds.intersect(&c).is_none()) {
                        None
                    } else {
                        Some((
                            cmd_index,
                            cmd.element_idx,
                            handle.clone(),
                            *tint,
                            cmd.bounds,
                        ))
                    }
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

        for (cmd_index, element_idx, handle, tint, bounds) in &image_commands {
            visited_indices.push(*element_idx);

            // Convert layout bounds to world-space dimensions.
            let world_w = bounds.width * points_to_world;
            let world_h = bounds.height * points_to_world;
            let world_x = bounds.x.mul_add(points_to_world, world_w * 0.5) - anchor_x;
            let world_y = -(bounds.y.mul_add(points_to_world, world_h * 0.5) - anchor_y);

            let mesh_handle = meshes.add(Rectangle::new(world_w, world_h));
            let material_handle = materials.add(StandardMaterial {
                base_color: *tint,
                base_color_texture: Some(handle.clone()),
                unlit: true,
                double_sided: true,
                cull_mode: None,
                alpha_mode: AlphaMode::Blend,
                depth_bias: if is_geometry {
                    cmd_index.to_f32() * constants::LAYER_DEPTH_BIAS
                } else {
                    0.0
                },
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
                Changed<Resolved<PanelTextAlpha>>,
            )>,
        ),
    >,
    pending_texts: Query<Entity, (With<PanelTextChild>, With<WorldText>, With<PendingGlyphs>)>,
    texts: Query<(&WorldText, &WorldTextStyle, &PanelTextChild, &ChildOf)>,
    panel_alpha: Query<&Resolved<PanelTextAlpha>, With<DiegeticPanel>>,
    defaults: Res<CascadeDefaults>,
    mut atlas: ResMut<MsdfAtlas>,
    font_registry: Res<FontRegistry>,
    shaping_cx: Res<TextShapingContext>,
    mut cache: ResMut<ShapedTextCache>,
    mut perf: ResMut<DiegeticPerfStats>,
    mut commands: Commands,
) {
    let shape_stage_start = Instant::now();
    let mut agg = TextBuildStats::default();
    let mut shaped_panels: std::collections::HashSet<Entity> = std::collections::HashSet::new();

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
        perf.panel_text.shape_ms = 0.0;
        perf.panel_text.parley_ms = 0.0;
        perf.panel_text.atlas_lookup_ms = 0.0;
        perf.panel_text.shaped_panels = 0;
        perf.panel_text.queued_glyphs = 0;
        perf.panel_text.pending_glyphs = 0;
        perf.panel_text.total_ms = perf.panel_text.mesh_build_ms;
        return;
    }

    for entity in to_process {
        let Ok((world_text, style, ptc, child_of)) = texts.get(entity) else {
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
            ptc.clip_rect,
        );

        agg.accumulate(&stats);
        shaped_panels.insert(child_of.parent());

        let all_ready = stats.glyphs > 0 && stats.ready_glyphs == stats.glyphs;
        let has_pending = stats.pending_glyphs > 0 || stats.queued_glyphs > 0;

        if all_ready {
            let ptq = PanelTextQuads {
                quads,
                render_mode: config.render_mode(),
                shadow_mode: config.shadow_mode(),
                alpha_mode: config.alpha_mode(),
            };
            // Tier-1 re-resolve: `PanelTextQuads.alpha_mode` is recomputed
            // from `LayoutTextStyle` every shape pass, so write a fresh
            // `Resolved<PanelTextAlpha>` alongside the quads. Falls through
            // to the parent panel's `Resolved<PanelTextAlpha>` (or the
            // global default if the parent lookup fails).
            let panel_fallback = panel_alpha.get(child_of.parent()).map_or_else(
                |_| PanelTextAlpha::global_default(&defaults),
                |resolved| resolved.0,
            );
            let resolved = PanelTextAlpha::entity_value(&ptq).unwrap_or(panel_fallback);
            commands.entity(entity).insert((ptq, Resolved(resolved)));
            commands.entity(entity).remove::<PendingGlyphs>();
            commands.entity(entity).insert(AwaitingReady);
        } else if has_pending {
            commands.entity(entity).insert_if_new(PendingGlyphs);
        }
    }

    perf.panel_text.shape_ms = shape_stage_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    perf.panel_text.parley_ms = agg.shape_ms;
    perf.panel_text.atlas_lookup_ms = agg.atlas_ms;
    perf.panel_text.shaped_panels = shaped_panels.len();
    perf.panel_text.queued_glyphs = agg.queued_glyphs;
    perf.panel_text.pending_glyphs = agg.pending_glyphs;
    perf.panel_text.total_ms = perf.panel_text.shape_ms + perf.panel_text.mesh_build_ms;
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
fn build_panel_batched_meshes(
    changed_quads: Query<&ChildOf, (With<PanelTextChild>, Changed<PanelTextQuads>)>,
    panel_children: Query<(Entity, &PanelTextQuads, &PanelTextChild, &ChildOf)>,
    old_meshes: Query<(Entity, &ChildOf), Or<(With<DiegeticTextMesh>, With<DiegeticShadowProxy>)>>,
    panels: Query<(&DiegeticPanel, Option<&HueOffset>, Option<&RenderLayers>)>,
    resolved_alphas: Query<&Resolved<PanelTextAlpha>, With<PanelTextChild>>,
    defaults: Res<CascadeDefaults>,
    atlas: Res<MsdfAtlas>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MsdfTextMaterial>>,
    mut shared_mats: ResMut<SharedMsdfMaterials>,
    rtt_registry: Res<PanelRttRegistry>,
    mut perf: ResMut<DiegeticPerfStats>,
    mut commands: Commands,
) {
    let mesh_build_start = Instant::now();

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
        build_panel_meshes_for_entity(
            panel_entity,
            &panel_children,
            &old_meshes,
            &panels,
            &resolved_alphas,
            &defaults,
            &atlas,
            &mut meshes,
            &mut materials,
            &mut shared_mats,
            &rtt_registry,
            &mut commands,
        );
    }

    perf.panel_text.mesh_build_ms =
        mesh_build_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    perf.panel_text.total_ms = perf.panel_text.shape_ms + perf.panel_text.mesh_build_ms;
}

#[allow(clippy::too_many_arguments, reason = "internal per-panel dispatch")]
fn build_panel_meshes_for_entity(
    panel_entity: Entity,
    panel_children: &Query<(Entity, &PanelTextQuads, &PanelTextChild, &ChildOf)>,
    old_meshes: &Query<(Entity, &ChildOf), Or<(With<DiegeticTextMesh>, With<DiegeticShadowProxy>)>>,
    panels: &Query<(&DiegeticPanel, Option<&HueOffset>, Option<&RenderLayers>)>,
    resolved_alphas: &Query<&Resolved<PanelTextAlpha>, With<PanelTextChild>>,
    defaults: &CascadeDefaults,
    atlas: &MsdfAtlas,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<MsdfTextMaterial>,
    shared_mats: &mut SharedMsdfMaterials,
    rtt_registry: &PanelRttRegistry,
    commands: &mut Commands,
) {
    let Ok((panel, hue_offset, panel_layers)) = panels.get(panel_entity) else {
        return;
    };
    let hue = hue_offset.map_or(0.0, |h| h.0);
    let is_geometry = panel.render_mode() == RenderMode::Geometry;
    let scene_layer = panel_layers.cloned().unwrap_or(RenderLayers::layer(0));

    // Collect all quads from this panel's `PanelTextChild` children
    // and track the maximum command index for layer ordering.
    let mut batches: HashMap<TextBatchKey, (AlphaMode, Vec<GlyphQuadData>)> = HashMap::new();
    let mut max_command_index: usize = 0;
    for (child_entity, ptq, ptc, child_of) in panel_children {
        if child_of.parent() != panel_entity {
            continue;
        }
        max_command_index = max_command_index.max(ptc.command_index);
        let clip_rect = panel_clip_rect_local(
            ptc.clip_rect,
            ptc.scale_x,
            ptc.scale_y,
            ptc.anchor_x,
            ptc.anchor_y,
        );
        let clip_rect = clip_rect_bits(clip_rect);
        let resolved_alpha = resolved_alphas.get(child_entity).map_or_else(
            |_| PanelTextAlpha::global_default(defaults).0,
            |resolved| resolved.0.0,
        );
        let alpha_bits = alpha_mode_bits(resolved_alpha);
        for (page_index, quad) in &ptq.quads {
            let key = TextBatchKey {
                render_mode: ptq.render_mode,
                shadow_mode: ptq.shadow_mode,
                page_index: *page_index,
                clip_rect,
                alpha_mode_bits: alpha_bits,
            };
            batches
                .entry(key)
                .or_insert_with(|| (resolved_alpha, Vec::new()))
                .1
                .push(*quad);
        }
    }

    let total_quads: usize = batches.values().map(|(_, v)| v.len()).sum();
    if total_quads == 0 {
        return;
    }

    // Despawn previous mesh children.
    for (mesh_entity, child_of) in old_meshes {
        if child_of.parent() == panel_entity {
            commands.entity(mesh_entity).despawn();
        }
    }

    let content_layer = rtt_registry
        .get_layer(panel_entity)
        .map_or_else(|| scene_layer.clone(), RenderLayers::layer);

    // Resolve the base StandardMaterial for text in this panel.
    let mut text_base = panel
        .text_material()
        .cloned()
        .unwrap_or_else(constants::default_panel_material);
    text_base.alpha_mode = AlphaMode::Blend;
    text_base.double_sided = true;
    text_base.cull_mode = None;
    if !is_geometry {
        text_base.unlit = true;
    }

    // Text depth bias renders above all geometry at or below the
    // last command index in the batch.
    let text_depth_bias = if is_geometry {
        max_command_index.saturating_add(1).to_f32() * constants::LAYER_DEPTH_BIAS
    } else {
        0.0
    };
    let text_oit_offset = if is_geometry {
        max_command_index.saturating_add(1).to_f32() * constants::OIT_DEPTH_STEP
    } else {
        0.0
    };

    spawn_batch_meshes(
        &batches,
        panel_entity,
        hue,
        atlas,
        meshes,
        materials,
        shared_mats,
        &content_layer,
        &scene_layer,
        &text_base,
        text_depth_bias,
        text_oit_offset,
        commands,
    );
}

/// Spawns visible mesh and optional shadow proxy entities for each batch
/// of glyph quads under the given `panel_entity`.
fn spawn_batch_meshes(
    batches: &HashMap<TextBatchKey, (AlphaMode, Vec<GlyphQuadData>)>,
    panel_entity: Entity,
    hue: f32,
    atlas: &MsdfAtlas,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<MsdfTextMaterial>,
    shared_mats: &mut SharedMsdfMaterials,
    content_layer: &RenderLayers,
    scene_layer: &RenderLayers,
    text_base: &StandardMaterial,
    text_depth_bias: f32,
    text_oit_offset: f32,
    commands: &mut Commands,
) {
    for (key, (alpha_mode, quads)) in batches {
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
            let mut batch_base = text_base.clone();
            batch_base.depth_bias = text_depth_bias;
            let material_handle = resolve_visible_material(
                key,
                hue,
                *alpha_mode,
                &batch_base,
                &page_image,
                atlas,
                materials,
                shared_mats,
                text_oit_offset,
            );
            spawn_visible_mesh(
                panel_entity,
                mesh_handle.clone(),
                material_handle,
                suppress_shadow,
                content_layer,
                commands,
            );
        }

        if needs_proxy {
            spawn_shadow_proxy(
                key,
                mesh_handle,
                page_image,
                panel_entity,
                hue,
                atlas,
                text_base,
                text_depth_bias,
                text_oit_offset,
                scene_layer,
                materials,
                commands,
            );
        }
    }
}

/// Returns the `MsdfTextMaterial` handle for a visible glyph batch.
///
/// Zero-hue default text uses a shared handle keyed by page / clip / depth /
/// alpha mode. Non-zero hue or non-default render modes produce a unique
/// material per batch.
fn resolve_visible_material(
    key: &TextBatchKey,
    hue: f32,
    alpha_mode: AlphaMode,
    batch_base: &StandardMaterial,
    page_image: &Handle<Image>,
    atlas: &MsdfAtlas,
    materials: &mut Assets<MsdfTextMaterial>,
    shared_mats: &mut SharedMsdfMaterials,
    text_oit_offset: f32,
) -> Handle<MsdfTextMaterial> {
    let clip_rect = clip_rect_from_bits(key.clip_rect);
    if hue.abs() < f32::EPSILON && key.render_mode == GlyphRenderMode::Text {
        shared_mats
            .handles
            .entry(SharedMsdfMaterialKey {
                page_index:      key.page_index,
                clip_rect:       key.clip_rect,
                depth_bias_bits: batch_base.depth_bias.to_bits(),
                alpha_mode_bits: key.alpha_mode_bits,
            })
            .or_insert_with(|| {
                materials.add(msdf_material::msdf_text_material(
                    batch_base.clone(),
                    MsdfAtlas::sdf_range().to_f32(),
                    atlas.width(),
                    atlas.height(),
                    page_image.clone(),
                    0.0,
                    glyph_render_mode_uniform(GlyphRenderMode::Text),
                    clip_rect,
                    text_oit_offset,
                    alpha_mode,
                ))
            })
            .clone()
    } else {
        materials.add(msdf_material::msdf_text_material(
            batch_base.clone(),
            MsdfAtlas::sdf_range().to_f32(),
            atlas.width(),
            atlas.height(),
            page_image.clone(),
            hue,
            glyph_render_mode_uniform(key.render_mode),
            clip_rect,
            text_oit_offset,
            alpha_mode,
        ))
    }
}

/// Spawns the visible glyph-mesh child entity under `panel_entity`. When
/// `suppress_shadow` is set (either because this batch is invisible, has a
/// companion shadow proxy, or has shadows off), adds `NotShadowCaster`.
fn spawn_visible_mesh(
    panel_entity: Entity,
    mesh_handle: Handle<Mesh>,
    material_handle: Handle<MsdfTextMaterial>,
    suppress_shadow: bool,
    content_layer: &RenderLayers,
    commands: &mut Commands,
) {
    let transform = Transform::from_xyz(0.0, 0.0, 0.0);
    if suppress_shadow {
        commands.entity(panel_entity).with_child((
            DiegeticTextMesh,
            NotShadowCaster,
            Mesh3d(mesh_handle),
            MeshMaterial3d(material_handle),
            transform,
            content_layer.clone(),
        ));
    } else {
        commands.entity(panel_entity).with_child((
            DiegeticTextMesh,
            Mesh3d(mesh_handle),
            MeshMaterial3d(material_handle),
            transform,
            content_layer.clone(),
        ));
    }
}

/// Spawns a shadow-proxy entity for a single glyph batch.
fn spawn_shadow_proxy(
    key: &TextBatchKey,
    mesh_handle: Handle<Mesh>,
    page_image: Handle<Image>,
    panel_entity: Entity,
    hue: f32,
    atlas: &MsdfAtlas,
    text_base: &StandardMaterial,
    text_depth_bias: f32,
    text_oit_offset: f32,
    scene_layer: &RenderLayers,
    materials: &mut Assets<MsdfTextMaterial>,
    commands: &mut Commands,
) {
    let shadow_render_mode = match key.shadow_mode {
        GlyphShadowMode::SolidQuad => glyph_render_mode_uniform(GlyphRenderMode::SolidQuad),
        GlyphShadowMode::PunchOut => glyph_render_mode_uniform(GlyphRenderMode::PunchOut),
        GlyphShadowMode::None | GlyphShadowMode::Text => {
            glyph_render_mode_uniform(GlyphRenderMode::Text)
        },
    };
    let clip_rect = clip_rect_from_bits(key.clip_rect);
    let mut proxy_base = text_base.clone();
    proxy_base.depth_bias = text_depth_bias - constants::LAYER_DEPTH_BIAS;

    let proxy_material = materials.add(msdf_material::msdf_shadow_proxy_material(
        proxy_base,
        MsdfAtlas::sdf_range().to_f32(),
        atlas.width(),
        atlas.height(),
        page_image,
        hue,
        shadow_render_mode,
        clip_rect,
        text_oit_offset,
    ));

    commands.entity(panel_entity).with_child((
        DiegeticShadowProxy,
        Mesh3d(mesh_handle),
        MeshMaterial3d(proxy_material),
        Transform::from_xyz(0.0, 0.0, 0.0),
        scene_layer.clone(),
    ));
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
    let measure = config.as_measure();

    if let Some(cached) = cache.get_shaped(text, &measure) {
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
    builder.push_default(parley::style::StyleProperty::FontFamily(
        parley::style::FontFamily::named(family_name),
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
            .map(|(tag, value)| parley::FontFeature {
                tag: parley::setting::Tag::from_bytes(tag),
                value,
            })
            .collect();
        builder.push_default(parley::style::StyleProperty::FontFeatures(
            parley::style::FontFeatures::List(std::borrow::Cow::Owned(parley_features)),
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
            top:      lm.block_min_coord,
            bottom:   lm.block_max_coord,
        });
        for item in line.items() {
            let parley::layout::PositionedLayoutItem::GlyphRun(run) = item else {
                continue;
            };
            let glyph_run = run.run();
            let mut advance_x = 0.0_f32;
            for cluster in glyph_run.clusters() {
                for glyph in cluster.glyphs() {
                    glyphs.push(ShapedGlyph {
                        glyph_id: glyph.id.to_u16(),
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
    cache.insert_shaped(text, &measure, run.clone(), dims);
    run
}

/// Shapes text and produces glyph quads in panel-local coordinates.
///
/// Uses the [`ShapedTextCache`] to avoid redundant parley shaping. Quad
/// construction from cached glyphs + atlas metrics is cheap arithmetic.
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
    clip_rect: Option<BoundingBox>,
) -> (Vec<(u32, GlyphQuadData)>, TextBuildStats) {
    let mut stats = TextBuildStats {
        texts: 1,
        ..Default::default()
    };
    let shape_start = Instant::now();
    let shaped = shape_text_cached(text, config, font_registry, shaping_cx, cache);
    stats.shape_ms = shape_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
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
            stats.atlas_ms = atlas_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
            return (Vec::new(), stats);
        }
    }

    let linear: LinearRgba = config.color().into();
    let color_arr = [linear.red, linear.green, linear.blue, linear.alpha];

    let em_scale = config.size() / atlas.canonical_size().to_f32();

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
        let glyph_y = bounds.y + sg.baseline + sg.y;

        // Glyph quad size in layout units.
        let quad_w = metrics.pixel_width.to_f32() * em_scale;
        let quad_h = metrics.pixel_height.to_f32() * em_scale;

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

    glyph_quad::clip_overlapping_quads(&mut quads);

    // Cull glyphs entirely outside the clip region, but leave partially
    // visible quads intact for shader-side clipping. Trimming CPU-side UVs
    // causes MSDF atlas bleed at clipped edges.
    if let Some(cr) = clip_rect {
        let clip_local = panel_clip_rect_local(Some(cr), scale_x, scale_y, anchor_x, anchor_y);
        quads.retain(|(_, quad)| {
            glyph_quad::clip_quad_to_rect(quad, clip_local.to_array()).is_some()
        });
    }

    stats.atlas_ms = atlas_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
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
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::render::msdf_material;

    /// Regression test for the panel-text reconciliation bug that double-rendered
    /// wrapped text. Multiple render commands for a single wrapped-text element
    /// share an `element_idx` but have distinct `command_index` values (one per
    /// wrapped line). The reconciler MUST key existing children by the
    /// `(element_idx, command_index)` pair — keying by `element_idx` alone
    /// collapses lines into a single entry, leaks stale entities on subsequent
    /// frames, and produces overlapping text renders.
    #[test]
    fn reconcile_keys_by_element_and_command_index() {
        // Simulate three existing PanelTextChild records from a prior frame:
        // one text element, wrapped across three lines.
        let existing: Vec<(Entity, PanelTextChild)> = (0..3)
            .map(|cmd| {
                let ptc = PanelTextChild {
                    element_idx:   7,
                    command_index: cmd,
                    bounds:        crate::layout::BoundingBox {
                        x:      0.0,
                        y:      cmd.to_f32() * 10.0,
                        width:  100.0,
                        height: 10.0,
                    },
                    scale_x:       1.0,
                    scale_y:       1.0,
                    anchor_x:      0.0,
                    anchor_y:      0.0,
                    clip_rect:     None,
                };
                (
                    Entity::from_raw_u32(cmd.try_into().expect("small")).expect("valid"),
                    ptc,
                )
            })
            .collect();

        // Build the key map using the same strategy as the reconciler.
        let mut by_key: HashMap<(usize, usize), Entity> = HashMap::new();
        for (entity, ptc) in &existing {
            by_key.insert((ptc.element_idx, ptc.command_index), *entity);
        }
        assert_eq!(
            by_key.len(),
            3,
            "three wrapped lines must produce three distinct keys; \
             collapsing to element_idx alone would show only 1 here"
        );

        // Building the same map keyed by element_idx only (the bug) would yield
        // a single entry — this assertion documents the failure mode.
        let mut by_element_only: HashMap<usize, Entity> = HashMap::new();
        for (entity, ptc) in &existing {
            by_element_only.insert(ptc.element_idx, *entity);
        }
        assert_eq!(
            by_element_only.len(),
            1,
            "element_idx-only keying collapses wrapped lines — the root cause \
             of the overlapping-text bug"
        );
    }

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
        let shared_handle = materials.add(msdf_material::msdf_text_material(
            base.clone(),
            4.0,
            256,
            256,
            atlas_image.clone(),
            0.0,
            0,
            constants::UNCLIPPED_TEXT_CLIP_RECT,
            0.0,
            AlphaMode::AlphaToCoverage,
        ));

        // Simulate the decision logic from extract_text_meshes.
        let mut decide = |hue: f32| -> Handle<MsdfTextMaterial> {
            if hue.abs() < f32::EPSILON {
                shared_handle.clone()
            } else {
                materials.add(msdf_material::msdf_text_material(
                    base.clone(),
                    4.0,
                    256,
                    256,
                    atlas_image.clone(),
                    hue,
                    0,
                    constants::UNCLIPPED_TEXT_CLIP_RECT,
                    0.0,
                    AlphaMode::AlphaToCoverage,
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
