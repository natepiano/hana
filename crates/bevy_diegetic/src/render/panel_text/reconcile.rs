use std::collections::HashMap;
use std::time::Instant;

use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::PanelTextLayout;
use super::PanelTextRuns;
use super::TextRunOf;
use super::layout::PanelTextDrawZIndex;
use super::layout::PanelTextDrawZIndexRank;
use crate::PanelElementId;
use crate::cascade;
use crate::cascade::Cascade;
use crate::cascade::CascadeAttr;
use crate::cascade::CascadeDefault;
use crate::cascade::HdrTextCoverageBias;
use crate::cascade::Override;
use crate::cascade::Resolved;
use crate::cascade::TextAlpha;
use crate::cascade::TextMaterial;
use crate::constants::MILLISECONDS_PER_SECOND;
use crate::layout::Anchor;
use crate::layout::BoundingBox;
use crate::layout::GlyphShadowMode;
use crate::layout::Lighting;
use crate::layout::RenderCommand;
use crate::layout::RenderCommandKind;
use crate::layout::ShadowCasting;
use crate::layout::Sidedness;
use crate::layout::TextStyle;
use crate::panel::ComputedDiegeticPanel;
use crate::panel::DiegeticPanel;
use crate::panel::DiegeticPerfStats;
use crate::panel::PanelPrecomposeCache;
use crate::render::clip;
use crate::render::constants::TEXT_Z_OFFSET;
use crate::render::draw_order::DrawCommandDepth;
use crate::render::draw_order::DrawOrder;
use crate::render::world_text::TextContent;

/// A reused panel-text child plus the components reconcile compares incoming
/// values against before deciding whether to write. The references borrow the
/// `existing_children` query for one reconcile pass.
#[derive(Clone, Copy)]
struct ReusableChild<'a> {
    entity:                 Entity,
    text:                   &'a TextContent,
    style:                  &'a TextStyle,
    layout:                 &'a PanelTextLayout,
    z_index:                &'a PanelTextDrawZIndex,
    z_index_rank:           &'a PanelTextDrawZIndexRank,
    alpha:                  Option<&'a Override<TextAlpha>>,
    material:               Option<&'a Override<TextMaterial>>,
    lighting:               Option<&'a Override<Lighting>>,
    sidedness:              Option<&'a Override<Sidedness>>,
    shadow_casting:         Option<&'a Override<ShadowCasting>>,
    glyph_shadow_mode:      Option<&'a Override<GlyphShadowMode>>,
    hdr_text_coverage_bias: Option<&'a Override<HdrTextCoverageBias>>,
}

/// One text render command resolved to its reconcile inputs: source element,
/// command depth, run `id`, per-run `line_index`, the string, its style,
/// layout bounds, and the effective clip rect.
type PendingTextChild = (
    usize,
    DrawCommandDepth,
    PanelElementId,
    usize,
    String,
    TextStyle,
    BoundingBox,
    BoundingBox,
);

/// Resolves the panel's text render commands into per-child reconcile inputs,
/// assigning each its run `id` (from the tree, auto fallback when absent) and a
/// per-run `line_index` so the reuse key is the content-stable `(id, line_index)`
/// rather than the former positional `(element_idx, command_index)`.
fn collect_text_commands(
    panel: &DiegeticPanel,
    commands: &[RenderCommand],
    draw_order: &DrawOrder,
    clip_rects: &[Option<BoundingBox>],
    viewport: BoundingBox,
) -> Vec<PendingTextChild> {
    let mut line_counter: HashMap<usize, usize> = HashMap::new();
    commands
        .iter()
        .enumerate()
        .filter_map(|(cmd_index, cmd)| match &cmd.kind {
            RenderCommandKind::Text { text, config } => {
                let active_clip = clip::effective_clip(cmd.bounds, clip_rects[cmd_index], viewport)
                    .unwrap_or_else(clip::empty_clip);
                let draw_depth = draw_order.depth_for(cmd_index)?;
                let id = panel
                    .tree()
                    .text_element_id(cmd.element_idx)
                    .cloned()
                    .unwrap_or_else(|| {
                        PanelElementId::auto(u32::try_from(cmd.element_idx).unwrap_or(0))
                    });
                let counter = line_counter.entry(cmd.element_idx).or_insert(0);
                let line_index = *counter;
                *counter += 1;
                Some((
                    cmd.element_idx,
                    draw_depth,
                    id,
                    line_index,
                    text.clone(),
                    config.clone(),
                    cmd.bounds,
                    active_clip,
                ))
            },
            _ => None,
        })
        .collect()
}

fn collect_existing_text_children<'a>(
    existing_run_entities: &[Entity],
    existing_runs: &'a Query<(
        &TextContent,
        &TextStyle,
        &PanelTextLayout,
        &PanelTextDrawZIndex,
        &PanelTextDrawZIndexRank,
        Option<&Override<TextAlpha>>,
        Option<&Override<TextMaterial>>,
        Option<&Override<Lighting>>,
        Option<&Override<Sidedness>>,
        Option<&Override<ShadowCasting>>,
        Option<&Override<GlyphShadowMode>>,
        Option<&Override<HdrTextCoverageBias>>,
    )>,
) -> HashMap<(PanelElementId, usize), ReusableChild<'a>> {
    let mut existing_by_key = HashMap::new();
    for &entity in existing_run_entities {
        let Ok((
            text,
            style,
            layout,
            z_index,
            z_index_rank,
            alpha,
            material,
            lighting,
            sidedness,
            shadow_casting,
            glyph_shadow_mode,
            hdr_text_coverage_bias,
        )) = existing_runs.get(entity)
        else {
            continue;
        };
        existing_by_key.insert(
            (layout.id.clone(), layout.line_index),
            ReusableChild {
                entity,
                text,
                style,
                layout,
                z_index,
                z_index_rank,
                alpha,
                material,
                lighting,
                sidedness,
                shadow_casting,
                glyph_shadow_mode,
                hdr_text_coverage_bias,
            },
        );
    }
    existing_by_key
}

