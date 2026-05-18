//! MSDF atlas configuration.

use core::error::Error;
use core::fmt::Display;
use core::fmt::Formatter;

use bevy::prelude::*;
use bevy::tasks;
use bevy_kana::ToF32;
use bevy_kana::ToU32;
use bevy_kana::ToUsize;

use super::constants::AVERAGE_GLYPH_COVERAGE;
use super::constants::DEFAULT_AUTO_GLYPH_WORKER_THREADS;
#[cfg(test)]
use super::constants::DEFAULT_CANONICAL_SIZE;
use super::constants::DEFAULT_GLYPH_PADDING;
use super::constants::DEFAULT_GLYPHS_PER_PAGE;
use super::constants::DEFAULT_SDF_RANGE;
use super::constants::MAX_CUSTOM_RASTER_SIZE;
use super::constants::MAX_GLYPHS_PER_PAGE;
use super::constants::MIN_CUSTOM_RASTER_SIZE;
use super::constants::MIN_GLYPHS_PER_PAGE;
use super::constants::SDF_RANGE;
use super::constants::SHELF_PACKING_EFFICIENCY;
use super::msdf_rasterizer::DistanceField;

/// Controls the pixel resolution of glyph rasterization (the canonical
/// size at which each glyph is rendered into the atlas).
///
/// Higher quality means sharper text at extreme zoom levels but uses
/// more memory per glyph and slower rasterization. Distance-field
/// rendering is resolution-independent — the shader handles scaling —
/// so this controls the *baseline fidelity* from which the distance
/// field is generated.
///
/// # Variants
///
/// | Variant   | Pixels | Use case                                       |
/// |-----------|--------|------------------------------------------------|
/// | `Tiny`    | 16     | Retro/pixel-art aesthetic, minimal memory      |
/// | `Small`   | 32     | Sharp at normal viewing distances              |
/// | `Medium`  | 64     | Good fidelity for most apps                    |
/// | `Large`   | 128    | Recommended default — sharp at extreme zoom    |
/// | `Huge`    | 256    | Maximum fidelity, 16× memory vs `Small`        |
/// | `Custom`  | 8–256  | Clamped to safe range, warns if out of bounds  |
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum RasterQuality {
    /// 16px — deliberately chunky, retro aesthetic, minimal memory.
    Tiny,
    /// 32px — sharp at normal viewing distances.
    Small,
    /// 64px — good fidelity for most apps.
    Medium,
    /// 128px — sharp at extreme zoom (default).
    #[default]
    Large,
    /// 256px — maximum fidelity, significant memory cost.
    Huge,
    /// Custom pixel size, clamped to 8–256. Values outside this range
    /// are clamped and a warning is logged.
    Custom(u32),
}

impl RasterQuality {
    /// Returns the canonical pixel size for this quality level.
    #[must_use]
    pub const fn pixel_size(self) -> u32 {
        match self {
            Self::Tiny => 16,
            Self::Small => 32,
            Self::Medium => 64,
            Self::Large => 128,
            Self::Huge => 256,
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

/// Which device produces the glyph distance-field bytes.
///
/// Orthogonal to [`DistanceField`], which describes what each texel
/// encodes. `RasterBackend` describes who computes it.
///
/// - [`RasterBackend::Cpu`] — `fdsm` on a worker pool (default).
/// - [`RasterBackend::Gpu`] — wgpu compute shader writing directly into the atlas page storage
///   texture.
///
/// The GPU backend is opt-in per atlas. Unsupported `(backend,
/// distance_field)` combinations are rejected by
/// [`AtlasConfig::validate`]. Device-feature loss (WebGL2, mobile
/// without compute) is handled at plugin-init time: an atlas configured
/// `Gpu` on an unsupported device falls back to `Cpu` with a warning.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum RasterBackend {
    /// Synchronous `fdsm` rasterization on worker threads (default).
    #[default]
    Cpu,
    /// Async wgpu compute rasterization, dispatched in the render
    /// schedule. SDF only in Phase 1; MSDF lands in Phase 2.
    Gpu,
}

/// Reasons an [`AtlasConfig`] is rejected by [`AtlasConfig::validate`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtlasConfigError {
    /// `(Gpu, Msdf)` is not implemented until Phase 2 of the GPU
    /// rasterizer rollout.
    GpuMsdfUnsupported,
}

impl Display for AtlasConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::GpuMsdfUnsupported => f.write_str("MSDF on GPU is not yet implemented (Phase 2)"),
        }
    }
}

impl Error for AtlasConfigError {}

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
    /// range `1..=tasks::available_parallelism()`, with a warning.
    Fixed(usize),
}

