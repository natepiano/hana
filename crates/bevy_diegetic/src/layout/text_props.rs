//! Text configuration types used by layout and standalone world text.

use std::hash::Hash;
use std::marker::PhantomData;

use bevy::color::Color;
use bevy::prelude::AlphaMode;
use bevy::prelude::Component;
use bevy::prelude::Reflect;

use super::Anchor;
use super::Dimension;
use super::FontFeatureFlags;
use super::FontFeatures;
use super::Unit;
use super::constants::DEFAULT_FONT_SIZE;

/// Controls how the layout engine breaks text across lines.
///
/// The engine splits text according to this mode and measures individual
/// runs via the [`MeasureTextFn`](crate::layout::MeasureTextFn) callback
/// to determine break points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum TextWrap {
    /// Break at word boundaries when text exceeds the element's width.
    ///
    /// Words are split on ASCII whitespace. The engine measures each word
    /// individually, accumulates widths on a line, and breaks when the
    /// next word would exceed the available width.
    #[default]
    Words,
    /// Break only at explicit `\n` characters.
    ///
    /// Each line between newlines is measured as a single run. The element's
    /// width is the widest line; height is the sum of all line heights.
    Newlines,
    /// Never wrap. The full text is measured as a single run and may
    /// overflow the element's bounds.
    None,
}

// ── Font property types ──────────────────────────────────────────────────────

/// Font weight (boldness) as a numeric value on the 1–1000 scale.
///
/// Standard weights: 100 (Thin) through 900 (Black). `400` is normal, `700` is bold.
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub struct FontWeight(pub f32);

impl FontWeight {
    /// Normal weight (400).
    pub const NORMAL: Self = Self(400.0);
    /// Bold weight (700).
    pub const BOLD: Self = Self(700.0);
    /// Light weight (300).
    pub const LIGHT: Self = Self(300.0);
}

impl Default for FontWeight {
    fn default() -> Self { Self::NORMAL }
}

/// Font slant (posture).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum FontSlant {
    /// Upright (roman) style.
    #[default]
    Normal,
    /// Italic style (true italic glyphs).
    Italic,
    /// Oblique style (slanted roman glyphs).
    Oblique,
}

/// Horizontal text alignment within bounds.
///
/// Used by [`TextProps<ForStandalone>`] for standalone text rendering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum TextAlign {
    /// Align to the left edge.
    #[default]
    Left,
    /// Center horizontally.
    Center,
    /// Align to the right edge.
    Right,
}

/// How the visible glyph renders.
///
/// Controls the text shader's coverage computation. Both modes use
/// `AlphaMode::Blend` for smooth anti-aliased edges. Discriminants are
/// `#[repr(u32)]` and explicit because they map directly to shader
/// constants in `slug_text.wgsl`; the compile-time assertions below keep
/// them in sync.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect)]
#[repr(u32)]
pub enum GlyphRenderMode {
    /// Normal text rendering — smooth alpha-blended edges.
    #[default]
    Text     = 1,
    /// Glyph quad filled everywhere except the letter outline (inverted alpha).
    PunchOut = 2,
}

impl GlyphRenderMode {
    #[must_use]
    const fn discriminant(self) -> u32 {
        match self {
            Self::Text => 1,
            Self::PunchOut => 2,
        }
    }
}

impl From<GlyphRenderMode> for u32 {
    fn from(render_mode: GlyphRenderMode) -> Self { render_mode.discriminant() }
}

/// Whether glyphs cast a shadow.
///
/// The visible glyph mesh casts its own coverage-silhouette shadow
/// directly. For a shadow with
/// no visible fill (ghost text), spawn a `Cast` glyph and set its fill
/// color alpha to `0`: the color pass paints nothing while the shadow
/// pass still writes the full letter silhouette.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect)]
pub enum GlyphShadowMode {
    /// Glyph casts no shadow.
    None,
    /// Glyph casts its coverage-silhouette shadow.
    #[default]
    Cast,
}

/// Whether glyph meshes render both faces or only the front face.
///
/// This only affects standalone [`WorldText`](crate::WorldText) rendering.
/// Layout text stores the value but does not use it directly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect)]
pub enum GlyphSidedness {
    /// Render both faces with no culling (default).
    #[default]
    DoubleSided,
    /// Render only the front face with back-face culling.
    OneSided,
}

/// Whether glyph materials respond to scene lighting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect)]
pub enum GlyphLighting {
    /// Use normal PBR lighting.
    #[default]
    Lit,
    /// Bypass PBR lighting and render with the authored material color.
    Unlit,
}

// ── Typestate markers ────────────────────────────────────────────────────────