/// Reconciles [`TextContent`] children for each changed [`ComputedDiegeticPanel`].
///
/// Resets [`DiegeticPerfStats::reconcile_ms`] to this pass's wall time each
/// frame; the image reconcile (ordered after) accumulates onto it.
pub(super) fn reconcile_panel_text_children(
    mut changed_panels: Query<
        (
            Entity,
            &mut DiegeticPanel,
            &ComputedDiegeticPanel,
            Option<&PanelTextRuns>,
        ),
        Changed<ComputedDiegeticPanel>,
    >,
    existing_runs: Query<(
        &TextContent,
        &TextStyle,
        &PanelTextLayout,
        &PanelTextDrawZIndex,
        &PanelTextDrawZIndexRank,
        Option<&Override<TextAlpha>>,
        Option<&Override<TextMaterial>>,
        Option<&Override<Lighting>>,
        Option<&Override<Sidedness>>,
        Option<&Override<ShadowCasting>>,
        Option<&Override<GlyphShadowMode>>,
        Option<&Override<HdrTextCoverageBias>>,
    )>,
    mut commands: Commands,
    mut perf: ResMut<DiegeticPerfStats>,
) {
    let reconcile_start = Instant::now();
    for (panel_entity, mut panel, computed, panel_runs) in &mut changed_panels {
        let Some(result) = computed.result() else {
            continue;
        };

        let points_to_world = panel.points_to_world();
        let scale_x = points_to_world;
        let scale_y = points_to_world;
        let (anchor_x, anchor_y) = panel.anchor_offsets();

        let clip_rects = clip::compute_clip_rects(&result.commands);
        let viewport = clip::panel_viewport(&panel);
        // Each text command carries its source element's run id (from the tree)
        // and a per-run line ordinal, so the reuse key is `(id, line_index)`:
        // content-stable, unlike a positional `(element_idx, command_index)`
        // pair that a sibling reorder would shift. Auto ids preserve that
        // positional behavior; named ids survive the reorder.
        let text_commands = collect_text_commands(
            &panel,
            &result.commands,
            computed.draw_order(),
            &clip_rects,
            viewport,
        );

        // Source the panel's existing runs from `PanelTextRuns` (the typed
        // text-run index) rather than scanning every `TextContent` child and
        // filtering by parent. `None` means no run has spawned yet (first pass).
        let existing_run_entities: &[Entity] = panel_runs.map_or(&[][..], |runs| &**runs);
        let existing_by_key = collect_existing_text_children(existing_run_entities, &existing_runs);

        let mut visited_keys: Vec<(PanelElementId, usize)> = Vec::new();
        let mut text_index: HashMap<PanelElementId, Entity> = HashMap::new();
        for (element_idx, draw_depth, id, line_index, text, config, bounds, clip) in &text_commands
        {
            // A label's own cascade overrides (`TextStyle::with_alpha_mode` /
            // `with_material` / `with_lighting` / `with_sidedness`) are
            // captured before `for_shaping()` clears them, then inserted as
            // `Override<A>` on the label. `None` means the label inherits the
            // panel value.
            let label_cascades = TextLabelCascadeOverrides::from_style(config);
            let style = config.for_shaping(Anchor::TopLeft);
            let z_index = PanelTextDrawZIndex(draw_depth.z_index());
            let z_index_rank = PanelTextDrawZIndexRank(draw_depth.z_index_rank());
            let panel_text_child = PanelTextLayout {
                id: id.clone(),
                line_index: *line_index,
                element_idx: *element_idx,
                draw_ordinal: draw_depth.draw_order_index(),
                depth_bias: draw_depth.clip_depth_nudge().get(),
                oit_depth_offset: draw_depth.oit_depth_offset().get(),
                bounds: *bounds,
                scale_x,
                scale_y,
                anchor_x,
                anchor_y,
                clip_rect: Some(*clip),
            };

            let key = (id.clone(), *line_index);
            visited_keys.push(key.clone());

            let entity = if let Some(&reusable) = existing_by_key.get(&key) {
                update_reused_panel_text_child(UpdateReusedChild {
                    commands: &mut commands,
                    reusable,
                    text,
                    style,
                    layout: panel_text_child,
                    z_index,
                    z_index_rank,
                    label_cascades,
                });
                reusable.entity
            } else {
                spawn_panel_text_child(SpawnPanelTextChild {
                    commands: &mut commands,
                    panel_entity,
                    text,
                    style,
                    layout: panel_text_child,
                    z_index,
                    z_index_rank,
                    label_cascades,
                })
            };

            // Address a run by its first line: `text_child(id)` resolves this
            // entity. Auto-id runs land here too but are unreachable — no caller
            // can build their `PanelElementId::Auto`.
            if *line_index == 0 {
                text_index.insert(id.clone(), entity);
            }
        }

        for &entity in existing_run_entities {
            let Ok((_, _, layout, _, _, _, _, _, _, _, _, _)) = existing_runs.get(entity) else {
                continue;
            };
            if !visited_keys.contains(&(layout.id.clone(), layout.line_index)) {
                commands.entity(entity).despawn();
            }
        }

        // Rebuilt from scratch each pass. Write it without tripping
        // `Changed<DiegeticPanel>`, or this `&mut DiegeticPanel` write would
        // re-dirty the panel and loop layout → reconcile every frame.
        panel.bypass_change_detection().text_index = text_index;
    }
    perf.reconcile_ms = reconcile_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
}

/// Inputs to [`spawn_panel_text_child`]. Grouped into a struct because reconcile
/// threads text, style, layout, and captured cascade overrides through to a
/// freshly spawned child.
struct SpawnPanelTextChild<'a, 'w, 's> {
    commands:       &'a mut Commands<'w, 's>,
    panel_entity:   Entity,
    text:           &'a str,
    style:          TextStyle,
    layout:         PanelTextLayout,
    z_index:        PanelTextDrawZIndex,
    z_index_rank:   PanelTextDrawZIndexRank,
    label_cascades: TextLabelCascadeOverrides,
}

/// Spawns a new panel-text child under `panel_entity` and applies whichever of
/// the captured cascade overrides the label authored. `None` for an override
/// means the label inherits the panel value.
fn spawn_panel_text_child(request: SpawnPanelTextChild<'_, '_, '_>) -> Entity {
    let SpawnPanelTextChild {
        commands,
        panel_entity,
        text,
        style,
        layout,
        z_index,
        z_index_rank,
        label_cascades,
    } = request;
    let mut spawned = Entity::PLACEHOLDER;
    commands.entity(panel_entity).with_children(|children| {
        // `TextRunOf` is inserted only here, on a newly spawned run; the reuse
        // branch must never re-insert it (a reused run already carries it, and
        // re-inserting would fire the relationship hook and mutate
        // `PanelTextRuns` on a no-op). `with_children` already adds `ChildOf`.
        let mut child = children.spawn((
            TextContent::new(text.to_owned()),
            style,
            layout,
            z_index,
            z_index_rank,
            TextRunOf(panel_entity),
        ));
        spawned = child.id();
        sync_label_cascade_overrides(&mut child, label_cascades, None);
    });
    spawned
}

