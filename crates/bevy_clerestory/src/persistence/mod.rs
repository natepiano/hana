//! Window state persistence: state types, serialization format, and I/O.

mod constants;
mod format;
mod load;
mod save;
mod window_state;

pub use format::WindowKey;
pub(crate) use load::get_default_state_path;
pub(crate) use load::get_state_path_for_app;
pub(crate) use load::load_all_states;
pub(crate) use save::save_active_window_state;
pub(crate) use save::save_all_states;
pub(crate) use save::save_window_state;
pub(crate) use window_state::SavedWindowMode;
pub(crate) use window_state::WindowState;