/// Context marker: text properties for the layout engine.
///
/// [`TextProps<ForLayout>`] (aliased as `TextConfig`) exposes wrapping
/// controls but not color, alignment, or anchor.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Reflect)]
pub struct ForLayout;

/// Context marker: text properties for standalone 3D text rendering.
///
/// [`TextProps<ForStandalone>`] (aliased as `TextStyle`) exposes color,
/// alignment, and anchor but not wrapping.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Reflect)]
pub struct ForStandalone;

// ── TextProps<C> ─────────────────────────────────────────────────────────────

/// Type alias for layout engine text configuration.
pub type LayoutTextStyle = TextProps<ForLayout>;

/// Type alias for standalone text styling (Bevy `Component`).
pub type WorldTextStyle = TextProps<ForStandalone>;

/// Unified text properties parameterized by usage context.
///
/// `TextProps<ForLayout>` is used by the layout engine for measurement
/// and wrapping. `TextProps<ForStandalone>` is used as a Bevy `Component`
/// for standalone `Text3d` entities.
///
/// All fields are private. Context-appropriate builder methods expose
/// only the fields that make sense for each context. Shared measurement
/// fields are accessible on both via the `impl<C>` block.
///
/// ```ignore
/// // Layout (aliased as `TextConfig`):
/// TextConfig::new(14.0).with_font(FontId::MONOSPACE.0).bold().no_wrap()
///
/// // Standalone (aliased as `TextStyle`):
/// TextStyle::new(24.0).with_font(FontId::MONOSPACE.0).bold().with_color(Color::RED)
/// ```
#[derive(Component, Clone, Debug, Reflect)]
pub struct TextProps<C: Send + Sync + 'static> {
    font_id:        u16,
    size:           f32,
    weight:         FontWeight,
    slant:          FontSlant,
    line_height:    f32,
    letter_spacing: f32,
    word_spacing:   f32,
    wrap:           TextWrap,
    color:          Color,
    align:          TextAlign,
    anchor:         Anchor,
    render_mode:    GlyphRenderMode,
    shadow_mode:    GlyphShadowMode,
    sidedness:      GlyphSidedness,
    lighting:       GlyphLighting,
    font_features:  FontFeatures,
    /// What unit `size` is expressed in. `None` = inherit from contextual config.
    /// Only [`ForLayout`] exposes this as authoring input; standalone font unit
    /// authoring lives in the cascade.
    unit:           Option<Unit>,
    /// Explicit meters-per-design-unit override. `None` = derive from `unit`.
    /// Only meaningful for [`ForStandalone`] — a post-cascade bypass applied by
    /// the renderer, independent of `Resolved<FontUnit>`.
    world_scale:    Option<f32>,
    /// Per-style alpha-mode override. `None` = inherit.
    /// Only [`ForLayout`] exposes this as authoring input; standalone alpha
    /// authoring lives in the cascade.
    alpha_mode:     Option<AlphaMode>,
    #[reflect(ignore)]
    context:        PhantomData<C>,
}

impl<C: Send + Sync + 'static> PartialEq for TextProps<C> {
    fn eq(&self, other: &Self) -> bool {
        self.font_id == other.font_id
            && self.size == other.size
            && self.weight == other.weight
            && self.slant == other.slant
            && self.line_height == other.line_height
            && self.letter_spacing == other.letter_spacing
            && self.word_spacing == other.word_spacing
            && self.wrap == other.wrap
            && self.color == other.color
            && self.align == other.align
            && self.anchor == other.anchor
            && self.render_mode == other.render_mode
            && self.shadow_mode == other.shadow_mode
            && self.sidedness == other.sidedness
            && self.lighting == other.lighting
            && self.font_features == other.font_features
            && self.unit == other.unit
            && self.world_scale == other.world_scale
            && self.alpha_mode == other.alpha_mode
    }
}

// ── Shared methods (both contexts) ───────────────────────────────────────────

