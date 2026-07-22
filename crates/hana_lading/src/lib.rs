//! Public API for startup disk-asset loading and completion reporting in Bevy.
//!
//! Applications define a startup asset set by implementing [`DiskAssets`] and
//! register its plugin type with [`DiskAssetsPlugin`]. The crate defines the
//! asset-set, loader, plugin, readable [`LoadProgress`] resource, and
//! completion-event APIs. The current plugin implementations establish the
//! registration relationship only; they do not yet load, poll, insert asset-set
//! resources, or deliver completion events.
//!
//! Application policy remains outside this crate. In particular, an
//! application decides whether a failed set blocks startup, enters a degraded
//! mode, or records additional failure details.

mod disk_asset_loader;
mod events;
mod lading_plugin;

pub use disk_asset_loader::DiskAssetLoader;
pub use disk_asset_loader::DiskAssets;
pub use events::AllSetsLoaded;
pub use events::AllSetsResolved;
pub use events::AssetSetLoadFailed;
pub use events::LoadFailed;
pub use events::LoadProgress;
pub use events::Loaded;
pub use lading_plugin::DiskAssetsPlugin;
pub use lading_plugin::LadingPlugin;
