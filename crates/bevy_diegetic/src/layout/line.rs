//! Authored panel-local line primitives.

use std::error::Error;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::{self};

use bevy::color::Color;
use bevy::math::Vec2;

use super::BoundingBox;
use super::Dimension;
use super::In;
use super::Mm;
use super::Pt;
use super::Px;
use crate::CalloutCap;
use crate::callouts::CalloutCapPrimitiveKind;
use crate::callouts::ResolvedCalloutCap;
use crate::callouts::ResolvedCalloutCapPrimitive;
use crate::render::HairlineFade;

const POINT_SPACE_SCALE: f32 = 1.0;
const MIN_LINE_LENGTH_SQUARED: f32 = f32::EPSILON;
const LINE_COVERAGE_PADDING: f32 = 0.5;
const DEFAULT_LINE_WIDTH: Dimension = Dimension {
    value: 1.0,
    unit:  None,
};
const DEFAULT_CAP_SIZE: Dimension = Dimension {
    value: 4.0,
    unit:  None,
};
const ZERO_DIMENSION: Dimension = Dimension {
    value: 0.0,
    unit:  None,
};
const DEFAULT_PERCENT: f32 = 0.0;

/// A line segment authored in an element's local panel coordinate space.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelLine {
    start:       PanelPoint,
    end:         PanelPoint,
    style:       LineStyle,
    start_inset: Dimension,
    end_inset:   Dimension,
}

/// Visual style for a [`PanelLine`].
#[derive(Clone, Debug, PartialEq)]
pub struct LineStyle {
    width:         Dimension,
    color:         Color,
    cap_size:      Dimension,
    start_cap:     CalloutCap,
    end_cap:       CalloutCap,
    hairline_fade: Option<HairlineFade>,
}

/// A 2D point authored relative to an element's resolved box.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PanelPoint {
    x: PanelCoord,
    y: PanelCoord,
}

/// One coordinate component authored along an element-local axis.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelCoord {
    kind: PanelCoordKind,
}

/// Stable source identity for one resolved panel line.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PanelLineSourceKey {
    /// Line authored in an element-owned `PanelDraw`.
    Element {
        /// Source element index in the `LayoutTree`.
        element_index: usize,
        /// Draw layer ordinal within the element.
        draw_ordinal:  usize,
        /// Line ordinal within the draw layer.
        line_ordinal:  usize,
    },
    /// Line produced by a post-layout source.
    External {
        /// Producer namespace chosen by the source.
        producer:     u64,
        /// Stable source id chosen by the producer.
        source:       u64,
        /// Line ordinal within the producer source.
        line_ordinal: usize,
    },
}

/// Stable source identity for one resolved line primitive.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PanelLinePrimitiveKey {
    /// Source line this primitive belongs to.
    pub line_source:       PanelLineSourceKey,
    /// Primitive ordinal within the line.
    pub primitive_ordinal: usize,
}

/// Clip policy used by the pure panel-line resolver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PanelLineClipPolicy {
    /// Clip to the inherited panel/parent clip intersected with owner bounds.
    OwnerBounds,
    /// Clip to the inherited clip as given (`None` means unclipped) and
    /// ignore owner bounds.
    Inherited,
}

/// Input context for resolving one authored `PanelLine`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PanelLineResolveContext {
    pub(crate) owner_bounds:         BoundingBox,
    pub(crate) inherited_clip:       Option<BoundingBox>,
    pub(crate) clip_policy:          PanelLineClipPolicy,
    pub(crate) source_command_index: usize,
    pub(crate) source_key:           PanelLineSourceKey,
}

enum ResolvedLineClip {
    Visible(Option<BoundingBox>),
    FullyClipped,
}

/// A resolved panel line ready for renderer consumption.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedPanelLine {
    /// Stable line source key.
    pub(crate) source_key:           PanelLineSourceKey,
    /// Command index used when the line was resolved.
    pub(crate) source_command_index: usize,
    /// Owning element bounds in layout coordinates.
    pub(crate) owner_bounds:         BoundingBox,
    /// Bounds covering all resolved primitives.
    pub(crate) visual_bounds:        BoundingBox,
    /// Effective clip for this line.
    pub(crate) clip:                 Option<BoundingBox>,
    /// Resolved start tip after authored start inset.
    pub(crate) start:                Vec2,
    /// Resolved end tip after authored end inset.
    pub(crate) end:                  Vec2,
    /// Resolved shaft start after cap inset.
    pub(crate) shaft_start:          Vec2,
    /// Resolved shaft end after cap inset.
    pub(crate) shaft_end:            Vec2,
    /// Resolved stroke width in point-space layout units.
    pub(crate) width:                f32,
    /// Resolved base line color.
    pub(crate) color:                Color,
    /// Authored hairline fade override; `None` inherits the owning element's
    /// resolution.
    pub(crate) hairline_fade:        Option<HairlineFade>,
    /// Shaft and cap primitives in stable part order.
    pub(crate) primitives:           Vec<ResolvedPanelLinePrimitive>,
}

/// Geometry for one resolved line primitive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PanelLinePrimitiveGeometry {
    /// A stroked line segment.
    Segment {
        /// Segment start point.
        start: Vec2,
        /// Segment end point.
        end:   Vec2,
        /// Segment stroke width.
        width: f32,
    },
    /// A filled cap form.
    Form {
        /// Primitive center point.
        center:    Vec2,
        /// Unit axis pointing along primitive length.
        axis:      Vec2,
        /// Half-size in axis/perpendicular coordinates.
        half_size: Vec2,
    },
}