impl<C: Send + Sync + 'static> TextProps<C> {
    /// Returns the font identifier.
    #[must_use]
    pub const fn font_id(&self) -> u16 { self.font_id }

    /// Returns the font size in layout units.
    #[must_use]
    pub const fn size(&self) -> f32 { self.size }

    /// Returns the font weight.
    #[must_use]
    pub const fn weight(&self) -> FontWeight { self.weight }

    /// Returns the font slant.
    #[must_use]
    pub const fn slant(&self) -> FontSlant { self.slant }

    /// Returns the line height in layout units (0.0 = use `size`).
    #[must_use]
    pub const fn line_height_raw(&self) -> f32 { self.line_height }

    /// Returns the letter spacing in layout units.
    #[must_use]
    pub const fn letter_spacing(&self) -> f32 { self.letter_spacing }

    /// Returns the word spacing in layout units.
    #[must_use]
    pub const fn word_spacing(&self) -> f32 { self.word_spacing }

    /// Sets the font identifier.
    #[must_use]
    pub const fn with_font(mut self, font_id: u16) -> Self {
        self.font_id = font_id;
        self
    }

    /// Sets the font weight.
    #[must_use]
    pub const fn with_weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    /// Shorthand for `with_weight(FontWeight::BOLD)`.
    #[must_use]
    pub const fn bold(mut self) -> Self {
        self.weight = FontWeight::BOLD;
        self
    }

    /// Sets the font slant.
    #[must_use]
    pub const fn with_slant(mut self, slant: FontSlant) -> Self {
        self.slant = slant;
        self
    }

    /// Shorthand for `with_slant(FontSlant::Italic)`.
    #[must_use]
    pub const fn italic(mut self) -> Self {
        self.slant = FontSlant::Italic;
        self
    }

    /// Sets the line height in layout units. `0.0` = use `size`.
    #[must_use]
    pub const fn with_line_height(mut self, line_height: f32) -> Self {
        self.line_height = line_height;
        self
    }

    /// Sets extra spacing between characters in layout units.
    #[must_use]
    pub const fn with_letter_spacing(mut self, spacing: f32) -> Self {
        self.letter_spacing = spacing;
        self
    }

    /// Sets extra spacing between words in layout units.
    #[must_use]
    pub const fn with_word_spacing(mut self, spacing: f32) -> Self {
        self.word_spacing = spacing;
        self
    }

    /// Returns the text color.
    #[must_use]
    pub const fn color(&self) -> Color { self.color }

    /// Sets the text color.
    #[must_use]
    pub const fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Returns the glyph render mode.
    #[must_use]
    pub const fn render_mode(&self) -> GlyphRenderMode { self.render_mode }

    /// Sets the glyph render mode.
    #[must_use]
    pub const fn with_render_mode(mut self, mode: GlyphRenderMode) -> Self {
        self.render_mode = mode;
        self
    }

    /// Returns the glyph shadow mode.
    #[must_use]
    pub const fn shadow_mode(&self) -> GlyphShadowMode { self.shadow_mode }

    /// Sets the glyph shadow mode.
    #[must_use]
    pub const fn with_shadow_mode(mut self, mode: GlyphShadowMode) -> Self {
        self.shadow_mode = mode;
        self
    }

    /// Returns whether glyph meshes render one or both faces.
    #[must_use]
    pub const fn sidedness(&self) -> GlyphSidedness { self.sidedness }

    /// Returns whether the glyph material responds to scene lighting.
    #[must_use]
    pub const fn lighting(&self) -> GlyphLighting { self.lighting }

    /// Returns the font feature overrides.
    #[must_use]
    pub const fn font_features(&self) -> FontFeatures { self.font_features }

    /// Sets font feature overrides.
    #[must_use]
    pub const fn with_font_features(mut self, features: FontFeatures) -> Self {
        self.font_features = features;
        self
    }

    /// Disables contextual alternates (`calt`).
    #[must_use]
    pub const fn without_contextual_alternates(mut self) -> Self {
        self.font_features = self.font_features.without(FontFeatureFlags::CALT);
        self
    }

    /// Disables standard ligatures (`liga`).
    #[must_use]
    pub const fn without_ligatures(mut self) -> Self {
        self.font_features = self.font_features.without(FontFeatureFlags::LIGA);
        self
    }

    /// Hashes all layout-affecting fields into `hasher`, excluding color.
    ///
    /// Uses exhaustive destructuring so that adding a new field to
    /// [`TextProps`] without updating this method is a compiler error.
    pub fn hash_layout(&self, hasher: &mut impl std::hash::Hasher) {
        // Destructure exhaustively — compiler error if a field is added.
        let Self {
            font_id,
            size,
            weight,
            slant,
            line_height,
            letter_spacing,
            word_spacing,
            wrap,
            align,
            anchor,
            font_features,
            // Render-only — explicitly skipped.
            color: _,
            render_mode: _,
            shadow_mode: _,
            sidedness: _,
            lighting: _,
            // Standalone-only — not relevant for layout shaping cache.
            unit: _,
            world_scale: _,
            // Render-only — affects compositing, not shaping.
            alpha_mode: _,
            context: _,
        } = self;

        font_id.hash(hasher);
        size.to_bits().hash(hasher);
        weight.0.to_bits().hash(hasher);
        (*slant as u8).hash(hasher);
        line_height.to_bits().hash(hasher);
        letter_spacing.to_bits().hash(hasher);
        word_spacing.to_bits().hash(hasher);
        (*wrap as u8).hash(hasher);
        (*align as u8).hash(hasher);
        (*anchor as u8).hash(hasher);
        font_features.hash(hasher);
    }

    /// Returns whether layout-affecting text fields match, ignoring fields
    /// that only affect rendering.
    pub(super) fn layout_eq_excluding_visuals(&self, other: &Self) -> bool {
        let Self {
            font_id,
            size,
            weight,
            slant,
            line_height,
            letter_spacing,
            word_spacing,
            wrap,
            align,
            anchor,
            font_features,
            unit,
            world_scale,
            // Render-only.
            color: _,
            render_mode: _,
            shadow_mode: _,
            sidedness: _,
            lighting: _,
            alpha_mode: _,
            context: _,
        } = self;

        *font_id == other.font_id
            && size.to_bits() == other.size.to_bits()
            && weight.0.to_bits() == other.weight.0.to_bits()
            && *slant == other.slant
            && line_height.to_bits() == other.line_height.to_bits()
            && letter_spacing.to_bits() == other.letter_spacing.to_bits()
            && word_spacing.to_bits() == other.word_spacing.to_bits()
            && *wrap == other.wrap
            && *align == other.align
            && *anchor == other.anchor
            && *font_features == other.font_features
            && *unit == other.unit
            && *world_scale == other.world_scale
    }

    /// Bit-equality over the fields a panel-text glyph mesh and material depend
    /// on, used to gate per-run rebuilds.
    ///
    /// Compares the measurement fields (`font_id`, `size`, `weight`, `slant`,
    /// `line_height`, letter/word spacing, `wrap`, `align`, `anchor`,
    /// `font_features`) via `to_bits`, plus the render fields baked into the
    /// mesh and material (`color`, `render_mode`, `shadow_mode`, `sidedness`,
    /// `lighting`).
    /// Excludes `unit`/`world_scale` (measurement context, not mesh inputs) and
    /// `alpha_mode` (gated separately through `Override<TextAlpha>`).
    pub(crate) fn gating_eq(&self, other: &Self) -> bool {
        let Self {
            font_id,
            size,
            weight,
            slant,
            line_height,
            letter_spacing,
            word_spacing,
            wrap,
            align,
            anchor,
            font_features,
            color,
            render_mode,
            shadow_mode,
            sidedness,
            lighting,
            unit: _,
            world_scale: _,
            alpha_mode: _,
            context: _,
        } = self;

        *font_id == other.font_id
            && size.to_bits() == other.size.to_bits()
            && weight.0.to_bits() == other.weight.0.to_bits()
            && *slant == other.slant
            && line_height.to_bits() == other.line_height.to_bits()
            && letter_spacing.to_bits() == other.letter_spacing.to_bits()
            && word_spacing.to_bits() == other.word_spacing.to_bits()
            && *wrap == other.wrap
            && *align == other.align
            && *anchor == other.anchor
            && *font_features == other.font_features
            && *color == other.color
            && *render_mode == other.render_mode
            && *shadow_mode == other.shadow_mode
            && *sidedness == other.sidedness
            && *lighting == other.lighting
    }

    /// Extracts measurement-relevant fields as a [`TextMeasure`].
    ///
    /// Used by [`MeasureTextFn`](crate::layout::MeasureTextFn) — no generic
    /// parameter, no infection into the layout engine.
    #[must_use]
    pub const fn as_measure(&self) -> TextMeasure {
        TextMeasure {
            font_id:        self.font_id,
            size:           self.size,
            weight:         self.weight,
            slant:          self.slant,
            line_height:    self.line_height,
            letter_spacing: self.letter_spacing,
            word_spacing:   self.word_spacing,
            font_features:  self.font_features,
        }
    }
}

