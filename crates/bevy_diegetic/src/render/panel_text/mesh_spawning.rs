use std::time::Instant;

use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use bevy_kana::ToF32;

use super::PanelText;
use super::PanelTextLayout;
use crate::cascade::CascadeDefault;
use crate::cascade::Resolved;
use crate::cascade::TextAlpha;
use crate::cascade::TextLighting;
use crate::cascade::TextSidedness;
use crate::constants::MILLISECONDS_PER_SECOND;
use crate::layout::GlyphLighting;
use crate::layout::GlyphShadowMode;
use crate::layout::GlyphSidedness;
use crate::panel::DiegeticPanel;
use crate::panel::DiegeticPerfStats;
use crate::render::constants;
use crate::render::world_text::TextContent;
use crate::text;
use crate::text::GlyphCache;
use crate::text::RenderMode;
use crate::text::RunStorage;
use crate::text::RunStorageKey;
use crate::text::TextMaterial;
use crate::text::TextMaterialInput;

/// Marker component for text mesh entities spawned by the renderer.
#[derive(Component)]
pub(super) struct DiegeticTextMesh;

/// Frees a prepared run's GPU storage when its glyph mesh is despawned.
///
/// `On<Remove>` fires before the component is dropped, so the mesh's
/// [`RunStorageKey`] is still readable. This observer is the sole run-storage
/// freer: every mesh despawn — a per-run rebuild, an emptied run, or a
/// recursively despawned `TextContent` label — routes its cleanup through here,
/// so no inline `remove_run_storage` survives in the build systems.
pub(super) fn free_run_storage_on_mesh_removal(
    trigger: On<Remove, DiegeticTextMesh>,
    storage_keys: Query<&RunStorageKey>,
    mut backend: ResMut<GlyphCache>,
) {
    if let Ok(storage_key) = storage_keys.get(trigger.event_target()) {
        backend.remove_run_storage(*storage_key);
    }
}

/// Rebuilds the glyph mesh for each panel-text run whose geometry changed, and
/// despawns the mesh of a run whose text emptied.
///
/// One run, one mesh: a rebuild touches only the changed run's mesh and GPU
/// buffers, leaving every sibling run on the panel untouched. Each despawn
/// frees its run storage through the `On<Remove, DiegeticTextMesh>` observer —
/// the geometry system never frees storage inline. Depth bias still derives
/// from `command_index` (stable across element reorder), so per-run rebuild
/// keeps Geometry-mode layering correct (M2).
pub(super) fn update_panel_text_geometry(
    changed_runs: Query<
        (Entity, &PanelText, &PanelTextLayout, &ChildOf),
        (With<TextContent>, Changed<PanelText>),
    >,
    mut emptied_runs: RemovedComponents<PanelText>,
    old_meshes: Query<(Entity, &ChildOf), With<DiegeticTextMesh>>,
    panels: Query<(&DiegeticPanel, Option<&RenderLayers>)>,
    resolved_alphas: Query<&Resolved<TextAlpha>, With<TextContent>>,
    resolved_lightings: Query<&Resolved<TextLighting>, With<TextContent>>,
    resolved_sidednesses: Query<&Resolved<TextSidedness>, With<TextContent>>,
    alpha_default: Res<CascadeDefault<TextAlpha>>,
    lighting_default: Res<CascadeDefault<TextLighting>>,
    sidedness_default: Res<CascadeDefault<TextSidedness>>,
    mut backend: ResMut<GlyphCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TextMaterial>>,
    mut storage_buffers: ResMut<Assets<ShaderBuffer>>,
    mut perf: ResMut<DiegeticPerfStats>,
    mut commands: Commands,
) {
    let mesh_build_start = Instant::now();

    for (label_entity, panel_run, panel_text_child, child_of) in &changed_runs {
        let Ok((panel, panel_layers)) = panels.get(child_of.parent()) else {
            continue;
        };
        despawn_label_mesh(label_entity, &old_meshes, &mut commands);
        let scene_layer = panel_layers.cloned().unwrap_or(RenderLayers::layer(0));
        let text_base = panel_base_material(panel);
        let resolved_alpha = resolved_alphas
            .get(label_entity)
            .map_or(alpha_default.0.0, |resolved| resolved.0.0);
        let resolved_lighting = resolved_lightings
            .get(label_entity)
            .map_or(lighting_default.0.0, |resolved| resolved.0.0);
        let resolved_sidedness = resolved_sidednesses
            .get(label_entity)
            .map_or(sidedness_default.0.0, |resolved| resolved.0.0);
        spawn_panel_text_run(PanelTextSpawnRequest {
            mesh_parent: label_entity,
            panel_run,
            panel_text_child,
            text_base: &text_base,
            resolved_alpha,
            resolved_lighting,
            resolved_sidedness,
            content_layer: &scene_layer,
            backend: &mut backend,
            meshes: &mut meshes,
            materials: &mut materials,
            storage_buffers: &mut storage_buffers,
            commands: &mut commands,
        });
    }

    // R10: when a run's text empties, `shape_panel_text_children` removes its
    // `PanelText` (not `Changed`, so the loop above misses it). Despawn the
    // now-stale mesh so the observer frees the run storage. Idempotent against a
    // recursively despawned `TextContent` — that label's mesh is already gone, so
    // the lookup finds nothing and no double-despawn occurs.
    for label_entity in emptied_runs.read() {
        despawn_label_mesh(label_entity, &old_meshes, &mut commands);
    }

    perf.panel_text.mesh_build_ms =
        mesh_build_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    perf.panel_text.total_ms = perf.panel_text.shape_ms + perf.panel_text.mesh_build_ms;
}

