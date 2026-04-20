//! Font registry backed by parley's `FontContext`.

use std::sync::Arc;
use std::sync::Mutex;

use bevy::prelude::Event;
use bevy::prelude::Resource;
use bevy_kana::ToU16;

/// How a font was loaded into the registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontSource {
    /// Embedded in the library binary (e.g., `JetBrains Mono`).
    Embedded,
    /// Loaded at runtime via `AssetServer` or `Assets<Font>::add()`.
    Loaded,
}

impl std::fmt::Display for FontSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Embedded => write!(f, "embedded"),
            Self::Loaded => write!(f, "loaded"),
        }
    }
}

/// Fired when a font is successfully registered and ready for use.
///
/// Observe this event to react when fonts become available — works for
/// both the embedded default font and fonts loaded via `AssetServer`.
///
/// ```ignore
/// app.add_observer(|trigger: On<FontRegistered>| {
///     info!("{} font ready: {} (id: {:?})", trigger.source, trigger.name, trigger.id);
/// });
/// ```
#[derive(Event, Clone, Debug)]
pub struct FontRegistered {
    /// The [`FontId`] assigned to this font.
    pub id:     FontId,
    /// The font family name.
    pub name:   String,
    /// Whether this font was embedded or loaded at runtime.
    pub source: FontSource,
}

/// Fired when a font file fails to load or parse.
///
/// Covers both I/O errors (file not found) and parse errors (corrupt
/// font data). Observe this event to show error UI or fall back to
/// a default font.
///
/// ```ignore
/// app.add_observer(|trigger: On<FontLoadFailed>| {
///     warn!("Font failed: {} — {}", trigger.path, trigger.error);
/// });
/// ```
#[derive(Event, Clone, Debug)]
pub struct FontLoadFailed {
    /// The asset path that failed to load.
    pub path:  String,
    /// Human-readable error description.
    pub error: String,
}
use parley::FontContext;
use parley::fontique::Blob;
use parley::fontique::FontInfoOverride;

use super::constants::DEFAULT_FAMILY;
use super::constants::EMBEDDED_FONT;
use super::font::Font;

/// Unique identifier for a loaded font family.
///
/// Used with [`TextConfig::with_font`](crate::TextConfig::with_font) and
/// [`TextStyle::with_font`](crate::TextStyle::with_font) to select which
/// font a text element uses.
///
/// Currently the only available font is [`MONOSPACE`](Self::MONOSPACE)
/// (`JetBrains Mono`), which is embedded in the library and used by default.
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
    /// Returns `None` if the embedded `JetBrains Mono` font fails to parse.
    /// This is infallible in practice because the font binary is compiled into
    /// the library and is known to be valid.
    #[must_use]
    pub fn new() -> Option<Self> {
        let mut font_cx = FontContext::default();

        font_cx.collection.register_fonts(
            Blob::from(EMBEDDED_FONT.to_vec()),
            Some(FontInfoOverride {
                family_name: Some(DEFAULT_FAMILY),
                ..Default::default()
            }),
        );

        let embedded_font = Font::from_bytes(DEFAULT_FAMILY, EMBEDDED_FONT)?;

        Some(Self {
            font_cx: Arc::new(Mutex::new(font_cx)),
            fonts:   vec![embedded_font],
        })
    }

    /// Returns the [`Font`] for a given [`FontId`].
    #[must_use]
    pub fn font(&self, id: impl Into<FontId>) -> Option<&Font> { self.fonts.get(usize::from(id.into().0)) }

    /// Returns the family name for a given [`FontId`].
    #[must_use]
    pub fn family_name(&self, id: FontId) -> Option<&str> { self.font(id).map(Font::name) }

    /// Registers an additional font from raw TTF/OTF bytes.
    ///
    /// Returns the [`FontId`] assigned to the new font, or `None` if the
    /// font data cannot be parsed.
    ///
    /// The font is immediately available for use in `TextConfig` and
    /// `TextStyle` via `.with_font(id.0)`. Glyphs are rasterized
    /// on demand into the MSDF atlas when text using this font is first
    /// rendered.
    ///
    /// # Example
    ///
    /// ```ignore
    /// const NOTO_SANS: &[u8] = include_bytes!("NotoSans-Regular.ttf");
    ///
    /// fn setup(mut registry: ResMut<FontRegistry>) {
    ///     let id = registry.register_font("Noto Sans", NOTO_SANS)
    ///         .expect("font should parse");
    ///     // Use `id.0` with `TextConfig::new(size).with_font(id.0)`.
    /// }
    /// ```
    pub fn register_font(&mut self, name: &str, data: &[u8]) -> Option<FontId> {
        let font = Font::from_bytes(name, data)?;

        // Register with parley's font collection.
        let mut font_cx = self
            .font_cx
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        font_cx.collection.register_fonts(
            Blob::from(data.to_vec()),
            Some(FontInfoOverride {
                family_name: Some(name),
                ..Default::default()
            }),
        );
        drop(font_cx);

        let id = FontId(self.fonts.len().to_u16());
        self.fonts.push(font);
        Some(id)
    }

    /// Returns the [`FontId`] for a font with the given family name.
    ///
    /// Returns `None` if no font with that name has been registered.
    #[must_use]
    pub fn font_id_by_name(&self, name: &str) -> Option<FontId> {
        self.fonts
            .iter()
            .position(|f| f.name() == name)
            .map(|i| FontId(i.to_u16()))
    }

    /// Returns the shared font context for use by the measurement closure.
    #[must_use]
    pub fn font_context(&self) -> Arc<Mutex<FontContext>> { Arc::clone(&self.font_cx) }

    /// Returns a cloned list of family names for the measurement closure.
    #[must_use]
    pub fn family_names(&self) -> Vec<String> {
        self.fonts.iter().map(Font::name).map(String::from).collect()
    }
}
