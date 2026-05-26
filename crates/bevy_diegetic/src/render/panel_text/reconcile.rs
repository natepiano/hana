use std::collections::HashMap;

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy_kana::ToF32;

use super::PanelTextLayout;
use crate::cascade::Override;
use crate::cascade::TextAlpha;
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
            let style = config.as_standalone();
            // A label's own alpha override (`LayoutTextStyle::with_alpha_mode`)
            // travels through `as_standalone` and becomes an `Override<TextAlpha>`
            // on the label, which the cascade resolves ahead of the panel's
            // inherited alpha. Absent → the label inherits.
            let label_alpha = style.alpha_mode();
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
            } else if let Some(alpha_mode) = label_alpha {
                // `Override<TextAlpha>` rides in the spawn bundle so it is
                // present when `seed_panel_child_alpha` fires on `PanelChild`,
                // seeding the label's own alpha with no settle frame.
                commands.entity(panel_entity).with_child((
                    WorldText::new(text.clone()),
                    style,
                    panel_text_child,
                    PanelChild,
                    Override(TextAlpha(alpha_mode)),
                ));
            } else {
                commands.entity(panel_entity).with_child((
                    WorldText::new(text.clone()),
                    style,
                    panel_text_child,
                    PanelChild,
                ));
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
                child.insert(Override(incoming));
            }
        },
        None => {
            if reusable.alpha.is_some() {
                child.remove::<Override<TextAlpha>>();
            }
        },
    }
}

/// Marker on image child entities spawned by the panel image reconciler.
#[derive(Component, Clone, Debug)]
pub(super) struct PanelImageChild {
    /// Index of the source element in the layout tree.
    pub element_idx: usize,
}

/// Reconciles image children for each changed [`ComputedDiegeticPanel`].
pub(super) fn reconcile_panel_image_children(
    changed_panels: Query<
        (Entity, &DiegeticPanel, &ComputedDiegeticPanel),
        Changed<ComputedDiegeticPanel>,
    >,
    existing_children: Query<(Entity, &PanelImageChild, &ChildOf)>,
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
                    Some((
                        cmd_index,
                        cmd.element_idx,
                        handle.clone(),
                        *tint,
                        cmd.bounds,
                    ))
                },
                _ => None,
            })
            .collect();

        let mut existing_by_idx: HashMap<usize, Entity> = HashMap::new();
        for (entity, panel_image_child, child_of) in &existing_children {
            if child_of.parent() == panel_entity {
                existing_by_idx.insert(panel_image_child.element_idx, entity);
            }
        }

        let mut visited_indices: Vec<usize> = Vec::new();
        for (cmd_index, element_idx, handle, tint, bounds) in &image_commands {
            visited_indices.push(*element_idx);

            let world_width = bounds.width * points_to_world;
            let world_height = bounds.height * points_to_world;
            let world_x = bounds.x.mul_add(points_to_world, world_width * 0.5) - anchor_x;
            let world_y = -(bounds.y.mul_add(points_to_world, world_height * 0.5) - anchor_y);

            let mesh_handle = meshes.add(Rectangle::new(world_width, world_height));
            let material_handle = materials.add(StandardMaterial {
                base_color: *tint,
                base_color_texture: Some(handle.clone()),
                unlit: true,
                double_sided: true,
                cull_mode: None,
                alpha_mode: AlphaMode::Blend,
                depth_bias: cmd_index.to_f32() * constants::LAYER_DEPTH_BIAS,
                ..default()
            });

            let transform = Transform::from_xyz(world_x, world_y, TEXT_Z_OFFSET);
            let panel_image_child = PanelImageChild {
                element_idx: *element_idx,
            };

            if let Some(&child_entity) = existing_by_idx.get(element_idx) {
                commands.entity(child_entity).insert((
                    panel_image_child,
                    Mesh3d(mesh_handle),
                    MeshMaterial3d(material_handle),
                    transform,
                    layer.clone(),
                ));
            } else {
                commands.entity(panel_entity).with_child((
                    panel_image_child,
                    Mesh3d(mesh_handle),
                    MeshMaterial3d(material_handle),
                    transform,
                    layer.clone(),
                ));
            }
        }

        for (entity, panel_image_child, child_of) in &existing_children {
            if child_of.parent() == panel_entity
                && !visited_indices.contains(&panel_image_child.element_idx)
            {
                commands.entity(entity).despawn();
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
    use std::sync::Arc;

    use bevy::prelude::*;
    use bevy_kana::ToF32;

    use super::reconcile_panel_text_children;
    use crate::Mm;
    use crate::cascade::Override;
    use crate::cascade::TextAlpha;
    use crate::constants::MONOSPACE_WIDTH_RATIO;
    use crate::layout::BoundingBox;
    use crate::layout::LayoutBuilder;
    use crate::layout::LayoutTextStyle;
    use crate::layout::LayoutTree;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;
    use crate::layout::WorldTextStyle;
    use crate::panel::DiegeticPanel;
    use crate::panel::DiegeticPanelCommands;
    use crate::panel::HeadlessLayoutPlugin;
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
}
