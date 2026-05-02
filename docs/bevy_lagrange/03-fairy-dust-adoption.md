# Phase 3: `fairy_dust` adoption of the new `bevy_lagrange` input model

**Status:** plan, not yet implemented.
**Depends on:** [Phase 2](./02-input-refactor.md) — `bevy_lagrange 0.0.4` must ship the public `OrbitCamInput`, the `enhanced_input` cargo feature, and the interaction lifecycle events before this phase starts.
**Owner:** natepiano.

## Why this phase exists

After Phase 2, `bevy_lagrange` is input-source-agnostic — but `fairy_dust`'s public API still assumes the `raw_input` model: `with_orbit_cam_configured` takes `FnOnce(&mut OrbitCam)`, the closure mutates `OrbitCam.button_orbit` etc. to set bindings. Under `enhanced_input` those mutations are silent no-ops because bindings live in the action context. We need to:

1. Make `bevy_enhanced_input` the opinionated default in fairy_dust (the original motivation for the whole input refactor).
2. Prevent the silent-no-op trap by typestate-splitting the camera-configuration capability so the type system enforces that you mutate the right thing for the input mode you're in.
3. Surface the new interaction events so the camera control panel (and other UI affordances) can highlight active gestures.

## Steps

### 1. Take a dependency on the in-workspace `bevy_lagrange` with `enhanced_input` feature on

`crates/fairy_dust/Cargo.toml`:

```toml
[dependencies]
bevy_lagrange = { workspace = true, features = ["enhanced_input"] }
```

The `workspace = true` form picks up the `path + version` setup from [Phase 1 step 4](./01-ingest.md#4-switch-the-workspace-bevy_lagrange-entry-to-path--version), so local builds use the in-tree source and published manifests pin the registry version.

### 2. Split the camera-configuration capability into two state-gated methods

Mirror bevy_lagrange's mode split in fairy_dust's typestate so the type system prevents the silent-no-op trap:

```rust
impl SprinkleBuilder<NoOrbitCam> {
    /// Spawn an `OrbitCam` driven by `LagrangePlugin::raw_input()`. The closure
    /// mutates the `OrbitCam` directly — `button_orbit`, `modifier_pan`,
    /// `input_control`, etc. all apply.
    pub fn with_orbit_cam_configured<F>(self, configure: F) -> SprinkleBuilder<WithOrbitCam>
    where F: FnOnce(&mut OrbitCam) + Send + Sync + 'static;

    /// Spawn an `OrbitCam` driven by `LagrangePlugin::enhanced_input()`. The
    /// closure receives the default `OrbitCamInputContext` so the caller can
    /// rebind `Action<Orbit>`, `Action<Pan>`, `Action<Zoom>` before the camera
    /// is spawned. Under this mode `OrbitCam`'s `button_orbit`/`modifier_pan`/
    /// `input_control` fields are ignored — that's why this method takes the
    /// action context, not the camera struct.
    pub fn with_orbit_cam_actions<F>(self, configure: F) -> SprinkleBuilder<WithOrbitCam>
    where F: FnOnce(&mut OrbitCamInputContext) + Send + Sync + 'static;
}
```

Both transition to `SprinkleBuilder<WithOrbitCam>`, so all downstream camera-attached capabilities (`with_stable_transparency`, etc.) work identically.

`with_orbit_cam_actions` is the opinionated default for `bevy_hana` examples; chain it for the rebindable enhanced_input experience. `with_orbit_cam_configured` stays for consumers who specifically want the raw_input model (perhaps because they've tuned the existing fields and don't want to rebuild bindings as actions).

### 3. Wire interaction events into the camera control panel

The camera control panel currently shows static "CAMERA / Mouse / Trackpad" content. After Phase 2, the panel can subscribe to the six interaction events and highlight the matching action row when the user is actively using it. Concretely:

- Restructure the panel layout from "Mouse | divider | Trackpad" two-column to **Action | Mouse | Trackpad** three-column, with rows for `Orbit`, `Pan`, `Zoom`.
- Add observers on `Orbit/Pan/ZoomInteractionBegin/End` that flip a `CameraControlPanelHighlight` resource (or component) for the matching row.
- The layout system reads that highlight state and applies the active color (the same `HUD_ACTIVE_COLOR` pattern used in the CONTROLS panels of other examples) to the action label while the gesture is in progress, fading back when the matching End event fires.

The panel's existing `Fit/Fit` sizing keeps it responsive to the new layout automatically.

## Open questions to resolve during implementation

1. **Should `with_orbit_cam_actions` be the default in `sprinkle_example()`?** Today neither method is called by default — the user picks one. After Phase 3, document that `with_orbit_cam_actions` is the recommended path; leave `with_orbit_cam_configured` available without sugar.
2. **Backward compatibility of `pub use bevy_lagrange::OrbitCam;`** — fairy_dust currently re-exports `OrbitCam` for use inside the `with_orbit_cam_configured` closure. Add a parallel `pub use bevy_lagrange::OrbitCamInputContext;` re-export for the actions closure.
3. **What happens if the user calls `with_camera_control_panel` without ever calling either `with_orbit_cam_*` method?** The panel shows but the highlights never fire. Today the typestate already requires `WithOrbitCam` for `with_stable_transparency`; consider whether `with_camera_control_panel` should also be gated, or whether a panel without highlights is fine. Default: keep it state-agnostic, since the panel still documents controls usefully even without an active camera.
4. **Highlight fade timing.** Begin events flip the highlight on instantly; should End events fade it off over a frame or two for visual smoothness, or snap off? Recommend snap off — the IDLE_FRAMES debounce already gives ~50ms of visual latency.

## Out of scope for Phase 3

- Rebinding UI for end users (they edit the action context in code).
- Gamepad input source (lives in `bevy_lagrange`, not fairy_dust).
- Refactoring the camera control panel into a reusable component crate (it stays in fairy_dust for now).

## Definition of done

- fairy_dust depends on in-workspace `bevy_lagrange` with `enhanced_input` feature on.
- `SprinkleBuilder<NoOrbitCam>` exposes both `with_orbit_cam_configured` (raw_input) and `with_orbit_cam_actions` (enhanced_input); both transition to `WithOrbitCam`.
- `world_text.rs` example uses `with_orbit_cam_actions` and the chain still compiles + runs cleanly.
- Camera control panel restructured to three columns (Action / Mouse / Trackpad).
- Panel highlights the matching row in `HUD_ACTIVE_COLOR` while a gesture is active; fades back on End.
- Resize-then-trackpad regression test still passes (stable transparency + enhanced_input combo on the orbit camera).