/// Applies an alpha-only change to a panel-text run in place: it mutates the run
/// material's `base.alpha_mode` and rebuilds nothing — the glyph mesh and its
/// three GPU buffers are alpha-independent.
///
/// A run that also changed geometry this frame is rebuilt by
/// [`update_panel_text_geometry`] with the resolved alpha already baked in, so
/// this system skips it via `Ref<PanelText>::is_changed()` (M7). With that skip
/// the two systems need no ordering edge: in any run order the geometry rebuild
/// owns a both-changed run, and the skip keeps this system off the material
/// handle the geometry system may have just despawned. The write is
/// value-guarded (R5) so a no-op resolution does not trip `Changed<TextMaterial>`.
///
/// After Phase 4's reparent the material lives two hops down — on the
/// `DiegeticTextMesh` child of the `TextContent` run — so the run's mesh is reached
/// through `ChildOf`, not on the label itself.
pub(super) fn update_panel_text_alpha(
    changed_alphas: Query<
        (Entity, Ref<PanelText>, &Resolved<TextAlpha>),
        (With<TextContent>, Changed<Resolved<TextAlpha>>),
    >,
    run_meshes: Query<(&ChildOf, &MeshMaterial3d<TextMaterial>), With<DiegeticTextMesh>>,
    mut materials: ResMut<Assets<TextMaterial>>,
) {
    for (label_entity, panel_run, resolved) in &changed_alphas {
        if panel_run.is_changed() {
            continue;
        }
        let resolved_alpha = resolved.0.0;
        for (child_of, material_handle) in &run_meshes {
            if child_of.parent() != label_entity {
                continue;
            }
            let Some(mut material) = materials.get_mut(&material_handle.0) else {
                continue;
            };
            if material.base.alpha_mode != resolved_alpha {
                material.base.alpha_mode = resolved_alpha;
            }
        }
    }
}

/// Despawns the [`DiegeticTextMesh`] child of `label`, if it has one. Storage
/// cleanup runs through the `On<Remove, DiegeticTextMesh>` observer.
fn despawn_label_mesh(
    label: Entity,
    old_meshes: &Query<(Entity, &ChildOf), With<DiegeticTextMesh>>,
    commands: &mut Commands,
) {
    for (mesh_entity, child_of) in old_meshes {
        if child_of.parent() == label {
            commands.entity(mesh_entity).despawn();
        }
    }
}

struct PanelTextSpawnRequest<'a, 'w, 's> {
    mesh_parent:        Entity,
    panel_run:          &'a PanelText,
    panel_text_child:   &'a PanelTextLayout,
    text_base:          &'a StandardMaterial,
    resolved_alpha:     AlphaMode,
    resolved_lighting:  GlyphLighting,
    resolved_sidedness: GlyphSidedness,
    content_layer:      &'a RenderLayers,
    backend:            &'a mut GlyphCache,
    meshes:             &'a mut Assets<Mesh>,
    materials:          &'a mut Assets<TextMaterial>,
    storage_buffers:    &'a mut Assets<ShaderBuffer>,
    commands:           &'a mut Commands<'w, 's>,
}

