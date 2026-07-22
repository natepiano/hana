use std::marker::PhantomData;

use bevy_app::App;
use bevy_app::Plugin;

use crate::DiskAssets;

/// Defines the global plugin entry point for startup asset loading.
///
/// The current implementation records plugin installation only. It installs no
/// loading, polling, or completion-delivery systems.
pub struct LadingPlugin;

impl Plugin for LadingPlugin {
    fn build(&self, app: &mut App) {
        tracing::debug!(
            entities = app.world().entities().len(),
            "installed LadingPlugin"
        );
    }
}

/// Defines plugin registration for one [`DiskAssets`] implementation.
///
/// This plugin ensures [`LadingPlugin`] is installed and records the asset-set
/// type. It does not call [`DiskAssets::load`] or insert the resulting resource.
pub struct DiskAssetsPlugin<T: DiskAssets>(PhantomData<fn() -> T>);

impl<T: DiskAssets> Default for DiskAssetsPlugin<T> {
    fn default() -> Self { Self(PhantomData) }
}

impl<T: DiskAssets> Plugin for DiskAssetsPlugin<T> {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<LadingPlugin>() {
            app.add_plugins(LadingPlugin);
        }
        tracing::debug!(
            asset_set = std::any::type_name::<T>(),
            "registered disk asset set plugin"
        );
    }
}
