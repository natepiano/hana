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
/// Controls the MSDF shader's alpha computation. All modes use
/// `AlphaMode::Blend` for smooth anti-aliased edges.
/// Discriminants are `#[repr(u32)]` and explicit because they map
/// directly to shader constants in `msdf_text.wgsl`. Adding or
/// reordering variants without updating the shader will cause a
/// compile-time test failure.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect)]
#[repr(u32)]
pub enum GlyphRenderMode {
    /// No visible text — only the shadow proxy renders (if shadow mode
    /// is not `None`). Useful for shadow-only effects.
    Invisible = 0,
    /// Normal MSDF text rendering — smooth alpha-blended edges.
    #[default]
    Text      = 1,
    /// Background quad with the text shape cut out (inverted alpha).
    PunchOut  = 2,
    /// Opaque quad matching the glyph shape (no MSDF decode).
    SolidQuad = 3,
}

impl GlyphRenderMode {
    #[must_use]
    const fn discriminant(self) -> u32 {
        match self {
            Self::Invisible => 0,
            Self::Text => 1,
            Self::PunchOut => 2,
            Self::SolidQuad => 3,
        }
    }
}

impl From<GlyphRenderMode> for u32 {
    fn from(render_mode: GlyphRenderMode) -> Self { render_mode.discriminant() }
}

/// What shape the shadow cast by glyphs takes.
///
/// Independent of [`GlyphRenderMode`] — the visible glyph and its shadow
/// can use different shapes. Shaped shadows (`Text`, `PunchOut`) spawn a
/// separate shadow proxy mesh with `AlphaMode::Mask` that is invisible
/// in the main pass but contributes to the shadow prepass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect)]
pub enum GlyphShadowMode {
    /// No shadow casting.
    None,
    /// Rectangular shadow from quad geometry.
    SolidQuad,
    /// Shadow follows the text outline (MSDF-decoded in prepass).
    #[default]
    Text,
    /// Shadow follows the punch-out shape (inverted MSDF in prepass).
    PunchOut,
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

/// Controls when text becomes visible during async glyph rasterization.
///
/// When glyphs are rasterized asynchronously, there is a brief window
/// where some glyphs are ready but others are still in flight. This
/// policy controls whether partially-rasterized text is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect)]
pub enum GlyphLoadingPolicy {
    /// Text is invisible until every glyph has been rasterized (default).
    /// Async tasks are still triggered for missing glyphs — the text
    /// simply appears all at once when the last glyph completes.
    #[default]
    WhenReady,
    /// Show glyphs as they become available. Missing glyphs are skipped,
    /// so text may appear with visible holes until rasterization finishes.
    Progressive,
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
    loading_policy: GlyphLoadingPolicy,
    font_features:  FontFeatures,
    /// What unit `size` is expressed in. `None` = inherit from global config.
    /// Only meaningful for [`ForStandalone`] — ignored by layout text.
    unit:           Option<Unit>,
    /// Explicit meters-per-design-unit override. `None` = derive from `unit`.
    /// Only meaningful for [`ForStandalone`] — ignored by layout text.
    world_scale:    Option<f32>,
    /// Per-style alpha-mode override. `None` = inherit from panel or resource default.
    alpha_mode:     Option<AlphaMode>,
    #[reflect(ignore)]
    _context:       PhantomData<C>,
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
            && self.loading_policy == other.loading_policy
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

    /// Returns the per-entity unit override, if set.
    #[must_use]
    pub const fn unit(&self) -> Option<Unit> { self.unit }

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

    /// Sets the text color (mutable reference variant).
    pub const fn set_color(&mut self, color: Color) { self.color = color; }

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

    /// Returns the glyph loading policy.
    #[must_use]
    pub const fn loading_policy(&self) -> GlyphLoadingPolicy { self.loading_policy }

    /// Sets the glyph loading policy.
    #[must_use]
    pub const fn with_loading_policy(mut self, policy: GlyphLoadingPolicy) -> Self {
        self.loading_policy = policy;
        self
    }

    /// Returns the per-style alpha-mode override, if any.
    ///
    /// `None` means "inherit" — resolution falls through to panel-level
    /// override (for panel text), then to
    /// [`CascadeDefaults::text_alpha`](crate::CascadeDefaults).
    #[must_use]
    pub const fn alpha_mode(&self) -> Option<AlphaMode> { self.alpha_mode }

