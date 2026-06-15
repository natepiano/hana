//! Render commands produced by the layout engine.

use bevy::asset::Handle;
use bevy::color::Color;
use bevy::image::Image;

use super::Border;
use super::BoundingBox;
use super::DrawZIndex;
use super::ResolvedPanelLine;
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
    /// Authored draw layer from the source element.
    pub z_index:     DrawZIndex,
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

/// Fixed draw step derived from a [`RenderCommandKind`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DrawStep {
    /// [`RenderCommandKind::Rectangle`], [`RenderCommandKind::Border`], and
    /// [`RenderCommandKind::Image`] commands.
    Fill,
    /// [`RenderCommandKind::Lines`] commands.
    Lines,
    /// [`RenderCommandKind::Text`] commands.
    Text,
}

impl DrawStep {
    /// Returns the stable ordinal for `DrawStep` sort keys.
    #[must_use]
    pub const fn ordinal(self) -> u8 {
        match self {
            Self::Fill => 0,
            Self::Lines => 1,
            Self::Text => 2,
        }
    }
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
    Lines {
        /// Resolved lines to render as one command group.
        lines: Vec<ResolvedPanelLine>,
    },
    /// Begin a clipping region. All subsequent commands until the matching
    /// [`ScissorEnd`](Self::ScissorEnd) are clipped to this bounding box.
    ScissorStart,
    /// End the most recent clipping region.
    ScissorEnd,
}

impl RenderCommandKind {
    /// Returns the fixed [`DrawStep`] for commands that draw pixels.
    #[must_use]
    pub(crate) const fn draw_step(&self) -> Option<DrawStep> {
        match self {
            Self::Rectangle { .. } | Self::Border { .. } | Self::Image { .. } => {
                Some(DrawStep::Fill)
            },
            Self::Lines { .. } => Some(DrawStep::Lines),
            Self::Text { .. } => Some(DrawStep::Text),
            Self::ScissorStart | Self::ScissorEnd => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILL_ORDINAL: u8 = 0;
    const LINES_ORDINAL: u8 = 1;
    const TEXT_ORDINAL: u8 = 2;

    #[test]
    fn draw_step_ordinals_are_fixed() {
        assert_eq!(DrawStep::Fill.ordinal(), FILL_ORDINAL);
        assert_eq!(DrawStep::Lines.ordinal(), LINES_ORDINAL);
        assert_eq!(DrawStep::Text.ordinal(), TEXT_ORDINAL);
    }

    #[test]
    fn render_command_kinds_map_to_draw_steps() {
        let cases = [
            (
                RenderCommandKind::Rectangle {
                    color:  Color::WHITE,
                    source: RectangleSource::Background,
                },
                Some(DrawStep::Fill),
            ),
            (
                RenderCommandKind::Border {
                    border: Border::default(),
                },
                Some(DrawStep::Fill),
            ),
            (
                RenderCommandKind::Image {
                    handle: Handle::<Image>::default(),
                    tint:   Color::WHITE,
                },
                Some(DrawStep::Fill),
            ),
            (
                RenderCommandKind::Lines { lines: Vec::new() },
                Some(DrawStep::Lines),
            ),
            (
                RenderCommandKind::Text {
                    text:   String::new(),
                    config: TextStyle::default(),
                },
                Some(DrawStep::Text),
            ),
            (RenderCommandKind::ScissorStart, None),
            (RenderCommandKind::ScissorEnd, None),
        ];

        for (kind, expected) in cases {
            assert_eq!(kind.draw_step(), expected);
        }
    }
}