fn spawn_panel_text_run(request: PanelTextSpawnRequest<'_, '_, '_>) {
    let PanelTextSpawnRequest {
        mesh_parent,
        panel_run,
        panel_text_child,
        text_base,
        resolved_alpha,
        resolved_lighting,
        resolved_sidedness,
        content_layer,
        backend,
        meshes,
        materials,
        storage_buffers,
        commands,
    } = request;
    let Ok(storage) = backend.ensure_run_storage(
        &panel_run.prepared,
        panel_run.clip_rect,
        meshes,
        storage_buffers,
    ) else {
        return;
    };

    let command_depth = panel_text_child.command_index.saturating_add(1).to_f32();
    let text_depth_bias = command_depth * constants::LAYER_DEPTH_BIAS;
    // Keep panel text at its real clip-space depth in OIT. A positive manual
    // offset here can pull glyph fragments in front of opaque occluders, making
    // text show through solid geometry. Normal/non-OIT layering still comes
    // from `depth_bias`; panel-local OIT text ordering should be solved without
    // moving the fragment depth.
    let text_oit_depth_offset = 0.0;

    let material = materials.add(panel_material(PanelMaterialInput {
        base:             text_base,
        depth_bias:       text_depth_bias,
        oit_depth_offset: text_oit_depth_offset,
        alpha_mode:       resolved_alpha,
        lighting:         resolved_lighting,
        sidedness:        resolved_sidedness,
        fill_color:       panel_run.fill_color,
        render_mode:      panel_run.render_mode.into(),
        storage:          &storage,
    }));
    spawn_visible_mesh(
        mesh_parent,
        storage.mesh,
        panel_run.prepared.storage_key(),
        material,
        panel_run.shadow_mode,
        content_layer,
        commands,
    );
}

fn panel_base_material(panel: &DiegeticPanel) -> StandardMaterial {
    let mut base = panel
        .text_material()
        .cloned()
        .unwrap_or_else(constants::default_panel_material);
    base.alpha_mode = AlphaMode::Blend;
    base
}

/// Inputs to [`panel_material`]. The lighting and sidedness fields carry the
/// label's resolved [`TextLighting`] / [`TextSidedness`] cascade values.
struct PanelMaterialInput<'a> {
    base:             &'a StandardMaterial,
    depth_bias:       f32,
    oit_depth_offset: f32,
    alpha_mode:       AlphaMode,
    lighting:         GlyphLighting,
    sidedness:        GlyphSidedness,
    fill_color:       Color,
    render_mode:      RenderMode,
    storage:          &'a RunStorage,
}

fn panel_material(input: PanelMaterialInput<'_>) -> TextMaterial {
    let PanelMaterialInput {
        base,
        depth_bias,
        oit_depth_offset,
        alpha_mode,
        lighting,
        sidedness,
        fill_color,
        render_mode,
        storage,
    } = input;
    let mut base = base.clone();
    base.depth_bias = depth_bias;
    base.alpha_mode = alpha_mode;
    base.unlit = matches!(lighting, GlyphLighting::Unlit);
    constants::apply_glyph_sidedness(&mut base, sidedness);
    text::text_material(TextMaterialInput {
        base,
        fill_color,
        render_mode,
        oit_depth_offset,
        curves: storage.curves.clone(),
        bands: storage.bands.clone(),
        glyphs: storage.glyphs.clone(),
    })
}