// ── Layout-only methods ──────────────────────────────────────────────────────

impl TextProps<ForLayout> {
    /// Creates a new layout config with the given font size.
    ///
    /// Accepts [`Pt`](crate::Pt), [`Mm`](crate::Mm), [`In`](crate::In),
    /// or bare `f32`. Newtypes carry their unit — bare `f32` uses the
    /// panel's `font_unit`.
    ///
    /// Defaults to word wrapping, normal weight, normal slant.
    #[must_use]
    pub fn new(size: impl Into<Dimension>) -> Self {
        let font_size = size.into();
        Self {
            font_id:        0,
            size:           font_size.value,
            weight:         FontWeight::NORMAL,
            slant:          FontSlant::Normal,
            line_height:    0.0,
            letter_spacing: 0.0,
            word_spacing:   0.0,
            wrap:           TextWrap::Words,
            color:          Color::WHITE,
            align:          TextAlign::Left,
            anchor:         Anchor::TopLeft,
            render_mode:    GlyphRenderMode::Text,
            shadow_mode:    GlyphShadowMode::Cast,
            sidedness:      GlyphSidedness::DoubleSided,
            lighting:       GlyphLighting::Lit,
            font_features:  FontFeatures::NONE,
            unit:           font_size.unit,
            world_scale:    None,
            alpha_mode:     None,
            context:        PhantomData,
        }
    }

