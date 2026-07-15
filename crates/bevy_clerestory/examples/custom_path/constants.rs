use bevy::prelude::*;

// app configuration
pub(super) const APP_DIRECTORY_NAME: &str = "my_custom_app";
pub(super) const PRIMARY_WINDOW_TITLE: &str = "Custom Path Example";
pub(super) const STATE_FILE_NAME: &str = "window_state.ron";

// fallback text
pub(super) const NOT_AVAILABLE_TEXT: &str = "N/A";

// layout
pub(super) const FONT_SIZE: f32 = 20.0;
pub(super) const MARGIN: Val = Val::Px(10.0);

// refresh rates
pub(super) const MILLIHERTZ_PER_HERTZ: u32 = 1000;
