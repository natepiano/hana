//! Asset loader for `.ttf` and `.otf` font files.

use bevy::asset::AssetLoader;
use bevy::asset::LoadContext;
use bevy::asset::io::Reader;
use bevy::reflect::TypePath;

use super::font::Font;

/// Loads `.ttf` and `.otf` font files into [`Font`] assets.
///
/// Registered automatically by [`DiegeticUiPlugin`](crate::DiegeticUiPlugin).
/// When a font asset finishes loading, the plugin registers it with
/// [`FontRegistry`](crate::FontRegistry) and fires a
/// [`FontRegistered`](crate::FontRegistered) event.
#[derive(Default, TypePath)]
pub struct FontLoader;

impl AssetLoader for FontLoader {
    type Asset = Font;
    type Settings = ();
    type Error = FontLoaderError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;

        // Extract family name from the font's name table.
        let path_str = load_context.path().to_string();
        let name = ttf_parser::Face::parse(&bytes, 0)
            .ok()
            .and_then(|face| {
                face.names()
                    .into_iter()
                    .filter(|n| n.name_id == ttf_parser::name_id::TYPOGRAPHIC_FAMILY)
                    .find_map(|n| n.to_string())
                    .or_else(|| {
                        face.names()
                            .into_iter()
                            .filter(|n| n.name_id == ttf_parser::name_id::FAMILY)
                            .find_map(|n| n.to_string())
                    })
            })
            .unwrap_or(path_str);

        Font::from_bytes(&name, &bytes).ok_or(FontLoaderError::ParseFailed)
    }

    fn extensions(&self) -> &[&str] { &["ttf", "otf"] }
}

/// Errors that can occur when loading a font file.
#[derive(Debug)]
pub enum FontLoaderError {
    /// I/O error reading the font file.
    Io(std::io::Error),
    /// The font data could not be parsed.
    ParseFailed,
}

impl From<std::io::Error> for FontLoaderError {
    fn from(err: std::io::Error) -> Self { Self::Io(err) }
}

impl std::fmt::Display for FontLoaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "failed to read font file: {err}"),
            Self::ParseFailed => write!(f, "failed to parse font data"),
        }
    }
}

impl std::error::Error for FontLoaderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::ParseFailed => None,
        }
    }
}