    /// Returns the per-label unit override, if set.
    #[must_use]
    pub const fn unit(&self) -> Option<Unit> { self.unit }

    /// Returns the per-label alpha-mode override, if any.
    ///
    /// This is authoring input for the panel-text reconciler. `None` means the
    /// label inherits the panel-level override, then `CascadeDefault<TextAlpha>`.
    #[must_use]
    pub const fn alpha_mode(&self) -> Option<AlphaMode> { self.alpha_mode }

    /// Sets the per-label text [`AlphaMode`] override.
    ///
    /// The panel-text reconciler captures this value before converting to
    /// [`WorldTextStyle`] and inserts `Override<TextAlpha>` on the label.
    #[must_use]
    pub const fn with_alpha_mode(mut self, alpha_mode: AlphaMode) -> Self {
        self.alpha_mode = Some(alpha_mode);
        self
    }

    /// Returns the text wrapping mode.
    #[must_use]
    pub const fn wrap_mode(&self) -> TextWrap { self.wrap }

    /// Sets the text wrapping mode.
    #[must_use]
    pub const fn wrap(mut self, mode: TextWrap) -> Self {
        self.wrap = mode;
        self
    }

    /// Disables text wrapping (text may overflow the element).
    #[must_use]
    pub const fn no_wrap(mut self) -> Self {
        self.wrap = TextWrap::None;
        self
    }

    /// Returns a copy with font-related dimensions multiplied by `factor`.
    ///
    /// Used by the layout engine to convert font sizes from font units to
    /// layout units in render commands. Non-dimensional fields (color, wrap
    /// mode, font features, etc.) are preserved unchanged.
    #[must_use]
    pub fn scaled(&self, factor: f32) -> Self {
        let mut copy = self.clone();
        copy.size *= factor;
        copy.line_height *= factor;
        copy.letter_spacing *= factor;
        copy.word_spacing *= factor;
        copy
    }

    /// Converts to a `TextStyle` for use with standalone [`WorldText`] entities.
    ///
    /// Copies shared render and measurement fields. The standalone-specific
    /// `anchor` defaults to [`Anchor::Center`] since panel text is positioned by
    /// the layout engine rather than by an anchor offset. Unit and alpha
    /// authoring stay on the layout style and are routed separately through the
    /// cascade.
    #[must_use]
    pub const fn as_standalone(&self) -> TextProps<ForStandalone> {
        TextProps::<ForStandalone> {
            font_id:        self.font_id,
            size:           self.size,
            weight:         self.weight,
            slant:          self.slant,
            line_height:    self.line_height,
            letter_spacing: self.letter_spacing,
            word_spacing:   self.word_spacing,
            wrap:           TextWrap::None,
            color:          self.color,
            align:          TextAlign::Left,
            anchor:         Anchor::TopLeft,
            render_mode:    self.render_mode,
            shadow_mode:    self.shadow_mode,
            sidedness:      self.sidedness,
            lighting:       self.lighting,
            font_features:  self.font_features,
            unit:           None,
            world_scale:    None,
            alpha_mode:     None,
            context:        PhantomData,
        }
    }
}

impl Default for TextProps<ForLayout> {
    fn default() -> Self { Self::new(DEFAULT_FONT_SIZE) }
}

// ── Standalone-only methods ──────────────────────────────────────────────────

impl TextProps<ForStandalone> {
    /// Creates a new style with the given font size.
    ///
    /// The size is interpreted in the entity's resolved [`FontUnit`](crate::FontUnit).
    /// Use `override_font_unit` on the entity or `CascadeDefault<FontUnit>` for
    /// unit choice; standalone construction does not accept unit-bearing
    /// newtypes.
    ///
    /// Defaults to centered anchor, white color, normal weight.
    #[must_use]
    pub const fn new(size: f32) -> Self {
        Self {
            font_id: 0,
            size,
            weight: FontWeight::NORMAL,
            slant: FontSlant::Normal,
            line_height: 0.0,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            wrap: TextWrap::None,
            color: Color::WHITE,
            align: TextAlign::Left,
            anchor: Anchor::Center,
            render_mode: GlyphRenderMode::Text,
            shadow_mode: GlyphShadowMode::Cast,
            sidedness: GlyphSidedness::DoubleSided,
            lighting: GlyphLighting::Lit,
            font_features: FontFeatures::NONE,
            unit: None,
            world_scale: None,
            alpha_mode: None,
            context: PhantomData,
        }
    }

