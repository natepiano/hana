use crate::input::bindings::OrbitCamBindings;
use crate::input::bindings::error::OrbitCamBindingsError;

pub(super) trait OrbitCamPresetConfig: Sized {
    fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError>;
}
