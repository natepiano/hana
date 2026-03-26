//! Atlas configuration types for the diegetic UI plugin.

use bevy::prelude::*;
use bevy::tasks::available_parallelism;

/// Minimum canonical rasterization size in pixels.
const MIN_CUSTOM_RASTER_SIZE: u32 = 8;

/// Maximum canonical rasterization size in pixels.
const MAX_CUSTOM_RASTER_SIZE: u32 = 256;

/// Minimum glyphs per atlas page.
const MIN_GLYPHS_PER_PAGE: u16 = 10;

/// Maximum glyphs per atlas page.
const MAX_GLYPHS_PER_PAGE: u16 = 2000;

/// Default glyphs per atlas page.
const DEFAULT_GLYPHS_PER_PAGE: u16 = 100;

/// Default auto-selected glyph raster worker count on sufficiently parallel machines.
const DEFAULT_AUTO_GLYPH_WORKER_THREADS: usize = 6;

/// Controls the pixel resolution of MSDF glyph rasterization.
///
/// Higher quality means sharper text at extreme zoom levels but uses more
/// memory per glyph. MSDF is resolution-independent — the shader handles
/// scaling — so this controls the *baseline fidelity* from which the
/// distance field is generated.
///
/// # Variants
///
/// | Variant    | Pixels | Use case                                     |
/// |------------|--------|----------------------------------------------|
/// | `Low`      | 16     | Retro/pixel-art aesthetic, minimal memory    |
/// | `Medium`   | 32     | Sharp at normal viewing distances             |
/// | `High`     | 64     | Sharp at extreme zoom (recommended default)  |
/// | `Extreme`  | 128    | Maximum fidelity, 16x memory vs `Medium`     |
/// | `Custom`   | 8–256  | Clamped to safe range, warns if out of bounds|
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum RasterQuality {
    /// 16px — deliberately chunky, retro aesthetic, minimal memory.
    Low,
    /// 32px — sharp at normal viewing distances.
    Medium,
    /// 64px — sharp even at extreme zoom (default).
    #[default]
    High,
    /// 128px — maximum fidelity, significant memory cost.
    Extreme,
    /// Custom pixel size, clamped to 8–256. Values outside this range
    /// are clamped and a warning is logged.
    Custom(u32),
}

impl RasterQuality {
    /// Returns the canonical pixel size for this quality level.
    #[must_use]
    pub const fn pixel_size(self) -> u32 {
        match self {
            Self::Low => 16,
            Self::Medium => 32,
            Self::High => 64,
            Self::Extreme => 128,
            Self::Custom(size) => {
                if size < MIN_CUSTOM_RASTER_SIZE {
                    MIN_CUSTOM_RASTER_SIZE
                } else if size > MAX_CUSTOM_RASTER_SIZE {
                    MAX_CUSTOM_RASTER_SIZE
                } else {
                    size
                }
            },
        }
    }
}

/// Controls how many worker threads are used for async glyph rasterization.
///
/// This setting applies to the shared MSDF text pipeline used by both
/// diegetic panels and standalone [`WorldText`](crate::WorldText).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum GlyphWorkerThreads {
    /// Use the crate's default heuristic: currently up to 6 worker threads,
    /// clamped to the machine's available parallelism.
    #[default]
    Auto,
    /// Request an explicit worker count. Values are clamped to the safe
    /// range `1..=available_parallelism()`, with a warning.
    Fixed(usize),
}

impl GlyphWorkerThreads {
    /// Resolves this policy to a concrete worker count for the current machine.
    #[must_use]
    pub fn resolve(self) -> usize {
        let available = available_parallelism().max(1);
        match self {
            Self::Auto => DEFAULT_AUTO_GLYPH_WORKER_THREADS.min(available),
            Self::Fixed(count) => count.clamp(1, available),
        }
    }
}