    /// Sets the font identifier.
    pub const fn set_font_id(&mut self, font_id: u16) { self.font_id = font_id; }

    /// Sets the font size.
    pub const fn set_size(&mut self, size: f32) { self.size = size; }

    /// Sets the font weight.
    pub const fn set_weight(&mut self, weight: FontWeight) { self.weight = weight; }

    /// Sets the font slant.
    pub const fn set_slant(&mut self, slant: FontSlant) { self.slant = slant; }

    /// Sets the line height in layout units. `0.0` = use `size`.
    pub const fn set_line_height(&mut self, line_height: f32) { self.line_height = line_height; }

    /// Sets extra spacing between characters in layout units.
    pub const fn set_letter_spacing(&mut self, spacing: f32) { self.letter_spacing = spacing; }

    /// Sets extra spacing between words in layout units.
    pub const fn set_word_spacing(&mut self, spacing: f32) { self.word_spacing = spacing; }

    /// Sets the text color.
    pub const fn set_color(&mut self, color: Color) { self.color = color; }

    /// Returns the text alignment.
    #[must_use]
    pub const fn text_align(&self) -> TextAlign { self.align }

    /// Returns the anchor point.
    #[must_use]
    pub const fn anchor(&self) -> Anchor { self.anchor }

    /// Sets horizontal text alignment within bounds.
    #[must_use]
    pub const fn with_align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }

    /// Sets horizontal text alignment within bounds.
    pub const fn set_align(&mut self, align: TextAlign) { self.align = align; }

    /// Sets the anchor point within the text block's bounding box.
    #[must_use]
    pub const fn with_anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// Sets the anchor point within the text block's bounding box.
    pub const fn set_anchor(&mut self, anchor: Anchor) { self.anchor = anchor; }

    /// Sets the glyph render mode.
    pub const fn set_render_mode(&mut self, mode: GlyphRenderMode) { self.render_mode = mode; }

    /// Sets the glyph shadow mode.
    pub const fn set_shadow_mode(&mut self, mode: GlyphShadowMode) { self.shadow_mode = mode; }

    /// Sets whether glyph meshes render one or both faces.
    #[must_use]
    pub const fn with_sidedness(mut self, sidedness: GlyphSidedness) -> Self {
        self.sidedness = sidedness;
        self
    }

    /// Sets whether glyph meshes render one or both faces.
    pub const fn set_sidedness(&mut self, sidedness: GlyphSidedness) { self.sidedness = sidedness; }

    /// Sets whether the glyph material responds to scene lighting.
    #[must_use]
    pub const fn with_lighting(mut self, lighting: GlyphLighting) -> Self {
        self.lighting = lighting;
        self
    }

    /// Sets whether the glyph material responds to scene lighting.
    pub const fn set_lighting(&mut self, lighting: GlyphLighting) { self.lighting = lighting; }

    /// Sets the glyph material to render unlit, bypassing PBR lighting.
    #[must_use]
    pub const fn with_unlit(mut self) -> Self {
        self.lighting = GlyphLighting::Unlit;
        self
    }

    /// Sets an explicit meters-per-design-unit override.
    ///
    /// When set, this value is used directly instead of deriving it from
    /// the unit. Use this when you need a specific physical scale that
    /// doesn't correspond to a standard [`Unit`].
    #[must_use]
    pub const fn with_world_scale(mut self, meters_per_unit: f32) -> Self {
        self.world_scale = Some(meters_per_unit);
        self
    }

    /// Sets or clears the explicit meters-per-design-unit override.
    pub const fn set_world_scale(&mut self, meters_per_unit: Option<f32>) {
        self.world_scale = meters_per_unit;
    }

    /// Returns the per-entity world scale override, if set.
    #[must_use]
    pub const fn world_scale(&self) -> Option<f32> { self.world_scale }

    /// Sets font feature overrides.
    pub const fn set_font_features(&mut self, features: FontFeatures) {
        self.font_features = features;
    }
}

impl Default for TextProps<ForStandalone> {
    fn default() -> Self { Self::new(DEFAULT_FONT_SIZE) }
}

