use std::time::Duration;
use std::time::Instant;

use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use bevy_kana::ToF32;

use super::PanelTextLayout;
use super::PreparedPanelText;
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

/// Updates the glyph geometry for each panel-text run whose layout changed, and
/// despawns the mesh of a run whose text emptied.
///
/// A run owns one mesh child and one storage slot keyed by its label entity
/// through [`RunStorageKey`]. When the run already has that mesh child, its
/// geometry is overwritten in place behind stable handles and only the material
/// is refreshed — no asset reallocation, no mesh-entity respawn. The first frame
/// a run appears it has no child, so this allocates its storage and spawns the
/// child. Each despawn frees the run's storage through the
/// `On<Remove, DiegeticTextMesh>` observer. Depth bias still derives from
/// `command_index` (stable across element reorder), so per-run update keeps
/// Geometry-mode layering correct (M2).
pub(super) fn update_panel_text_geometry(
    changed_runs: Query<
        (Entity, &PreparedPanelText, &PanelTextLayout, &ChildOf),
        (With<TextContent>, Changed<PreparedPanelText>),
    >,
    mut emptied_runs: RemovedComponents<PreparedPanelText>,
    run_meshes: Query<(Entity, &ChildOf, &MeshMaterial3d<TextMaterial>), With<DiegeticTextMesh>>,
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
    // Sub-stage accumulators across the run loop, surfaced as the indented
    // `pack` / `upload` / `material` rows under `mesh`.
    let mut pack_time = Duration::ZERO;
    let mut upload_time = Duration::ZERO;
    let mut material_time = Duration::ZERO;

    for (label_entity, panel_run, panel_text_child, child_of) in &changed_runs {
        let Ok((panel, panel_layers)) = panels.get(child_of.parent()) else {
            continue;
        };
        let storage_key = RunStorageKey::from(label_entity);

        // pack: assemble this run's render data from cached glyph outlines.
        let pack_start = Instant::now();
        let render_data = backend.build_run_render_data(&panel_run.prepared, panel_run.clip_rect);
        pack_time += pack_start.elapsed();
        let Ok(render_data) = render_data else {
            // Clipping removed every quad: drop any stale mesh child so its
            // storage frees and nothing renders.
            despawn_label_mesh(label_entity, &run_meshes, &mut commands);
            continue;
        };

        // upload: write the mesh and three GPU buffers in place.
        let upload_start = Instant::now();
        let storage =
            backend.commit_run_storage(storage_key, render_data, &mut meshes, &mut storage_buffers);
        upload_time += upload_start.elapsed();

        // material: build and write this run's material.
        let material_start = Instant::now();
        let resolved_alpha = resolved_alphas
            .get(label_entity)
            .map_or(alpha_default.0.0, |resolved| resolved.0.0);
        let resolved_lighting = resolved_lightings
            .get(label_entity)
            .map_or(lighting_default.0.0, |resolved| resolved.0.0);
        let resolved_sidedness = resolved_sidednesses
            .get(label_entity)
            .map_or(sidedness_default.0.0, |resolved| resolved.0.0);
        let material = panel_run_material(PanelRunMaterial {
            panel,
            panel_run,
            panel_text_child,
            resolved_alpha,
            resolved_lighting,
            resolved_sidedness,
            storage: &storage,
        });

        // The run already has its mesh child: its geometry was overwritten in
        // place above, so refresh only the material behind its handle. The first
        // frame a run appears it has no child, so allocate the material and spawn
        // the mesh.
        if let Some(handle) = existing_run_material(label_entity, &run_meshes) {
            if let Some(mut slot) = materials.get_mut(&handle) {
                *slot = material;
            }
        } else {
            let scene_layer = panel_layers.cloned().unwrap_or(RenderLayers::layer(0));
            let material = materials.add(material);
            spawn_visible_mesh(
                label_entity,
                storage.mesh,
                storage_key,
                material,
                panel_run.shadow_mode,
                &scene_layer,
                &mut commands,
            );
        }
        material_time += material_start.elapsed();
    }

    // R10: when a run's text empties, `shape_panel_text_children` removes its
    // `PreparedPanelText` (not `Changed`, so the loop above misses it). Despawn the
    // now-stale mesh so the observer frees the run storage. Idempotent against a
    // recursively despawned `TextContent` — that label's mesh is already gone, so
    // the lookup finds nothing and no double-despawn occurs.
    for label_entity in emptied_runs.read() {
        despawn_label_mesh(label_entity, &run_meshes, &mut commands);
    }

    perf.panel_text.mesh_build_ms =
        mesh_build_start.elapsed().as_secs_f32() * MILLISECONDS_PER_SECOND;
    perf.panel_text.mesh_pack_ms = pack_time.as_secs_f32() * MILLISECONDS_PER_SECOND;
    perf.panel_text.mesh_upload_ms = upload_time.as_secs_f32() * MILLISECONDS_PER_SECOND;
    perf.panel_text.mesh_material_ms = material_time.as_secs_f32() * MILLISECONDS_PER_SECOND;
    perf.panel_text.total_ms = perf.panel_text.shape_ms + perf.panel_text.mesh_build_ms;
}