impl GlyphWorkerThreads {
    /// Resolves this policy to a concrete worker count for the current machine.
    #[must_use]
    pub fn resolve(self) -> usize {
        let available = tasks::available_parallelism().max(1);
        match self {
            Self::Auto => DEFAULT_AUTO_GLYPH_WORKER_THREADS.min(available),
            Self::Fixed(count) => count.clamp(1, available),
        }
    }
}

/// Configuration for the MSDF glyph atlas.
///
/// Controls how glyphs are rasterized and how atlas pages are sized. Insert
/// as a resource before adding [`DiegeticUiPlugin`](crate::DiegeticUiPlugin)
/// to override defaults:
///
/// ```ignore
/// App::new()
///     .insert_resource(
///         AtlasConfig::new()
///             .with_quality(RasterQuality::Tiny)
///             .with_glyphs_per_page(50),
///     )
///     .add_plugins(DiegeticUiPlugin);
/// ```
///
/// # Memory estimation
///
/// Each atlas page is an RGBA texture. The page size is computed
/// automatically from the raster quality and glyphs-per-page budget. An
/// `info!` log shows the estimated memory per page at startup so you can
/// tune for your target platform.
///
/// # Defaults
///
/// - **Quality**: [`RasterQuality::Large`] (128px) — sharp at extreme zoom
/// - **Glyphs per page**: 100 — fits most Latin text comfortably
/// - **Glyph worker threads**: [`GlyphWorkerThreads::Auto`] — currently up to 6 worker threads,
///   clamped to the machine's available parallelism
#[derive(Resource, Clone, Copy, Debug)]
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

    /// Distance-field variant used for glyph rasterization. Defaults to
    /// [`DistanceField::Msdf`] (multi-channel — sharp at corners). Use
    /// [`DistanceField::Sdf`] for smoother curves at the cost of corner
    /// fidelity.
    pub distance_field: DistanceField,

    /// Device that produces the distance-field bytes. Defaults to
    /// [`RasterBackend::Cpu`]. GPU support is Phase-1 SDF-only; the
    /// `(Gpu, Msdf)` pair is rejected by [`AtlasConfig::validate`]
    /// until Phase 2 lands.
    pub backend: RasterBackend,
}

impl AtlasConfig {
    /// Creates a new config with default values.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            quality:              RasterQuality::Large,
            glyphs_per_page:      DEFAULT_GLYPHS_PER_PAGE,
            glyph_worker_threads: GlyphWorkerThreads::Auto,
            distance_field:       DistanceField::Msdf,
            backend:              RasterBackend::Cpu,
        }
    }

    /// Sets the rasterization quality.
    #[must_use]
    pub const fn with_quality(mut self, quality: RasterQuality) -> Self {
        self.quality = quality;
        self
    }

    /// Sets the target number of glyphs per atlas page. Clamped to 10–2000.
    #[must_use]
    pub const fn with_glyphs_per_page(mut self, count: u16) -> Self {
        self.glyphs_per_page = count;
        self
    }

    /// Sets the async glyph raster worker policy.
    #[must_use]
    pub const fn with_glyph_worker_threads(mut self, workers: GlyphWorkerThreads) -> Self {
        self.glyph_worker_threads = workers;
        self
    }

    /// Sets which distance-field variant the atlas rasterizes glyphs with.
    #[must_use]
    pub const fn with_distance_field(mut self, mode: DistanceField) -> Self {
        self.distance_field = mode;
        self
    }

    /// Sets which device produces the glyph distance-field bytes.
    #[must_use]
    pub const fn with_backend(mut self, backend: RasterBackend) -> Self {
        self.backend = backend;
        self
    }

    /// Default per-glyph SDF range in pixels (mirrors
    /// [`super::constants::DEFAULT_SDF_RANGE`]). Exposed so the GPU
    /// dispatcher can read it from the resource without a separate
    /// constants import.
    #[must_use]
    pub const fn sdf_range(&self) -> f64 { DEFAULT_SDF_RANGE }

    /// Default per-glyph padding in texels.
    #[must_use]
    pub const fn glyph_padding(&self) -> u32 { DEFAULT_GLYPH_PADDING }

    /// Validates the `(backend, distance_field)` pair.
    ///
    /// Returns `Err(AtlasConfigError::GpuMsdfUnsupported)` for
    /// `(Gpu, Msdf)` until Phase 2 of the GPU rasterizer rollout lands.
    /// All other combinations are accepted.
    ///
    /// # Errors
    ///
    /// Returns an error for any unsupported `(backend, distance_field)`
    /// combination.
    pub const fn validate(&self) -> Result<(), AtlasConfigError> {
        match (self.backend, self.distance_field) {
            (RasterBackend::Gpu, DistanceField::Msdf) => Err(AtlasConfigError::GpuMsdfUnsupported),
            _ => Ok(()),
        }
    }
}

