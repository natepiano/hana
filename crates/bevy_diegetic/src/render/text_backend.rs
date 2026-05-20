//! Text renderer backend selection.

use bevy::prelude::Resource;
use bevy::reflect::Reflect;

/// Text rendering backend used after shaping and glyph placement.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Reflect)]
pub enum TextRenderer {
    /// Existing distance-field atlas renderer.
    #[default]
    DistanceField,
    /// Experimental Slug curve-backed renderer.
    Slug,
}

/// Global text renderer preference.
#[derive(Clone, Copy, Debug, Default, Resource)]
pub struct TextRendererPreference {
    backend: TextRenderer,
}

impl TextRendererPreference {
    /// Creates a preference for `backend`.
    #[must_use]
    pub const fn new(backend: TextRenderer) -> Self { Self { backend } }

    /// Creates a preference for the experimental Slug renderer.
    #[must_use]
    pub const fn slug() -> Self {
        Self {
            backend: TextRenderer::Slug,
        }
    }

    /// Selected text renderer backend.
    #[must_use]
    pub const fn backend(self) -> TextRenderer { self.backend }

    /// Sets the selected text renderer backend.
    pub const fn set_backend(&mut self, backend: TextRenderer) { self.backend = backend; }
}
