//! Text rendering systems for panel and world-space glyph meshes.

mod access;
mod alpha;
mod batching;
mod glyph_cascade;
mod layout;
mod reconcile;
mod relationship;
mod shaping;

use bevy::camera::visibility::VisibilitySystems;
use bevy::prelude::*;

pub use self::access::DiegeticTextMut;
pub use self::access::PanelText;
pub use self::access::PanelTextReader;
pub use self::access::TextEdit;
use self::alpha::seed_panel_text_child_alpha;
pub use self::batching::DiegeticTextBatch;
use self::batching::commit_batch_buffers;
use self::batching::update_batch_bounds;
use self::batching::update_panel_text_batches;
use self::batching::write_batch_run_transforms;
use self::glyph_cascade::seed_panel_text_child_glyph;
pub use self::layout::PanelTextLayout;
use self::reconcile::reconcile_panel_image_children;
use self::reconcile::reconcile_panel_text_children;
pub use self::relationship::PanelTextRuns;
pub use self::relationship::TextRunOf;
use self::shaping::shape_panel_text_children;
use super::PanelChildSystems;
use super::text_shaping::TextShapingContext;
use super::world_text;
use crate::cascade::CascadePlugin;
use crate::cascade::TextAlpha;
use crate::layout::GlyphRenderMode;
use crate::layout::GlyphShadowMode;
use crate::layout::Lighting;
use crate::layout::ShapedTextCache;
use crate::layout::Sidedness;
use crate::panel::DiegeticPerfStats;
use crate::text::PreparedTextRun;

/// Stores a prepared text run for a panel [`TextContent`](crate::TextContent) child.
///
/// Internal render-pipeline component, distinct from the public
/// [`PanelText`](crate::PanelText) `SystemParam` callers use to get/set run text
/// by id.
#[derive(Component)]
pub(super) struct PreparedPanelText {
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
        app.add_plugins(CascadePlugin::<Lighting>::default());
        app.add_plugins(CascadePlugin::<Sidedness>::default());
        app.add_observer(seed_panel_text_child_alpha);
        app.add_observer(seed_panel_text_child_glyph);
        app.init_resource::<TextShapingContext>();
        app.init_resource::<ShapedTextCache>();
        app.init_resource::<DiegeticPerfStats>();
        app.add_systems(
            PostUpdate,
            (
                reconcile_panel_text_children.in_set(PanelChildSystems::Build),
                // After the text reconcile so the two passes' shared
                // `DiegeticPerfStats::reconcile_ms` reset-then-accumulate
                // sequence is deterministic.
                reconcile_panel_image_children
                    .in_set(PanelChildSystems::Build)
                    .after(reconcile_panel_text_children),
                shape_panel_text_children.after(reconcile_panel_text_children),
                world_text::emit_world_text_ready.after(VisibilitySystems::CalculateBounds),
            ),
        );
        // `update_panel_text_batches` is ordered by the `.after(...)` and
        // `.before(TransformSystems::Propagate)` calls below; the later systems
        // update run transforms, refresh batch `Aabb`s, and upload dirty record
        // buffers.
        app.add_systems(
            PostUpdate,
            (
                update_panel_text_batches
                    .after(shape_panel_text_children)
                    .before(TransformSystems::Propagate),
                write_batch_run_transforms.after(TransformSystems::Propagate),
                update_batch_bounds
                    .after(write_batch_run_transforms)
                    .after(VisibilitySystems::CalculateBounds)
                    .before(VisibilitySystems::CheckVisibility),
                commit_batch_buffers
                    .after(update_panel_text_batches)
                    .after(write_batch_run_transforms),
            ),
        );
    }
}
