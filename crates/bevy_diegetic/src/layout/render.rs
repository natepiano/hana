//! Render commands produced by the layout engine.

use bevy::asset::Handle;
use bevy::color::Color;
use bevy::image::Image;

use super::Border;
use super::BoundingBox;
use super::ResolvedPanelShape;
use super::TextStyle;

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
    /// Coplanar geometry draw slot, the panel-local `DrawOrdinal` source.
    ///
    /// Slot-consuming kinds ([`Rectangle`](RenderCommandKind::Rectangle),
    /// [`Border`](RenderCommandKind::Border), [`Image`](RenderCommandKind::Image),
    /// [`Shapes`](RenderCommandKind::Shapes)) each occupy one slot in emission
    /// order. `Text` and scissor commands record the next slot without
    /// consuming it, so text-heavy panels don't inflate later geometry
    /// ordinals toward the default draw layer.
    pub draw_slot:   usize,
}

/// Distinguishes the origin of a [`RenderCommandKind::Rectangle`] command.
///
/// Matching Clay, both element backgrounds and [`Border::between_children`]
/// lines are emitted as `Rectangle` commands. This enum lets consumers
/// (like the color-only fast path) apply the correct color source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RectangleSource {
    /// `Element` background fill.
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
        text:   String,
        /// Text configuration (font, size, etc.).
        config: TextStyle,
    },
    /// A border outline.
    Border {
        /// Border specification.
        border: Border,
    },
    /// An image (textured quad).
    Image {
        /// Handle to the image asset.
        handle: Handle<Image>,
        /// Tint color multiplied against the texture (white = no tint).
        tint:   Color,
    },
    /// Resolved panel-local line primitives.
    Shapes {
        /// Resolved lines to render as one command group.
        shapes: Vec<ResolvedPanelShape>,
    },
    /// Begin a clipping region. All subsequent commands until the matching
    /// [`ScissorEnd`](Self::ScissorEnd) are clipped to this bounding box.
    ScissorStart,
    /// End the most recent clipping region.
    ScissorEnd,
}

impl RenderCommandKind {
    /// Whether this command occupies a [`RenderCommand::draw_slot`]. Text gets
    /// its draw ordinal from `DrawLayer` and scissor commands draw nothing,
    /// so neither consumes a slot.
    #[must_use]
    pub const fn consumes_draw_slot(&self) -> bool {
        match self {
            Self::Rectangle { .. }
            | Self::Border { .. }
            | Self::Image { .. }
            | Self::Shapes { .. } => true,
            Self::Text { .. } | Self::ScissorStart | Self::ScissorEnd => false,
        }
    }
}