impl TextProps<ForStandalone> {
    /// Converts to a `TextConfig` for use with the shaping/rendering pipeline.
    ///
    /// Copies shared measurement and render fields into a layout config for
    /// shaping. The layout-specific `wrap` field is set to [`TextWrap::None`].
    /// Unit and alpha default to inherit because standalone authoring for those
    /// properties lives in the cascade.
    #[must_use]
    pub const fn as_layout_config(&self) -> TextProps<ForLayout> {
        TextProps::<ForLayout> {
            font_id:        self.font_id,
            size:           self.size,
            weight:         self.weight,
            slant:          self.slant,
            line_height:    self.line_height,
            letter_spacing: self.letter_spacing,
            word_spacing:   self.word_spacing,
            wrap:           TextWrap::None,
            color:          self.color,
            align:          TextAlign::Left,
            anchor:         Anchor::Center,
            render_mode:    self.render_mode,
            shadow_mode:    self.shadow_mode,
            sidedness:      self.sidedness,
            lighting:       self.lighting,
            font_features:  self.font_features,
            unit:           None,
            world_scale:    None,
            alpha_mode:     None,
            context:        PhantomData,
        }
    }
}

// ── TextMeasure ──────────────────────────────────────────────────────────────

/// The subset of text properties needed for measurement.
///
/// Extracted from [`TextProps<C>`] via [`as_measure()`](TextProps::as_measure).
/// This is what [`MeasureTextFn`](crate::layout::MeasureTextFn) receives — no
/// generic parameter, no infection into the layout engine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextMeasure {
    /// Font identifier.
    pub font_id:        u16,
    /// Font size in layout units.
    pub size:           f32,
    /// Font weight.
    pub weight:         FontWeight,
    /// Font slant.
    pub slant:          FontSlant,
    /// Line height (0.0 = use `size`).
    pub line_height:    f32,
    /// Letter spacing in layout units.
    pub letter_spacing: f32,
    /// Word spacing in layout units.
    pub word_spacing:   f32,
    /// OpenType feature overrides.
    pub font_features:  FontFeatures,
}

impl TextMeasure {
    /// Returns a copy with font-related dimensions multiplied by `factor`.
    ///
    /// Used by the layout engine to convert font sizes from font units to
    /// layout units when the two differ (e.g. points → millimeters).
    #[must_use]
    pub fn scaled(mut self, factor: f32) -> Self {
        self.size *= factor;
        self.line_height *= factor;
        self.letter_spacing *= factor;
        self.word_spacing *= factor;
        self
    }
}

/// Measured dimensions of a text string.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextDimensions {
    /// Width in layout units.
    pub width:       f32,
    /// Height in layout units.
    pub height:      f32,
    /// Per-line height from parley (includes font's natural line gap
    /// when no explicit override is set).
    pub line_height: f32,
}

// ── Shader discriminant assertions ──────────────────────────────────────────
//
// These compile-time assertions ensure that `GlyphRenderMode` discriminants
// stay in sync with the `render_mode` constants in `slug_text.wgsl` (and the
// matching `RenderMode` variants). If you add or reorder variants, update
// the shader constants to match and adjust these assertions.

const _: () = assert!(GlyphRenderMode::Text.discriminant() == 1);
const _: () = assert!(GlyphRenderMode::PunchOut.discriminant() == 2);

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests use expect for clearer failure messages"
)]
mod tests {
    use super::*;
    use crate::layout::BoundingBox;

    #[test]
    fn as_standalone_from_layout_preserves_size() {
        let layout = TextProps::<ForLayout>::new(24.0);
        let standalone = layout.as_standalone();
        assert!(
            (standalone.size() - 24.0).abs() < f32::EPSILON,
            "size should be preserved"
        );
    }

    #[test]
    fn as_standalone_from_layout_defaults_to_top_left_anchor() {
        // Panel text is positioned by layout coordinates, so the converted
        // standalone text must start at the command origin.
        let layout = TextProps::<ForLayout>::new(12.0);
        let standalone = layout.as_standalone();
        assert_eq!(
            standalone.anchor(),
            Anchor::TopLeft,
            "as_standalone() should default to TopLeft for layout-positioned text"
        );
    }

    #[test]
    fn with_anchor_overrides_default() {
        let layout = TextProps::<ForLayout>::new(12.0);
        let standalone = layout.as_standalone().with_anchor(Anchor::TopLeft);
        assert_eq!(standalone.anchor(), Anchor::TopLeft);
    }

    // ── TextProps::gating_eq ───────────────────────────────────────

    #[test]
    fn gating_eq_true_for_identical_style() {
        let style = TextProps::<ForStandalone>::new(24.0).with_color(Color::WHITE);
        assert!(style.gating_eq(&style.clone()));
    }

    #[test]
    fn gating_eq_detects_size_change() {
        let base = TextProps::<ForStandalone>::new(24.0);
        let bigger = TextProps::<ForStandalone>::new(48.0);
        assert!(!base.gating_eq(&bigger));
    }

