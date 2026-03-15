//! Font registry backed by parley's `FontContext`.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;

use bevy::prelude::Resource;
use parley::FontContext;
use parley::fontique::Blob;
use parley::fontique::FontInfoOverride;

/// Embedded `JetBrains Mono` Regular font binary (SIL Open Font License).
const EMBEDDED_FONT: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");

/// Default font family name.
const DEFAULT_FAMILY: &str = "JetBrains Mono";

/// Unique identifier for a loaded font family within the diegetic UI system.
///
/// Maps to [`TextConfig::font_id`](crate::TextConfig). The registry assigns
/// these sequentially as fonts are registered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FontId(pub u16);

impl FontId {
    /// The built-in monospace font (`JetBrains Mono`).
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

    /// Registers a font from raw TTF/OTF bytes. Returns the assigned [`FontId`].
    pub fn register_font(&mut self, bytes: &[u8], family_name: &str) -> FontId {
        {
            let mut font_cx = self.font_cx.lock().unwrap_or_else(PoisonError::into_inner);
            font_cx.collection.register_fonts(
                Blob::from(bytes.to_vec()),
                Some(FontInfoOverride {
                    family_name: Some(family_name),
                    ..Default::default()
                }),
            );
        }

        #[allow(clippy::cast_possible_truncation)]
        let id = FontId(self.families.len() as u16);
        self.families.push((*family_name).to_string());
        id
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
