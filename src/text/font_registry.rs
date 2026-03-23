//! Font registry backed by parley's `FontContext`.

use std::sync::Arc;
use std::sync::Mutex;

use bevy::prelude::Resource;
use parley::FontContext;
use parley::fontique::Blob;
use parley::fontique::FontInfoOverride;

use super::font::Font;

/// Embedded `JetBrains Mono` Regular font binary (SIL Open Font License).
pub const EMBEDDED_FONT: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");

/// Default font family name.
const DEFAULT_FAMILY: &str = "JetBrains Mono";

/// Unique identifier for a loaded font family.
///
/// Used with [`TextConfig::with_font`](crate::TextConfig::with_font) and
/// [`TextStyle::with_font`](crate::TextStyle::with_font) to select which
/// font a text element uses.
///
/// Currently the only available font is [`MONOSPACE`](Self::MONOSPACE)
/// (JetBrains Mono), which is embedded in the library and used by default.
/// Custom font loading will be added in a future release.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FontId(pub u16);

impl FontId {
    /// The built-in monospace font (`JetBrains Mono`).
    ///
    /// This is the default font used by all text when no font is specified.
    pub const MONOSPACE: Self = Self(0);
}

/// Resource managing font loading via parley's `FontContext`.
///
/// Fonts are registered by embedding raw TTF/OTF bytes. The registry
/// maps [`FontId`] values to [`Font`] structs that provide access to
/// font-level typographic metrics.
///
/// The embedded `JetBrains Mono` font is always available as [`FontId::MONOSPACE`].
///
/// Access the registry in Bevy systems via `Res<FontRegistry>`:
///
/// ```ignore
/// fn my_system(registry: Res<FontRegistry>) {
///     let font = registry.font(FontId::MONOSPACE).unwrap();
///     let metrics = font.metrics(48.0);
///     info!("ascent: {}", metrics.ascent);
/// }
/// ```
#[derive(Resource)]
pub struct FontRegistry {
    /// Shared font context — also held by the measurement closure.
    font_cx: Arc<Mutex<FontContext>>,
    /// Parsed fonts indexed by [`FontId`].
    fonts:   Vec<Font>,
}

impl FontRegistry {
    /// Creates a new registry with the embedded default font.
    ///
    /// # Panics
    ///
    /// Panics if the embedded `JetBrains Mono` font fails to parse. This is
    /// infallible in practice because the font binary is compiled into the
    /// library and is known to be valid.
    #[must_use]
    pub fn new() -> Self {
        let mut font_cx = FontContext::default();

        font_cx.collection.register_fonts(
            Blob::from(EMBEDDED_FONT.to_vec()),
            Some(FontInfoOverride {
                family_name: Some(DEFAULT_FAMILY),
                ..Default::default()
            }),
        );

        // Pre-parse the embedded font's metrics. This is infallible for our
        // known-good embedded font, but we handle the Option for robustness.
        let embedded_font = Font::from_bytes(DEFAULT_FAMILY, EMBEDDED_FONT)
            .expect("embedded JetBrains Mono font should parse successfully");

        Self {
            font_cx: Arc::new(Mutex::new(font_cx)),
            fonts:   vec![embedded_font],
        }
    }

    /// Returns the [`Font`] for a given [`FontId`].
    #[must_use]
    pub fn font(&self, id: impl Into<FontId>) -> Option<&Font> {
        self.fonts.get(id.into().0 as usize)
    }

    /// Returns the family name for a given [`FontId`].
    #[must_use]
    pub fn family_name(&self, id: FontId) -> Option<&str> { self.font(id).map(Font::name) }

    /// Returns the shared font context for use by the measurement closure.
    #[must_use]
    pub fn font_context(&self) -> Arc<Mutex<FontContext>> { Arc::clone(&self.font_cx) }

    /// Returns a cloned list of family names for the measurement closure.
    #[must_use]
    pub fn family_names(&self) -> Vec<String> {
        self.fonts.iter().map(|f| (*f.name()).to_string()).collect()
    }
}

impl Default for FontRegistry {
    fn default() -> Self { Self::new() }
}
