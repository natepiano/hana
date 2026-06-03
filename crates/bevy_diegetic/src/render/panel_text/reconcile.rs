use std::collections::HashMap;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy_kana::ToF32;

use super::PanelTextLayout;
use super::PanelTextRuns;
use super::TextRunOf;
use crate::PanelFieldId;
use crate::cascade;
use crate::cascade::Override;
use crate::cascade::TextAlpha;
use crate::cascade::TextLighting;
use crate::cascade::TextSidedness;
use crate::layout::Anchor;
use crate::layout::BoundingBox;
use crate::layout::GlyphLighting;
use crate::layout::GlyphSidedness;
use crate::layout::RenderCommand;
use crate::layout::RenderCommandKind;
use crate::layout::TextStyle;
use crate::panel::ComputedDiegeticPanel;
use crate::panel::DiegeticPanel;
use crate::render::clip;
use crate::render::constants;
use crate::render::constants::TEXT_Z_OFFSET;
use crate::render::world_text::TextContent;

/// A reused panel-text child plus the components reconcile compares incoming
/// values against before deciding whether to write. The references borrow the
/// `existing_children` query for one reconcile pass.
#[derive(Clone, Copy)]
struct ReusableChild<'a> {
    entity:    Entity,
    text:      &'a TextContent,
    style:     &'a TextStyle,
    layout:    &'a PanelTextLayout,
    alpha:     Option<&'a Override<TextAlpha>>,
    lighting:  Option<&'a Override<TextLighting>>,
    sidedness: Option<&'a Override<TextSidedness>>,
}

/// One text render command resolved to its reconcile inputs: source
/// `(element_idx, command_index)`, run `id`, per-run `line_index`, the string,
/// its style, layout bounds, and the effective clip rect.
type PendingTextChild = (
    usize,
    usize,
    PanelFieldId,
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
                let id = panel
                    .tree()
                    .element_field_id(cmd.element_idx)
                    .cloned()
                    .unwrap_or_else(|| {
                        PanelFieldId::auto(u32::try_from(cmd.element_idx).unwrap_or(0))
                    });
                let counter = line_counter.entry(cmd.element_idx).or_insert(0);
                let line_index = *counter;
                *counter += 1;
                Some((
                    cmd.element_idx,
                    cmd_index,
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

/// Reconciles [`TextContent`] children for each changed [`ComputedDiegeticPanel`].
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
        Option<&Override<TextAlpha>>,
        Option<&Override<TextLighting>>,
        Option<&Override<TextSidedness>>,
    )>,
    mut commands: Commands,
) {
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
        // content-stable, unlike the former `(element_idx, command_index)` pair
        // that a sibling reorder shifted (R7). Auto ids preserve that positional
        // behavior; named ids survive the reorder.
        let text_commands = collect_text_commands(&panel, &result.commands, &clip_rects, viewport);

        // Source the panel's existing runs from `PanelTextRuns` (the typed
        // text-run index) rather than scanning every `TextContent` child and
        // filtering by parent. `None` means no run has spawned yet (first pass).
        let existing_run_entities: &[Entity] = panel_runs.map_or(&[][..], |runs| &**runs);
        let mut existing_by_key: HashMap<(PanelFieldId, usize), ReusableChild> = HashMap::new();
        for &entity in existing_run_entities {
            let Ok((text, style, layout, alpha, lighting, sidedness)) = existing_runs.get(entity)
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
                    alpha,
                    lighting,
                    sidedness,
                },
            );
        }

        let mut visited_keys: Vec<(PanelFieldId, usize)> = Vec::new();
        let mut text_index: HashMap<PanelFieldId, Entity> = HashMap::new();
        for (element_idx, cmd_index, id, line_index, text, config, bounds, clip) in &text_commands {
            // A label's own cascade overrides (`TextStyle::with_alpha_mode` /
            // `with_lighting` / `with_sidedness`) are captured before
            // `for_shaping()` clears them, then inserted as `Override<A>` on
            // the label. `None` means the label inherits the panel value.
            let label_alpha = config.alpha_mode();
            let label_lighting = config.lighting();
            let label_sidedness = config.sidedness();
            let style = config.for_shaping(Anchor::TopLeft);
            let panel_text_child = PanelTextLayout {
                id: id.clone(),
                line_index: *line_index,
                element_idx: *element_idx,
                command_index: *cmd_index,
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
                    label_alpha,
                    label_lighting,
                    label_sidedness,
                });
                reusable.entity
            } else {
                spawn_panel_text_child(SpawnPanelTextChild {
                    commands: &mut commands,
                    panel_entity,
                    text,
                    style,
                    layout: panel_text_child,
                    label_alpha,
                    label_lighting,
                    label_sidedness,
                })
            };

            // Address a run by its first line: `text_child(id)` resolves this
            // entity. Auto-id runs land here too but are unreachable — no caller
            // can build their `PanelFieldId::Auto`.
            if *line_index == 0 {
                text_index.insert(id.clone(), entity);
            }
        }

        for &entity in existing_run_entities {
            let Ok((_, _, layout, _, _, _)) = existing_runs.get(entity) else {
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
}

/// Inputs to [`spawn_panel_text_child`]. Grouped into a struct because reconcile
/// threads text, style, layout, and three captured cascade overrides through to
/// a freshly spawned child.
struct SpawnPanelTextChild<'a, 'w, 's> {
    commands:        &'a mut Commands<'w, 's>,
    panel_entity:    Entity,
    text:            &'a str,
    style:           TextStyle,
    layout:          PanelTextLayout,
    label_alpha:     Option<AlphaMode>,
    label_lighting:  Option<GlyphLighting>,
    label_sidedness: Option<GlyphSidedness>,
}

