mod current_monitor;
mod identity;
#[cfg(feature = "monitor-probe")]
mod monitor_probe;
mod topology;

use bevy::prelude::*;
pub use current_monitor::CurrentMonitor;
use current_monitor::clear_monitor_selection_inputs;
use current_monitor::install_current_monitor_from_association;
pub(crate) use current_monitor::update_current_monitor;
use identity::MonitorConfiguration;
pub use identity::MonitorId;
pub use identity::MonitorIdentity;
use identity::MonitorIdentityRegistry;
pub use topology::LiveMonitor;
pub use topology::MonitorConnected;
pub use topology::MonitorDisconnected;
pub use topology::MonitorInfo;
pub use topology::MonitorTopologyRevision;
pub use topology::Monitors;
use topology::init_monitors;
use topology::update_monitors;

use crate::ClerestoryPreStartupSet;
use crate::Platform;

/// Plugin that manages the `Monitors` resource.
pub(crate) struct MonitorPlugin;

impl Plugin for MonitorPlugin {
    fn build(&self, app: &mut App) {
        let configuration = MonitorConfiguration::register(*app.world().resource::<Platform>());
        app.insert_resource(configuration)
            .init_resource::<MonitorIdentityRegistry>()
            .add_observer(clear_monitor_selection_inputs)
            .add_observer(install_current_monitor_from_association)
            .add_systems(
                PreStartup,
                init_monitors.in_set(ClerestoryPreStartupSet::MonitorsInitialized),
            )
            .add_systems(Update, (update_monitors, update_current_monitor).chain());
    }
}
