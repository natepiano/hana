use std::collections::HashMap;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy_kana::ToF32;

use super::PanelTextLayout;
use crate::cascade;
use crate::cascade::Override;
use crate::cascade::TextAlpha;
use crate::layout::BoundingBox;
use crate::layout::RenderCommandKind;
use crate::layout::WorldTextStyle;
use crate::panel::ComputedDiegeticPanel;
use crate::panel::DiegeticPanel;
use crate::render::clip;
use crate::render::constants;
use crate::render::constants::TEXT_Z_OFFSET;
use crate::render::world_text::PanelChild;
use crate::render::world_text::WorldText;

/// A reused panel-text child plus the components reconcile compares incoming
/// values against before deciding whether to write. The references borrow the
/// `existing_children` query for one reconcile pass.
#[derive(Clone, Copy)]
struct ReusableChild<'a> {
    entity: Entity,
    text:   &'a WorldText,
    style:  &'a WorldTextStyle,
    layout: &'a PanelTextLayout,
    alpha:  Option<&'a Override<TextAlpha>>,
}

/// Reconciles [`WorldText`] children for each changed [`ComputedDiegeticPanel`].
pub(super) fn reconcile_panel_text_children(
    changed_panels: Query<
        (Entity, &DiegeticPanel, &ComputedDiegeticPanel),
        Changed<ComputedDiegeticPanel>,
    >,
    existing_children: Query<(
        Entity,
        &WorldText,
        &WorldTextStyle,
        &PanelTextLayout,
        Option<&Override<TextAlpha>>,
        &ChildOf,
    )>,
    mut commands: Commands,
) {
    for (panel_entity, panel, computed) in &changed_panels {
        let Some(result) = computed.result() else {
            continue;
        };

        let points_to_world = panel.points_to_world();
        let scale_x = points_to_world;
        let scale_y = points_to_world;
        let (anchor_x, anchor_y) = panel.anchor_offsets();

        let clip_rects = clip::compute_clip_rects(&result.commands);
        let viewport = clip::panel_viewport(panel);
        let text_commands: Vec<_> = result
            .commands
            .iter()
            .enumerate()
            .filter_map(|(cmd_index, cmd)| match &cmd.kind {
                RenderCommandKind::Text { text, config } => {
                    let active_clip =
                        clip::effective_clip(cmd.bounds, clip_rects[cmd_index], viewport)
                            .unwrap_or_else(clip::empty_clip);
                    Some((
                        cmd.element_idx,
                        cmd_index,
                        text.clone(),
                        config.clone(),
                        cmd.bounds,
                        active_clip,
                    ))
                },
                _ => None,
            })
            .collect();

        // Children are reused by `(element_idx, command_index)`. That key is
        // stable for a static layout but not content-stable: a row reorder
        // changes the command index, so an unchanged run keyed to a moved slot
        // respawns rather than reuses. Reuse holds across regenerate-only and
        // re-measure passes that keep the command order, not across reorders
        // (R7).
        let mut existing_by_key: HashMap<(usize, usize), ReusableChild> = HashMap::new();
        for (entity, text, style, layout, alpha, child_of) in &existing_children {
            if child_of.parent() == panel_entity {
                existing_by_key.insert(
                    (layout.element_idx, layout.command_index),
                    ReusableChild {
                        entity,
                        text,
                        style,
                        layout,
                        alpha,
                    },
                );
            }
        }

        let mut visited_keys: Vec<(usize, usize)> = Vec::new();
        for (element_idx, cmd_index, text, config, bounds, clip) in &text_commands {
            // A label's own alpha override (`LayoutTextStyle::with_alpha_mode`)
            // is captured before `as_standalone()`, then inserted as
            // `Override<TextAlpha>` on the label. Absent means the label
            // inherits the panel alpha.
            let label_alpha = config.alpha_mode();
            let style = config.as_standalone();
            let panel_text_child = PanelTextLayout {
                element_idx: *element_idx,
                command_index: *cmd_index,
                bounds: *bounds,
                scale_x,
                scale_y,
                anchor_x,
                anchor_y,
                clip_rect: Some(*clip),
            };

            let key = (*element_idx, *cmd_index);
            visited_keys.push(key);

            if let Some(&reusable) = existing_by_key.get(&key) {
                update_reused_panel_text_child(
                    &mut commands,
                    reusable,
                    text,
                    style,
                    panel_text_child,
                    label_alpha,
                );
            } else {
                commands.entity(panel_entity).with_children(|children| {
                    let mut child = children.spawn((
                        WorldText::new(text.clone()),
                        style,
                        panel_text_child,
                        PanelChild,
                    ));
                    if let Some(alpha_mode) = label_alpha {
                        cascade::apply_cascade_override(&mut child, TextAlpha(alpha_mode));
                    }
                });
            }
        }

        for (entity, _, _, layout, _, child_of) in &existing_children {
            if child_of.parent() == panel_entity
                && !visited_keys.contains(&(layout.element_idx, layout.command_index))
            {
                commands.entity(entity).despawn();
            }
        }
    }
}