/// Resolved primitive shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelLinePrimitiveKind {
    /// Stroked segment or open-arrow wing.
    Segment,
    /// Filled triangular arrowhead.
    Triangle,
    /// Filled circular cap.
    Circle,
    /// Filled square cap.
    Square,
    /// Filled diamond cap.
    Diamond,
}

/// A shaft or cap primitive resolved from a `PanelLine`.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedPanelLinePrimitive {
    /// Stable primitive source key.
    pub(crate) source_key: PanelLinePrimitiveKey,
    /// Primitive kind.
    pub(crate) kind:       PanelLinePrimitiveKind,
    /// Primitive geometry in layout coordinates.
    pub(crate) geometry:   PanelLinePrimitiveGeometry,
    /// Resolved primitive color.
    pub(crate) color:      Color,
    /// Primitive visual bounds.
    pub(crate) bounds:     BoundingBox,
    /// Effective clip for this primitive.
    pub(crate) clip:       Option<BoundingBox>,
    /// Stable order within the resolved line.
    pub(crate) part_order: usize,
}

#[derive(Clone, Debug, PartialEq)]
enum PanelCoordKind {
    Start(Dimension),
    End(Dimension),
    Percent(f32),
}

/// Error returned when a panel-line scalar is not finite.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InvalidPanelScalar {
    value: f32,
}

impl InvalidPanelScalar {
    /// Returns the invalid scalar value.
    #[must_use]
    pub const fn value(self) -> f32 { self.value }
}

impl Display for InvalidPanelScalar {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "panel-line scalar must be finite, got {}", self.value)
    }
}

impl Error for InvalidPanelScalar {}

impl PanelLine {
    /// Creates a line from `start` to `end`.
    #[must_use]
    pub fn new(start: impl Into<PanelPoint>, end: impl Into<PanelPoint>) -> Self {
        Self {
            start:       start.into(),
            end:         end.into(),
            style:       LineStyle::default(),
            start_inset: ZERO_DIMENSION,
            end_inset:   ZERO_DIMENSION,
        }
    }

    /// Sets the full line style.
    #[must_use]
    pub const fn style(mut self, style: LineStyle) -> Self {
        self.style = style;
        self
    }

    /// Sets the stroke width.
    #[must_use]
    pub fn width(mut self, width: impl Into<Dimension>) -> Self {
        self.style = self.style.width(width);
        self
    }

    /// Sets the line color.
    #[must_use]
    pub const fn color(mut self, color: Color) -> Self {
        self.style = self.style.color(color);
        self
    }

    /// Sets the default cap size used by caps without an explicit override.
    #[must_use]
    pub fn cap_size(mut self, cap_size: impl Into<Dimension>) -> Self {
        self.style = self.style.cap_size(cap_size);
        self
    }

    /// Sets the cap at the start of the line.
    #[must_use]
    pub const fn start_cap(mut self, cap: CalloutCap) -> Self {
        self.style = self.style.start_cap(cap);
        self
    }

    /// Sets the cap at the end of the line.
    #[must_use]
    pub const fn end_cap(mut self, cap: CalloutCap) -> Self {
        self.style = self.style.end_cap(cap);
        self
    }

    /// Overrides the hairline fade policy for this line. Lines without an
    /// override inherit the owning element's resolution (element override
    /// else the panel's cascade-resolved [`HairlineFade`]). Lines with
    /// different fade policies still merge into one analytic path — the
    /// shader fades each coverage evaluation by its winning curve's
    /// exponent — so a never-fading ruler spine and fading minor ticks abut
    /// without an anti-aliasing junction.
    #[must_use]
    pub const fn hairline_fade(mut self, fade: HairlineFade) -> Self {
        self.style = self.style.hairline_fade(fade);
        self
    }

    /// Insets the visible start tip inward from `start`.
    #[must_use]
    pub fn start_inset(mut self, inset: impl Into<Dimension>) -> Self {
        self.start_inset = inset.into();
        self
    }

    /// Insets the visible end tip inward from `end`.
    #[must_use]
    pub fn end_inset(mut self, inset: impl Into<Dimension>) -> Self {
        self.end_inset = inset.into();
        self
    }

    /// Returns the authored start point.
    #[must_use]
    pub const fn start(&self) -> &PanelPoint { &self.start }

    /// Returns the authored end point.
    #[must_use]
    pub const fn end(&self) -> &PanelPoint { &self.end }

    /// Returns the line style.
    #[must_use]
    pub const fn line_style(&self) -> &LineStyle { &self.style }

    /// Returns the authored start inset.
    #[must_use]
    pub const fn start_inset_dimension(&self) -> Dimension { self.start_inset }

    /// Returns the authored end inset.
    #[must_use]
    pub const fn end_inset_dimension(&self) -> Dimension { self.end_inset }

    pub(crate) fn scaled(&self, default_scale: f32) -> Self {
        Self {
            start:       self.start.scaled(default_scale),
            end:         self.end.scaled(default_scale),
            style:       self.style.scaled(default_scale),
            start_inset: scaled_dimension(self.start_inset, default_scale),
            end_inset:   scaled_dimension(self.end_inset, default_scale),
        }
    }
}