/// Spawns a new panel-text child under `panel_entity` and applies whichever of
/// the three captured cascade overrides (alpha, lighting, sidedness) the label
/// authored. `None` for an override means the label inherits the panel value.
fn spawn_panel_text_child(request: SpawnPanelTextChild<'_, '_, '_>) -> Entity {
    let SpawnPanelTextChild {
        commands,
        panel_entity,
        text,
        style,
        layout,
        label_alpha,
        label_lighting,
        label_sidedness,
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
            TextRunOf(panel_entity),
        ));
        spawned = child.id();
        if let Some(alpha_mode) = label_alpha {
            cascade::apply_cascade_override(&mut child, TextAlpha(alpha_mode));
        }
        if let Some(lighting) = label_lighting {
            cascade::apply_cascade_override(&mut child, TextLighting(lighting));
        }
        if let Some(sidedness) = label_sidedness {
            cascade::apply_cascade_override(&mut child, TextSidedness(sidedness));
        }
    });
    spawned
}

/// Inputs to [`update_reused_panel_text_child`]. Grouped into a struct because
/// reconcile threads text, style, layout, and three captured cascade overrides
/// through to a reused child.
struct UpdateReusedChild<'a, 'w, 's> {
    commands:        &'a mut Commands<'w, 's>,
    reusable:        ReusableChild<'a>,
    text:            &'a str,
    style:           TextStyle,
    layout:          PanelTextLayout,
    label_alpha:     Option<AlphaMode>,
    label_lighting:  Option<GlyphLighting>,
    label_sidedness: Option<GlyphSidedness>,
}

