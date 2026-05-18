//! Optional CPU mirror of GPU-rasterized atlas pages.
//!
//! Phase 1: stub. The CPU mirror exists so debug tooling
//! (`dump_atlas_png`, the `page_pixels` accessor) keeps working under
//! `RasterBackend::Gpu`. Implementing the actual `copy_texture_to_buffer`
//! + `map_async` callback is Phase 1.5 work — until then, the pages show the initial cleared state
//!   on CPU side while the GPU texture holds the live distance field.

// Intentionally empty for Phase 1. The dispatch system writes texels
// directly into the storage texture; the CPU pixel buffer in the
// atlas page lags until a follow-up readback pass lands.
