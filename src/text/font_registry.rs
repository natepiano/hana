//! Font registry backed by parley's `FontContext`.

use std::sync::Arc;
use std::sync::Mutex;

use bevy::prelude::Resource;
use parley::FontContext;
use parley::fontique::Blob;
use parley::fontique::FontInfoOverride;

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
/// maps [`FontId`] values to font family names for lookup during
/// text measurement.
///
/// The embedded `JetBrains Mono` font is always available as [`FontId::MONOSPACE`].
#[derive(Resource)]
pub struct FontRegistry {
    /// Shared font context — also held by the measurement closure.
    font_cx:  Arc<Mutex<FontContext>>,
    /// Map from `FontId` index to font family name.
    families: Vec<String>,
}

impl FontRegistry {
    /// Creates a new registry with the embedded default font.
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

        Self {
            font_cx:  Arc::new(Mutex::new(font_cx)),
            families: vec![(*DEFAULT_FAMILY).to_string()],
        }
    }

    /// Returns the family name for a given [`FontId`].
    #[must_use]
    pub fn family_name(&self, id: FontId) -> Option<&str> {
        self.families.get(id.0 as usize).map(String::as_str)
    }

    /// Returns the shared font context for use by the measurement closure.
    #[must_use]
    pub fn font_context(&self) -> Arc<Mutex<FontContext>> { Arc::clone(&self.font_cx) }

    /// Returns a cloned list of family names for the measurement closure.
    #[must_use]
    pub fn family_names(&self) -> Vec<String> { self.families.clone() }
}

impl Default for FontRegistry {
    fn default() -> Self { Self::new() }
}