/// Writes each gated component of a reused panel-text child only when it
/// differs, so an unchanged run stays un-`Changed` and
/// `shape_panel_text_children` plus the mesh rebuild skip it.
///
/// `gating_eq` excludes render-context fields and compares floats by bits;
/// the cascade overrides (alpha, lighting, sidedness) are gated on their own
/// because `gating_eq` ignores them. Writing one unconditionally would re-fire
/// `Changed<Resolved<A>>` on every run and defeat the per-run short-circuit
/// downstream.
fn update_reused_panel_text_child(request: UpdateReusedChild<'_, '_, '_>) {
    let UpdateReusedChild {
        commands,
        reusable,
        text,
        style,
        layout,
        label_alpha,
        label_lighting,
        label_sidedness,
    } = request;
    let mut child = commands.entity(reusable.entity);
    if reusable.text.text() != text {
        child.insert(TextContent::new(text.to_owned()));
    }
    if !reusable.style.gating_eq(&style) {
        child.insert(style);
    }
    if !reusable.layout.gating_eq(&layout) {
        child.insert(layout);
    }
    match label_alpha {
        Some(alpha_mode) => {
            let incoming = TextAlpha(alpha_mode);
            if reusable.alpha.map(|node_override| node_override.0) != Some(incoming) {
                cascade::apply_cascade_override(&mut child, incoming);
            }
        },
        None => {
            if reusable.alpha.is_some() {
                cascade::remove_cascade_override::<TextAlpha>(&mut child);
            }
        },
    }
    match label_lighting {
        Some(lighting) => {
            let incoming = TextLighting(lighting);
            if reusable.lighting.map(|node_override| node_override.0) != Some(incoming) {
                cascade::apply_cascade_override(&mut child, incoming);
            }
        },
        None => {
            if reusable.lighting.is_some() {
                cascade::remove_cascade_override::<TextLighting>(&mut child);
            }
        },
    }
    match label_sidedness {
        Some(sidedness) => {
            let incoming = TextSidedness(sidedness);
            if reusable.sidedness.map(|node_override| node_override.0) != Some(incoming) {
                cascade::apply_cascade_override(&mut child, incoming);
            }
        },
        None => {
            if reusable.sidedness.is_some() {
                cascade::remove_cascade_override::<TextSidedness>(&mut child);
            }
        },
    }
}

/// Marker plus cached reconcile inputs for an image child entity.
///
/// `reconcile_panel_image_children` compares the incoming `handle` / `tint` /
/// `bounds` / `command_index` against these cached values to decide whether to
/// skip the child, mutate its tint in place, or rebuild its mesh and material.
#[derive(Component, Clone, Debug)]
pub(super) struct PanelImageChild {
    /// Index of the source element in the layout tree (the reuse key).
    pub element_idx:   usize,
    /// Render-command slot the material's `depth_bias` derives from. A sibling
    /// insert/remove shifts this without changing the visual inputs, so it is
    /// gated like the rest to keep image layering correct under reorder.
    pub command_index: usize,
    /// Image asset handle from the most recent build.
    pub handle:        Handle<Image>,
    /// Tint color from the most recent build.
    pub tint:          Color,
    /// Layout bounds from the most recent build.
    pub bounds:        BoundingBox,
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
pub(super) fn reconcile_panel_image_children(
    changed_panels: Query<
        (Entity, &DiegeticPanel, &ComputedDiegeticPanel),
        Changed<ComputedDiegeticPanel>,
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
) {
    for (panel_entity, panel, computed) in &changed_panels {
        let Some(result) = computed.result() else {
            continue;
        };

        let points_to_world = panel.points_to_world();
        let (anchor_x, anchor_y) = panel.anchor_offsets();
        let layer = RenderLayers::layer(0);

        let clip_rects = clip::compute_clip_rects(&result.commands);
        let viewport = clip::panel_viewport(panel);
        let image_commands: Vec<_> = result
            .commands
            .iter()
            .enumerate()
            .filter_map(|(cmd_index, cmd)| match &cmd.kind {
                RenderCommandKind::Image { handle, tint } => {
                    clip::effective_clip(cmd.bounds, clip_rects[cmd_index], viewport)?;
                    Some(PanelImageChild {
                        element_idx:   cmd.element_idx,
                        command_index: cmd_index,
                        handle:        handle.clone(),
                        tint:          *tint,
                        bounds:        cmd.bounds,
                    })
                },
                _ => None,
            })
            .collect();

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
                commands.entity(panel_entity).with_child((
                    incoming,
                    visuals.mesh,
                    visuals.material,
                    visuals.transform,
                    layer.clone(),
                ));
            }
        }

        for (entity, cached, _, child_of) in &existing_children {
            if child_of.parent() == panel_entity && !visited_indices.contains(&cached.element_idx) {
                commands.entity(entity).despawn();
            }
        }
    }
}

/// Panel-to-world placement factors for one reconcile pass.
struct ImageGeometry {
    points_to_world: f32,
    anchor_x:        f32,
    anchor_y:        f32,
}