impl PanelLineSourceKey {
    /// Creates an element-owned panel-line source key.
    #[must_use]
    pub const fn element(element_index: usize, draw_ordinal: usize, line_ordinal: usize) -> Self {
        Self::Element {
            element_index,
            draw_ordinal,
            line_ordinal,
        }
    }

    /// Creates a post-layout source key.
    #[must_use]
    pub const fn external(producer: u64, source: u64, line_ordinal: usize) -> Self {
        Self::External {
            producer,
            source,
            line_ordinal,
        }
    }
}

impl PanelLinePrimitiveKey {
    /// Creates a primitive key under `line_source`.
    #[must_use]
    pub const fn new(line_source: PanelLineSourceKey, primitive_ordinal: usize) -> Self {
        Self {
            line_source,
            primitive_ordinal,
        }
    }
}

impl ResolvedPanelLine {
    /// Returns the stable line source key.
    #[must_use]
    pub const fn source_key(&self) -> PanelLineSourceKey { self.source_key }

    /// Returns the command index used when this line was resolved.
    #[must_use]
    pub const fn source_command_index(&self) -> usize { self.source_command_index }

    /// Returns the owning element bounds.
    #[must_use]
    pub const fn owner_bounds(&self) -> BoundingBox { self.owner_bounds }

    /// Returns the bounds covering all resolved primitives.
    #[must_use]
    pub const fn visual_bounds(&self) -> BoundingBox { self.visual_bounds }

    /// Returns the effective clip for this line.
    #[must_use]
    pub const fn clip(&self) -> Option<BoundingBox> { self.clip }

    /// Returns the resolved start tip after authored start inset.
    #[must_use]
    pub const fn start(&self) -> Vec2 { self.start }

    /// Returns the resolved end tip after authored end inset.
    #[must_use]
    pub const fn end(&self) -> Vec2 { self.end }

    /// Returns the resolved shaft start after cap inset.
    #[must_use]
    pub const fn shaft_start(&self) -> Vec2 { self.shaft_start }

    /// Returns the resolved shaft end after cap inset.
    #[must_use]
    pub const fn shaft_end(&self) -> Vec2 { self.shaft_end }

    /// Returns the resolved stroke width.
    #[must_use]
    pub const fn width(&self) -> f32 { self.width }

    /// Returns the resolved base line color.
    #[must_use]
    pub const fn color(&self) -> Color { self.color }

    /// Returns the authored hairline fade override, if any.
    #[must_use]
    pub const fn hairline_fade(&self) -> Option<HairlineFade> { self.hairline_fade }

    /// Returns shaft and cap primitives in stable part order.
    #[must_use]
    pub fn primitives(&self) -> &[ResolvedPanelLinePrimitive] { &self.primitives }
}

impl ResolvedPanelLinePrimitive {
    /// Returns the stable primitive source key.
    #[must_use]
    pub const fn source_key(&self) -> PanelLinePrimitiveKey { self.source_key }

    /// Returns the primitive kind.
    #[must_use]
    pub const fn kind(&self) -> PanelLinePrimitiveKind { self.kind }

    /// Returns the primitive geometry.
    #[must_use]
    pub const fn geometry(&self) -> PanelLinePrimitiveGeometry { self.geometry }

    /// Returns the resolved primitive color.
    #[must_use]
    pub const fn color(&self) -> Color { self.color }

    /// Returns this primitive's visual bounds.
    #[must_use]
    pub const fn bounds(&self) -> BoundingBox { self.bounds }

    /// Returns this primitive's effective clip.
    #[must_use]
    pub const fn clip(&self) -> Option<BoundingBox> { self.clip }

    /// Returns this primitive's stable order within the resolved line.
    #[must_use]
    pub const fn part_order(&self) -> usize { self.part_order }
}

impl PanelLineResolveContext {
    pub(crate) const fn new(
        owner_bounds: BoundingBox,
        inherited_clip: Option<BoundingBox>,
        clip_policy: PanelLineClipPolicy,
        source_command_index: usize,
        source_key: PanelLineSourceKey,
    ) -> Self {
        Self {
            owner_bounds,
            inherited_clip,
            clip_policy,
            source_command_index,
            source_key,
        }
    }
}