/// Configuration for the MSDF glyph atlas.
///
/// Controls how glyphs are rasterized and how atlas pages are sized.
/// Pass to [`DiegeticUiPlugin::with_atlas`](crate::DiegeticUiPlugin::with_atlas)
/// to override defaults.
///
/// # Memory estimation
///
/// Each atlas page is an RGBA texture. The page size is computed
/// automatically from the raster quality and glyphs-per-page budget.
/// When you call [`DiegeticUiPlugin::with_atlas`](crate::DiegeticUiPlugin::with_atlas), an `info!`
/// log shows the estimated memory per page so you can tune for your
/// target platform.
///
/// # Defaults
///
/// - **Quality**: [`RasterQuality::High`] (64px) — sharp at extreme zoom
/// - **Glyphs per page**: 100 — fits most Latin text comfortably
/// - **Glyph worker threads**: [`GlyphWorkerThreads::Auto`] — currently up to 6 worker threads,
///   clamped to the machine's available parallelism
///
/// # Example
///
/// ```ignore
/// // Retro look with small atlas pages:
/// App::new().add_plugins(
///     DiegeticUiPlugin::with_atlas()
///         .quality(RasterQuality::Low)
///         .glyphs_per_page(50)
/// )
/// ```
#[derive(Clone, Copy, Debug)]
pub struct AtlasConfig {
    /// Rasterization quality — controls the canonical pixel size used
    /// for MSDF generation. Higher quality = sharper at zoom but more
    /// memory per glyph.
    pub quality: RasterQuality,

    /// Target number of glyphs per atlas page. This is an **estimate** —
    /// actual capacity depends on the font and character mix (a page of
    /// narrow characters like `l` and `i` fits more than a page of wide
    /// characters like `W` and `M`). When a page fills, a new page is
    /// allocated automatically regardless of the budget. Smaller values
    /// reduce per-page memory but may increase the number of pages (and
    /// draw calls) for text-heavy apps. Clamped to 10–2000.
    pub glyphs_per_page: u16,

    /// Async worker policy for MSDF glyph rasterization. Applies to both
    /// panel text and [`WorldText`](crate::WorldText).
    pub glyph_worker_threads: GlyphWorkerThreads,
}

impl AtlasConfig {
    /// Creates a new config with default values.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            quality:              RasterQuality::High,
            glyphs_per_page:      DEFAULT_GLYPHS_PER_PAGE,
            glyph_worker_threads: GlyphWorkerThreads::Auto,
        }
    }
}

impl Default for AtlasConfig {
    fn default() -> Self {
        Self {
            quality:              RasterQuality::default(),
            glyphs_per_page:      DEFAULT_GLYPHS_PER_PAGE,
            glyph_worker_threads: GlyphWorkerThreads::default(),
        }
    }
}

impl AtlasConfig {
    /// Returns the canonical rasterization size in pixels, after clamping.
    #[must_use]
    pub const fn canonical_size(&self) -> u32 { self.quality.pixel_size() }

    /// Returns the glyphs-per-page budget, after clamping.
    #[must_use]
    pub const fn clamped_glyphs_per_page(&self) -> u16 {
        if self.glyphs_per_page < MIN_GLYPHS_PER_PAGE {
            MIN_GLYPHS_PER_PAGE
        } else if self.glyphs_per_page > MAX_GLYPHS_PER_PAGE {
            MAX_GLYPHS_PER_PAGE
        } else {
            self.glyphs_per_page
        }
    }

    /// Returns the resolved glyph worker count for this machine.
    #[must_use]
    pub fn clamped_glyph_worker_threads(&self) -> usize { self.glyph_worker_threads.resolve() }

