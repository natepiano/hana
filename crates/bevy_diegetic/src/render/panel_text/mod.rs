//! Text rendering systems for panel and world-space glyph meshes.

mod alpha;
mod glyph_cascade;
mod layout;
mod mesh_spawning;
mod reconcile;
mod shaping;

use bevy::camera::visibility::VisibilitySystems;
use bevy::prelude::*;

use self::alpha::seed_panel_child_alpha;
use self::glyph_cascade::seed_panel_child_glyph;
pub use self::layout::PanelTextLayout;
use self::mesh_spawning::free_run_storage_on_mesh_removal;
use self::mesh_spawning::update_panel_text_alpha;
use self::mesh_spawning::update_panel_text_geometry;
use self::reconcile::reconcile_panel_image_children;
use self::reconcile::reconcile_panel_text_children;
use self::shaping::shape_panel_text_children;
use super::PanelChildSystems;
use super::text_shaping::TextShapingContext;
use super::world_text;
use crate::cascade::CascadePlugin;
use crate::cascade::TextAlpha;
use crate::cascade::TextLighting;
use crate::cascade::TextSidedness;
use crate::layout::GlyphRenderMode;
use crate::layout::GlyphShadowMode;
use crate::layout::ShapedTextCache;
use crate::panel::DiegeticPerfStats;
use crate::text::PreparedTextRun;

/// Stores a prepared text run for a panel [`TextContent`](crate::TextContent) child.
#[derive(Component)]
pub(super) struct PanelText {
    /// Prepared text run.
    pub prepared:    PreparedTextRun,
    /// Glyph render mode for this text element.
    pub render_mode: GlyphRenderMode,
    /// Glyph shadow mode for this text element.
    pub shadow_mode: GlyphShadowMode,
    /// Text fill color.
    pub fill_color:  Color,
    /// Optional panel-local clipping rect.
    pub clip_rect:   Option<[f32; 4]>,
}

/// Plugin that adds text rendering for diegetic panels.
///
/// Reconciles panel text/image children, runs text shaping for panel text, and
/// builds the glyph meshes.
pub(super) struct TextRenderPlugin;

impl Plugin for TextRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(CascadePlugin::<TextAlpha>::default());
        app.add_plugins(CascadePlugin::<TextLighting>::default());
        app.add_plugins(CascadePlugin::<TextSidedness>::default());
        app.add_observer(seed_panel_child_alpha);
        app.add_observer(seed_panel_child_glyph);
        app.add_observer(free_run_storage_on_mesh_removal);
        app.init_resource::<TextShapingContext>();
        app.init_resource::<ShapedTextCache>();
        app.init_resource::<DiegeticPerfStats>();
        app.add_systems(
            PostUpdate,
            (
                reconcile_panel_text_children.in_set(PanelChildSystems::Build),
                reconcile_panel_image_children.in_set(PanelChildSystems::Build),
                shape_panel_text_children.after(reconcile_panel_text_children),
                update_panel_text_geometry
                    .after(shape_panel_text_children)
                    .before(TransformSystems::Propagate),
                update_panel_text_alpha
                    .after(shape_panel_text_children)
                    .before(TransformSystems::Propagate)
                    .in_set(PanelChildSystems::Build),
                world_text::emit_world_text_ready.after(VisibilitySystems::CalculateBounds),
            ),
        );
    }
}