pub(crate) fn resolve_panel_line(
    line: &PanelLine,
    context: PanelLineResolveContext,
) -> Option<ResolvedPanelLine> {
    let width = positive_dimension(line.line_style().width_dimension())?;
    let cap_size = non_negative_dimension(line.line_style().cap_size_dimension())?;
    let start_inset = non_negative_dimension(line.start_inset_dimension())?;
    let end_inset = non_negative_dimension(line.end_inset_dimension())?;
    let raw_start = resolve_point(line.start(), context.owner_bounds)?;
    let raw_end = resolve_point(line.end(), context.owner_bounds)?;
    let raw_delta = raw_end - raw_start;
    if raw_delta.length_squared() <= MIN_LINE_LENGTH_SQUARED {
        return None;
    }

    let direction = raw_delta.normalize();
    let start = raw_start + direction * start_inset;
    let end = raw_end - direction * end_inset;

    let color = line.line_style().color_value();
    let start_cap = line.line_style().start_cap_value().resolved_primitives(
        cap_size,
        color,
        resolve_point_dimension,
    );
    let end_cap = line.line_style().end_cap_value().resolved_primitives(
        cap_size,
        color,
        resolve_point_dimension,
    );
    if !start_cap.shaft_inset.is_finite() || !end_cap.shaft_inset.is_finite() {
        return None;
    }

    let shaft_start = start + direction * start_cap.shaft_inset.max(0.0);
    let shaft_end = end - direction * end_cap.shaft_inset.max(0.0);
    let clip = match resolve_clip(
        context.owner_bounds,
        context.inherited_clip,
        context.clip_policy,
    ) {
        ResolvedLineClip::Visible(clip) => clip,
        ResolvedLineClip::FullyClipped => return None,
    };
    let mut primitives = Vec::new();
    if (shaft_end - shaft_start).dot(direction) > f32::EPSILON {
        push_segment_primitive(
            &mut primitives,
            context.source_key,
            shaft_start,
            shaft_end,
            width,
            color,
            clip,
        );
    }
    push_cap_primitives(
        &mut primitives,
        context.source_key,
        start,
        direction,
        &start_cap,
        width,
        clip,
    );
    push_cap_primitives(
        &mut primitives,
        context.source_key,
        end,
        -direction,
        &end_cap,
        width,
        clip,
    );

    let visual_bounds = union_primitive_bounds(&primitives)?;
    if let Some(clip_bounds) = clip
        && visual_bounds.intersect(&clip_bounds).is_none()
    {
        return None;
    }

    Some(ResolvedPanelLine {
        source_key: context.source_key,
        source_command_index: context.source_command_index,
        owner_bounds: context.owner_bounds,
        visual_bounds,
        clip,
        start,
        end,
        shaft_start,
        shaft_end,
        width,
        color,
        hairline_fade: line.line_style().hairline_fade_value(),
        primitives,
    })
}

fn resolve_point(point: &PanelPoint, owner_bounds: BoundingBox) -> Option<Vec2> {
    Some(Vec2::new(
        resolve_coord(point.x(), owner_bounds.x, owner_bounds.width)?,
        resolve_coord(point.y(), owner_bounds.y, owner_bounds.height)?,
    ))
}

fn resolve_coord(coord: &PanelCoord, origin: f32, size: f32) -> Option<f32> {
    if let Some(dimension) = coord.start_dimension() {
        return Some(origin + finite_dimension(dimension)?);
    }
    if let Some(dimension) = coord.end_dimension() {
        return Some(origin + size - finite_dimension(dimension)?);
    }
    let percent = coord.percent_value()?;
    if percent.is_finite() {
        Some(size.mul_add(percent, origin))
    } else {
        None
    }
}

fn resolve_clip(
    owner_bounds: BoundingBox,
    inherited_clip: Option<BoundingBox>,
    clip_policy: PanelLineClipPolicy,
) -> ResolvedLineClip {
    let clip = match clip_policy {
        PanelLineClipPolicy::OwnerBounds => match inherited_clip {
            Some(inherited_clip) => {
                let Some(intersection) = inherited_clip.intersect(&owner_bounds) else {
                    return ResolvedLineClip::FullyClipped;
                };
                intersection
            },
            None => owner_bounds,
        },
        PanelLineClipPolicy::Inherited => return ResolvedLineClip::Visible(inherited_clip),
    };
    ResolvedLineClip::Visible(Some(clip))
}

fn push_segment_primitive(
    primitives: &mut Vec<ResolvedPanelLinePrimitive>,
    line_source: PanelLineSourceKey,
    start: Vec2,
    end: Vec2,
    width: f32,
    color: Color,
    clip: Option<BoundingBox>,
) {
    let Some(bounds) = segment_bounds(start, end, width) else {
        return;
    };
    let primitive_ordinal = primitives.len();
    primitives.push(ResolvedPanelLinePrimitive {
        source_key: PanelLinePrimitiveKey::new(line_source, primitive_ordinal),
        kind: PanelLinePrimitiveKind::Segment,
        geometry: PanelLinePrimitiveGeometry::Segment { start, end, width },
        color,
        bounds,
        clip,
        part_order: primitive_ordinal,
    });
}

fn push_cap_primitives(
    primitives: &mut Vec<ResolvedPanelLinePrimitive>,
    line_source: PanelLineSourceKey,
    tip: Vec2,
    direction: Vec2,
    cap: &ResolvedCalloutCap,
    width: f32,
    clip: Option<BoundingBox>,
) {
    for primitive in cap.primitives() {
        push_cap_primitive(
            primitives,
            line_source,
            tip,
            direction,
            *primitive,
            width,
            clip,
        );
    }
}

fn push_cap_primitive(
    primitives: &mut Vec<ResolvedPanelLinePrimitive>,
    line_source: PanelLineSourceKey,
    tip: Vec2,
    direction: Vec2,
    primitive: ResolvedCalloutCapPrimitive,
    width: f32,
    clip: Option<BoundingBox>,
) {
    if !primitive.length.is_finite()
        || !primitive.width.is_finite()
        || primitive.length <= 0.0
        || primitive.width <= 0.0
    {
        return;
    }

    match primitive.kind {
        CalloutCapPrimitiveKind::OpenArrowWing(wing) => {
            let end = tip
                + direction * primitive.length
                + perp(direction) * primitive.width * wing.sign();
            push_segment_primitive(
                primitives,
                line_source,
                tip,
                end,
                width,
                primitive.color,
                clip,
            );
        },
        CalloutCapPrimitiveKind::Triangle => {
            let axis = -direction;
            push_form_primitive(
                primitives,
                line_source,
                PanelLinePrimitiveKind::Triangle,
                tip - axis * primitive.length,
                axis,
                Vec2::new(primitive.length, primitive.width),
                primitive.color,
                clip,
            );
        },
        CalloutCapPrimitiveKind::Circle => {
            push_centered_cap_form(
                primitives,
                line_source,
                PanelLinePrimitiveKind::Circle,
                tip,
                direction,
                primitive,
                clip,
            );
        },
        CalloutCapPrimitiveKind::Square => {
            push_centered_cap_form(
                primitives,
                line_source,
                PanelLinePrimitiveKind::Square,
                tip,
                direction,
                primitive,
                clip,
            );
        },
        CalloutCapPrimitiveKind::Diamond => {
            push_centered_cap_form(
                primitives,
                line_source,
                PanelLinePrimitiveKind::Diamond,
                tip,
                direction,
                primitive,
                clip,
            );
        },
    }
}

