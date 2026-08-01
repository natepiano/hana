//! Keymap data and runtime support.

use bevy_enhanced_input::prelude::CustomInput;
use bevy_enhanced_input::prelude::CustomInputs;

mod compiled;
mod constants;
mod document;
mod merged;
mod reload;
mod runtime;
mod schema;

pub(super) use compiled::CommandHandle;
pub(crate) use compiled::CompiledKeymap;
pub(crate) use compiled::Generation;
pub(crate) use document::KeymapDocument;
pub(crate) use merged::MergedKeymap;
pub(crate) use reload::PendingReload;
pub(crate) use reload::ReloadConfiguration;
pub(crate) use reload::ReloadRequest;
pub(crate) use reload::commit_defaults;
pub(crate) use reload::commit_reload;
pub(super) use runtime::KeymapRuntime;
pub(super) use runtime::cancel_pending_sequences;
pub(super) use runtime::route_input;
pub(crate) use schema::reference_default_bytes;
pub(crate) use schema::schema_bytes;

pub(super) fn set_event_source(
    keymap_runtime: &mut KeymapRuntime,
    custom_input: CustomInput,
    is_active: bool,
    custom_inputs: &mut CustomInputs,
) {
    runtime::set_event_source(keymap_runtime, custom_input, is_active, custom_inputs);
}
