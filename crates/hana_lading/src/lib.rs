//! Public API for startup disk-asset loading and completion reporting in Bevy.
//!
//! Applications define a startup asset set by implementing [`DiskAssets`] and
//! register its plugin type with [`DiskAssetsPlugin`]. The crate defines the
//! asset-set, loader, plugin, readable [`LoadProgress`] resource, and
//! completion-event APIs.
//!
//! Each set resource exists before its [`Loaded`] event. At that point every
//! tracked root and recursive dependency is loaded into Bevy's asset storage;
//! renderer-specific GPU preparation may still be pending. Progress is updated
//! before per-set observers run, and per-set observers run before global
//! completion observers: their direct resource mutations are visible to global
//! completion observers, but commands they queue are not guaranteed to apply
//! before global completion. Each set emits exactly one of
//! [`Loaded`] or [`LoadFailed`]. On failure, the type-erased
//! [`AssetSetLoadFailed`] event is delivered immediately before the typed
//! [`LoadFailed`] event. Cross-set terminal-event order is unspecified.
//!
//! [`AllSetsLoaded`] is emitted only when every registered set succeeds.
//! [`AllSetsResolved`] always follows it for a clean batch and is the only
//! global event emitted for a batch containing failures. Only direct resource
//! mutations made by one completion observer are guaranteed to be visible to
//! the following completion observer; deferred commands wait for the next
//! command flush.
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
