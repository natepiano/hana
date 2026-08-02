mod dispatch;
mod held;
mod key_edge;

use bevy_enhanced_input::prelude::CustomInput;
use bevy_enhanced_input::prelude::CustomInputs;
pub(crate) use dispatch::cancel_pending_sequences;
pub(crate) use dispatch::reset_physical_input;
pub(crate) use dispatch::route_input;
pub(crate) use held::KeymapRuntime;

pub(super) fn set_event_source(
    keymap_runtime: &mut KeymapRuntime,
    custom_input: CustomInput,
    is_active: bool,
    custom_inputs: &mut CustomInputs,
) {
    keymap_runtime.set_event_source(custom_input, is_active, custom_inputs);
}