/// Inputs to [`update_reused_panel_text_child`]. Grouped into a struct because
/// reconcile threads text, style, layout, and captured cascade overrides through
/// to a reused child.
struct UpdateReusedChild<'a, 'w, 's> {
    commands:       &'a mut Commands<'w, 's>,
    reusable:       ReusableChild<'a>,
    text:           &'a str,
    style:          TextStyle,
    layout:         PanelTextLayout,
    z_index:        PanelTextDrawZIndex,
    z_index_rank:   PanelTextDrawZIndexRank,
    label_cascades: TextLabelCascadeOverrides,
}

/// Cascade overrides authored by one `TextStyle` before shaping clears
/// render-only fields from the glyph-layout style.
struct TextLabelCascadeOverrides {
    alpha:                  Cascade<AlphaMode>,
    material:               Cascade<Handle<StandardMaterial>>,
    lighting:               Cascade<Lighting>,
    sidedness:              Cascade<Sidedness>,
    shadow_casting:         Cascade<ShadowCasting>,
    glyph_shadow_mode:      Cascade<GlyphShadowMode>,
    hdr_text_coverage_bias: Cascade<f32>,
}

impl TextLabelCascadeOverrides {
    fn from_style(style: &TextStyle) -> Self {
        Self {
            alpha:                  style.alpha_mode(),
            material:               style.material().cloned(),
            lighting:               style.lighting(),
            sidedness:              style.sidedness(),
            shadow_casting:         style.shadow_casting(),
            glyph_shadow_mode:      style.shadow_mode(),
            hdr_text_coverage_bias: style.hdr_text_coverage_bias(),
        }
    }
}

/// Writes each component of a reused panel-text child only when it differs, so
/// an unchanged run stays un-`Changed`.
///
/// `shape_panel_text_children` classifies `Changed<TextStyle>` with
/// `TextStyle::gating_eq`: geometry fields rebuild glyphs, while render-only
/// fields refresh `PreparedPanelText` and material-table rows. Cascade
/// overrides (alpha, material, lighting, sidedness, HDR text coverage bias) are
/// gated on their own because writing one unconditionally would re-fire
/// `Changed<Resolved<A>>` on every run and defeat the per-run short-circuit
/// downstream.
fn update_reused_panel_text_child(request: UpdateReusedChild<'_, '_, '_>) {
    let UpdateReusedChild {
        commands,
        reusable,
        text,
        style,
        layout,
        z_index,
        z_index_rank,
        label_cascades,
    } = request;
    let mut child = commands.entity(reusable.entity);
    if reusable.text.text() != text {
        child.insert(TextContent::new(text.to_owned()));
    }
    if reusable.style != &style {
        child.insert(style);
    }
    if !reusable.layout.gating_eq(&layout) {
        child.insert(layout);
    }
    if *reusable.z_index != z_index {
        child.insert(z_index);
    }
    if *reusable.z_index_rank != z_index_rank {
        child.insert(z_index_rank);
    }
    sync_label_cascade_overrides(&mut child, label_cascades, Some(reusable));
}

fn sync_label_cascade_overrides(
    child: &mut EntityCommands<'_>,
    label_cascades: TextLabelCascadeOverrides,
    reusable: Option<ReusableChild<'_>>,
) {
    sync_cascade_override(
        child,
        label_cascades.alpha.map(TextAlpha),
        reusable.and_then(|r| r.alpha),
    );
    sync_cascade_override(
        child,
        label_cascades.material.map(TextMaterial),
        reusable.and_then(|r| r.material),
    );
    sync_cascade_override(
        child,
        label_cascades.lighting,
        reusable.and_then(|r| r.lighting),
    );
    sync_cascade_override(
        child,
        label_cascades.sidedness,
        reusable.and_then(|r| r.sidedness),
    );
    sync_cascade_override(
        child,
        label_cascades.shadow_casting,
        reusable.and_then(|r| r.shadow_casting),
    );
    sync_cascade_override(
        child,
        label_cascades.glyph_shadow_mode,
        reusable.and_then(|r| r.glyph_shadow_mode),
    );
    sync_cascade_override(
        child,
        label_cascades
            .hdr_text_coverage_bias
            .map(HdrTextCoverageBias),
        reusable.and_then(|r| r.hdr_text_coverage_bias),
    );
}

fn sync_cascade_override<A>(
    child: &mut EntityCommands<'_>,
    incoming: Cascade<A>,
    current: Option<&Override<A>>,
) where
    A: CascadeAttr,
    CascadeDefault<A>: Default + Resource,
{
    match incoming {
        Cascade::Override(value) => {
            if current.map(|node_override| &node_override.0) != Some(&value) {
                cascade::apply_cascade_override(child, value);
            }
        },
        Cascade::Inherit => {
            if current.is_some() {
                cascade::remove_cascade_override::<A>(child);
            }
        },
    }
}

/// Marker plus cached reconcile inputs for an image child entity.
///
/// `reconcile_panel_image_children` compares the incoming `handle`, `tint`,
/// `bounds`, `draw_depth`, and `shadow_casting` against these cached values to
/// decide whether to skip the child, mutate its tint/shadow state in place, or
/// rebuild its mesh and material.
#[derive(Component, Clone, Debug)]
pub(super) struct PanelImageChild {
    /// Index of the source element in the layout tree (the reuse key).
    pub element_idx:    usize,
    /// Projected ordering values for the source image command.
    pub draw_depth:     DrawCommandDepth,
    /// Image asset handle from the most recent build.
    pub handle:         Handle<Image>,
    /// Tint color from the most recent build.
    pub tint:           Color,
    /// Layout bounds from the most recent build.
    pub bounds:         BoundingBox,
    /// Shadow-casting policy resolved from the panel cascade for this image.
    pub shadow_casting: ShadowCasting,
}

/// A reused image child plus the material handle reconcile mutates for a
/// tint-only change. References borrow the `existing_children` query for one
/// reconcile pass.
#[derive(Clone, Copy)]
struct ReusableImageChild<'a> {
    entity:   Entity,
    cached:   &'a PanelImageChild,
    material: &'a MeshMaterial3d<StandardMaterial>,
}

/// Mesh, material, and transform for one image child.
struct ImageVisuals {
    mesh:      Mesh3d,
    material:  MeshMaterial3d<StandardMaterial>,
    transform: Transform,
}

