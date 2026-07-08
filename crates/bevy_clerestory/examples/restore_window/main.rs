//! Interactive example for testing window restoration, fullscreen modes, and multi-window
//! management.
//!
//! Run with: `cargo run --example restore_window`
//!
//! Controls (all windows):
//! - Press `Enter` for exclusive fullscreen (uses selected video mode)
//! - Press `B` for borderless fullscreen
//! - Press `W` for windowed mode
//! - Press `Up`/`Down` to cycle through available video modes
//! - Press `Space` to spawn a new managed window
//! - Press `P` to toggle persistence mode (`RememberAll` / `ActiveOnly`)
//! - Press `Ctrl+Shift+Backspace` to clear saved state and quit
//! - Press `Q` to quit

mod constants;
mod debug;
mod display;
mod events;
mod input;
mod mode_observers;
mod setup;

use std::env::var;

use bevy::pbr::PbrPlugin;
use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasPlugin;
use bevy_clerestory::WindowManagerPlugin;
use constants::PRIMARY_WINDOW_TITLE;
use constants::TEST_MODE_ENVIRONMENT_VARIABLE;
use events::MismatchStates;
use events::RestoredStates;
use events::WindowsSettledCount;
use input::KeyboardInputMode;
use input::SelectedVideoModes;
use setup::WindowCounter;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: PRIMARY_WINDOW_TITLE.into(),
                        ..default()
                    }),
                    ..default()
                })
                // This window manager renders only flat UI, so GPU mesh preprocessing and its
                // frustum-culling compute pass are pure overhead. Disabling them also avoids a
                // startup crash on GPUs whose `max_storage_buffers_per_shader_stage` is below the
                // 8 that the frustum-culling bind group requires (e.g. Asahi/Mesa, limit 6).
                .set(PbrPlugin {
                    use_gpu_instance_buffer_builder: false,
                    ..default()
                }),
        )
        .add_plugins(WindowManagerPlugin)
        .add_plugins(BrpExtrasPlugin::default())
        .add_observer(setup::on_spawn_managed_window)
        .add_observer(events::on_window_restored)
        .add_observer(events::on_window_restore_mismatch)
        .add_observer(setup::on_secondary_window_added)
        .add_observer(setup::on_secondary_window_removed)
        .add_observer(mode_observers::on_set_borderless_fullscreen)
        .add_observer(mode_observers::on_set_windowed)
        .add_observer(mode_observers::on_set_exclusive_fullscreen)
        .add_observer(mode_observers::on_toggle_persistence)
        .add_observer(mode_observers::on_clear_state_and_quit)
        .add_observer(mode_observers::on_quit_app)
        .add_observer(debug::on_monitor_connected)
        .add_observer(debug::on_monitor_disconnected)
        .insert_resource(KeyboardInputMode::from(
            var(TEST_MODE_ENVIRONMENT_VARIABLE).is_err(),
        ))
        .init_resource::<SelectedVideoModes>()
        .init_resource::<WindowCounter>()
        .init_resource::<RestoredStates>()
        .init_resource::<MismatchStates>()
        .init_resource::<WindowsSettledCount>()
        .add_systems(Startup, (setup::setup, debug::log_monitor_ids))
        .add_systems(
            Update,
            (
                display::update_primary_display,
                display::update_secondary_displays,
                input::handle_global_input.run_if(input::keyboard_enabled),
                input::handle_window_mode_input.run_if(input::keyboard_enabled),
                debug::debug_winit_monitor,
                debug::debug_window_changed,
                debug::debug_scale_factor_changed,
            ),
        )
        .run();
}
