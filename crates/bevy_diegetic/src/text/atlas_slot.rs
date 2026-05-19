//! Atlas slot — wraps the world-level [`GlyphAtlas`] with a parallel-
//! swap state machine so the runtime can switch distance-field modes
//! without flicker, race conditions, or in-place atlas clears.
//!
//! Steady state is [`AtlasSlot::Single`]. When the user toggles a new
//! distance-field preference, the driver builds a fresh `pending`
//! atlas in the new mode and transitions to [`AtlasSlot::Swapping`].
//! While swapping, materials keep sampling from `active`; the worker
//! pool (shared with `active`) repopulates `pending`. When every
//! cached or in-flight glyph from `active` has been re-rasterized into
//! `pending`, the slot completes the swap and `active` drops.

use bevy::image::Image;
use bevy::prelude::Assets;
use bevy::prelude::Handle;
use bevy::prelude::Resource;

use super::atlas::AsyncGlyphPollStats;
use super::atlas::GlyphAtlas;
use super::atlas_config::RasterBackend;
use super::atlas_config::RasterQuality;
use super::gpu_rasterizer::GpuRenderJob;
use super::msdf_rasterizer::DistanceField;

/// World-level resource. Owns the currently-active atlas and, during a
/// mode-switch, the pending replacement.
#[derive(Resource)]
#[allow(
    clippy::large_enum_variant,
    reason = "AtlasSlot is a single world resource — the 224-byte size delta is per-app, not per-instance, and boxing the Swapping variant would add indirection for no real-world memory savings."
)]
pub enum AtlasSlot {
    /// Steady state. One atlas; everyone uses it.
    Single(GlyphAtlas),

    /// Mid-swap state.
    /// - `active`      : materials sample from here this frame
    /// - `pending`     : being populated by background workers
    Swapping {
        /// Atlas materials sample from this frame.
        active:  GlyphAtlas,
        /// Atlas being populated by background workers in the new mode.
        pending: GlyphAtlas,
    },
}

impl AtlasSlot {
    /// What materials sample from this frame.
    #[must_use]
    pub const fn active(&self) -> &GlyphAtlas {
        match self {
            Self::Single(a) | Self::Swapping { active: a, .. } => a,
        }
    }

    /// Mutable access to the active atlas — used by GPU upload paths
    /// that flush dirty pages built by the active atlas.
    pub const fn active_mut(&mut self) -> &mut GlyphAtlas {
        match self {
            Self::Single(a) | Self::Swapping { active: a, .. } => a,
        }
    }

    /// Where new rasterizations should be queued. Returns the pending
    /// atlas during a swap so new glyph requests land in the atlas
    /// that will become active next.
    pub const fn rasterize_target_mut(&mut self) -> &mut GlyphAtlas {
        match self {
            Self::Single(a) => a,
            Self::Swapping { pending, .. } => pending,
        }
    }

    /// Distance-field variant the world is rendering with right now.
    #[must_use]
    pub fn distance_field(&self) -> DistanceField { self.active().distance_field() }

    /// Distance-field variant the slot is transitioning to, if any.
    #[must_use]
    pub fn target_distance_field(&self) -> Option<DistanceField> {
        match self {
            Self::Single(_) => None,
            Self::Swapping { pending, .. } => Some(pending.distance_field()),
        }
    }

    /// Whether a parallel-atlas swap is in flight. Text-shaping systems
    /// check this to gate the "Ready" emit — emitting new
    /// `PanelTextQuads` mid-swap would let the batcher build materials
    /// whose UVs come from `pending` but whose image handle still
    /// points to `active`, producing visible glyph corruption. With
    /// this gate, the text-shaping pass still queues every visible
    /// glyph onto `pending` via `rasterize_target_mut` but does not
    /// emit new quad data until the swap finalizes.
    #[must_use]
    pub const fn is_swapping(&self) -> bool { matches!(self, Self::Swapping { .. }) }

    /// Convenience accessors that delegate to `active()`.
    #[must_use]
    pub const fn width(&self) -> u32 { self.active().width() }

    /// Atlas page height.
    #[must_use]
    pub const fn height(&self) -> u32 { self.active().height() }

    /// GPU image handle for a given page on the active atlas.
    #[must_use]
    pub fn image_handle(&self, page: u32) -> Option<&Handle<Image>> {
        self.active().image_handle(page)
    }

    /// Number of atlas pages on the active atlas.
    #[must_use]
    pub const fn page_count(&self) -> usize { self.active().page_count() }

