//! Text rendering systems for panel and world-space glyph meshes.

mod batching;
mod reconcile;
mod shaping;

use bevy::camera::visibility::VisibilitySystems;
use bevy::prelude::*;

use self::batching::PanelTextAlpha;
use self::batching::build_panel_slug_meshes;
use self::reconcile::reconcile_panel_image_children;
use self::reconcile::reconcile_panel_text_children;
use self::shaping::shape_panel_text_children;
use super::panel_rtt;
use super::text_shaping::TextShapingContext;
use super::world_text;
use super::world_text::PanelTextChild;
use super::world_text::PendingGlyphs;
use super::world_text::WorldText;
use crate::cascade::CascadeEntityPlugin;
use crate::cascade::CascadePanelChildPlugin;
use crate::layout::ShapedTextCache;
use crate::panel::DiegeticPerfStats;
use crate::slug_text_spike::SlugBackendCompleted;

/// Plugin that adds slug text rendering for diegetic panels.
///
/// Reconciles panel text/image children, runs text shaping for panel
/// text, and builds the slug glyph meshes for panel and world text.
pub(super) struct TextRenderPlugin;

impl Plugin for TextRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(CascadePanelChildPlugin::<PanelTextAlpha>::default());
        app.add_plugins(CascadeEntityPlugin::<world_text::WorldTextAlpha>::default());
        app.add_plugins(CascadeEntityPlugin::<world_text::WorldFontUnit>::default());
        app.init_resource::<TextShapingContext>();
        app.init_resource::<ShapedTextCache>();
        app.init_resource::<DiegeticPerfStats>();
        app.add_systems(
            PostUpdate,
            (
                panel_rtt::setup_panel_rtt,
                reconcile_panel_text_children.after(panel_rtt::setup_panel_rtt),
                reconcile_panel_image_children.after(panel_rtt::setup_panel_rtt),
                shape_panel_text_children.after(reconcile_panel_text_children),
                build_panel_slug_meshes.after(shape_panel_text_children),
                world_text::render_world_text,
                world_text::emit_world_text_ready.after(VisibilitySystems::CalculateBounds),
            ),
        );
        app.add_observer(mark_text_pending_on_slug_completed);
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
