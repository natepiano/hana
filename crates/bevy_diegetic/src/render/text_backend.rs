//! Text renderer backend selection.

use bevy::prelude::Resource;

/// Text rendering backend used after shaping and glyph placement.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextRendererBackend {
    /// Existing distance-field atlas renderer.
    #[default]
    DistanceField,
    /// Experimental Slug curve-backed renderer.
    #[cfg(feature = "slug_text")]
    Slug,
}

/// Global text renderer preference.
#[derive(Clone, Copy, Debug, Default, Resource)]
pub struct TextRendererPreference {
    backend: TextRendererBackend,
}

impl TextRendererPreference {
    /// Creates a preference for `backend`.
    #[must_use]
    pub const fn new(backend: TextRendererBackend) -> Self { Self { backend } }

    /// Creates a preference for the experimental Slug renderer.
    #[cfg(feature = "slug_text")]
    #[must_use]
    pub const fn slug() -> Self {
        Self {
            backend: TextRendererBackend::Slug,
        }
    }

    /// Selected text renderer backend.
    #[must_use]
    pub const fn backend(self) -> TextRendererBackend { self.backend }
}
