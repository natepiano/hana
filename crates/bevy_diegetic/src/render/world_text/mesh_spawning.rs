use std::time::Instant;

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::Face;
use bevy::render::storage::ShaderBuffer;

use super::PanelChild;
use super::WorldText;
use crate::cascade::FontUnit;
use crate::cascade::Resolved;
use crate::cascade::TextAlpha;
use crate::constants::MILLISECONDS_PER_SECOND;
use crate::layout::GlyphLighting;
use crate::layout::GlyphRenderMode;
use crate::layout::GlyphShadowMode;
use crate::layout::GlyphSidedness;
use crate::layout::WorldTextStyle;
use crate::render::constants;
use crate::text;
use crate::text::GlyphCache;
use crate::text::PreparedTextRun;
use crate::text::RenderMode;
use crate::text::RunStorageKey;
use crate::text::TextMaterial;
use crate::text::TextMaterialInput;

/// Marker for mesh entities spawned by the world text renderer.
#[derive(Component)]
pub struct WorldTextMesh;

/// Despawns existing text mesh children of the given parent entity.
pub(super) fn despawn_mesh_children(
    parent: Entity,
    old_meshes: &Query<(Entity, &ChildOf), With<WorldTextMesh>>,
    commands: &mut Commands,
) {
    for (mesh_entity, child_of) in old_meshes {
        if child_of.parent() == parent {
            commands.entity(mesh_entity).despawn();
        }
    }
}

/// Frees a world-text run's GPU storage when its glyph mesh is despawned.
///
/// `On<Remove>` fires before the component is dropped, so the mesh's
/// [`RunStorageKey`] is still readable. The world-text twin of panel text's
/// `free_run_storage_on_mesh_removal`: every `WorldTextMesh` despawn — a
/// geometry rebuild or a despawned `WorldText` entity — frees only that run's
/// entry in the shared [`GlyphCache`], never the whole map.
pub fn free_run_storage_on_world_mesh_removal(
    trigger: On<Remove, WorldTextMesh>,
    storage_keys: Query<&RunStorageKey>,
    mut backend: ResMut<GlyphCache>,
) {
    if let Ok(storage_key) = storage_keys.get(trigger.event_target()) {
        backend.remove_run_storage(*storage_key);
    }
}