/// Updates one reused image child against its cached inputs: skips it when
/// nothing changed, mutates `base_color` in place on a tint-only change, or
/// rebuilds its mesh and material when the handle, bounds, or command slot
/// moved.
///
/// Image tint has no cascade, so this comparison is the only no-op suppressor.
/// Because `materials.get_mut` marks the asset modified on access, the tint
/// branch is reached only when the cached tint actually differs (R5/F8). A
/// `command_index` move rebuilds the material because `depth_bias` lives there.
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
        && cached.command_index == incoming.command_index
        && bounds_bits(&cached.bounds) == bounds_bits(&incoming.bounds);

    if visuals_unchanged {
        if cached.tint == incoming.tint {
            return;
        }
        if let Some(mut material) = materials.get_mut(&reusable.material.0)
            && material.base_color != incoming.tint
        {
            material.base_color = incoming.tint;
        }
        commands.entity(reusable.entity).insert(incoming);
        return;
    }

    let visuals = build_image_visuals(&incoming, geometry, meshes, materials);
    commands.entity(reusable.entity).insert((
        incoming,
        visuals.mesh,
        visuals.material,
        visuals.transform,
        layer.clone(),
    ));
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
        depth_bias: incoming.command_index.to_f32() * constants::LAYER_DEPTH_BIAS,
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

    use bevy::asset::AssetPlugin;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;
    use bevy_kana::ToF32;

    use super::PanelImageChild;
    use super::reconcile_panel_image_children;
    use super::reconcile_panel_text_children;
    use crate::Mm;
    use crate::PanelFieldId;
    use crate::PanelText;
    use crate::cascade::Override;
    use crate::cascade::TextAlpha;
    use crate::constants::MONOSPACE_WIDTH_RATIO;
    use crate::layout::BoundingBox;
    use crate::layout::El;
    use crate::layout::LayoutBuilder;
    use crate::layout::LayoutTree;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;
    use crate::layout::TextStyle;
    use crate::panel::ComputedDiegeticPanel;
    use crate::panel::DiegeticPanel;
    use crate::panel::DiegeticPanelCommands;
    use crate::panel::HeadlessLayoutPlugin;
    use crate::render::constants::LAYER_DEPTH_BIAS;
    use crate::render::panel_text::PanelTextLayout;
    use crate::render::panel_text::PanelTextRuns;
    use crate::render::world_text::TextContent;
    use crate::text::DiegeticTextMeasurer;

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
        builder.text("Alpha", TextStyle::new(10.0).with_color(first));
        builder.text("Beta", TextStyle::new(10.0).with_color(second));
        builder.build()
    }

    fn single_alpha_text_tree(color: Color, alpha: AlphaMode) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text(
            "Glow",
            TextStyle::new(10.0)
                .with_color(color)
                .with_alpha_mode(alpha),
        );
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
        let id = PanelFieldId::named("run");
        let existing: Vec<(Entity, PanelTextLayout)> = (0..3)
            .map(|line| {
                let panel_text_child = PanelTextLayout {
                    id:            id.clone(),
                    line_index:    line,
                    element_idx:   7,
                    command_index: line,
                    bounds:        BoundingBox {
                        x:      0.0,
                        y:      line.to_f32() * 10.0,
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
                    Entity::from_raw_u32(line.try_into().expect("small")).expect("valid"),
                    panel_text_child,
                )
            })
            .collect();

        let mut by_key: HashMap<(PanelFieldId, usize), Entity> = HashMap::new();
        for (entity, layout) in &existing {
            by_key.insert((layout.id.clone(), layout.line_index), *entity);
        }
        assert_eq!(by_key.len(), 3);

        let mut by_id_only: HashMap<PanelFieldId, Entity> = HashMap::new();
        for (entity, layout) in &existing {
            by_id_only.insert(layout.id.clone(), *entity);
        }
        assert_eq!(by_id_only.len(), 1);
    }

    fn one_text_tree(text: &str) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text(text, TextStyle::new(10.0));
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

    // ── Panel↔run relationship lifecycle (Phase 4 `TextRunOf`/`PanelTextRuns`) ──

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
        builder.text_id(PanelFieldId::named("a"), first, TextStyle::new(10.0));
        builder.text_id(PanelFieldId::named("b"), second, TextStyle::new(10.0));
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
            data.text_child(&PanelFieldId::named("a")).is_some(),
            "named run 'a' resolves O(1) after repopulate",
        );
        assert!(
            data.text_child(&PanelFieldId::named("b")).is_some(),
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
    /// a per-build counter over `text()` calls in build order (`text_id` does not
    /// consume it), so an auto run's id is its position among the autos; the named
    /// run's id is fixed.
    fn autos_then_named_tree(autos: &[&str], named: &str) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        for text in autos {
            builder.text(*text, TextStyle::new(10.0));
        }
        builder.text_id(PanelFieldId::named("keep"), named, TextStyle::new(10.0));
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

    /// Accumulates the `StandardMaterial` ids that fired `AssetEvent::Modified`,
    /// so a test can assert an unchanged image's material was never re-touched.
    #[derive(Resource, Default)]
    struct ModifiedMaterials(Vec<AssetId<StandardMaterial>>);

    fn record_modified_materials(
        mut events: MessageReader<AssetEvent<StandardMaterial>>,
        mut probe: ResMut<ModifiedMaterials>,
    ) {
        for event in events.read() {
            if let AssetEvent::Modified { id } = event {
                probe.0.push(*id);
            }
        }
    }

    /// App with headless layout plus the gated image reconcile and a probe that
    /// records every modified `StandardMaterial`.
    fn image_reconcile_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(AssetPlugin::default())
            .insert_resource(monospace_measurer())
            .add_plugins(HeadlessLayoutPlugin)
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_resource::<ModifiedMaterials>()
            .add_systems(
                PostUpdate,
                (reconcile_panel_image_children, record_modified_materials).chain(),
            );
        app
    }

    /// A single image leaf sized in panel units; `with_background` toggles the
    /// element's background, which prepends a rectangle command and shifts the
    /// image's `command_index` without changing its `element_idx`.
    fn one_image_tree(handle: Handle<Image>, tint: Color, with_background: bool) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        let mut element = El::new().size(10.0, 10.0);
        if with_background {
            element = element.background(Color::srgb(0.2, 0.2, 0.2));
        }
        builder.image(element, handle, tint);
        builder.build()
    }

    fn two_image_tree(handle: Handle<Image>, first: Color, second: Color) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.image(El::new().size(10.0, 10.0), handle.clone(), first);
        builder.image(El::new().size(10.0, 10.0), handle, second);
        builder.build()
    }

    fn spawn_image_panel(app: &mut App, tree: LayoutTree) -> Entity {
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

    /// The single image child's entity and its material handle.
    fn single_image_child(app: &mut App) -> (Entity, Handle<StandardMaterial>) {
        let mut state =
            app.world_mut()
                .query::<(Entity, &PanelImageChild, &MeshMaterial3d<StandardMaterial>)>();
        let children: Vec<_> = state
            .iter(app.world())
            .map(|(entity, _, material)| (entity, material.0.clone()))
            .collect();
        assert_eq!(children.len(), 1, "expected exactly one image child");
        children[0].clone()
    }

    /// Every image child as `(element_idx, material handle)`, sorted by index.
    fn image_children_by_element(app: &mut App) -> Vec<(usize, Handle<StandardMaterial>)> {
        let mut state = app
            .world_mut()
            .query::<(&PanelImageChild, &MeshMaterial3d<StandardMaterial>)>();
        let mut children: Vec<_> = state
            .iter(app.world())
            .map(|(cached, material)| (cached.element_idx, material.0.clone()))
            .collect();
        children.sort_by_key(|(element_idx, _)| *element_idx);
        children
    }

    fn image_mesh_handle(app: &App, entity: Entity) -> Handle<Mesh> {
        app.world()
            .get::<Mesh3d>(entity)
            .expect("image child should carry a mesh")
            .0
            .clone()
    }

    fn material_base_color(app: &App, handle: &Handle<StandardMaterial>) -> Color {
        app.world()
            .resource::<Assets<StandardMaterial>>()
            .get(handle)
            .expect("material asset should exist")
            .base_color
    }

    fn material_depth_bias(app: &App, handle: &Handle<StandardMaterial>) -> f32 {
        app.world()
            .resource::<Assets<StandardMaterial>>()
            .get(handle)
            .expect("material asset should exist")
            .depth_bias
    }

    #[test]
    fn tint_only_change_mutates_base_color_without_rebuilding_mesh() {
        let mut app = image_reconcile_app();
        let handle = Handle::<Image>::default();
        let panel = spawn_image_panel(
            &mut app,
            one_image_tree(handle.clone(), Color::WHITE, false),
        );
        app.update();

        let (entity, material_before) = single_image_child(&mut app);
        let mesh_before = image_mesh_handle(&app, entity);
        assert_eq!(material_base_color(&app, &material_before), Color::WHITE);

        // Change only the tint — handle, bounds, and command slot are unchanged.
        let red = Color::srgb(1.0, 0.0, 0.0);
        app.world_mut()
            .commands()
            .set_tree(panel, one_image_tree(handle, red, false));
        app.update();

        let (_, material_after) = single_image_child(&mut app);
        let mesh_after = image_mesh_handle(&app, entity);
        assert_eq!(
            mesh_before, mesh_after,
            "a tint-only change must not rebuild the mesh"
        );
        assert_eq!(
            material_before, material_after,
            "a tint-only change reuses the material asset in place"
        );
        assert_eq!(
            material_base_color(&app, &material_after),
            red,
            "base_color is mutated in place to the new tint"
        );
    }

    #[test]
    fn unchanged_image_material_is_not_re_touched() {
        let mut app = image_reconcile_app();
        let handle = Handle::<Image>::default();
        let panel = spawn_image_panel(
            &mut app,
            two_image_tree(handle.clone(), Color::WHITE, Color::WHITE),
        );
        app.update();

        let children = image_children_by_element(&mut app);
        let unchanged_material = children[0].1.clone();
        let recolored_material = children[1].1.clone();

        // Start recording from a clean slate, then recolor only the second image.
        app.world_mut()
            .resource_mut::<ModifiedMaterials>()
            .0
            .clear();
        let red = Color::srgb(1.0, 0.0, 0.0);
        app.world_mut()
            .commands()
            .set_tree(panel, two_image_tree(handle, Color::WHITE, red));
        for _ in 0..4 {
            app.update();
        }

        let modified = &app.world().resource::<ModifiedMaterials>().0;
        assert!(
            !modified.contains(&unchanged_material.id()),
            "the unchanged image's material must not be re-touched (no get_mut)"
        );
        assert!(
            modified.contains(&recolored_material.id()),
            "the recolored image's material is mutated in place"
        );
    }

    #[test]
    fn command_index_shift_rebuilds_material_so_depth_bias_stays_correct() {
        let mut app = image_reconcile_app();
        let handle = Handle::<Image>::default();
        // No background: the image is the first render command (command_index 0).
        let panel = spawn_image_panel(
            &mut app,
            one_image_tree(handle.clone(), Color::WHITE, false),
        );
        app.update();

        let (_, material_before) = single_image_child(&mut app);
        assert_eq!(
            material_depth_bias(&app, &material_before).to_bits(),
            0.0_f32.to_bits(),
            "the lone image sits at command_index 0"
        );

        // Adding a background to the element prepends a rectangle command,
        // shifting the image to command_index 1 while its element_idx is stable.
        app.world_mut()
            .commands()
            .set_tree(panel, one_image_tree(handle, Color::WHITE, true));
        app.update();

        let (_, material_after) = single_image_child(&mut app);
        assert_ne!(
            material_before, material_after,
            "a command_index shift rebuilds the material (a new asset)"
        );
        assert_eq!(
            material_depth_bias(&app, &material_after).to_bits(),
            LAYER_DEPTH_BIAS.to_bits(),
            "the rebuilt material picks up the shifted command_index's depth bias"
        );
    }
}