    /// Computes the atlas page size in pixels for this configuration.
    ///
    /// Uses the canonical raster size, SDF range, and padding to estimate
    /// average glyph bitmap dimensions, then sizes the page to fit
    /// approximately the requested number of glyphs. The actual capacity
    /// varies by font — narrow characters pack tighter than wide ones.
    /// Pages that fill beyond this estimate simply overflow to a new page.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    pub fn page_size(&self) -> u32 {
        let canonical = self.canonical_size();
        // Average glyph bitmap dimension — most glyphs use ~75% of the
        // canonical size plus padding. This is tighter than worst-case
        // (which would over-allocate) but still safe because `etagere`
        // overflows to a new page if a glyph doesn't fit.
        let total_pad = 2 + 4; // DEFAULT_GLYPH_PADDING + DEFAULT_SDF_RANGE
        let avg_glyph = (canonical as f32 * 0.75) as u32 + 2 * total_pad;
        let glyphs = f32::from(self.clamped_glyphs_per_page());
        // Shelf packing is ~80% efficient.
        let area = glyphs * (avg_glyph as f32) * (avg_glyph as f32) / 0.80;
        // Round up to next multiple of 4 for GPU texture row alignment.
        let side = area.sqrt().ceil() as u32;
        (side + 3) & !3
    }

    /// Returns the estimated memory per atlas page in bytes (RGBA texture).
    #[must_use]
    pub fn estimated_page_bytes(&self) -> usize {
        let size = self.page_size();
        (size * size * 4) as usize
    }

    /// Logs warnings for out-of-range values and an info summary.
    pub(super) fn log_and_clamp(&self) {
        if let RasterQuality::Custom(size) = self.quality
            && !(MIN_CUSTOM_RASTER_SIZE..=MAX_CUSTOM_RASTER_SIZE).contains(&size)
        {
            warn!(
                "AtlasConfig: custom raster size {size} clamped to {}–{} range",
                MIN_CUSTOM_RASTER_SIZE, MAX_CUSTOM_RASTER_SIZE
            );
        }
        if !(MIN_GLYPHS_PER_PAGE..=MAX_GLYPHS_PER_PAGE).contains(&self.glyphs_per_page) {
            warn!(
                "AtlasConfig: glyphs_per_page {} clamped to {}–{} range",
                self.glyphs_per_page, MIN_GLYPHS_PER_PAGE, MAX_GLYPHS_PER_PAGE
            );
        }
        if let GlyphWorkerThreads::Fixed(count) = self.glyph_worker_threads {
            let available = available_parallelism().max(1);
            if !(1..=available).contains(&count) {
                warn!(
                    "AtlasConfig: glyph_worker_threads {} clamped to 1–{} range",
                    count, available
                );
            }
        }

        let page_size = self.page_size();
        let bytes = self.estimated_page_bytes();
        let kb = bytes / 1024;
        let worker_threads = self.clamped_glyph_worker_threads();
        info!(
            "Atlas config: {:?}, ~{} glyphs/page (estimate), {page_size}x{page_size}px pages (~{kb}KB each), {worker_threads} glyph workers",
            self.quality,
            self.clamped_glyphs_per_page()
        );
    }
}

/// Physical unit for interpreting numeric dimensions.
///
/// Used by [`UnitConfig`] to define what "1.0" means for layout dimensions
/// and font sizes. `Custom(f32)` is an escape hatch for any unit not
/// covered by the named variants — the value is meters per unit.
///
/// # Examples
///
/// ```ignore
/// Unit::Meters          // 1 unit = 1 meter (Bevy default)
/// Unit::Millimeters     // 1 unit = 1mm
/// Unit::Points          // 1 unit = 1 typographic point (1/72 inch)
/// Unit::Inches          // 1 unit = 1 inch
/// Unit::Custom(0.01)    // 1 unit = 1 centimeter
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub enum Unit {
    /// 1 unit = 1 meter. Bevy's default world-space convention.
    Meters,
    /// 1 unit = 1 millimeter (0.001 m).
    Millimeters,
    /// 1 unit = 1 typographic point (1/72 inch ≈ 0.000353 m).
    Points,
    /// 1 unit = 1 inch (0.0254 m).
    Inches,
    /// 1 unit = the given number of meters.
    Custom(f32),
}

/// Minimum `meters_per_unit` for [`Unit::Custom`], equal to [`Unit::Points`].
///
/// Units smaller than a typographic point would cause font sizes to shrink
/// below 1.0 when converted to points for the layout engine, hitting parley's
/// integer quantization and producing incorrect baselines.
const MIN_CUSTOM_MPU: f32 = 0.0254 / 72.0;

impl Unit {
    /// Returns the conversion factor from this unit to meters.
    ///
    /// [`Unit::Custom`] values below [`Unit::Points`] (0.000353 m) are clamped.
    #[must_use]
    pub const fn meters_per_unit(self) -> f32 {
        match self {
            Self::Meters => 1.0,
            Self::Millimeters => 0.001,
            Self::Points => 0.0254 / 72.0,
            Self::Inches => 0.0254,
            Self::Custom(mpu) => {
                if mpu < MIN_CUSTOM_MPU {
                    MIN_CUSTOM_MPU
                } else {
                    mpu
                }
            },
        }
    }

    /// Returns the multiplier to convert a value in this unit to typographic points.
    #[must_use]
    pub fn to_points(self) -> f32 { self.meters_per_unit() / Self::Points.meters_per_unit() }
}

