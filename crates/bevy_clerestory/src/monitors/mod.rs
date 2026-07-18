mod current_monitor;
mod identity;
mod topology;

use bevy::prelude::*;
pub use current_monitor::CurrentMonitor;
pub(crate) use current_monitor::update_current_monitor;
pub use identity::MonitorId;
pub use topology::MonitorConnected;
pub use topology::MonitorDisconnected;
pub use topology::MonitorInfo;
pub use topology::Monitors;
pub(crate) use topology::init_monitors;
use topology::update_monitors;

/// Plugin that manages the `Monitors` resource.
pub(crate) struct MonitorPlugin;

impl Plugin for MonitorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, init_monitors)
            .add_systems(Update, update_monitors);
    }
}