fn push_centered_cap_form(
    primitives: &mut Vec<ResolvedPanelLinePrimitive>,
    line_source: PanelLineSourceKey,
    kind: PanelLinePrimitiveKind,
    tip: Vec2,
    direction: Vec2,
    primitive: ResolvedCalloutCapPrimitive,
    clip: Option<BoundingBox>,
) {
    let half_size = Vec2::new(primitive.length * 0.5, primitive.width * 0.5);
    push_form_primitive(
        primitives,
        line_source,
        kind,
        tip - direction * half_size.x,
        direction,
        half_size,
        primitive.color,
        clip,
    );
}

fn push_form_primitive(
    primitives: &mut Vec<ResolvedPanelLinePrimitive>,
    line_source: PanelLineSourceKey,
    kind: PanelLinePrimitiveKind,
    center: Vec2,
    axis: Vec2,
    half_size: Vec2,
    color: Color,
    clip: Option<BoundingBox>,
) {
    if !center.is_finite() || !axis.is_finite() || !half_size.is_finite() {
        return;
    }
    let Some(axis) = axis.try_normalize() else {
        return;
    };
    let Some(bounds) = oriented_form_bounds(center, axis, half_size) else {
        return;
    };
    let primitive_ordinal = primitives.len();
    primitives.push(ResolvedPanelLinePrimitive {
        source_key: PanelLinePrimitiveKey::new(line_source, primitive_ordinal),
        kind,
        geometry: PanelLinePrimitiveGeometry::Form {
            center,
            axis,
            half_size,
        },
        color,
        bounds,
        clip,
        part_order: primitive_ordinal,
    });
}

fn segment_bounds(start: Vec2, end: Vec2, width: f32) -> Option<BoundingBox> {
    if !start.is_finite() || !end.is_finite() || !width.is_finite() || width <= 0.0 {
        return None;
    }
    let padding = width.mul_add(0.5, LINE_COVERAGE_PADDING);
    Some(BoundingBox {
        x:      start.x.min(end.x) - padding,
        y:      start.y.min(end.y) - padding,
        width:  (start.x - end.x).abs() + padding * 2.0,
        height: (start.y - end.y).abs() + padding * 2.0,
    })
}

fn oriented_form_bounds(center: Vec2, axis: Vec2, half_size: Vec2) -> Option<BoundingBox> {
    if half_size.x <= 0.0 || half_size.y <= 0.0 {
        return None;
    }
    let perp = perp(axis);
    let x_extent = perp
        .x
        .abs()
        .mul_add(half_size.y, axis.x.abs() * half_size.x)
        + LINE_COVERAGE_PADDING;
    let y_extent = perp
        .y
        .abs()
        .mul_add(half_size.y, axis.y.abs() * half_size.x)
        + LINE_COVERAGE_PADDING;
    Some(BoundingBox {
        x:      center.x - x_extent,
        y:      center.y - y_extent,
        width:  x_extent * 2.0,
        height: y_extent * 2.0,
    })
}

fn union_primitive_bounds(primitives: &[ResolvedPanelLinePrimitive]) -> Option<BoundingBox> {
    let mut iter = primitives.iter();
    let first = iter.next()?.bounds;
    Some(iter.fold(first, |bounds, primitive| {
        union_bounds(bounds, primitive.bounds)
    }))
}

fn union_bounds(a: BoundingBox, b: BoundingBox) -> BoundingBox {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (a.x + a.width).max(b.x + b.width);
    let y1 = (a.y + a.height).max(b.y + b.height);
    BoundingBox {
        x:      x0,
        y:      y0,
        width:  x1 - x0,
        height: y1 - y0,
    }
}

fn finite_dimension(dimension: Dimension) -> Option<f32> {
    if !dimension.value.is_finite() {
        return None;
    }
    let value = dimension.to_points(POINT_SPACE_SCALE);
    value.is_finite().then_some(value)
}

fn non_negative_dimension(dimension: Dimension) -> Option<f32> {
    let value = finite_dimension(dimension)?;
    (value >= 0.0).then_some(value)
}

fn positive_dimension(dimension: Dimension) -> Option<f32> {
    let value = finite_dimension(dimension)?;
    (value > 0.0).then_some(value)
}

fn resolve_point_dimension(dimension: Dimension) -> f32 { dimension.to_points(POINT_SPACE_SCALE) }

fn perp(direction: Vec2) -> Vec2 { Vec2::new(-direction.y, direction.x) }

