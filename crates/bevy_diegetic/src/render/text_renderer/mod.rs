//! Text rendering systems for panel and world-space glyph meshes.

mod batching;
mod reconcile;
mod shaping;

use bevy::camera::visibility::VisibilitySystems;
use bevy::prelude::*;

use self::batching::PanelTextAlpha;
use self::batching::SharedMsdfMaterials;
use self::batching::build_panel_batched_meshes;
use self::batching::build_panel_slug_meshes;
use self::batching::sync_panel_hue_offset;
use self::reconcile::poll_atlas_glyphs;
use self::reconcile::reconcile_panel_image_children;
use self::reconcile::reconcile_panel_text_children;
use self::shaping::shape_panel_text_children;
use super::glyph_material::GlyphMaterial;
use super::panel_rtt;
use super::text_backend::TextRenderer;
use super::text_backend::TextRendererPreference;
use super::text_shaping::TextShapingContext;
use super::world_text;
use super::world_text::PanelTextChild;
use super::world_text::PendingGlyphs;
use super::world_text::WorldText;
use crate::cascade::CascadeEntityPlugin;
use crate::cascade::CascadePanelChildPlugin;
use crate::layout::ShapedTextCache;
use crate::panel::DiegeticPerfStats;
use crate::slug_text_spike::SlugBackend;
use crate::slug_text_spike::SlugBackendCompleted;
use crate::slug_text_spike::SlugTextSpikePlugin;
use crate::text::AtlasSwapCompleted;
use crate::text::AtlasSwapStarted;

/// Plugin that adds MSDF text rendering for diegetic panels.
///
/// Registers the [`GlyphMaterial`], adds the text extraction system,
/// and sets up rendering.
pub(super) struct TextRenderPlugin;

impl Plugin for TextRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<GlyphMaterial>::default());
        app.add_plugins(SlugTextSpikePlugin);
        app.add_plugins(CascadePanelChildPlugin::<PanelTextAlpha>::default());
        app.add_plugins(CascadeEntityPlugin::<world_text::WorldTextAlpha>::default());
        app.add_plugins(CascadeEntityPlugin::<world_text::WorldFontUnit>::default());
        app.init_resource::<TextShapingContext>();
        app.init_resource::<TextRendererPreference>();
        app.init_resource::<ShapedTextCache>();
        app.init_resource::<SharedMsdfMaterials>();
        app.init_resource::<DiegeticPerfStats>();
        app.add_systems(
            PostUpdate,
            (
                panel_rtt::setup_panel_rtt,
                poll_atlas_glyphs,
                clear_slug_storage_on_msdf_backend_changed,
                reconcile_panel_text_children
                    .after(poll_atlas_glyphs)
                    .after(panel_rtt::setup_panel_rtt),
                reconcile_panel_image_children
                    .after(poll_atlas_glyphs)
                    .after(panel_rtt::setup_panel_rtt),
                mark_text_pending_on_backend_changed,
                shape_panel_text_children
                    .after(reconcile_panel_text_children)
                    .after(poll_atlas_glyphs),
                build_panel_batched_meshes.after(shape_panel_text_children),
                build_panel_slug_meshes.after(shape_panel_text_children),
                sync_panel_hue_offset.after(build_panel_batched_meshes),
                world_text::render_world_text.after(poll_atlas_glyphs),
                world_text::emit_world_text_ready.after(VisibilitySystems::CalculateBounds),
            ),
        );
        app.add_observer(mark_text_pending_on_swap_started);
        app.add_observer(mark_text_pending_on_swap_completed);
        app.add_observer(mark_text_pending_on_slug_completed);
    }
}

/// Observer: when the atlas driver starts a parallel-swap, mark every
/// visible text entity with [`PendingGlyphs`]. The next
/// `shape_panel_text_children` / `render_world_text` pass then queues
/// the entity's visible glyphs onto pending via `rasterize_target_mut`,
/// scoping the swap workload to "currently on screen" instead of "every
/// glyph the atlas has ever cached".
fn mark_text_pending_on_swap_started(
    _trigger: On<AtlasSwapStarted>,
    panel_children: Query<Entity, With<PanelTextChild>>,
    world_texts: Query<Entity, With<WorldText>>,
    mut commands: Commands,
) {
    for entity in &panel_children {
        commands.entity(entity).insert_if_new(PendingGlyphs);
    }
    for entity in &world_texts {
        commands.entity(entity).insert_if_new(PendingGlyphs);
    }
}

/// Observer: when the swap finalizes, mark every visible text entity
/// with [`PendingGlyphs`] again so the text-shaping pass re-runs
/// against the new active atlas. That pass emits fresh quad data,
/// which lets the batcher rebuild materials with the new image
/// handle and (for world text) re-spawns meshes referencing the new
/// atlas.
fn mark_text_pending_on_swap_completed(
    _trigger: On<AtlasSwapCompleted>,
    panel_children: Query<Entity, With<PanelTextChild>>,
    world_texts: Query<Entity, With<WorldText>>,
    mut commands: Commands,
) {
    for entity in &panel_children {
        commands.entity(entity).insert_if_new(PendingGlyphs);
    }
    for entity in &world_texts {
        commands.entity(entity).insert_if_new(PendingGlyphs);
    }
}

fn mark_text_pending_on_backend_changed(
    text_backend: Res<TextRendererPreference>,
    panel_children: Query<Entity, With<PanelTextChild>>,
    world_texts: Query<Entity, With<WorldText>>,
    mut commands: Commands,
) {
    if !text_backend.is_changed() {
        return;
    }
    if text_backend.backend() == TextRenderer::Slug {
        mark_all_text_pending(&panel_children, &world_texts, &mut commands);
        return;
    }
    mark_all_text_pending(&panel_children, &world_texts, &mut commands);
}

fn clear_slug_storage_on_msdf_backend_changed(
    text_backend: Res<TextRendererPreference>,
    mut slug_backend: ResMut<SlugBackend>,
) {
    if text_backend.is_changed() && text_backend.backend() != TextRenderer::Slug {
        slug_backend.clear_run_storage();
    }
}

fn mark_all_text_pending(
    panel_children: &Query<Entity, With<PanelTextChild>>,
    world_texts: &Query<Entity, With<WorldText>>,
    commands: &mut Commands,
) {
    for entity in panel_children {
        commands.entity(entity).insert_if_new(PendingGlyphs);
    }
    for entity in world_texts {
        commands.entity(entity).insert_if_new(PendingGlyphs);
    }
}

fn mark_text_pending_on_slug_completed(
    _trigger: On<SlugBackendCompleted>,
    panel_children: Query<Entity, With<PanelTextChild>>,
    world_texts: Query<Entity, With<WorldText>>,
    mut commands: Commands,
) {
    mark_all_text_pending(&panel_children, &world_texts, &mut commands);
}
