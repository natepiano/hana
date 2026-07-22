# `hana_lading`

`hana_lading` tracks fixed startup asset sets loaded from disk. An application
groups related handles in a `DiskAssets` resource, registers
`DiskAssetsPlugin<T>`, and observes typed or type-erased terminal events before
making its own startup decision.

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

## Protective examples

`successful_loading` is the normal-use example. It applies a loaded PNG to its
cube in `Loaded<StartupAssets>`, enters `Ready` after `AllSetsLoaded`, and keeps
the set result, aggregate progress, scene use, and application decision visible
in a persistent panel and reflected evidence.

`degraded_failure` loads one required PNG and fails one optional PNG. It retains
required content, records the generic failure, enters `Ready`, and explains the
degraded decision. `catastrophic_failure` fails a required PNG, records and
renders the same evidence, and remains in `Loading`.

All three examples use Fairy Dust for presentation. After selecting the
crate-local asset root, they obtain the underlying `&mut App` through
`app_mut()` and register each startup set through `DiskAssetsPlugin<T>` using
ordinary Bevy methods. Their reflected state, evidence resources, panel content,
and scene markers provide BRP-readable evidence that matches the visible
result.

The examples show that every returned handle is tracked, failures terminate,
generic observers can record all set types, direct records precede global
completion, and policy stays application-owned. The Phase 3
`recursive_dependencies_gate_and_fail` contract test separately proves that a
set waits for recursive dependencies, because PNG fixtures have no recursive
children.