    #[test]
    fn gating_eq_detects_color_change() {
        // color is render-only for measurement, so layout_eq_excluding_visuals
        // ignores it — but the mesh material bakes it in, so gating_eq must not.
        let base = TextProps::<ForStandalone>::new(24.0).with_color(Color::WHITE);
        let recolored = base.clone().with_color(Color::BLACK);
        assert!(base.layout_eq_excluding_visuals(&recolored));
        assert!(!base.gating_eq(&recolored));
    }

    #[test]
    fn as_standalone_drops_layout_unit_and_alpha_authoring() {
        let standalone = TextProps::<ForLayout>::new(crate::Pt(24.0))
            .with_alpha_mode(AlphaMode::Add)
            .as_standalone();

        assert_eq!(
            standalone.unit, None,
            "unit authoring stays on LayoutTextStyle and routes through FontUnit"
        );
        assert_eq!(
            standalone.alpha_mode, None,
            "alpha authoring stays on LayoutTextStyle and routes through TextAlpha"
        );
    }

    #[test]
    fn gating_eq_ignores_world_scale() {
        let base = TextProps::<ForStandalone>::new(24.0);
        let scaled = base.clone().with_world_scale(0.01);
        assert!(base.gating_eq(&scaled));
    }

    #[test]
    fn gating_eq_distinguishes_signed_zero() {
        // to_bits, not ==: +0.0 and -0.0 are distinct bit patterns, matching
        // the layout layer's own comparison.
        let positive = TextProps::<ForStandalone>::new(24.0).with_line_height(0.0);
        let negative = TextProps::<ForStandalone>::new(24.0).with_line_height(-0.0);
        assert!(!positive.gating_eq(&negative));
    }

    #[test]
    fn anchor_offset_top_left_is_zero() {
        let (x, y) = Anchor::TopLeft.offset(100.0, 50.0);
        assert!((x).abs() < f32::EPSILON);
        assert!((y).abs() < f32::EPSILON);
    }

    #[test]
    fn anchor_offset_center_is_half() {
        let (x, y) = Anchor::Center.offset(100.0, 50.0);
        assert!((x - 50.0).abs() < f32::EPSILON);
        assert!((y - 25.0).abs() < f32::EPSILON);
    }

    #[test]
    fn anchor_offset_bottom_right_is_full() {
        let (x, y) = Anchor::BottomRight.offset(100.0, 50.0);
        assert!((x - 100.0).abs() < f32::EPSILON);
        assert!((y - 50.0).abs() < f32::EPSILON);
    }

    // ── BoundingBox::intersect ─────────────────────────────────────

    fn bbox(x: f32, y: f32, width: f32, height: f32) -> BoundingBox {
        BoundingBox {
            x,
            y,
            width,
            height,
        }
    }

    fn approx_eq(a: &BoundingBox, b: &BoundingBox) -> bool {
        (a.x - b.x).abs() < f32::EPSILON
            && (a.y - b.y).abs() < f32::EPSILON
            && (a.width - b.width).abs() < f32::EPSILON
            && (a.height - b.height).abs() < f32::EPSILON
    }

    #[test]
    fn intersect_overlapping_boxes() {
        let a = bbox(0.0, 0.0, 10.0, 10.0);
        let b = bbox(5.0, 5.0, 10.0, 10.0);
        let result = a.intersect(&b).expect("should overlap");
        assert!(approx_eq(&result, &bbox(5.0, 5.0, 5.0, 5.0)));
    }

    #[test]
    fn intersect_contained_box() {
        let outer = bbox(0.0, 0.0, 100.0, 100.0);
        let inner = bbox(10.0, 20.0, 30.0, 40.0);
        let result = outer.intersect(&inner).expect("should overlap");
        assert!(approx_eq(&result, &inner));
    }

    #[test]
    fn intersect_disjoint_boxes() {
        let a = bbox(0.0, 0.0, 10.0, 10.0);
        let b = bbox(20.0, 20.0, 10.0, 10.0);
        assert!(a.intersect(&b).is_none());
    }

    #[test]
    fn intersect_touching_edges() {
        let a = bbox(0.0, 0.0, 10.0, 10.0);
        let b = bbox(10.0, 0.0, 10.0, 10.0);
        assert!(a.intersect(&b).is_none());
    }

    #[test]
    fn intersect_zero_size_box() {
        let a = bbox(5.0, 5.0, 0.0, 0.0);
        let b = bbox(0.0, 0.0, 10.0, 10.0);
        assert!(a.intersect(&b).is_none());
    }

    #[test]
    fn intersect_identical_boxes() {
        let a = bbox(10.0, 20.0, 30.0, 40.0);
        let result = a.intersect(&a).expect("should overlap");
        assert!(approx_eq(&result, &a));
    }
}