/// Writes each gated component of a reused panel-text child only when it
/// differs, so an unchanged run stays un-`Changed` and
/// `shape_panel_text_children` plus the mesh rebuild skip it.
///
/// `gating_eq` excludes render-context fields and compares floats by bits;
/// alpha is gated on its own because `gating_eq` ignores `alpha_mode`. Writing
/// it unconditionally would re-fire `Changed<Resolved<TextAlpha>>` on every run
/// and defeat the per-run alpha short-circuit downstream.
fn update_reused_panel_text_child(
    commands: &mut Commands,
    reusable: ReusableChild,
    text: &str,
    style: WorldTextStyle,
    layout: PanelTextLayout,
    label_alpha: Option<AlphaMode>,
) {
    let mut child = commands.entity(reusable.entity);
    if reusable.text.text() != text {
        child.insert(WorldText::new(text.to_owned()));
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

    use bevy::asset::AssetPlugin;
    use bevy::prelude::*;
    use bevy_kana::ToF32;

    use super::PanelImageChild;
    use super::reconcile_panel_image_children;
    use super::reconcile_panel_text_children;
    use crate::Mm;
    use crate::cascade::Override;
    use crate::cascade::TextAlpha;
    use crate::constants::MONOSPACE_WIDTH_RATIO;
    use crate::layout::BoundingBox;
    use crate::layout::El;
    use crate::layout::LayoutBuilder;
    use crate::layout::LayoutTextStyle;
    use crate::layout::LayoutTree;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;
    use crate::layout::WorldTextStyle;
    use crate::panel::DiegeticPanel;
    use crate::panel::DiegeticPanelCommands;
    use crate::panel::HeadlessLayoutPlugin;
    use crate::render::constants::LAYER_DEPTH_BIAS;
    use crate::render::panel_text::PanelTextLayout;
    use crate::render::world_text::PanelChild;
    use crate::render::world_text::WorldText;
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
            Ref<WorldText>,
            Ref<WorldTextStyle>,
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
        builder.text("Alpha", LayoutTextStyle::new(10.0).with_color(first));
        builder.text("Beta", LayoutTextStyle::new(10.0).with_color(second));
        builder.build()
    }

    fn single_alpha_text_tree(color: Color, alpha: AlphaMode) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text(
            "Glow",
            LayoutTextStyle::new(10.0)
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
            .query_filtered::<(Entity, &WorldText), With<PanelChild>>();
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
    fn reconcile_keys_by_element_and_command_index() {
        let existing: Vec<(Entity, PanelTextLayout)> = (0..3)
            .map(|cmd| {
                let panel_text_child = PanelTextLayout {
                    element_idx:   7,
                    command_index: cmd,
                    bounds:        BoundingBox {
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
                    panel_text_child,
                )
            })
            .collect();

        let mut by_key: HashMap<(usize, usize), Entity> = HashMap::new();
        for (entity, panel_text_child) in &existing {
            by_key.insert(
                (panel_text_child.element_idx, panel_text_child.command_index),
                *entity,
            );
        }
        assert_eq!(by_key.len(), 3);

        let mut by_element_only: HashMap<usize, Entity> = HashMap::new();
        for (entity, panel_text_child) in &existing {
            by_element_only.insert(panel_text_child.element_idx, *entity);
        }
        assert_eq!(by_element_only.len(), 1);
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