/// Reconciles image children for each changed [`ComputedDiegeticPanel`].
///
/// Accumulates onto [`DiegeticPerfStats::reconcile_ms`]; the text reconcile
/// (ordered first) resets it each frame.
pub(super) fn reconcile_panel_image_children(
    changed_panels: Query<
        (
            Entity,
            &DiegeticPanel,
            &ComputedDiegeticPanel,
            &PanelPrecomposeCache,
            Option<&RenderLayers>,
            Option<&Resolved<ShadowCasting>>,
        ),
        Or<(
            Changed<ComputedDiegeticPanel>,
            Changed<RenderLayers>,
            Changed<Resolved<ShadowCasting>>,
        )>,
    >,
    existing_children: Query<(
        Entity,
        &PanelImageChild,
        &MeshMaterial3d<StandardMaterial>,
        &ChildOf,
    )>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut perf: ResMut<DiegeticPerfStats>,
) {
    let reconcile_start = Instant::now();
    for (panel_entity, panel, computed, precompose_cache, panel_layers, panel_shadow_casting) in
        &changed_panels
    {
        let Some(result) = computed.result() else {
            continue;
        };

        let points_to_world = panel.points_to_world();
        let (anchor_x, anchor_y) = panel.anchor_offsets();
        let layer = panel_layers.cloned().unwrap_or(RenderLayers::layer(0));
        let shadow_casting = panel_shadow_casting.map_or(ShadowCasting::On, |resolved| resolved.0);

        let clip_rects = clip::compute_clip_rects(&result.commands);
        let viewport = clip::panel_viewport(panel);
        let image_commands = collect_panel_image_commands(
            &result.commands,
            computed.draw_order(),
            precompose_cache,
            &clip_rects,
            viewport,
            shadow_casting,
        );

        let mut existing_by_idx: HashMap<usize, ReusableImageChild> = HashMap::new();
        for (entity, cached, material, child_of) in &existing_children {
            if child_of.parent() == panel_entity {
                existing_by_idx.insert(
                    cached.element_idx,
                    ReusableImageChild {
                        entity,
                        cached,
                        material,
                    },
                );
            }
        }

        let mut visited_indices: Vec<usize> = Vec::new();
        for incoming in image_commands {
            visited_indices.push(incoming.element_idx);
            let geometry = ImageGeometry {
                points_to_world,
                anchor_x,
                anchor_y,
            };
            if let Some(reusable) = existing_by_idx.get(&incoming.element_idx).copied() {
                reconcile_existing_image(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    reusable,
                    incoming,
                    &geometry,
                    &layer,
                );
            } else {
                let visuals =
                    build_image_visuals(&incoming, &geometry, &mut meshes, &mut materials);
                let shadow_casting = incoming.shadow_casting;
                commands.entity(panel_entity).with_children(|children| {
                    let mut child = children.spawn((
                        incoming,
                        visuals.mesh,
                        visuals.material,
                        visuals.transform,
                        layer.clone(),
                    ));
                    apply_image_shadow_casting(&mut child, shadow_casting);
                });
            }
        }

        for (entity, cached, _, child_of) in &existing_children {
            if child_of.parent() == panel_entity && !visited_indices.contains(&cached.element_idx) {
                commands.entity(entity).despawn();
            }
        }
    }
    perf.reconcile_ms = reconcile_start
        .elapsed()
        .as_secs_f32()
        .mul_add(MILLISECONDS_PER_SECOND, perf.reconcile_ms);
}

fn collect_panel_image_commands(
    commands: &[RenderCommand],
    draw_order: &DrawOrder,
    precompose_cache: &PanelPrecomposeCache,
    clip_rects: &[Option<BoundingBox>],
    viewport: BoundingBox,
    shadow_casting: ShadowCasting,
) -> Vec<PanelImageChild> {
    commands
        .iter()
        .enumerate()
        .filter_map(|(cmd_index, cmd)| {
            if cmd.kind.draw_batch_family().is_some() {
                return None;
            }
            clip::effective_clip(cmd.bounds, clip_rects[cmd_index], viewport)?;
            let draw_depth = draw_order.depth_for(cmd_index)?;
            match &cmd.kind {
                RenderCommandKind::Image { handle, tint } => Some(PanelImageChild {
                    element_idx: cmd.element_idx,
                    draw_depth,
                    handle: handle.clone(),
                    tint: *tint,
                    bounds: cmd.bounds,
                    shadow_casting,
                }),
                RenderCommandKind::PrecomposeLdr => {
                    let entry = precompose_cache.entry(cmd.element_idx)?;
                    Some(PanelImageChild {
                        element_idx: cmd.element_idx,
                        draw_depth,
                        handle: entry.image.clone(),
                        tint: Color::WHITE,
                        bounds: cmd.bounds,
                        shadow_casting,
                    })
                },
                _ => None,
            }
        })
        .collect()
}

/// Panel-to-world placement factors for one reconcile pass.
struct ImageGeometry {
    points_to_world: f32,
    anchor_x:        f32,
    anchor_y:        f32,
}

/// Updates one reused image child against its cached inputs: skips it when
/// nothing changed, mutates `base_color` in place on a tint-only change, or
/// rebuilds its mesh and material when the handle, bounds, or command depth
/// moved.
///
/// Image tint has no cascade, so this comparison is the only no-op suppressor.
/// Because `materials.get_mut` marks the asset modified on access, the tint
/// branch is reached only when the cached tint actually differs. A `draw_depth`
/// move still rebuilds the image visual; same-z-index command-index shifts keep
/// the same material depth bias, but z-index-rank changes do not.
fn reconcile_existing_image(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    reusable: ReusableImageChild,
    incoming: PanelImageChild,
    geometry: &ImageGeometry,
    layer: &RenderLayers,
) {
    let cached = reusable.cached;
    let visuals_unchanged = cached.handle == incoming.handle
        && cached.draw_depth == incoming.draw_depth
        && bounds_bits(&cached.bounds) == bounds_bits(&incoming.bounds);
    let tint_changed = cached.tint != incoming.tint;
    let shadow_changed = cached.shadow_casting != incoming.shadow_casting;

    if visuals_unchanged {
        let mut entity_commands = commands.entity(reusable.entity);
        entity_commands.insert(layer.clone());
        apply_image_shadow_casting(&mut entity_commands, incoming.shadow_casting);
        if !tint_changed && !shadow_changed {
            return;
        }
        if tint_changed
            && let Some(mut material) = materials.get_mut(&reusable.material.0)
            && material.base_color != incoming.tint
        {
            material.base_color = incoming.tint;
        }
        entity_commands.insert(incoming);
        return;
    }

    let visuals = build_image_visuals(&incoming, geometry, meshes, materials);
    let shadow_casting = incoming.shadow_casting;
    let mut entity_commands = commands.entity(reusable.entity);
    entity_commands.insert((
        incoming,
        visuals.mesh,
        visuals.material,
        visuals.transform,
        layer.clone(),
    ));
    apply_image_shadow_casting(&mut entity_commands, shadow_casting);
}