    /// Sets the per-style alpha-mode override.
    ///
    /// The library default is [`AlphaMode::Blend`] — see
    /// [`StableTransparency`](crate::StableTransparency) for guidance on
    /// when to use each mode, how `StableTransparency` pairs with
    /// [`AlphaMode::Blend`]/[`AlphaMode::Premultiplied`], and when
    /// [`AlphaMode::AlphaToCoverage`] + MSAA is the better path.
    #[must_use]
    pub const fn with_alpha_mode(mut self, mode: AlphaMode) -> Self {
        self.alpha_mode = Some(mode);
        self
    }

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
            loading_policy: _,
            // Standalone-only — not relevant for layout shaping cache.
            unit: _,
            world_scale: _,
            // Render-only — affects compositing, not shaping.
            alpha_mode: _,
            _context: _,
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
            anchor:         Anchor::Center,
            render_mode:    GlyphRenderMode::Text,
            shadow_mode:    GlyphShadowMode::Text,
            sidedness:      GlyphSidedness::DoubleSided,
            loading_policy: GlyphLoadingPolicy::WhenReady,
            font_features:  FontFeatures::NONE,
            unit:           font_size.unit,
            world_scale:    None,
            alpha_mode:     None,
            _context:       PhantomData,
        }
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
    /// Copies all shared fields. The standalone-specific `anchor` defaults to
    /// [`Anchor::TopLeft`] since panel text is positioned by the layout
    /// engine rather than by an anchor offset.
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
            anchor:         Anchor::Center,
            render_mode:    self.render_mode,
            shadow_mode:    self.shadow_mode,
            sidedness:      self.sidedness,
            loading_policy: self.loading_policy,
            font_features:  self.font_features,
            unit:           self.unit,
            world_scale:    None,
            alpha_mode:     self.alpha_mode,
            _context:       PhantomData,
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
    /// Accepts [`Pt`](crate::Pt), [`Mm`](crate::Mm), [`In`](crate::In),
    /// or bare `f32`. Newtypes carry their unit — bare `f32` uses the
    /// global [`UnitConfig::world_font`](crate::UnitConfig).
    ///
    /// Defaults to centered anchor, white color, normal weight.
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
            wrap:           TextWrap::None,
            color:          Color::WHITE,
            align:          TextAlign::Left,
            anchor:         Anchor::Center,
            render_mode:    GlyphRenderMode::Text,
            shadow_mode:    GlyphShadowMode::Text,
            sidedness:      GlyphSidedness::DoubleSided,
            loading_policy: GlyphLoadingPolicy::WhenReady,
            font_features:  FontFeatures::NONE,
            unit:           font_size.unit,
            world_scale:    None,
            alpha_mode:     None,
            _context:       PhantomData,
        }
    }

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

    /// Sets the anchor point within the text block's bounding box.
    #[must_use]
    pub const fn with_anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// Sets whether glyph meshes render one or both faces.
    #[must_use]
    pub const fn with_sidedness(mut self, sidedness: GlyphSidedness) -> Self {
        self.sidedness = sidedness;
        self
    }

    /// Sets the unit that [`size`](Self::size) is expressed in.
    ///
    /// When set, the renderer converts the size to world meters using the
    /// unit's [`meters_per_unit`](Unit::meters_per_unit) factor. When `None`
    /// (the default), the global
    /// [`UnitConfig::world_font`](crate::UnitConfig) is used.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Explicit unit override (rare — prefer newtypes in new()):
    /// WorldTextStyle::new(12.0).with_unit(Unit::Points)
    ///
    /// // Preferred — newtype carries the unit:
    /// WorldTextStyle::new(Pt(12.0))
    /// ```
    #[must_use]
    pub const fn with_unit(mut self, unit: Unit) -> Self {
        self.unit = Some(unit);
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

    /// Returns the per-entity world scale override, if set.
    #[must_use]
    pub const fn world_scale(&self) -> Option<f32> { self.world_scale }
}

impl Default for TextProps<ForStandalone> {
    fn default() -> Self { Self::new(DEFAULT_FONT_SIZE) }
}

impl TextProps<ForStandalone> {
    /// Converts to a `TextConfig` for use with the shaping/rendering pipeline.
    ///
    /// Copies all shared measurement fields and color. The layout-specific
    /// `wrap` field is set to [`TextWrap::None`] since standalone text does
    /// not word-wrap by default.
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
            loading_policy: self.loading_policy,
            font_features:  self.font_features,
            unit:           self.unit,
            world_scale:    self.world_scale,
            alpha_mode:     self.alpha_mode,
            _context:       PhantomData,
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
// stay in sync with the constants in `assets/shaders/msdf_text.wgsl`.
// If you add or reorder variants, update the shader constants to match
// and adjust these assertions.

const _: () = assert!(GlyphRenderMode::Invisible.discriminant() == 0);
const _: () = assert!(GlyphRenderMode::Text.discriminant() == 1);
const _: () = assert!(GlyphRenderMode::PunchOut.discriminant() == 2);
const _: () = assert!(GlyphRenderMode::SolidQuad.discriminant() == 3);

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
    fn as_standalone_from_layout_defaults_to_center_anchor() {
        // Regression guard: `as_standalone()` on a `ForLayout` config has no
        // anchor to copy, so it defaults to `Center`. Callers that need a
        // specific anchor must chain `.with_anchor()` after conversion.
        let layout = TextProps::<ForLayout>::new(12.0);
        let standalone = layout.as_standalone();
        assert_eq!(
            standalone.anchor(),
            Anchor::Center,
            "as_standalone() should default to Center (callers must override)"
        );
    }

    #[test]
    fn with_anchor_overrides_default() {
        let layout = TextProps::<ForLayout>::new(12.0);
        let standalone = layout.as_standalone().with_anchor(Anchor::TopLeft);
        assert_eq!(standalone.anchor(), Anchor::TopLeft);
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