/// Global unit configuration for layout dimensions and font sizes.
///
/// Defines the default interpretation of numeric values throughout the
/// system. Individual [`DiegeticPanel`](crate::DiegeticPanel) entities can
/// override `layout_unit` and `font_unit`. Individual
/// [`WorldText`](crate::WorldText) entities can override their font unit
/// with [`WorldFontUnit`](crate::WorldFontUnit).
///
/// # Defaults
///
/// - `layout`: [`Unit::Meters`] — panel dimensions are in meters
/// - `font`: [`Unit::Points`] — panel font sizes are in typographic points
/// - `world_font`: [`Unit::Meters`] — standalone `WorldText` sizes are in meters
///
/// Panel text uses points because panels are document-like (A4, business
/// cards). Standalone `WorldText` uses meters because it lives in the 3D
/// scene — `WorldTextStyle::new().with_size(0.2)` produces 20cm tall text,
/// which is visible at typical viewing distances.
///
/// # Examples
///
/// ```ignore
/// // Default — no config needed:
/// // panels: 18pt text on a 0.5m panel
/// // world:  0.2m tall standalone text
///
/// // Override for a "human scale" scene where WorldText should use points:
/// UnitConfig { world_font: Unit::Points, ..default() }
/// ```
#[derive(Resource, Clone, Copy, Debug, Reflect)]
pub struct UnitConfig {
    /// Unit for panel `width`/`height` and layout dimensions.
    pub layout:     Unit,
    /// Unit for font sizes in [`LayoutTextStyle`](crate::LayoutTextStyle)
    /// (used by [`DiegeticPanel`](crate::DiegeticPanel) text).
    pub font:       Unit,
    /// Unit for font sizes in [`WorldTextStyle`](crate::WorldTextStyle)
    /// (used by standalone [`WorldText`](crate::WorldText) entities).
    pub world_font: Unit,
}

impl Default for UnitConfig {
    fn default() -> Self {
        Self {
            layout:     Unit::Meters,
            font:       Unit::Points,
            world_font: Unit::Meters,
        }
    }
}

impl UnitConfig {
    /// Returns the font-to-layout conversion factor for panels.
    ///
    /// Multiply a font size (in `self.font` units) by this value to get
    /// the equivalent size in `self.layout` units.
    #[must_use]
    pub fn font_scale(&self) -> f32 { self.font.meters_per_unit() / self.layout.meters_per_unit() }
}

// ── Dimensional newtypes ─────────────────────────────────────────────────────

/// A value in typographic points (1/72 inch ≈ 0.353 mm).
#[derive(Clone, Copy, Debug)]
pub struct Pt(pub f32);

/// A value in millimeters.
#[derive(Clone, Copy, Debug)]
pub struct Mm(pub f32);

/// A value in inches.
#[derive(Clone, Copy, Debug)]
pub struct In(pub f32);

impl From<Pt> for f32 {
    fn from(v: Pt) -> Self { v.0 * Unit::Points.meters_per_unit() }
}

impl From<Mm> for f32 {
    fn from(v: Mm) -> Self { v.0 * Unit::Millimeters.meters_per_unit() }
}

impl From<In> for f32 {
    fn from(v: In) -> Self { v.0 * Unit::Inches.meters_per_unit() }
}

// ── Paper sizes ──────────────────────────────────────────────────────────────