/// Applies an alpha-only change to a standalone world-text run in place: it
/// mutates the run material's `base.alpha_mode` and rebuilds nothing — the glyph
/// mesh and its three GPU buffers are alpha-independent.
///
/// A run whose geometry also changed this frame is rebuilt by `render_world_text`
/// with the resolved alpha already baked in, so this system skips it when
/// `WorldText`, `WorldTextStyle`, or `Resolved<FontUnit>` changed (M7) — the
/// three signals that drive a geometry rebuild. With that skip the two systems
/// need no ordering edge: in any run order the geometry rebuild owns a
/// both-changed run, and the skip keeps this system off a material handle the
/// geometry pass may have just despawned. The write is value-guarded (R5) so a
/// no-op resolution does not trip `Changed<TextMaterial>`. An entity with no
/// `WorldTextMesh` child — empty text, or one not yet built — is a no-op: the
/// inner loop finds nothing (F4).
pub fn update_world_text_alpha(
    changed_alphas: Query<
        (
            Entity,
            Ref<WorldText>,
            Ref<WorldTextStyle>,
            Option<Ref<Resolved<FontUnit>>>,
            &Resolved<TextAlpha>,
        ),
        (Without<PanelChild>, Changed<Resolved<TextAlpha>>),
    >,
    run_meshes: Query<(&ChildOf, &MeshMaterial3d<TextMaterial>), With<WorldTextMesh>>,
    mut materials: ResMut<Assets<TextMaterial>>,
) {
    for (entity, world_text, style, unit, resolved) in &changed_alphas {
        if world_text.is_changed()
            || style.is_changed()
            || unit.is_some_and(|unit| unit.is_changed())
        {
            continue;
        }
        let resolved_alpha = resolved.0.0;
        for (child_of, material_handle) in &run_meshes {
            if child_of.parent() != entity {
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

/// Configures a `StandardMaterial`'s `double_sided` and `cull_mode` fields
/// from a [`GlyphSidedness`] choice.
const fn apply_sidedness(base: &mut StandardMaterial, sidedness: GlyphSidedness) {
    match sidedness {
        GlyphSidedness::DoubleSided => {
            base.double_sided = true;
            base.cull_mode = None;
        },
        GlyphSidedness::OneSided => {
            base.double_sided = false;
            base.cull_mode = Some(Face::Back);
        },
    }
}

pub(super) struct MeshSpawnAssets<'a, 'w, 's> {
    pub(super) meshes:          &'a mut Assets<Mesh>,
    pub(super) materials:       &'a mut Assets<TextMaterial>,
    pub(super) storage_buffers: &'a mut Assets<ShaderBuffer>,
    pub(super) commands:        &'a mut Commands<'w, 's>,
}

/// Spawns the visible mesh for a world-text run. The mesh casts its
/// own coverage-silhouette shadow unless the style's shadow mode is
/// [`GlyphShadowMode::None`].
pub(super) fn spawn_world_text_meshes(
    prepared: &PreparedTextRun,
    backend: &mut GlyphCache,
    entity: Entity,
    style: &WorldTextStyle,
    alpha_mode: AlphaMode,
    assets: &mut MeshSpawnAssets<'_, '_, '_>,
) -> f32 {
    let mesh_start = Instant::now();
    let Ok(storage) =
        backend.ensure_run_storage(prepared, None, assets.meshes, assets.storage_buffers)
    else {
        return 0.0;
    };

    let material_handle = assets.materials.add(world_text_material(
        style,
        alpha_mode,
        style.render_mode().into(),
        storage.curves,
        storage.bands,
        storage.glyphs,
    ));
    spawn_visible_mesh(
        entity,
        storage.mesh,
        prepared.storage_key(),
        material_handle,
        style.shadow_mode(),
        assets.commands,
    );

    mesh_start
        .elapsed()
        .as_secs_f32()
        .mul_add(MILLISECONDS_PER_SECOND, 0.0)
}

fn world_text_material(
    style: &WorldTextStyle,
    alpha_mode: AlphaMode,
    render_mode: RenderMode,
    curves: Handle<ShaderBuffer>,
    bands: Handle<ShaderBuffer>,
    glyphs: Handle<ShaderBuffer>,
) -> TextMaterial {
    let mut base = StandardMaterial {
        depth_bias: -constants::LAYER_DEPTH_BIAS,
        alpha_mode,
        ..Default::default()
    };
    apply_sidedness(&mut base, style.sidedness());
    base.unlit = matches!(style.lighting(), GlyphLighting::Unlit);
    text::text_material(TextMaterialInput {
        base,
        fill_color: style.color(),
        render_mode,
        // World text positions in 3D with real transform offsets, so it needs no
        // coplanar OIT layer offset.
        oit_depth_offset: 0.0,
        curves,
        bands,
        glyphs,
    })
}

fn spawn_visible_mesh(
    entity: Entity,
    mesh: Handle<Mesh>,
    storage_key: RunStorageKey,
    material: Handle<TextMaterial>,
    shadow_mode: GlyphShadowMode,
    commands: &mut Commands,
) {
    match shadow_mode {
        GlyphShadowMode::None => {
            commands.entity(entity).with_child((
                WorldTextMesh,
                storage_key,
                NotShadowCaster,
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Transform::IDENTITY,
            ));
        },
        GlyphShadowMode::Cast => {
            commands.entity(entity).with_child((
                WorldTextMesh,
                storage_key,
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Transform::IDENTITY,
            ));
        },
    }
}

impl From<GlyphRenderMode> for RenderMode {
    fn from(render_mode: GlyphRenderMode) -> Self {
        match render_mode {
            GlyphRenderMode::Text => Self::Text,
            GlyphRenderMode::PunchOut => Self::PunchOut,
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::sync::Arc;

    use bevy::asset::AssetPlugin;
    use bevy::prelude::*;
    use bevy::render::storage::ShaderBuffer;
    use bevy_kana::ToF32;

    use super::WorldTextMesh;
    use super::free_run_storage_on_world_mesh_removal;
    use super::update_world_text_alpha;
    use crate::Mm;
    use crate::cascade::CascadeDefault;
    use crate::cascade::CascadeDefaults;
    use crate::cascade::CascadeEntityCommandsExt;
    use crate::cascade::CascadePlugin;
    use crate::cascade::FontUnit;
    use crate::cascade::Resolved;
    use crate::cascade::TextAlpha;
    use crate::constants::MONOSPACE_WIDTH_RATIO;
    use crate::layout::LayoutTextStyle;
    use crate::layout::ShapedTextCache;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;
    use crate::layout::WorldTextStyle;
    use crate::panel::DiegeticPanel;
    use crate::panel::HeadlessLayoutPlugin;
    use crate::render::panel_text::TextRenderPlugin;
    use crate::render::text_shaping::TextShapingContext;
    use crate::render::world_text;
    use crate::render::world_text::WorldText;
    use crate::text::DiegeticTextMeasurer;
    use crate::text::FontRegistry;
    use crate::text::GlyphCache;
    use crate::text::TextMaterial;

    /// App with just the standalone world-text pipeline: the geometry system,
    /// the alpha system, the spawn-seed observer, and the run-storage cleanup
    /// observer, over real fonts and asset storage so a spawned run produces a
    /// mesh.
    fn world_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(AssetPlugin::default())
            .add_plugins(TransformPlugin)
            .init_resource::<CascadeDefaults>()
            .add_plugins(CascadePlugin::<TextAlpha>::default())
            .add_plugins(CascadePlugin::<FontUnit>::default())
            .insert_resource(FontRegistry::new().expect("embedded font should parse"))
            .init_resource::<TextShapingContext>()
            .init_resource::<ShapedTextCache>()
            .init_resource::<GlyphCache>()
            .init_asset::<Mesh>()
            .init_asset::<ShaderBuffer>()
            .init_asset::<TextMaterial>()
            .add_observer(world_text::seed_world_text_overrides)
            .add_observer(free_run_storage_on_world_mesh_removal)
            .add_systems(
                PostUpdate,
                (world_text::render_world_text, update_world_text_alpha),
            );
        app
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

    /// App with both text pipelines wired through the real [`TextRenderPlugin`],
    /// so a panel-label run and a standalone world-text run share one
    /// [`GlyphCache`].
    fn mixed_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(AssetPlugin::default())
            .add_plugins(TransformPlugin)
            .insert_resource(monospace_measurer())
            .add_plugins(HeadlessLayoutPlugin)
            .insert_resource(FontRegistry::new().expect("embedded font should parse"))
            .init_resource::<GlyphCache>()
            .init_asset::<Mesh>()
            .init_asset::<ShaderBuffer>()
            .init_asset::<TextMaterial>()
            .init_asset::<StandardMaterial>()
            .add_plugins(TextRenderPlugin);
        app
    }

    fn settle(app: &mut App) {
        for _ in 0..4 {
            app.update();
        }
    }

    /// The [`WorldTextMesh`] entity parented under `parent`, if it has one.
    fn world_mesh_of(app: &mut App, parent: Entity) -> Option<Entity> {
        let mut state = app
            .world_mut()
            .query_filtered::<(Entity, &ChildOf), With<WorldTextMesh>>();
        state
            .iter(app.world())
            .find(|(_, child_of)| child_of.parent() == parent)
            .map(|(entity, _)| entity)
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

    fn run_storage_len(app: &App) -> usize {
        app.world().resource::<GlyphCache>().run_storage_len()
    }

    #[test]
    fn alpha_only_change_updates_material_without_respawning_mesh() {
        let mut app = world_app();
        app.world_mut()
            .resource_mut::<CascadeDefault<TextAlpha>>()
            .0 = TextAlpha(AlphaMode::Blend);
        let entity = app
            .world_mut()
            .spawn((WorldText::new("Hi"), WorldTextStyle::new(Mm(6.0))))
            .id();
        settle(&mut app);

        let mesh_before = world_mesh_of(&mut app, entity).expect("world text mesh should exist");
        assert_eq!(material_alpha(&app, mesh_before), AlphaMode::Blend);
        assert_eq!(run_storage_len(&app), 1);

        // The standalone authors no alpha override, so a global-default change
        // moves its Resolved<TextAlpha> — an alpha-only update.
        app.world_mut()
            .resource_mut::<CascadeDefault<TextAlpha>>()
            .0 = TextAlpha(AlphaMode::Add);
        settle(&mut app);

        let mesh_after =
            world_mesh_of(&mut app, entity).expect("world text mesh should still exist");
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
    fn alpha_override_updates_material_without_respawning_mesh() {
        let mut app = world_app();
        let entity = app
            .world_mut()
            .spawn((WorldText::new("Hi"), WorldTextStyle::new(Mm(6.0))))
            .id();
        settle(&mut app);

        let mesh_before = world_mesh_of(&mut app, entity).expect("world text mesh should exist");
        assert_eq!(material_alpha(&app, mesh_before), AlphaMode::Blend);
        assert_eq!(run_storage_len(&app), 1);

        app.world_mut()
            .commands()
            .entity(entity)
            .override_text_alpha(AlphaMode::Add);
        settle(&mut app);

        let mesh_after =
            world_mesh_of(&mut app, entity).expect("world text mesh should still exist");
        assert_eq!(
            mesh_before, mesh_after,
            "an alpha-only override must not respawn the mesh"
        );
        assert_eq!(
            material_alpha(&app, mesh_after),
            AlphaMode::Add,
            "the alpha system should mutate base.alpha_mode in place"
        );
        assert_eq!(
            run_storage_len(&app),
            1,
            "no run storage churn on an alpha-only override"
        );
    }

    #[test]
    fn content_change_rebuilds_the_runs_mesh() {
        let mut app = world_app();
        let entity = app
            .world_mut()
            .spawn((WorldText::new("Hi"), WorldTextStyle::new(Mm(6.0))))
            .id();
        settle(&mut app);
        let mesh_before = world_mesh_of(&mut app, entity).expect("world text mesh should exist");
        assert_eq!(run_storage_len(&app), 1);

        // A content change rides Changed<WorldText> → re-runs text shaping, then
        // despawns and respawns. The observer frees the old run's storage as the
        // mesh despawns.
        app.world_mut()
            .get_mut::<WorldText>(entity)
            .expect("entity carries WorldText")
            .set_text("Hello");
        settle(&mut app);

        let mesh_after = world_mesh_of(&mut app, entity).expect("rebuilt mesh should exist");
        assert_ne!(
            mesh_before, mesh_after,
            "a content change respawns the run's mesh (a new entity)"
        );
        assert_eq!(
            run_storage_len(&app),
            1,
            "the rebuilt run holds exactly one storage entry"
        );
    }

    #[test]
    fn alpha_change_with_no_mesh_child_is_a_no_op() {
        let mut app = world_app();
        app.world_mut()
            .resource_mut::<CascadeDefault<TextAlpha>>()
            .0 = TextAlpha(AlphaMode::Blend);
        // Empty text never spawns a mesh.
        let entity = app
            .world_mut()
            .spawn((WorldText::new(""), WorldTextStyle::new(Mm(6.0))))
            .id();
        settle(&mut app);
        assert!(
            world_mesh_of(&mut app, entity).is_none(),
            "empty text spawns no mesh"
        );
        assert_eq!(run_storage_len(&app), 0);

        // The alpha-only branch finds no WorldTextMesh child — a no-op, not a panic.
        app.world_mut()
            .resource_mut::<CascadeDefault<TextAlpha>>()
            .0 = TextAlpha(AlphaMode::Add);
        settle(&mut app);

        assert!(
            world_mesh_of(&mut app, entity).is_none(),
            "still no mesh after the alpha change"
        );
        assert_eq!(
            run_storage_len(&app),
            0,
            "no storage created by an alpha no-op"
        );
        assert_eq!(
            app.world()
                .get::<Resolved<TextAlpha>>(entity)
                .expect("entity carries Resolved<TextAlpha>")
                .0
                .0,
            AlphaMode::Add,
            "the resolved alpha still tracked the default change",
        );
    }

    #[test]
    fn geometry_rebuild_leaves_a_coexisting_panel_run_storage_intact() {
        let mut app = mixed_app();
        // A panel-label run and a standalone world-text run share one GlyphCache.
        app.world_mut().spawn(
            DiegeticPanel::world()
                .size(Mm(100.0), Mm(50.0))
                .layout(|b| {
                    b.text("Panel", LayoutTextStyle::new(10.0));
                })
                .build()
                .expect("panel should build"),
        );
        let standalone = app
            .world_mut()
            .spawn((WorldText::new("World"), WorldTextStyle::new(Mm(6.0))))
            .id();
        settle(&mut app);
        assert_eq!(
            run_storage_len(&app),
            2,
            "one panel run + one world-text run hold storage"
        );

        // Rebuild only the standalone. The panel is untouched, so the old blunt
        // clear_run_storage() would have wiped the panel run too; per-run removal
        // must leave it intact.
        app.world_mut()
            .get_mut::<WorldText>(standalone)
            .expect("standalone carries WorldText")
            .set_text("World2");
        settle(&mut app);

        assert_eq!(
            run_storage_len(&app),
            2,
            "the world-text rebuild frees only its own run; the panel run survives"
        );
    }
}