/// Applies an alpha-only change to a panel-text run in place: it mutates the run
/// material's `base.alpha_mode` and rebuilds nothing — the glyph mesh and its
/// three GPU buffers are alpha-independent.
///
/// A run that also changed geometry this frame is rebuilt by
/// [`update_panel_text_geometry`] with the resolved alpha already baked in, so
/// this system skips it via `Ref<PreparedPanelText>::is_changed()` (M7). With that skip
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
        (Entity, Ref<PreparedPanelText>, &Resolved<TextAlpha>),
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
    run_meshes: &Query<(Entity, &ChildOf, &MeshMaterial3d<TextMaterial>), With<DiegeticTextMesh>>,
    commands: &mut Commands,
) {
    for (mesh_entity, child_of, _) in run_meshes {
        if child_of.parent() == label {
            commands.entity(mesh_entity).despawn();
        }
    }
}

/// Material handle of the [`DiegeticTextMesh`] child parented under `label`, if
/// it has one.
fn existing_run_material(
    label: Entity,
    run_meshes: &Query<(Entity, &ChildOf, &MeshMaterial3d<TextMaterial>), With<DiegeticTextMesh>>,
) -> Option<Handle<TextMaterial>> {
    run_meshes
        .iter()
        .find(|(_, child_of, _)| child_of.parent() == label)
        .map(|(_, _, material)| material.0.clone())
}

/// Inputs for building one panel-text run's material from its resolved cascade
/// values and storage handles.
struct PanelRunMaterial<'a> {
    panel:              &'a DiegeticPanel,
    panel_run:          &'a PreparedPanelText,
    panel_text_child:   &'a PanelTextLayout,
    resolved_alpha:     AlphaMode,
    resolved_lighting:  GlyphLighting,
    resolved_sidedness: GlyphSidedness,
    storage:            &'a RunStorage,
}

fn panel_run_material(request: PanelRunMaterial<'_>) -> TextMaterial {
    let PanelRunMaterial {
        panel,
        panel_run,
        panel_text_child,
        resolved_alpha,
        resolved_lighting,
        resolved_sidedness,
        storage,
    } = request;
    let base = panel_base_material(panel);
    let command_depth = panel_text_child.command_index.saturating_add(1).to_f32();
    let text_depth_bias = command_depth * constants::LAYER_DEPTH_BIAS;
    panel_material(PanelMaterialInput {
        base: &base,
        depth_bias: text_depth_bias,
        // Keep panel text at its real clip-space depth in OIT. A positive manual
        // offset here can pull glyph fragments in front of opaque occluders,
        // making text show through solid geometry. Normal/non-OIT layering still
        // comes from `depth_bias`; panel-local OIT text ordering should be solved
        // without moving the fragment depth.
        oit_depth_offset: 0.0,
        alpha_mode: resolved_alpha,
        lighting: resolved_lighting,
        sidedness: resolved_sidedness,
        fill_color: panel_run.fill_color,
        render_mode: panel_run.render_mode.into(),
        storage,
    })
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

    fn material_fill_color(app: &App, mesh: Entity) -> Vec4 {
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
            .extension
            .uniforms
            .fill_color
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
    fn recolor_updates_the_changed_run_in_place() {
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

        // Recolor only "Alpha". Color rides in PreparedPanelText.fill_color, so its run
        // is a geometry update; "Beta" is byte-identical and must be left alone.
        app.world_mut()
            .commands()
            .set_tree(panel, two_text_tree(Color::BLACK, Color::WHITE));
        settle(&mut app);

        let recolored_mesh_after =
            mesh_of_label(&mut app, recolored).expect("Alpha mesh should still exist");
        let untouched_mesh_after =
            mesh_of_label(&mut app, untouched).expect("Beta mesh should still exist");
        // Both runs keep their mesh entity: the changed run overwrites its mesh
        // and material in place instead of respawning a new entity.
        assert_eq!(
            recolored_mesh_before, recolored_mesh_after,
            "the recolored run reuses its mesh entity"
        );
        assert_eq!(
            untouched_mesh_before, untouched_mesh_after,
            "the unchanged run keeps its mesh entity"
        );
        // The recolor reached the changed run's material and left the other alone.
        assert_eq!(
            material_fill_color(&app, recolored_mesh_after),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
            "Alpha's fill color is now black"
        );
        assert_eq!(
            material_fill_color(&app, untouched_mesh_after),
            Vec4::new(1.0, 1.0, 1.0, 1.0),
            "Beta's fill color stays white"
        );
        assert_eq!(
            run_storage_len(&app),
            2,
            "recolor reuses both runs' storage with no churn"
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
        // its Resolved<TextAlpha> changes while PreparedPanelText does not — an
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

        // Empty the run's text. `shape_panel_text_children` removes `PreparedPanelText`
        // (not `Changed`), and the geometry system's
        // `RemovedComponents<PreparedPanelText>` reaction despawns the now-stale mesh
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