impl Default for AtlasConfig {
    fn default() -> Self { Self::new() }
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
    pub fn page_size(&self) -> u32 {
        let canonical = self.canonical_size();
        // Average glyph bitmap dimension — most glyphs use ~75% of the
        // canonical size plus padding. This is tighter than worst-case
        // (which would over-allocate) but still safe because `etagere`
        // overflows to a new page if a glyph doesn't fit.
        let total_pad = DEFAULT_GLYPH_PADDING + SDF_RANGE;
        let avg_glyph = (canonical.to_f32() * AVERAGE_GLYPH_COVERAGE).to_u32() + 2 * total_pad;
        let glyphs = f32::from(self.clamped_glyphs_per_page());
        let area = glyphs * avg_glyph.to_f32() * avg_glyph.to_f32() / SHELF_PACKING_EFFICIENCY;
        // Round up to next multiple of 4 for GPU texture row alignment.
        let side = area.sqrt().ceil().to_u32();
        (side + 3) & !3
    }

    /// Returns the estimated memory per atlas page in bytes (RGBA texture).
    #[must_use]
    pub fn estimated_page_bytes(&self) -> usize {
        (self.page_size() * self.page_size() * 4).to_usize()
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
            let available = tasks::available_parallelism().max(1);
            if !(1..=available).contains(&count) {
                warn!(
                    "AtlasConfig: glyph_worker_threads {} clamped to 1–{} range",
                    count, available
                );
            }
        }

        let page_size = self.page_size();
        let bytes = self.estimated_page_bytes();
        let kilobytes = bytes / 1024;
        let worker_threads = self.clamped_glyph_worker_threads();
        info!(
            "Atlas config: {:?}, ~{} glyphs/page (estimate), {page_size}x{page_size}px pages (~{kilobytes}KB each), {worker_threads} glyph workers",
            self.quality,
            self.clamped_glyphs_per_page()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_quality_pixel_sizes() {
        assert_eq!(RasterQuality::Tiny.pixel_size(), 16);
        assert_eq!(RasterQuality::Small.pixel_size(), 32);
        assert_eq!(RasterQuality::Medium.pixel_size(), 64);
        assert_eq!(RasterQuality::Large.pixel_size(), DEFAULT_CANONICAL_SIZE);
        assert_eq!(RasterQuality::Huge.pixel_size(), 256);
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
        assert_eq!(config.quality, RasterQuality::Large);
        assert_eq!(config.glyphs_per_page, DEFAULT_GLYPHS_PER_PAGE);
        assert_eq!(config.glyph_worker_threads, GlyphWorkerThreads::Auto);
        assert_eq!(config.canonical_size(), DEFAULT_CANONICAL_SIZE);
        assert_eq!(config.distance_field, DistanceField::Msdf);
    }

    #[test]
    fn with_distance_field_overrides_default() {
        let config = AtlasConfig::new().with_distance_field(DistanceField::Sdf);
        assert_eq!(config.distance_field, DistanceField::Sdf);
    }

    #[test]
    fn glyph_worker_threads_auto_resolves_to_at_most_six() {
        let resolved = GlyphWorkerThreads::Auto.resolve();
        assert!(resolved >= 1);
        assert!(resolved <= tasks::available_parallelism().max(1));
        assert!(resolved <= DEFAULT_AUTO_GLYPH_WORKER_THREADS);
    }

    #[test]
    fn glyph_worker_threads_fixed_clamps() {
        assert_eq!(GlyphWorkerThreads::Fixed(0).resolve(), 1);
        assert_eq!(
            GlyphWorkerThreads::Fixed(usize::MAX).resolve(),
            tasks::available_parallelism().max(1)
        );
    }

    #[test]
    fn page_size_is_aligned_to_4() {
        for quality in [
            RasterQuality::Tiny,
            RasterQuality::Small,
            RasterQuality::Medium,
            RasterQuality::Large,
            RasterQuality::Huge,
        ] {
            for glyphs in [10, 50, 100, 500, 1000] {
                let config = AtlasConfig {
                    quality,
                    glyphs_per_page: glyphs,
                    glyph_worker_threads: GlyphWorkerThreads::Auto,
                    distance_field: DistanceField::Msdf,
                    backend: RasterBackend::Cpu,
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
