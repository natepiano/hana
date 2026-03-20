//! Render commands produced by the layout engine.

use bevy::color::Color;

use super::types::Border;
use super::types::BoundingBox;
use super::types::TextConfig;

/// A single render command produced by the layout pass.
///
/// The layout engine outputs a flat, ordered list of these commands.
/// Consumers iterate them to draw rectangles, text, borders, and clip regions.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderCommand {
    /// Computed bounding box in layout coordinates.
    pub bounds:      BoundingBox,
    /// What to render.
    pub kind:        RenderCommandKind,
    /// Index of the source element in the `LayoutTree`.
    pub element_idx: usize,
}

/// Distinguishes the origin of a [`RenderCommandKind::Rectangle`] command.
///
/// Matching Clay, both element backgrounds and [`Border::between_children`]
/// lines are emitted as `Rectangle` commands. This enum lets consumers
/// (like the color-only fast path) apply the correct color source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RectangleSource {
    /// Element background fill.
    Background,
    /// Line drawn between children from a [`Border::between_children`] width.
    BetweenChildrenBorder,
}

/// The specific visual to render.
#[derive(Clone, Debug, PartialEq)]
pub enum RenderCommandKind {
    /// A filled rectangle.
    Rectangle {
        /// Fill color.
        color:  Color,
        /// Whether this rectangle is a background or a between-children border.
        source: RectangleSource,
    },
    /// A text string.
    Text {
        /// The text content.
        text:       String,
        /// Text configuration (font, size, etc.).
        config:     TextConfig,
        /// Number of quads emitted during the last full mesh build.
        ///
        /// Set by the text renderer after `shape_text_to_quads` — which skips
        /// glyphs without atlas entries (spaces, etc.) — so this count may be
        /// less than the shaped glyph count. The color-only fast path uses
        /// this to produce a correctly aligned vertex color array.
        quad_count: usize,
    },
    /// A border outline.
    Border {
        /// Border specification.
        border: Border,
    },
    /// Begin a clipping region. All subsequent commands until the matching
    /// [`ScissorEnd`](Self::ScissorEnd) are clipped to this bounding box.
    ScissorStart,
    /// End the most recent clipping region.
    ScissorEnd,
}
