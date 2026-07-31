//! Keymap data and runtime support.

mod compiled;
mod constants;
mod document;
mod merged;
mod schema;

pub(crate) use compiled::CompiledKeymap;
pub(crate) use compiled::Generation;
pub(crate) use document::KeymapDocument;
pub(crate) use merged::MergedKeymap;
#[expect(
    unused_imports,
    reason = "keymap reload inspects resolved edits before compiling a replacement"
)]
pub(crate) use merged::ResolvedEdits;
#[cfg_attr(
    not(test),
    expect(
        unused_imports,
        reason = "the plugin's finish step is the only non-test caller of this entry point"
    )
)]
pub(crate) use schema::schema_bytes;