fn spawn_visible_mesh(
    mesh_parent: Entity,
    mesh_handle: Handle<Mesh>,
    storage_key: RunStorageKey,
    material_handle: Handle<TextMaterial>,
    shadow_mode: GlyphShadowMode,
    content_layer: &RenderLayers,
    commands: &mut Commands,
) {
    match shadow_mode {
        GlyphShadowMode::None => {
            commands.entity(mesh_parent).with_child((
                DiegeticTextMesh,
                storage_key,
                NotShadowCaster,
                Mesh3d(mesh_handle),
                MeshMaterial3d(material_handle),
                Transform::IDENTITY,
                content_layer.clone(),
            ));
        },
        GlyphShadowMode::Cast => {
            commands.entity(mesh_parent).with_child((
                DiegeticTextMesh,
                storage_key,
                Mesh3d(mesh_handle),
                MeshMaterial3d(material_handle),
                Transform::IDENTITY,
                content_layer.clone(),
            ));
        },
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

    use bevy::asset::AssetPlugin;
    use bevy::prelude::*;
    use bevy_kana::ToF32;

    use super::*;
    use crate::Mm;
    use crate::cascade::CascadeDefault;
    use crate::cascade::CascadePlugin;
    use crate::constants::MONOSPACE_WIDTH_RATIO;
    use crate::layout::LayoutBuilder;
    use crate::layout::LayoutTree;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;
    use crate::layout::TextStyle;
    use crate::panel::DiegeticPanel;
    use crate::panel::DiegeticPanelCommands;
    use crate::panel::HeadlessLayoutPlugin;
    use crate::render::panel_text::alpha;
    use crate::render::panel_text::reconcile;
    use crate::render::panel_text::shaping;
    use crate::render::text_shaping::TextShapingContext;
    use crate::render::world_text::TextContent;
    use crate::text::DiegeticTextMeasurer;
    use crate::text::FontRegistry;

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

    /// App with the full panel-text render pipeline — layout, reconcile, text
    /// shaping, the geometry/alpha split, and the storage-cleanup observer —
    /// over real fonts and asset storage, so a spawned run produces a mesh.
    fn pipeline_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(AssetPlugin::default())
            .add_plugins(TransformPlugin)
            .insert_resource(monospace_measurer())
            .add_plugins(HeadlessLayoutPlugin)
            .add_plugins(CascadePlugin::<TextAlpha>::default())
            .add_plugins(CascadePlugin::<TextLighting>::default())
            .add_plugins(CascadePlugin::<TextSidedness>::default())
            .insert_resource(FontRegistry::new().expect("embedded font should parse"))
            .init_resource::<TextShapingContext>()
            .init_resource::<GlyphCache>()
            .init_asset::<Mesh>()
            .init_asset::<ShaderBuffer>()
            .init_asset::<TextMaterial>()
            .add_observer(alpha::seed_panel_text_child_alpha)
            .add_observer(super::super::glyph_cascade::seed_panel_text_child_glyph)
            .add_observer(free_run_storage_on_mesh_removal)
            .add_systems(
                PostUpdate,
                (
                    reconcile::reconcile_panel_text_children,
                    shaping::shape_panel_text_children
                        .after(reconcile::reconcile_panel_text_children),
                    update_panel_text_geometry
                        .after(shaping::shape_panel_text_children)
                        .before(TransformSystems::Propagate),
                    update_panel_text_alpha
                        .after(shaping::shape_panel_text_children)
                        .before(TransformSystems::Propagate),
                ),
            );
        app
    }

    fn two_text_tree(first: Color, second: Color) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text("Alpha", TextStyle::new(10.0).with_color(first));
        builder.text("Beta", TextStyle::new(10.0).with_color(second));
        builder.build()
    }

    fn single_text_tree() -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text("Hi", TextStyle::new(10.0));
        builder.build()
    }

    fn single_alpha_text_tree(alpha: AlphaMode) -> LayoutTree {
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text("Hi", TextStyle::new(10.0).with_alpha_mode(alpha));
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

    /// Pumps frames until the pipeline settles so every run that should exist
    /// has its mesh.
    fn settle(app: &mut App) {
        for _ in 0..4 {
            app.update();
        }
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

    /// The `DiegeticTextMesh` entity parented under `label`, if it has one.
    fn mesh_of_label(app: &mut App, label: Entity) -> Option<Entity> {
        let mut state = app
            .world_mut()
            .query_filtered::<(Entity, &ChildOf), With<DiegeticTextMesh>>();
        state
            .iter(app.world())
            .find(|(_, child_of)| child_of.parent() == label)
            .map(|(entity, _)| entity)
    }

    fn run_storage_len(app: &App) -> usize {
        app.world().resource::<GlyphCache>().run_storage_len()
    }

    fn material_alpha(app: &App, mesh: Entity) -> AlphaMode {
        let handle = app
            .world()
            .get::<MeshMaterial3d<TextMaterial>>(mesh)
            .expect("text mesh should carry its material")
            .0
            .clone();
        app.world()
            .resource::<Assets<TextMaterial>>()
            .get(&handle)
            .expect("material asset should exist")
            .base
            .alpha_mode
    }

    #[test]
    fn each_label_gets_a_mesh_child_not_the_panel() {
        let mut app = pipeline_app();
        let panel = spawn_panel(&mut app, two_text_tree(Color::WHITE, Color::WHITE));
        settle(&mut app);

        let labels = labels_by_text(&mut app);
        let alpha = labels["Alpha"];
        let beta = labels["Beta"];
        // The mesh is a child of its label (Phase 4 reparent), not the panel.
        assert!(mesh_of_label(&mut app, alpha).is_some());
        assert!(mesh_of_label(&mut app, beta).is_some());
        assert!(
            mesh_of_label(&mut app, panel).is_none(),
            "no mesh should hang directly off the panel after the reparent"
        );
    }

    #[test]
    fn recolor_rebuilds_only_the_changed_runs_mesh() {
        let mut app = pipeline_app();
        let panel = spawn_panel(&mut app, two_text_tree(Color::WHITE, Color::WHITE));
        settle(&mut app);

        let labels = labels_by_text(&mut app);
        let recolored = labels["Alpha"];
        let untouched = labels["Beta"];
        let recolored_mesh_before =
            mesh_of_label(&mut app, recolored).expect("Alpha mesh should exist");
        let untouched_mesh_before =
            mesh_of_label(&mut app, untouched).expect("Beta mesh should exist");

        // Recolor only "Alpha". Color rides in PanelText.fill_color, so its run
        // is a geometry rebuild; "Beta" is byte-identical and must be left alone.
        app.world_mut()
            .commands()
            .set_tree(panel, two_text_tree(Color::BLACK, Color::WHITE));
        settle(&mut app);

        let recolored_mesh_after =
            mesh_of_label(&mut app, recolored).expect("Alpha mesh should still exist");
        let untouched_mesh_after =
            mesh_of_label(&mut app, untouched).expect("Beta mesh should still exist");
        assert_ne!(
            recolored_mesh_before, recolored_mesh_after,
            "the recolored run's mesh should be rebuilt (a new entity)"
        );
        assert_eq!(
            untouched_mesh_before, untouched_mesh_after,
            "the unchanged run's mesh should be preserved"
        );
    }

    #[test]
    fn despawning_a_label_frees_its_run_storage() {
        let mut app = pipeline_app();
        spawn_panel(&mut app, single_text_tree());
        settle(&mut app);
        assert_eq!(run_storage_len(&app), 1, "the one run should hold storage");

        let label = labels_by_text(&mut app)["Hi"];
        app.world_mut().entity_mut(label).despawn();
        settle(&mut app);

        assert_eq!(
            run_storage_len(&app),
            0,
            "the On<Remove, DiegeticTextMesh> observer should free the run storage"
        );
    }

    #[test]
    fn whole_panel_despawn_frees_all_run_storage() {
        let mut app = pipeline_app();
        let panel = spawn_panel(&mut app, two_text_tree(Color::WHITE, Color::WHITE));
        settle(&mut app);
        assert_eq!(run_storage_len(&app), 2, "two runs should hold storage");

        app.world_mut().entity_mut(panel).despawn();
        settle(&mut app);

        assert_eq!(
            run_storage_len(&app),
            0,
            "despawning the panel frees every run's storage"
        );
    }

    #[test]
    fn alpha_only_change_updates_material_without_respawning_mesh() {
        let mut app = pipeline_app();
        app.world_mut()
            .resource_mut::<CascadeDefault<TextAlpha>>()
            .0 = TextAlpha(AlphaMode::Blend);
        spawn_panel(&mut app, single_text_tree());
        settle(&mut app);

        let label = labels_by_text(&mut app)["Hi"];
        let mesh_before = mesh_of_label(&mut app, label).expect("Hi mesh should exist");
        assert_eq!(material_alpha(&app, mesh_before), AlphaMode::Blend);

        // Change only the global default alpha: the label has no override, so
        // its Resolved<TextAlpha> changes while PanelText does not — an
        // alpha-only update.
        app.world_mut()
            .resource_mut::<CascadeDefault<TextAlpha>>()
            .0 = TextAlpha(AlphaMode::Add);
        settle(&mut app);

        let mesh_after = mesh_of_label(&mut app, label).expect("Hi mesh should still exist");
        assert_eq!(
            mesh_before, mesh_after,
            "an alpha-only change must not respawn the mesh"
        );
        assert_eq!(
            material_alpha(&app, mesh_after),
            AlphaMode::Add,
            "the alpha system should mutate base.alpha_mode in place"
        );
        assert_eq!(
            run_storage_len(&app),
            1,
            "no run storage churn on an alpha-only change"
        );
    }

    #[test]
    fn label_authored_alpha_seeds_resolved_and_first_material() {
        let mut app = pipeline_app();
        spawn_panel(&mut app, single_alpha_text_tree(AlphaMode::Add));
        settle(&mut app);

        let label = labels_by_text(&mut app)["Hi"];
        assert_eq!(
            app.world()
                .get::<Resolved<TextAlpha>>(label)
                .expect("label should carry resolved text alpha")
                .0
                .0,
            AlphaMode::Add
        );
        let mesh = mesh_of_label(&mut app, label).expect("Hi mesh should exist");
        assert_eq!(
            material_alpha(&app, mesh),
            AlphaMode::Add,
            "the first material should use the label-authored alpha"
        );
    }

    #[test]
    fn emptying_a_run_removes_its_mesh_and_frees_storage() {
        let mut app = pipeline_app();
        let panel = spawn_panel(&mut app, single_text_tree());
        settle(&mut app);
        let label = labels_by_text(&mut app)["Hi"];
        assert!(mesh_of_label(&mut app, label).is_some());
        assert_eq!(run_storage_len(&app), 1);

        // Empty the run's text. `shape_panel_text_children` removes `PanelText`
        // (not `Changed`), and the geometry system's
        // `RemovedComponents<PanelText>` reaction despawns the now-stale mesh
        // (R10); the observer frees its run storage. No stale glyph remains.
        let mut builder = LayoutBuilder::new(100.0, 50.0);
        builder.text("", TextStyle::new(10.0));
        app.world_mut().commands().set_tree(panel, builder.build());
        settle(&mut app);

        let remaining_meshes = {
            let mut state = app
                .world_mut()
                .query_filtered::<Entity, With<DiegeticTextMesh>>();
            state.iter(app.world()).count()
        };
        assert_eq!(remaining_meshes, 0, "the emptied run leaves no mesh behind");
        assert_eq!(
            run_storage_len(&app),
            0,
            "the emptied run's storage is freed"
        );
    }

    #[test]
    fn new_run_mesh_has_a_propagated_global_transform_by_the_second_frame() {
        let mut app = pipeline_app();
        // Place the panel off the origin so a propagated transform is
        // distinguishable from the identity default (R6).
        let panel = app
            .world_mut()
            .spawn((
                DiegeticPanel::world()
                    .size(Mm(100.0), Mm(50.0))
                    .with_tree(single_text_tree())
                    .build()
                    .expect("panel should build"),
                Transform::from_xyz(3.0, -2.0, 1.0),
            ))
            .id();

        // The mesh spawns `.before(TransformSystems::Propagate)`, so it acquires
        // a propagated `GlobalTransform` the frame it appears rather than staying
        // untransformed for a frame.
        app.update();
        app.update();

        let label = labels_by_text(&mut app)["Hi"];
        let mesh = mesh_of_label(&mut app, label).expect("Hi mesh should exist by frame 2");
        let panel_world = *app
            .world()
            .get::<GlobalTransform>(panel)
            .expect("panel should have a GlobalTransform");
        let mesh_world = *app
            .world()
            .get::<GlobalTransform>(mesh)
            .expect("mesh should have a propagated GlobalTransform");
        assert_eq!(
            panel_world.translation(),
            Vec3::new(3.0, -2.0, 1.0),
            "panel sits off the origin"
        );
        assert_eq!(
            mesh_world.translation(),
            panel_world.translation(),
            "the mesh inherits the panel's world position — propagation reached the grandchild"
        );
    }

    #[test]
    fn idle_frame_does_not_rebuild_or_retouch_a_run() {
        let mut app = pipeline_app();
        spawn_panel(&mut app, single_text_tree());
        settle(&mut app);

        let label = labels_by_text(&mut app)["Hi"];
        let mesh_before = mesh_of_label(&mut app, label).expect("Hi mesh should exist");

        // Nothing changes this frame: no geometry rebuild, no alpha write.
        app.update();

        let mesh_after = mesh_of_label(&mut app, label).expect("Hi mesh should still exist");
        assert_eq!(
            mesh_before, mesh_after,
            "an idle frame must not respawn the mesh"
        );
        assert_eq!(run_storage_len(&app), 1);
    }
}
