//! Text rendering systems for panel and world-space glyph meshes.

mod batching;
mod reconcile;
mod shaping;

use bevy::prelude::*;

use self::batching::PanelTextAlpha;
use self::batching::SharedMsdfMaterials;
use self::batching::build_panel_batched_meshes;
use self::batching::sync_panel_hue_offset;
use self::reconcile::poll_atlas_glyphs;
use self::reconcile::reconcile_panel_image_children;
use self::reconcile::reconcile_panel_text_children;
use self::shaping::shape_panel_text_children;
use super::msdf_material::MsdfTextMaterial;
use super::panel_rtt;
use super::text_shaping::TextShapingContext;
use super::world_text;
use crate::cascade::CascadeEntityPlugin;
use crate::cascade::CascadePanelChildPlugin;
use crate::layout::ShapedTextCache;
use crate::panel::DiegeticPerfStats;

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
