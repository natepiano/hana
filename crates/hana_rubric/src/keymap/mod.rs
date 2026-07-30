//! Keymap data and runtime support.

mod constants;
mod document;

#[expect(
    unused_imports,
    reason = "KeymapDocument is the crate-level entry point for later keymap loading stages"
)]
pub(crate) use document::KeymapDocument;