impl LineStyle {
    /// Sets the stroke width.
    #[must_use]
    pub fn width(mut self, width: impl Into<Dimension>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the line color.
    #[must_use]
    pub const fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Sets the default cap size used by caps without an explicit override.
    #[must_use]
    pub fn cap_size(mut self, cap_size: impl Into<Dimension>) -> Self {
        self.cap_size = cap_size.into();
        self
    }

    /// Sets the cap at the start of the line.
    #[must_use]
    pub const fn start_cap(mut self, cap: CalloutCap) -> Self {
        self.start_cap = cap;
        self
    }

    /// Sets the cap at the end of the line.
    #[must_use]
    pub const fn end_cap(mut self, cap: CalloutCap) -> Self {
        self.end_cap = cap;
        self
    }

    /// Returns the stroke width.
    #[must_use]
    pub const fn width_dimension(&self) -> Dimension { self.width }

    /// Returns the line color.
    #[must_use]
    pub const fn color_value(&self) -> Color { self.color }

    /// Returns the default cap size.
    #[must_use]
    pub const fn cap_size_dimension(&self) -> Dimension { self.cap_size }

    /// Returns the start cap.
    #[must_use]
    pub const fn start_cap_value(&self) -> CalloutCap { self.start_cap }

    /// Returns the end cap.
    #[must_use]
    pub const fn end_cap_value(&self) -> CalloutCap { self.end_cap }

    /// Overrides the hairline fade policy for lines using this style.
    #[must_use]
    pub const fn hairline_fade(mut self, fade: HairlineFade) -> Self {
        self.hairline_fade = Some(fade);
        self
    }

    /// Returns the hairline fade override, if any.
    #[must_use]
    pub const fn hairline_fade_value(&self) -> Option<HairlineFade> { self.hairline_fade }

    pub(crate) fn scaled(&self, default_scale: f32) -> Self {
        Self {
            width:         scaled_dimension(self.width, default_scale),
            color:         self.color,
            cap_size:      scaled_dimension(self.cap_size, default_scale),
            start_cap:     self.start_cap.scaled_dimensions(default_scale),
            end_cap:       self.end_cap.scaled_dimensions(default_scale),
            hairline_fade: self.hairline_fade,
        }
    }
}

impl Default for LineStyle {
    fn default() -> Self {
        Self {
            width:         DEFAULT_LINE_WIDTH,
            color:         Color::WHITE,
            cap_size:      DEFAULT_CAP_SIZE,
            start_cap:     CalloutCap::None,
            end_cap:       CalloutCap::None,
            hairline_fade: None,
        }
    }
}

impl PanelPoint {
    /// Creates a point from X and Y coordinate components.
    #[must_use]
    pub fn new(x: impl Into<PanelCoord>, y: impl Into<PanelCoord>) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
        }
    }

    /// Returns the X coordinate component.
    #[must_use]
    pub const fn x(&self) -> &PanelCoord { &self.x }

    /// Returns the Y coordinate component.
    #[must_use]
    pub const fn y(&self) -> &PanelCoord { &self.y }

    pub(crate) fn scaled(&self, default_scale: f32) -> Self {
        Self {
            x: self.x.scaled(default_scale),
            y: self.y.scaled(default_scale),
        }
    }
}

impl<X, Y> From<(X, Y)> for PanelPoint
where
    X: Into<PanelCoord>,
    Y: Into<PanelCoord>,
{
    fn from((x, y): (X, Y)) -> Self { Self::new(x, y) }
}

impl PanelCoord {
    /// Creates a coordinate measured from the left or top edge.
    #[must_use]
    pub fn start(value: impl Into<Dimension>) -> Self {
        Self {
            kind: PanelCoordKind::Start(value.into()),
        }
    }

    /// Creates a coordinate measured inward from the right or bottom edge.
    #[must_use]
    pub fn end(value: impl Into<Dimension>) -> Self {
        Self {
            kind: PanelCoordKind::End(value.into()),
        }
    }

    /// Creates a percent coordinate.
    ///
    /// Percent values outside `0.0..=1.0` are allowed. Non-finite values
    /// resolve to `0.0`; use [`try_percent`](Self::try_percent) to reject them.
    #[must_use]
    pub const fn percent(value: f32) -> Self {
        if value.is_finite() {
            Self {
                kind: PanelCoordKind::Percent(value),
            }
        } else {
            Self {
                kind: PanelCoordKind::Percent(DEFAULT_PERCENT),
            }
        }
    }

    /// Creates a percent coordinate, rejecting non-finite values.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidPanelScalar`] when `value` is NaN or infinite.
    pub const fn try_percent(value: f32) -> Result<Self, InvalidPanelScalar> {
        if value.is_finite() {
            Ok(Self {
                kind: PanelCoordKind::Percent(value),
            })
        } else {
            Err(InvalidPanelScalar { value })
        }
    }

    /// Returns the start-edge dimension if this is a start coordinate.
    #[must_use]
    pub const fn start_dimension(&self) -> Option<Dimension> {
        match self.kind {
            PanelCoordKind::Start(value) => Some(value),
            PanelCoordKind::End(_) | PanelCoordKind::Percent(_) => None,
        }
    }

    /// Returns the end-edge dimension if this is an end coordinate.
    #[must_use]
    pub const fn end_dimension(&self) -> Option<Dimension> {
        match self.kind {
            PanelCoordKind::End(value) => Some(value),
            PanelCoordKind::Start(_) | PanelCoordKind::Percent(_) => None,
        }
    }