    /// Number of dirty pages across every contained atlas.
    #[must_use]
    pub fn total_dirty_page_count(&self) -> usize {
        match self {
            Self::Single(a) => a.dirty_page_count(),
            Self::Swapping { active, pending } => {
                active.dirty_page_count() + pending.dirty_page_count()
            },
        }
    }

    /// Moves all per-atlas GPU jobs into `out` for render extraction.
    pub(crate) fn drain_gpu_render_jobs(&mut self, out: &mut Vec<GpuRenderJob>) {
        match self {
            Self::Single(a) => a.drain_gpu_render_jobs(out),
            Self::Swapping { active, pending } => {
                active.drain_gpu_render_jobs(out);
                pending.drain_gpu_render_jobs(out);
            },
        }
    }

    /// Polls completed async glyph rasterizations on every contained
    /// atlas. During a swap, both `active` and `pending` drain so
    /// late-arriving results aren't dropped silently. Returns `true`
    /// if any rasterizations completed.
    pub fn poll_async_glyphs(&mut self) -> bool {
        match self {
            Self::Single(a) => a.poll_async_glyphs(),
            Self::Swapping { active, pending } => {
                let a = active.poll_async_glyphs();
                let p = pending.poll_async_glyphs();
                a || p
            },
        }
    }

    /// Aggregated diagnostic version of [`Self::poll_async_glyphs`].
    pub fn poll_async_glyphs_stats(&mut self) -> AsyncGlyphPollStats {
        match self {
            Self::Single(a) => a.poll_async_glyphs_stats(),
            Self::Swapping { active, pending } => {
                let mut stats = active.poll_async_glyphs_stats();
                let p = pending.poll_async_glyphs_stats();
                stats.completed += p.completed;
                stats.inserted += p.inserted;
                stats.invisible += p.invisible;
                stats.pages_added += p.pages_added;
                stats.max_raster_ms = stats.max_raster_ms.max(p.max_raster_ms);
                stats.max_active_jobs = stats.max_active_jobs.max(p.max_active_jobs);
                stats
            },
        }
    }

    /// Syncs dirty CPU pages on every contained atlas to GPU. During
    /// a swap, both `active` and `pending` flush — pending must have
    /// its pages uploaded before the swap completes so the next-frame
    /// material samples from valid GPU memory.
    pub fn sync_to_gpu(&mut self, images: &mut Assets<Image>) {
        match self {
            Self::Single(a) => a.sync_to_gpu(images),
            Self::Swapping { active, pending } => {
                active.sync_to_gpu(images);
                pending.sync_to_gpu(images);
            },
        }
    }

    /// Finalizes a `Swapping` → `Single` transition. The old `active`
    /// drops; `pending` becomes the new active. No-op if currently
    /// `Single`.
    pub fn complete_swap(&mut self) {
        let taken = std::mem::take(self);
        *self = match taken {
            Self::Swapping { pending, .. } => Self::Single(pending),
            single @ Self::Single(_) => single,
        };
    }
}

impl Default for AtlasSlot {
    fn default() -> Self { Self::Single(GlyphAtlas::default()) }
}

/// User-facing atlas knobs the driver watches. Any mismatch between
/// this preference and the active atlas's `(distance_field,
/// canonical_size)` tuple triggers a parallel-atlas swap.
///
/// Both fields default to the underlying type's `Default`:
/// [`DistanceField::Sdf`] and [`RasterQuality::Small`] (32 px).
/// The plugin seeds this resource from [`AtlasConfig`] at startup,
/// so apps that set up an `AtlasConfig` get their config values as
/// the initial preference.
///
/// [`AtlasConfig`]: super::atlas_config::AtlasConfig
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AtlasPreference {
    /// MSDF (multi-channel, sharp at corners) vs. SDF (single-channel,
    /// smooth on curves).
    pub distance_field: DistanceField,
    /// Canonical rasterization size in pixels.
    pub quality:        RasterQuality,
    /// Which device produces glyph distance-field bytes.
    pub backend:        RasterBackend,
}

/// Fired when the driver transitions the slot into the `Swapping`
/// state. Render-side observers re-mark visible text entities so the
/// text-shaping pass queues their glyphs onto the new pending atlas.
#[derive(bevy::ecs::event::Event, Clone, Copy, Debug)]
pub struct AtlasSwapStarted;

/// Fired when the driver completes the `Swapping` → `Single` transition.
///
/// Render-side observers re-mark visible text entities so the
/// text-shaping pass emits fresh quad data against the new active
/// atlas, which triggers the batcher to rebuild materials with the
/// new image handle.
#[derive(bevy::ecs::event::Event, Clone, Copy, Debug)]
pub struct AtlasSwapCompleted;
