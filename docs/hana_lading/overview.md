# `hana_lading`

`hana_lading` defines a tracked startup-loading API for assets loaded from disk.
An application groups related handles in a resource that implements
`DiskAssets`, then registers that resource type with `DiskAssetsPlugin<T>`.

Phase 1 defines the public asset-set, loader, plugin, progress-resource, and
completion-event types. The current plugins establish their registration
relationship but do not start loads or insert a `DiskAssets` resource. Loading,
polling, resource insertion, and event delivery arrive in Phase 2.

## Loading API

`DiskAssets::load` receives a `DiskAssetLoader`. Its `load` and
`load_with_settings` methods start Bevy asset loads, return typed handles for
the resource, and retain one additional strong handle for completion tracking.
Every tracked handle must contain an `AssetPath`; a pathless handle triggers a
panic that names the attempted path because path-based completion tracking
cannot resolve it.

An `AssetPath` can include a label. For example, callers can pass a path created
with `GltfAssetLabel::Scene(0).from_asset(path)` while keeping that load inside
the tracked API.

## Completion evidence

Typed events report success or failure for one `DiskAssets` implementation:

- `Loaded<T>` reports a successful set.
- `LoadFailed<T>` provides the failed path and shared `AssetLoadError`.

Global events let application code observe loading without knowing each set's
concrete type:

- `AssetSetLoadFailed` identifies a failed set by `TypeId`, type name, path,
  and shared error.
- `AllSetsLoaded` reports that every set succeeded.
- `AllSetsResolved` reports that every set reached a terminal result and
  includes the failed-set count.

`LoadProgress` is readable resource state with loaded and total set counts. Its
fields and event fields are private and exposed through read-only accessors.
Only `hana_lading` constructs completion evidence and updates progress state,
so every observer reads the same values.

## Application boundary

`hana_lading` reports outcomes but does not choose application policy. The
application owns failure severity, durable failure records, state transitions,
and any degraded behavior. It can observe `Loaded<T>`, `AllSetsLoaded`, or
`AllSetsResolved` and update its own state.

The crate does not cover hot reload or unload watching, runtime registration of
asset sets, runtime-generated media, or application startup policy.