fn apply_image_shadow_casting(child: &mut EntityCommands<'_>, shadow_casting: ShadowCasting) {
    match shadow_casting {
        ShadowCasting::On => {
            child.remove::<NotShadowCaster>();
        },
        ShadowCasting::Off => {
            child.insert(NotShadowCaster);
        },
    }
}

/// Builds the rectangle mesh, tinted-texture material, and panel-local
/// transform for one image child.
fn build_image_visuals(
    incoming: &PanelImageChild,
    geometry: &ImageGeometry,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> ImageVisuals {
    let bounds = &incoming.bounds;
    let world_width = bounds.width * geometry.points_to_world;
    let world_height = bounds.height * geometry.points_to_world;
    let world_x = bounds
        .x
        .mul_add(geometry.points_to_world, world_width * 0.5)
        - geometry.anchor_x;
    let world_y = -(bounds
        .y
        .mul_add(geometry.points_to_world, world_height * 0.5)
        - geometry.anchor_y);

    let mesh = meshes.add(Rectangle::new(world_width, world_height));
    let material = materials.add(StandardMaterial {
        base_color: incoming.tint,
        base_color_texture: Some(incoming.handle.clone()),
        unlit: true,
        double_sided: true,
        cull_mode: None,
        alpha_mode: AlphaMode::Blend,
        depth_bias: incoming.draw_depth.screen_depth_bias().get(),
        ..default()
    });

    ImageVisuals {
        mesh:      Mesh3d(mesh),
        material:  MeshMaterial3d(material),
        transform: Transform::from_xyz(world_x, world_y, TEXT_Z_OFFSET),
    }
}

/// A [`BoundingBox`]'s four floats as raw bits for exact comparison.
const fn bounds_bits(bounds: &BoundingBox) -> [u32; 4] {
    [
        bounds.x.to_bits(),
        bounds.y.to_bits(),
        bounds.width.to_bits(),
        bounds.height.to_bits(),
    ]
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;
    use bevy_kana::ToF32;

    use super::reconcile_panel_text_children;
    use crate::Mm;
    use crate::PanelElementId;
    use crate::PanelText;
    use crate::cascade::Override;
    use crate::cascade::TextAlpha;
    use crate::constants::MONOSPACE_WIDTH_RATIO;
    use crate::layout::BoundingBox;
    use crate::layout::DrawZIndex;
    use crate::layout::LayoutBuilder;
    use crate::layout::LayoutTree;
    use crate::layout::RectangleSource;
    use crate::layout::RenderCommand;
    use crate::layout::RenderCommandKind;
    use crate::layout::ShadowCasting;
    use crate::layout::Text;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;
    use crate::layout::TextStyle;
    use crate::panel::ComputedDiegeticPanel;
    use crate::panel::DiegeticPanel;
    use crate::panel::DiegeticPanelCommands;
    use crate::panel::HeadlessLayoutPlugin;
    use crate::panel::PanelPrecomposeCache;
    use crate::panel::PrecomposeCacheEntry;
    use crate::render::clip;
    use crate::render::constants::LAYER_DEPTH_BIAS;
    use crate::render::draw_order::DrawOrder;
    use crate::render::panel_text::PanelTextLayout;
    use crate::render::panel_text::PanelTextRuns;
    use crate::render::world_text::TextContent;
    use crate::text::DiegeticTextMeasurer;

    const BACKGROUND_ELEMENT_INDEX: usize = 0;
    const IMAGE_ELEMENT_INDEX: usize = 1;
    const PRECOMPOSE_CAMERA_BITS: u64 = 2;
    const PRECOMPOSE_ELEMENT_INDEX: usize = 2;
    const PRECOMPOSE_HELPER_BITS: u64 = 1;
    const TEST_BOUNDS_SIZE: f32 = 10.0;
    const TEST_VIEWPORT_SIZE: f32 = 100.0;
    const TEST_Z_INDEX: DrawZIndex = DrawZIndex(0);

    /// Records which reused children had each gated component rewritten in the
    /// most recent reconcile pass, so a test can assert an unchanged run stays
    /// un-`Changed`.
    #[derive(Resource, Default)]
    struct ChangedProbe {
        text:   Vec<Entity>,
        style:  Vec<Entity>,
        layout: Vec<Entity>,
        alpha:  Vec<Entity>,
    }

    /// Captures, each frame, the labels whose gated components changed since the
    /// probe last ran. Runs after reconcile + its command flush.
    fn probe_changed(
        mut probe: ResMut<ChangedProbe>,
        labels: Query<(
            Entity,
            Ref<TextContent>,
            Ref<TextStyle>,
            Ref<PanelTextLayout>,
            Option<Ref<Override<TextAlpha>>>,
        )>,
    ) {
        probe.text.clear();
        probe.style.clear();
        probe.layout.clear();
        probe.alpha.clear();
        for (entity, text, style, layout, alpha) in &labels {
            if text.is_changed() {
                probe.text.push(entity);
            }
            if style.is_changed() {
                probe.style.push(entity);
            }
            if layout.is_changed() {
                probe.layout.push(entity);
            }
            if alpha.is_some_and(|node_override| node_override.is_changed()) {
                probe.alpha.push(entity);
            }
        }
    }

    fn monospace_measurer() -> DiegeticTextMeasurer {
        DiegeticTextMeasurer {
            measure_fn: Arc::new(|text: &str, measure: &TextMeasure| {
                let char_width = measure.size * MONOSPACE_WIDTH_RATIO;
                let width = text
                    .lines()
                    .map(|line| line.chars().count().to_f32() * char_width)
                    .fold(0.0_f32, f32::max);
                let line_count = text.lines().count().max(1).to_f32();
                TextDimensions {
                    width,
                    height: measure.size * line_count,
                    line_height: measure.size,
                }
            }),
        }
    }

    /// App with headless layout plus the gated reconcile and a change probe
    /// chained after the command flush.
    fn reconcile_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(monospace_measurer());
        app.add_plugins(HeadlessLayoutPlugin);
        app.init_resource::<ChangedProbe>();
        app.add_systems(
            PostUpdate,
            (reconcile_panel_text_children, ApplyDeferred, probe_changed).chain(),
        );
        app
    }

    fn two_text_tree(first: Color, second: Color) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text(("Alpha", TextStyle::new(10.0).with_color(first)));
        builder.text(("Beta", TextStyle::new(10.0).with_color(second)));
        builder.build()
    }

    fn single_alpha_text_tree(color: Color, alpha: AlphaMode) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text((
            "Glow",
            TextStyle::new(10.0)
                .with_color(color)
                .with_alpha_mode(alpha),
        ));
        builder.build()
    }

    fn spawn_panel(app: &mut App, tree: LayoutTree) -> Entity {
        app.world_mut()
            .spawn(
                DiegeticPanel::world()
                    .size(Mm(100.0), Mm(50.0))
                    .with_tree(tree)
                    .build()
                    .expect("panel should build"),
            )
            .id()
    }

    fn labels_by_text(app: &mut App) -> HashMap<String, Entity> {
        let mut state = app
            .world_mut()
            .query_filtered::<(Entity, &TextContent), With<TextContent>>();
        state
            .iter(app.world())
            .map(|(entity, text)| (text.text().to_owned(), entity))
            .collect()
    }

    #[test]
    fn unchanged_run_is_not_rewritten_across_a_visual_only_rebuild() {
        let mut app = reconcile_app();
        let panel = spawn_panel(&mut app, two_text_tree(Color::WHITE, Color::WHITE));
        app.update();

        let labels = labels_by_text(&mut app);
        let recolored = labels["Alpha"];
        let untouched = labels["Beta"];

        // Visual-only: only "Alpha" changes color. "Beta" stays byte-identical,
        // so its text/style/layout must not be rewritten.
        app.world_mut()
            .commands()
            .set_tree(panel, two_text_tree(Color::BLACK, Color::WHITE));
        app.update();

        let probe = app.world().resource::<ChangedProbe>();
        // The recolored run's style is rewritten; its text and layout are not.
        assert!(probe.style.contains(&recolored));
        assert!(!probe.text.contains(&recolored));
        assert!(!probe.layout.contains(&recolored));
        // The byte-identical run is left entirely alone.
        assert!(!probe.text.contains(&untouched));
        assert!(!probe.style.contains(&untouched));
        assert!(!probe.layout.contains(&untouched));
    }

    #[test]
    fn alpha_unchanged_reused_child_keeps_its_override() {
        let mut app = reconcile_app();
        let panel = spawn_panel(
            &mut app,
            single_alpha_text_tree(Color::WHITE, AlphaMode::Add),
        );
        app.update();

        let label = labels_by_text(&mut app)["Glow"];
        assert!(
            app.world().get::<Override<TextAlpha>>(label).is_some(),
            "explicit-alpha label should carry Override<TextAlpha> after the first pass"
        );

        // Recolor only; alpha stays AlphaMode::Add.
        app.world_mut()
            .commands()
            .set_tree(panel, single_alpha_text_tree(Color::BLACK, AlphaMode::Add));
        app.update();

        let probe = app.world().resource::<ChangedProbe>();
        // The color change rewrites the style, proving reconcile ran on this run.
        assert!(probe.style.contains(&label));
        // The unchanged alpha override is not re-touched.
        assert!(!probe.alpha.contains(&label));
    }

    #[test]
    fn reconcile_keys_by_run_id_and_line_index() {
        // One wrapped run (shared id) across three lines: the `(id, line_index)`
        // key distinguishes the three children, while keying by id alone would
        // collapse them — the property reconcile relies on to reuse each line.
        let id = PanelElementId::named("run");
        let existing: Vec<(Entity, PanelTextLayout)> = (0..3)
            .map(|line| {
                let panel_text_child = PanelTextLayout {
                    id:               id.clone(),
                    line_index:       line,
                    element_idx:      7,
                    draw_ordinal:     line,
                    depth_bias:       line.to_f32() * LAYER_DEPTH_BIAS,
                    oit_depth_offset: 0.0,
                    bounds:           BoundingBox {
                        x:      0.0,
                        y:      line.to_f32() * 10.0,
                        width:  100.0,
                        height: 10.0,
                    },
                    scale_x:          1.0,
                    scale_y:          1.0,
                    anchor_x:         0.0,
                    anchor_y:         0.0,
                    clip_rect:        None,
                };
                (
                    Entity::from_raw_u32(line.try_into().expect("small")).expect("valid"),
                    panel_text_child,
                )
            })
            .collect();

        let mut by_key: HashMap<(PanelElementId, usize), Entity> = HashMap::new();
        for (entity, layout) in &existing {
            by_key.insert((layout.id.clone(), layout.line_index), *entity);
        }
        assert_eq!(by_key.len(), 3);

        let mut by_id_only: HashMap<PanelElementId, Entity> = HashMap::new();
        for (entity, layout) in &existing {
            by_id_only.insert(layout.id.clone(), *entity);
        }
        assert_eq!(by_id_only.len(), 1);
    }

    fn one_text_tree(text: &str) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text((text, TextStyle::new(10.0)));
        builder.build()
    }

    /// App with headless layout and the gated reconcile (`PostUpdate`), so a test
    /// can edit the authoritative tree and watch the layout pipeline relayout and
    /// reconcile re-derive the run child end to end.
    fn text_source_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(monospace_measurer());
        app.add_plugins(HeadlessLayoutPlugin);
        app.add_systems(PostUpdate, reconcile_panel_text_children);
        app
    }

    fn first_text_child(app: &mut App) -> Entity {
        let mut state = app
            .world_mut()
            .query_filtered::<Entity, With<TextContent>>();
        let children: Vec<Entity> = state.iter(app.world()).collect();
        assert_eq!(children.len(), 1, "expected exactly one text child");
        children[0]
    }

    fn content_width(app: &App, panel: Entity) -> f32 {
        app.world()
            .get::<ComputedDiegeticPanel>(panel)
            .expect("computed panel should exist")
            .content_bounds()
            .expect("content bounds should exist")
            .width
    }

    #[test]
    fn editing_run_text_in_the_tree_relayouts_and_re_derives_the_child() {
        let mut app = text_source_app();
        let panel = spawn_panel(&mut app, one_text_tree("Hi"));
        app.update();
        app.update();

        let child = first_text_child(&mut app);
        let element_idx = app
            .world()
            .get::<PanelTextLayout>(child)
            .expect("child should carry its layout")
            .element_idx;
        let before = content_width(&app, panel);

        // Tree-authoritative edit — exactly what `PanelText` / `DiegeticTextMut`
        // do internally: write `El.text` and bump the tree revision.
        app.world_mut()
            .get_mut::<DiegeticPanel>(panel)
            .expect("panel should exist")
            .sync_run_text_cache(element_idx, "Hello World");
        app.update();

        // The tree carries the new string ...
        assert_eq!(
            app.world()
                .get::<DiegeticPanel>(panel)
                .expect("panel should exist")
                .tree()
                .element_text(element_idx),
            Some("Hello World"),
        );
        // ... reconcile re-derived the run child from it ...
        let child = first_text_child(&mut app);
        assert_eq!(
            app.world()
                .get::<TextContent>(child)
                .expect("child should carry TextContent")
                .text(),
            "Hello World",
        );
        // ... and the wider string grew the content bounds.
        let after = content_width(&app, panel);
        assert!(
            after > before,
            "editing the run should relayout: width {before} -> {after}",
        );
    }

    // ── Panel↔run relationship lifecycle (`TextRunOf`/`PanelTextRuns`) ──

    /// Records whether any panel's [`PanelTextRuns`] changed since the probe last
    /// ran, so a test can assert a reuse-only reconcile leaves the set untouched.
    #[derive(Resource, Default)]
    struct RunsChangedProbe {
        changed: bool,
    }

    fn probe_runs_changed(
        mut probe: ResMut<RunsChangedProbe>,
        changed: Query<(), Changed<PanelTextRuns>>,
    ) {
        probe.changed = !changed.is_empty();
    }

    /// Like [`reconcile_app`], but probes [`PanelTextRuns`] change detection
    /// instead of the per-component change probe.
    fn relationship_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(monospace_measurer());
        app.add_plugins(HeadlessLayoutPlugin);
        app.init_resource::<RunsChangedProbe>();
        app.add_systems(
            PostUpdate,
            (
                reconcile_panel_text_children,
                ApplyDeferred,
                probe_runs_changed,
            )
                .chain(),
        );
        app
    }

    fn empty_tree() -> LayoutTree { LayoutBuilder::new(100.0, 50.0).build() }

    fn two_named_tree(first: &str, second: &str) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text(Text::new(first, TextStyle::new(10.0)).id(PanelElementId::named("a")));
        builder.text(Text::new(second, TextStyle::new(10.0)).id(PanelElementId::named("b")));
        builder.build()
    }

    #[test]
    fn two_no_op_reconcile_passes_leave_panel_text_runs_unchanged() {
        let mut app = relationship_app();
        let panel = spawn_panel(&mut app, two_text_tree(Color::WHITE, Color::WHITE));

        // Frame 1 spawns both runs, which mutates the relationship set.
        app.update();
        assert!(
            app.world().resource::<RunsChangedProbe>().changed,
            "the spawn pass populates PanelTextRuns"
        );
        // Frame 2 settles: nothing dirties the panel, so the set is left alone.
        app.update();

        // Two further visual-only rebuilds: recolor only, same auto ids, so
        // reconcile reuses every run entity. `TextRunOf` is never re-inserted, so
        // the relationship set must not register a change on either pass.
        for color in [Color::BLACK, Color::srgb(0.5, 0.5, 0.5)] {
            app.world_mut()
                .commands()
                .set_tree(panel, two_text_tree(color, Color::WHITE));
            app.update();
            assert!(
                !app.world().resource::<RunsChangedProbe>().changed,
                "a reuse-only reconcile must not mutate PanelTextRuns",
            );
        }
    }

    #[test]
    fn set_tree_empties_the_run_set_then_reconcile_repopulates_it() {
        let mut app = relationship_app();
        let panel = spawn_panel(&mut app, two_named_tree("Alpha", "Beta"));
        app.update();
        assert_eq!(
            app.world()
                .get::<PanelTextRuns>(panel)
                .map(RelationshipTarget::len),
            Some(2),
            "two named runs spawn under the panel",
        );

        // Swap to a text-less tree: every run is unvisited this pass, so reconcile
        // despawns them and the relationship set empties.
        app.world_mut().commands().set_tree(panel, empty_tree());
        app.update();
        let emptied = app
            .world()
            .get::<PanelTextRuns>(panel)
            .map_or(0, RelationshipTarget::len);
        assert_eq!(emptied, 0, "set_tree to an empty tree drops every run");

        // Swap back to a multi-run tree: reconcile repopulates the set and the
        // named index resolves each run by a single O(1) `text_child` lookup.
        app.world_mut()
            .commands()
            .set_tree(panel, two_named_tree("Gamma", "Delta"));
        app.update();
        let runs = app
            .world()
            .get::<PanelTextRuns>(panel)
            .expect("the set repopulates from the new tree");
        assert_eq!(runs.len(), 2, "both new runs re-enter the set");

        let data = app
            .world()
            .get::<DiegeticPanel>(panel)
            .expect("panel exists");
        assert!(
            data.text_child(&PanelElementId::named("a")).is_some(),
            "named run 'a' resolves O(1) after repopulate",
        );
        assert!(
            data.text_child(&PanelElementId::named("b")).is_some(),
            "named run 'b' resolves O(1) after repopulate",
        );
    }

    #[test]
    fn panel_despawn_drops_all_runs_without_double_despawn() {
        let mut app = relationship_app();
        let panel = spawn_panel(&mut app, two_named_tree("Alpha", "Beta"));
        app.update();

        let runs: Vec<Entity> = app
            .world()
            .get::<PanelTextRuns>(panel)
            .expect("a reconciled panel carries its runs")
            .iter()
            .collect();
        assert_eq!(runs.len(), 2, "both runs are tracked before despawn");

        // `ChildOf`'s `linked_spawn` is the sole recursive despawn path;
        // `PanelTextRuns` deliberately omits `linked_spawn`, so despawning the
        // panel must drop every run exactly once with no second recursive pass.
        app.world_mut().entity_mut(panel).despawn();
        app.update();

        for run in runs {
            assert!(
                !app.world().entities().contains(run),
                "every run despawns with its panel",
            );
        }
        assert!(
            !app.world().entities().contains(panel),
            "the panel itself is gone",
        );
    }

    /// A run of auto text elements followed by one named run. Auto ids come from
    /// a per-build counter over unnamed `text()` declarations in build order, so
    /// an auto run's id is its position among the autos; the named run's id is
    /// fixed.
    fn autos_then_named_tree(autos: &[&str], named: &str) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        for text in autos {
            builder.text((*text, TextStyle::new(10.0)));
        }
        builder.text(Text::new(named, TextStyle::new(10.0)).id(PanelElementId::named("keep")));
        builder.build()
    }

    #[test]
    fn a_structural_edit_keeps_named_runs_but_repositions_auto_runs() {
        let mut app = reconcile_app();
        // One auto run ("first") and one named run ("keep").
        let panel = spawn_panel(&mut app, autos_then_named_tree(&["first"], "keep"));
        app.update();

        let before = labels_by_text(&mut app);
        let named_before = before["keep"];
        let first_before = before["first"];

        // Insert an auto sibling ahead of "first". The named run's key is
        // `(Named("keep"), 0)` — id-stable, so it keeps its entity (TR-D). The
        // auto ids are positional: "inserted" takes `Auto(0)` and reuses the old
        // entity, so "first" shifts to `Auto(1)` and lands on a fresh entity —
        // content identity does not follow an auto run across the edit.
        app.world_mut()
            .commands()
            .set_tree(panel, autos_then_named_tree(&["inserted", "first"], "keep"));
        app.update();

        let after = labels_by_text(&mut app);
        assert_eq!(
            after["keep"], named_before,
            "the named run keeps its entity across the structural edit",
        );
        assert_ne!(
            after["first"], first_before,
            "the auto run is positional: \"first\" shifts to a new entity when a sibling is inserted ahead",
        );
    }

    /// Monospace measurer that counts every call, so a test can assert a no-op
    /// edit re-measures nothing.
    fn counting_measurer(counter: Arc<AtomicUsize>) -> DiegeticTextMeasurer {
        DiegeticTextMeasurer {
            measure_fn: Arc::new(move |text: &str, measure: &TextMeasure| {
                counter.fetch_add(1, Ordering::Relaxed);
                let char_width = measure.size * MONOSPACE_WIDTH_RATIO;
                let width = text
                    .lines()
                    .map(|line| line.chars().count().to_f32() * char_width)
                    .fold(0.0_f32, f32::max);
                let line_count = text.lines().count().max(1).to_f32();
                TextDimensions {
                    width,
                    height: measure.size * line_count,
                    line_height: measure.size,
                }
            }),
        }
    }

    /// [`text_source_app`] with a call-counting measurer in place of the plain
    /// monospace one.
    fn counting_text_source_app(counter: Arc<AtomicUsize>) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(counting_measurer(counter));
        app.add_plugins(HeadlessLayoutPlugin);
        app.add_systems(PostUpdate, reconcile_panel_text_children);
        app
    }

    #[test]
    fn an_unchanged_set_text_fires_no_measure() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut app = counting_text_source_app(Arc::clone(&counter));
        let panel = spawn_panel(&mut app, one_text_tree("Hi"));
        app.update();
        app.update();
        let baseline = counter.load(Ordering::Relaxed);
        assert!(baseline > 0, "the initial layout measures the run");

        // Rewrite the byte-identical string through the public edit path.
        // `TextEdit::set_text` read-compares before taking the `&mut DiegeticPanel`
        // borrow, so an unchanged string never dirties the panel, never relayouts,
        // and so `MeasureTextFn` fires zero more times (TR-L).
        let wrote = app
            .world_mut()
            .run_system_once(move |mut text: PanelText| text.set_sole_text(panel, "Hi"))
            .expect("system runs");
        assert!(wrote, "the lone run resolves");
        app.update();

        assert_eq!(
            counter.load(Ordering::Relaxed),
            baseline,
            "a no-op set_text must not re-invoke MeasureTextFn",
        );
    }

    #[test]
    fn image_batch_family_commands_do_not_spawn_legacy_children() {
        let commands = vec![
            background_command(BACKGROUND_ELEMENT_INDEX),
            image_command(IMAGE_ELEMENT_INDEX),
            precompose_command(PRECOMPOSE_ELEMENT_INDEX),
        ];
        let draw_order = DrawOrder::from_commands(&commands);
        let clip_rects = clip::compute_clip_rects(&commands);
        let mut precompose_cache = PanelPrecomposeCache::default();
        precompose_cache.entries_mut().insert(
            PRECOMPOSE_ELEMENT_INDEX,
            PrecomposeCacheEntry {
                image:        Handle::<Image>::default(),
                helper_panel: Entity::from_bits(PRECOMPOSE_HELPER_BITS),
                camera:       Entity::from_bits(PRECOMPOSE_CAMERA_BITS),
                pixel_size:   UVec2::ONE,
            },
        );

        let image_children = super::collect_panel_image_commands(
            &commands,
            &draw_order,
            &precompose_cache,
            &clip_rects,
            test_viewport(),
            ShadowCasting::On,
        );

        assert!(image_children.is_empty());
    }

    fn background_command(element_idx: usize) -> RenderCommand {
        RenderCommand {
            bounds: test_bounds(),
            kind: RenderCommandKind::Rectangle {
                color:  Color::WHITE,
                source: RectangleSource::Background,
            },
            element_idx,
            z_index: TEST_Z_INDEX,
        }
    }

    fn image_command(element_idx: usize) -> RenderCommand {
        RenderCommand {
            bounds: test_bounds(),
            kind: RenderCommandKind::Image {
                handle: Handle::<Image>::default(),
                tint:   Color::WHITE,
            },
            element_idx,
            z_index: TEST_Z_INDEX,
        }
    }

    fn precompose_command(element_idx: usize) -> RenderCommand {
        RenderCommand {
            bounds: test_bounds(),
            kind: RenderCommandKind::PrecomposeLdr,
            element_idx,
            z_index: TEST_Z_INDEX,
        }
    }

    fn test_bounds() -> BoundingBox {
        BoundingBox {
            x:      0.0,
            y:      0.0,
            width:  TEST_BOUNDS_SIZE,
            height: TEST_BOUNDS_SIZE,
        }
    }

    fn test_viewport() -> BoundingBox {
        BoundingBox {
            x:      0.0,
            y:      0.0,
            width:  TEST_VIEWPORT_SIZE,
            height: TEST_VIEWPORT_SIZE,
        }
    }
}