    /// Returns the percent value if this is a percent coordinate.
    #[must_use]
    pub const fn percent_value(&self) -> Option<f32> {
        match self.kind {
            PanelCoordKind::Percent(value) => Some(value),
            PanelCoordKind::Start(_) | PanelCoordKind::End(_) => None,
        }
    }

    pub(crate) fn scaled(&self, default_scale: f32) -> Self {
        match self.kind {
            PanelCoordKind::Start(value) => Self {
                kind: PanelCoordKind::Start(scaled_dimension(value, default_scale)),
            },
            PanelCoordKind::End(value) => Self {
                kind: PanelCoordKind::End(scaled_dimension(value, default_scale)),
            },
            PanelCoordKind::Percent(value) => Self {
                kind: PanelCoordKind::Percent(value),
            },
        }
    }
}

impl Default for PanelCoord {
    fn default() -> Self { Self::start(ZERO_DIMENSION) }
}

impl From<Dimension> for PanelCoord {
    fn from(value: Dimension) -> Self { Self::start(value) }
}

impl From<f32> for PanelCoord {
    fn from(value: f32) -> Self { Self::start(value) }
}

impl From<Pt> for PanelCoord {
    fn from(value: Pt) -> Self { Self::start(value) }
}

impl From<Mm> for PanelCoord {
    fn from(value: Mm) -> Self { Self::start(value) }
}

impl From<In> for PanelCoord {
    fn from(value: In) -> Self { Self::start(value) }
}

impl From<Px> for PanelCoord {
    fn from(value: Px) -> Self { Self::start(value) }
}

fn scaled_dimension(dimension: Dimension, default_scale: f32) -> Dimension {
    Dimension {
        value: dimension.to_points(default_scale),
        unit:  None,
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic when line resolution unexpectedly fails"
)]
mod tests {
    use bevy::color::Color;

    use super::DEFAULT_CAP_SIZE;
    use super::DEFAULT_LINE_WIDTH;
    use super::LineStyle;
    use super::PanelCoord;
    use super::PanelLine;
    use super::PanelLineClipPolicy;
    use super::PanelLinePrimitiveKind;
    use super::PanelLineResolveContext;
    use super::PanelLineSourceKey;
    use super::PanelPoint;
    use super::resolve_panel_line;
    use crate::BoundingBox;
    use crate::CalloutCap;
    use crate::Mm;

    #[test]
    fn percent_allows_overflow_values() {
        assert!(matches!(
            PanelCoord::try_percent(-0.25),
            Ok(coord) if coord.percent_value() == Some(-0.25)
        ));
        assert!(matches!(
            PanelCoord::try_percent(1.25),
            Ok(coord) if coord.percent_value() == Some(1.25)
        ));
    }

    #[test]
    fn try_percent_rejects_non_finite_values() {
        assert!(matches!(
            PanelCoord::try_percent(f32::NAN),
            Err(invalid) if invalid.value().is_nan()
        ));
        assert!(PanelCoord::try_percent(f32::INFINITY).is_err());
    }

    #[test]
    fn percent_defaults_non_finite_values_to_zero() {
        let fallback = PanelCoord::percent(f32::NEG_INFINITY);

        assert_eq!(fallback.percent_value(), Some(0.0));
    }

    #[test]
    fn dimension_inputs_default_to_start_coordinates() {
        let point = PanelPoint::new(Mm(5.0), 10.0);

        assert_eq!(point.x().start_dimension(), Some(Mm(5.0).into()));
        assert_eq!(point.y().start_dimension(), Some(10.0.into()));
    }

    #[test]
    fn line_style_default_uses_positive_no_cap_values() {
        let style = LineStyle::default();

        assert_eq!(style.width_dimension(), DEFAULT_LINE_WIDTH);
        assert!(style.width_dimension().value > 0.0);
        assert_eq!(style.color_value(), Color::WHITE);
        assert_eq!(style.cap_size_dimension(), DEFAULT_CAP_SIZE);
        assert!(style.cap_size_dimension().value > 0.0);
        assert_eq!(style.start_cap_value(), CalloutCap::None);
        assert_eq!(style.end_cap_value(), CalloutCap::None);
    }

    #[test]
    fn line_builder_sets_style_and_insets() {
        let line = PanelLine::new((0.0, 1.0), (PanelCoord::end(Mm(0.0)), 1.0))
            .width(Mm(0.3))
            .color(Color::BLACK)
            .cap_size(Mm(2.0))
            .start_cap(CalloutCap::circle())
            .end_cap(CalloutCap::diamond())
            .start_inset(Mm(0.4))
            .end_inset(Mm(0.5));

        assert_eq!(line.line_style().width_dimension(), Mm(0.3).into());
        assert_eq!(line.line_style().color_value(), Color::BLACK);
        assert_eq!(line.line_style().cap_size_dimension(), Mm(2.0).into());
        assert_eq!(line.line_style().start_cap_value(), CalloutCap::circle());
        assert_eq!(line.line_style().end_cap_value(), CalloutCap::diamond());
        assert_eq!(line.start_inset_dimension(), Mm(0.4).into());
        assert_eq!(line.end_inset_dimension(), Mm(0.5).into());
    }

    #[test]
    fn resolve_line_coordinates_from_owner_bounds() {
        let line = PanelLine::new(
            (PanelCoord::start(5.0), PanelCoord::percent(0.25)),
            (PanelCoord::end(-10.0), PanelCoord::percent(0.75)),
        );
        let resolved = resolve_panel_line(&line, context()).expect("line should resolve");

        assert_eq!(resolved.start, bevy::math::Vec2::new(15.0, 40.0));
        assert_eq!(resolved.end, bevy::math::Vec2::new(120.0, 80.0));
        assert_eq!(resolved.primitives.len(), 1);
    }