/// Standard paper and card sizes.
///
/// Each variant stores its dimensions in millimeters internally and
/// converts to meters (or any [`Unit`]) on request. Implements
/// [`PanelSize`] so it can be passed directly to
/// [`DiegeticPanelBuilder::size`](crate::DiegeticPanelBuilder::size).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub enum PaperSize {
    // ── ISO A series ─────────────────────────────────────────────────
    /// A0 — 841 × 1189 mm.
    A0,
    /// A1 — 594 × 841 mm.
    A1,
    /// A2 — 420 × 594 mm.
    A2,
    /// A3 — 297 × 420 mm.
    A3,
    /// A4 — 210 × 297 mm.
    A4,
    /// A5 — 148 × 210 mm.
    A5,
    /// A6 — 105 × 148 mm.
    A6,
    /// A7 — 74 × 105 mm.
    A7,
    /// A8 — 52 × 74 mm.
    A8,

    // ── ISO B series ─────────────────────────────────────────────────
    /// B0 — 1000 × 1414 mm.
    B0,
    /// B1 — 707 × 1000 mm.
    B1,
    /// B2 — 500 × 707 mm.
    B2,
    /// B3 — 353 × 500 mm.
    B3,
    /// B4 — 250 × 353 mm.
    B4,
    /// B5 — 176 × 250 mm.
    B5,

    // ── North American ───────────────────────────────────────────────
    /// US Letter — 8.5 × 11 inches (215.9 × 279.4 mm).
    USLetter,
    /// US Legal — 8.5 × 14 inches (215.9 × 355.6 mm).
    USLegal,
    /// US Ledger / Tabloid — 11 × 17 inches (279.4 × 431.8 mm).
    USLedger,
    /// US Executive — 7.25 × 10.5 inches (184.2 × 266.7 mm).
    USExecutive,

    // ── Cards ────────────────────────────────────────────────────────
    /// US Business Card — 3.5 × 2 inches (88.9 × 50.8 mm).
    BusinessCard,
    /// Index Card 3 × 5 inches (76.2 × 127.0 mm).
    IndexCard3x5,
    /// Index Card 4 × 6 inches (101.6 × 152.4 mm).
    IndexCard4x6,
    /// Index Card 5 × 8 inches (127.0 × 203.2 mm).
    IndexCard5x8,

    // ── Photo ────────────────────────────────────────────────────────
    /// Photo 4 × 6 inches (101.6 × 152.4 mm).
    Photo4x6,
    /// Photo 5 × 7 inches (127.0 × 177.8 mm).
    Photo5x7,
    /// Photo 8 × 10 inches (203.2 × 254.0 mm).
    Photo8x10,

    // ── Poster ───────────────────────────────────────────────────────
    /// Poster 18 × 24 inches (457.2 × 609.6 mm).
    Poster18x24,
    /// Poster 24 × 36 inches (609.6 × 914.4 mm).
    Poster24x36,
}

impl PaperSize {
    /// Width in millimeters (shorter dimension).
    #[must_use]
    pub const fn width_mm(self) -> f32 {
        match self {
            Self::A0 => 841.0,
            Self::A1 => 594.0,
            Self::A2 => 420.0,
            Self::A3 => 297.0,
            Self::A4 => 210.0,
            Self::A5 => 148.0,
            Self::A6 => 105.0,
            Self::A7 => 74.0,
            Self::A8 => 52.0,
            Self::B0 => 1000.0,
            Self::B1 => 707.0,
            Self::B2 => 500.0,
            Self::B3 => 353.0,
            Self::B4 => 250.0,
            Self::B5 => 176.0,
            Self::USLetter => 215.9,
            Self::USLegal => 215.9,
            Self::USLedger => 279.4,
            Self::USExecutive => 184.15,
            Self::BusinessCard => 88.9,
            Self::IndexCard3x5 => 76.2,
            Self::IndexCard4x6 => 101.6,
            Self::IndexCard5x8 => 127.0,
            Self::Photo4x6 => 101.6,
            Self::Photo5x7 => 127.0,
            Self::Photo8x10 => 203.2,
            Self::Poster18x24 => 457.2,
            Self::Poster24x36 => 609.6,
        }
    }

    /// Height in millimeters (longer dimension).
    #[must_use]
    pub const fn height_mm(self) -> f32 {
        match self {
            Self::A0 => 1189.0,
            Self::A1 => 841.0,
            Self::A2 => 594.0,
            Self::A3 => 420.0,
            Self::A4 => 297.0,
            Self::A5 => 210.0,
            Self::A6 => 148.0,
            Self::A7 => 105.0,
            Self::A8 => 74.0,
            Self::B0 => 1414.0,
            Self::B1 => 1000.0,
            Self::B2 => 707.0,
            Self::B3 => 500.0,
            Self::B4 => 353.0,
            Self::B5 => 250.0,
            Self::USLetter => 279.4,
            Self::USLegal => 355.6,
            Self::USLedger => 431.8,
            Self::USExecutive => 266.7,
            Self::BusinessCard => 50.8,
            Self::IndexCard3x5 => 127.0,
            Self::IndexCard4x6 => 152.4,
            Self::IndexCard5x8 => 203.2,
            Self::Photo4x6 => 152.4,
            Self::Photo5x7 => 177.8,
            Self::Photo8x10 => 254.0,
            Self::Poster18x24 => 609.6,
            Self::Poster24x36 => 914.4,
        }
    }

    /// Width in meters.
    #[must_use]
    pub const fn width(self) -> f32 { self.width_mm() * 0.001 }

    /// Height in meters.
    #[must_use]
    pub const fn height(self) -> f32 { self.height_mm() * 0.001 }

    /// Width in the given unit.
    #[must_use]
    pub fn width_in(self, unit: Unit) -> f32 {
        self.width_mm() * Unit::Millimeters.meters_per_unit() / unit.meters_per_unit()
    }

