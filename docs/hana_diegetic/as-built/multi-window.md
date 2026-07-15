# hana_diegetic — multi-window screen-space panels

Screen-space diegetic panels (title bars, control panels, description panels)
can target any `Window` entity, not just the primary one. Each panel carries a
`WindowRef`; the screen-space system positions it against that window's
dimensions and spawns an overlay camera whose `Camera.target` points at that
window. Single-window apps behave as if the field weren't there — the default
is `WindowRef::Primary`.

All code lives in `crates/hana_diegetic/src/screen_space/mod.rs`, plus the
`window` field on `CoordinateSpace::Screen`
(`panel/coordinate_space.rs`) and the builder methods (`panel/builder.rs`).

## Public API

`CoordinateSpace::Screen` (in `panel/coordinate_space.rs`) carries a
`window: WindowRef`:

```rust
Screen {
    position:      ScreenPosition,
    width:         Sizing,
    height:        Sizing,
    camera_order:  isize,
    render_layers: RenderLayers,
    /// Defaults to `WindowRef::Primary`.
    window:        WindowRef,
}
```

Builder methods on `DiegeticPanelBuilder<Screen, ...>` (`panel/builder.rs`,
both `const fn`):

- `.window(window: WindowRef) -> Self`
- `.window_entity(entity: Entity) -> Self` — sugar for
  `.window(WindowRef::Entity(entity))`

`new_screen()` initializes `window` to `WindowRef::Primary`, so any call site
that never calls `.window(...)` targets the primary window — identical to a
single-window app.

Non-goals (still not supported): one panel mirrored into multiple windows;
runtime reparenting of a panel between windows (association is set at build
time); auto-picking a window by cursor/focus; per-window camera-order
namespacing (orders collide across windows if a caller reuses them — caller's
responsibility).

## Window resolution

`resolve_window_ref(window_ref, &primary) -> Option<Entity>`:

- `WindowRef::Primary` → `primary.single().ok()`. Missing `PrimaryWindow`
  (e.g. a headless test with no `WindowPlugin`) returns `None` and emits
  `warn_once!` so the dropped panel is visible instead of silent.
- `WindowRef::Entity(e)` → `Some(e)` unconditionally.

Every screen-space system takes `primary: Query<Entity, With<PrimaryWindow>>`
and routes through this one helper. It is the single site to change if Bevy
ever allows more than one `PrimaryWindow`.

`window_size_map(&windows) -> HashMap<Entity, (f32, f32)>` builds an
`Entity → (width, height)` map each frame, skipping windows with a zero
dimension (unsized on frame 1). Bounded by window count (1–3), so the
per-panel lookup is one `HashMap::get`.

## Positioning

Two `Update` systems, both iterating panels and resolving each against its own
window via `resolve_window_ref` + the size map. A panel whose window doesn't
resolve or isn't in the size map is skipped (`continue`) — one bad window
never disables positioning for the others.

- `resolve_screen_space_panel_dimensions` — converts each panel's `width` /
  `height` `Sizing` to pixels against its window via `resolve_screen_axis`
  (`Fixed`→value, `Percent`→`window_axis*frac`, `Fit`→clamped content size,
  `Grow`→window axis clamped to `[min,max]`), then triggers
  `PanelDimensionsChanged`. Runs after the world-fit layout pass so `Fit`
  panels size from measured content.
- `position_screen_space_panels` — sets `Transform.translation` from the
  resolved anchor position and window half-extents, applying any resolved
  depth/rotation overrides. Runs after `PanelDimensionsChanged` observers.

## Overlay camera and light

`setup_screen_space_view_for_panel` (called from the `On<Add, DiegeticPanel>`
observer `setup_screen_space_view` and the `Changed<DiegeticPanel>` system
`setup_changed_screen_space_views`) spawns, per screen-space panel:

- One **camera** per unique `(camera_order, render_layers, window)` triple.
  Orthographic, `ScalingMode::WindowSize` (1 world unit = 1 logical pixel),
  `ClearColorConfig::None`, `RenderTarget::Window(WindowRef::Entity(window))`.
  Marked with `ScreenSpaceCamera { render_layers, order, window: Entity }`.
  Sharing is detected by scanning existing `ScreenSpaceCamera` components for
  a matching triple — no side registry.
- One **directional light** per unique `render_layers` (app-wide singleton per
  layer), marked `ScreenSpaceLight { render_layers }`.

The light is keyed by `render_layers` **only**, not by window: directional-light
contributions accumulate across cameras sharing a layer in Bevy's PBR shader.
A second identical light on the same layer would double the illuminance of
every panel on that layer app-wide, so opening a second window would visibly
brighten existing panels. One light per layer keeps single-window brightness
exact.

## Cleanup

Two observers, single-owner teardown:

- `cleanup_screen_space_view` (`On<Remove, DiegeticPanel>`) is the **sole
  owner** of camera and light despawn. For each matching camera it checks
  whether any *other* surviving panel still resolves to that camera's window;
  if none, despawns the camera. It iterates cameras (not windows) so it can
  reap orphan cameras whose `WindowRef::Primary` panels can no longer resolve
  after the primary window itself was despawned. The light is despawned only
  when no panel on its layer survives in *any* window.
- `cleanup_screen_space_on_window_close` (`On<Remove, Window>`) despawns
  **only** the panels whose target window matches the removed entity. Their
  cameras/lights are torn down as a cascade through
  `cleanup_screen_space_view`.

Routing all camera/light despawn through the one observer prevents a
double-despawn panic (despawning an already-despawned entity panics). Cleanup
is observer-based, not hierarchy-based: cameras/lights/panels are deliberately
**not** parented to the window entity (a `Camera` carries a `Transform`, a
`Window` does not — parenting would impose an unusual hierarchy convention).

Both `On<Remove>` observers read the panel's `CoordinateSpace::Screen` fields
while the component is still live (`On<Remove>` fires before the component is
dropped).

## Plugin registration

`ScreenSpacePlugin::build` registers all three observers
(`setup_screen_space_view`, `cleanup_screen_space_view`,
`cleanup_screen_space_on_window_close`) and the `Update` systems under the
`ScreenSpaceSystems` set ordering (dimension resolve → observer flush →
attachment resolve → position). `propagate_screen_space_render_layers` runs in
`PostUpdate` after `PanelChildSystems::Build`; it walks panel-child hierarchies
propagating `RenderLayers` down and is window-agnostic.

## Gotchas

- A panel targeting `WindowRef::Entity(e)` for a window that never existed (or
  is unsized this frame) is silently skipped by the positioning systems — no
  camera draws it and no warning fires (only `Primary`-without-`PrimaryWindow`
  warns). Verify the window entity is spawned and sized.
- Camera orders are not namespaced per window. Two panels in two windows with
  the same `camera_order` get two distinct cameras (keyed by the window), but
  their `Camera.order` values still collide within Bevy's global camera-order
  space — pick distinct orders across windows if ordering matters.
- The demo `screen_space.rs` example is single-window. There is no shipped
  two-window example.