    #[test]
    fn resolve_line_clips_to_owner_and_inherited_clip() {
        let line = PanelLine::new((0.0, 0.0), (PanelCoord::end(0.0), 0.0));
        let resolved = resolve_panel_line(
            &line,
            PanelLineResolveContext::new(
                owner_bounds(),
                Some(BoundingBox {
                    x:      0.0,
                    y:      0.0,
                    width:  30.0,
                    height: 100.0,
                }),
                PanelLineClipPolicy::OwnerBounds,
                3,
                PanelLineSourceKey::element(1, 0, 0),
            ),
        )
        .expect("line should resolve");

        assert_eq!(
            resolved.clip,
            Some(BoundingBox {
                x:      10.0,
                y:      20.0,
                width:  20.0,
                height: 80.0,
            })
        );
    }

    #[test]
    fn visible_overflow_uses_inherited_clip() {
        let line = PanelLine::new((0.0, 0.0), (PanelCoord::end(-30.0), 0.0));
        let inherited = BoundingBox {
            x:      0.0,
            y:      0.0,
            width:  200.0,
            height: 200.0,
        };
        let resolved = resolve_panel_line(
            &line,
            PanelLineResolveContext::new(
                owner_bounds(),
                Some(inherited),
                PanelLineClipPolicy::Inherited,
                0,
                PanelLineSourceKey::element(1, 0, 0),
            ),
        )
        .expect("visible overflow line should resolve");

        assert_eq!(resolved.clip, Some(inherited));
        assert!(resolved.visual_bounds.x + resolved.visual_bounds.width > 110.0);
    }

    #[test]
    fn visible_overflow_without_inherited_clip_resolves_unclipped() {
        let line = PanelLine::new((0.0, 0.0), (PanelCoord::end(-30.0), 0.0));
        let resolved = resolve_panel_line(
            &line,
            PanelLineResolveContext::new(
                owner_bounds(),
                None,
                PanelLineClipPolicy::Inherited,
                0,
                PanelLineSourceKey::element(1, 0, 0),
            ),
        )
        .expect("unclipped visible overflow line should resolve");

        assert_eq!(resolved.clip, None);
        assert!(resolved.visual_bounds.x + resolved.visual_bounds.width > 110.0);
    }

    #[test]
    fn resolve_line_expands_caps_into_stable_primitives() {
        let line = PanelLine::new((0.0, 0.0), (40.0, 0.0))
            .width(2.0)
            .cap_size(4.0)
            .start_cap(CalloutCap::arrow().open())
            .end_cap(CalloutCap::circle());
        let resolved = resolve_panel_line(&line, context()).expect("line should resolve");

        assert_eq!(resolved.primitives.len(), 4);
        assert_eq!(resolved.primitives[0].kind, PanelLinePrimitiveKind::Segment);
        assert_eq!(resolved.primitives[1].kind, PanelLinePrimitiveKind::Segment);
        assert_eq!(resolved.primitives[2].kind, PanelLinePrimitiveKind::Segment);
        assert_eq!(resolved.primitives[3].kind, PanelLinePrimitiveKind::Circle);
        for (ordinal, primitive) in resolved.primitives.iter().enumerate() {
            assert_eq!(primitive.source_key.primitive_ordinal, ordinal);
            assert_eq!(primitive.part_order, ordinal);
        }
    }

    #[test]
    fn over_inset_line_collapses_shaft_but_keeps_caps() {
        let line = PanelLine::new((0.0, 0.0), (10.0, 0.0))
            .width(2.0)
            .cap_size(4.0)
            .start_inset(6.0)
            .end_inset(6.0)
            .start_cap(CalloutCap::circle())
            .end_cap(CalloutCap::circle());
        let resolved = resolve_panel_line(&line, context()).expect("caps should still resolve");

        assert_eq!(resolved.primitives.len(), 2);
        assert!(
            resolved
                .primitives
                .iter()
                .all(|primitive| { primitive.kind == PanelLinePrimitiveKind::Circle })
        );
    }

    #[test]
    fn resolve_line_rejects_non_finite_or_non_positive_scalars() {
        let width_nan = PanelLine::new((0.0, 0.0), (10.0, 0.0)).width(f32::NAN);
        let cap_nan = PanelLine::new((0.0, 0.0), (10.0, 0.0)).cap_size(f32::NAN);
        let zero_width = PanelLine::new((0.0, 0.0), (10.0, 0.0)).width(0.0);

        assert!(resolve_panel_line(&width_nan, context()).is_none());
        assert!(resolve_panel_line(&cap_nan, context()).is_none());
        assert!(resolve_panel_line(&zero_width, context()).is_none());
    }

    fn context() -> PanelLineResolveContext {
        PanelLineResolveContext::new(
            owner_bounds(),
            Some(BoundingBox {
                x:      0.0,
                y:      0.0,
                width:  200.0,
                height: 200.0,
            }),
            PanelLineClipPolicy::OwnerBounds,
            0,
            PanelLineSourceKey::element(1, 0, 0),
        )
    }

    const fn owner_bounds() -> BoundingBox {
        BoundingBox {
            x:      10.0,
            y:      20.0,
            width:  100.0,
            height: 80.0,
        }
    }
}