    /// Height in the given unit.
    #[must_use]
    pub fn height_in(self, unit: Unit) -> f32 {
        self.height_mm() * Unit::Millimeters.meters_per_unit() / unit.meters_per_unit()
    }
}

// ── PanelSize trait ──────────────────────────────────────────────────────────

/// Trait for types that can provide panel dimensions in meters.
///
/// Implemented by [`PaperSize`] and tuples of values convertible to `f32`
/// (e.g., `(Pt(612.0), Pt(792.0))` or `(Mm(210.0), Mm(297.0))`).
pub trait PanelSize {
    /// Returns `(width, height)` in meters.
    fn dimensions(self) -> (f32, f32);
}

impl PanelSize for PaperSize {
    fn dimensions(self) -> (f32, f32) { (self.width(), self.height()) }
}

impl<W: Into<f32>, H: Into<f32>> PanelSize for (W, H) {
    fn dimensions(self) -> (f32, f32) { (self.0.into(), self.1.into()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_quality_pixel_sizes() {
        assert_eq!(RasterQuality::Low.pixel_size(), 16);
        assert_eq!(RasterQuality::Medium.pixel_size(), 32);
        assert_eq!(RasterQuality::High.pixel_size(), 64);
        assert_eq!(RasterQuality::Extreme.pixel_size(), 128);
    }

    #[test]
    fn raster_quality_custom_clamps() {
        // Below minimum.
        assert_eq!(RasterQuality::Custom(1).pixel_size(), 8);
        assert_eq!(RasterQuality::Custom(7).pixel_size(), 8);
        // At boundaries.
        assert_eq!(RasterQuality::Custom(8).pixel_size(), 8);
        assert_eq!(RasterQuality::Custom(256).pixel_size(), 256);
        // Above maximum.
        assert_eq!(RasterQuality::Custom(257).pixel_size(), 256);
        assert_eq!(RasterQuality::Custom(1000).pixel_size(), 256);
        // In range.
        assert_eq!(RasterQuality::Custom(48).pixel_size(), 48);
    }

    #[test]
    fn glyphs_per_page_clamps() {
        let below = AtlasConfig {
            glyphs_per_page: 1,
            ..AtlasConfig::default()
        };
        assert_eq!(below.clamped_glyphs_per_page(), 10);

        let above = AtlasConfig {
            glyphs_per_page: 5000,
            ..AtlasConfig::default()
        };
        assert_eq!(above.clamped_glyphs_per_page(), 2000);

        let in_range = AtlasConfig {
            glyphs_per_page: 75,
            ..AtlasConfig::default()
        };
        assert_eq!(in_range.clamped_glyphs_per_page(), 75);
    }

    #[test]
    fn default_config_values() {
        let config = AtlasConfig::default();
        assert_eq!(config.quality, RasterQuality::High);
        assert_eq!(config.glyphs_per_page, 100);
        assert_eq!(config.glyph_worker_threads, GlyphWorkerThreads::Auto);
        assert_eq!(config.canonical_size(), 64);
    }

    #[test]
    fn glyph_worker_threads_auto_resolves_to_at_most_six() {
        let resolved = GlyphWorkerThreads::Auto.resolve();
        assert!(resolved >= 1);
        assert!(resolved <= available_parallelism().max(1));
        assert!(resolved <= DEFAULT_AUTO_GLYPH_WORKER_THREADS);
    }

    #[test]
    fn glyph_worker_threads_fixed_clamps() {
        assert_eq!(GlyphWorkerThreads::Fixed(0).resolve(), 1);
        assert_eq!(
            GlyphWorkerThreads::Fixed(usize::MAX).resolve(),
            available_parallelism().max(1)
        );
    }

    #[test]
    fn page_size_is_aligned_to_4() {
        for quality in [
            RasterQuality::Low,
            RasterQuality::Medium,
            RasterQuality::High,
            RasterQuality::Extreme,
        ] {
            for glyphs in [10, 50, 100, 500, 1000] {
                let config = AtlasConfig {
                    quality,
                    glyphs_per_page: glyphs,
                    glyph_worker_threads: GlyphWorkerThreads::Auto,
                };
                let size = config.page_size();
                assert_eq!(
                    size % 4,
                    0,
                    "page_size {size} not aligned to 4 for {quality:?}/{glyphs}"
                );
                assert!(size > 0, "page_size should be non-zero");
            }
        }
    }
}
