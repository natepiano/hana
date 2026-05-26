//! Text rendering systems for panel and world-space glyph meshes.

mod alpha;
mod layout;
mod mesh_spawning;
mod reconcile;
mod shaping;

use bevy::camera::visibility::VisibilitySystems;
use bevy::prelude::*;

use self::alpha::seed_panel_child_alpha;
pub use self::layout::PanelTextLayout;
use self::mesh_spawning::build_panel_text_meshes;
use self::reconcile::reconcile_panel_image_children;
use self::reconcile::reconcile_panel_text_children;
use self::shaping::shape_panel_text_children;
use super::text_shaping::TextShapingContext;
use super::world_text;
use crate::cascade::CascadePlugin;
use crate::cascade::TextAlpha;
use crate::layout::GlyphRenderMode;
use crate::layout::GlyphShadowMode;
use crate::layout::ShapedTextCache;
use crate::panel::DiegeticPerfStats;
use crate::text::SlugPreparedTextRun;

/// Stores a prepared text run for a panel [`WorldText`](crate::WorldText) child.
#[derive(Component)]
pub(super) struct PanelText {
    /// Prepared text run.
    pub prepared:    SlugPreparedTextRun,
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
/// Reconciles panel text/image children, runs text shaping for panel
/// text, and builds glyph meshes for panel and world text.
pub(super) struct TextRenderPlugin;

impl Plugin for TextRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(CascadePlugin::<TextAlpha>::default());
        app.add_observer(world_text::seed_world_text_overrides);
        app.add_observer(seed_panel_child_alpha);
        app.init_resource::<TextShapingContext>();
        app.init_resource::<ShapedTextCache>();
        app.init_resource::<DiegeticPerfStats>();
        app.add_systems(
            PostUpdate,
            (
                reconcile_panel_text_children,
                reconcile_panel_image_children,
                shape_panel_text_children.after(reconcile_panel_text_children),
                build_panel_text_meshes
                    .after(shape_panel_text_children)
                    .before(TransformSystems::Propagate),
                world_text::render_world_text.before(TransformSystems::Propagate),
                world_text::emit_world_text_ready.after(VisibilitySystems::CalculateBounds),
            ),
        );
    }
}
