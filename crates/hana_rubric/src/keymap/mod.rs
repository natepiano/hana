//! Keymap data and runtime support.

mod compiled;
mod constants;
mod document;
mod merged;
pub(crate) mod runtime;
mod schema;

pub(super) use compiled::CommandHandle;
pub(crate) use compiled::CompiledKeymap;
pub(crate) use compiled::Generation;
pub(crate) use document::KeymapDocument;
pub(crate) use merged::MergedKeymap;
pub(crate) use runtime::route_input;
#[cfg_attr(
    not(test),
    expect(
        unused_imports,
        reason = "the plugin's finish step is the only non-test caller of this entry point"
    )
)]
pub(crate) use schema::schema_bytes;
