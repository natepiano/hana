//! Constants for the typography overlay.

/// Fraction of the first glyph's advance width used as the spacing
/// unit between arrow columns.
pub const ARROW_SPACING_RATIO: f32 = 0.28;

/// Gap between labels and the elements they annotate, relative to font size.
/// Multiplied by `font_size * LAYOUT_TO_WORLD` at usage to get world units.
pub const LABEL_GAP_RATIO: f32 = 0.02;

/// Default line width for overlay gizmos (in pixels).
pub const DEFAULT_LINE_WIDTH: f32 = 0.5;

/// Radius of origin/advancement dots relative to font size.
/// Multiplied by `font_size * LAYOUT_TO_WORLD` at usage to get world units.
pub const DOT_RADIUS_RATIO: f32 = 0.01;

/// Arrowhead line length relative to font size.
/// Multiplied by `font_size * LAYOUT_TO_WORLD` at usage to get world units.
pub const ARROWHEAD_RATIO: f32 = 0.017;

/// Gap between arrow tips and metric lines relative to font size.
/// Multiplied by `font_size * LAYOUT_TO_WORLD` at usage to get world units.
pub const ARROW_GAP_RATIO: f32 = 0.012;

/// Label for the advancement dimension arrow.
pub const LABEL_ADVANCEMENT: &str = "advancement";

/// Label for the ascent metric line and dimension arrow.
pub const LABEL_ASCENT: &str = "ascent";

/// Label for the baseline metric line.
pub const LABEL_BASELINE: &str = "baseline";

/// Label for the bottom metric line.
pub const LABEL_BOTTOM: &str = "bottom";

/// Label for the bounding box callout.
pub const LABEL_BOUNDING_BOX: &str = "bounding box";

/// Label for the cap height metric line and dimension arrow.
pub const LABEL_CAP_HEIGHT: &str = "cap height";

/// Label for the descent metric line and dimension arrow.
pub const LABEL_DESCENT: &str = "descent";

/// Label for the line height dimension arrow.
pub const LABEL_LINE_HEIGHT: &str = "line height";

/// Label for the origin callout.
pub const LABEL_ORIGIN: &str = "origin";

/// Font size for metric labels relative to the text's font size.
/// Apple's reference diagram uses labels roughly 1/10th the display size.
pub const LABEL_SIZE_RATIO: f32 = 0.06;

/// Label for the top metric line.
pub const LABEL_TOP: &str = "top";

/// Label for the x-height metric line and dimension arrow.
pub const LABEL_X_HEIGHT: &str = "x-height";

/// Layout-units-to-world-units conversion factor.
pub const LAYOUT_TO_WORLD: f32 = 0.01;

/// Line width for arrows, callout lines, and arrow points.
pub const THICK_LINE_WIDTH: f32 = 2.5;

/// Line width for metric lines, bounding boxes, and callout backgrounds.
pub const THIN_LINE_WIDTH: f32 = 1.0;

/// Margin for `ZoomToFit` when framing a scene or entity.
pub const ZOOM_TO_FIT_MARGIN: f32 = 0.05;
