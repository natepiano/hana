# bevy_lagrange input refactor — `OrbitCamInput` + pluggable sources + interaction events

**Status:** plan, not yet implemented.
**Target version:** `bevy_lagrange 0.0.4`.
**Owner:** natepiano (we own bevy_lagrange; no external users yet, so we can change the API freely before publication).

## Why

Today bevy_lagrange's input handling is private and tightly coupled to a single source: `mouse_key_tracker` reads `ButtonInput<MouseButton>`, `MouseMotion`, `MouseWheel`, `PinchGesture` directly into a private `MouseKeyTracker` resource, and `TouchTracker` does the same for touch. The controller in `orbit_cam::orbit_cam` then consumes both.

This means:

- A consumer who wants to drive the camera from `bevy_enhanced_input` (or any other input system) cannot — there is no public input contract to populate.
- There are no events for direct-input interactions (orbit/pan/zoom start/stop). Only programmatic camera operations (`ZoomToFit`, `PlayAnimation`, etc.) emit `Begin`/`End` events. UI affordances that want to highlight "currently orbiting" must roll their own edge detection on `Changed<Transform>` or raw input.
- Touch input lives in a separate resource (`TouchTracker`) from mouse/key input, so any future input source has to write to two places.

We want bevy_lagrange to be a clean, pluggable camera-state crate with a public input contract, multiple shipped input sources, and lifecycle events for direct-input interactions. `bevy_enhanced_input` becomes the opinionated default in `fairy_dust`, but bevy_lagrange itself stays input-source-agnostic.

## Three layers

### Layer 1: Public input contract

A new public resource replaces today's private `MouseKeyTracker` and absorbs `TouchTracker`:

```rust
#[derive(Resource, Default, Debug)]
pub struct OrbitCamInput {
    pub orbit:               Vec2, // delta this frame
    pub pan:                 Vec2, // delta this frame
    pub scroll_line:         f32,  // accumulated this frame
    pub scroll_pixel:        f32,  // accumulated this frame
    pub orbit_button_change: OrbitButtonChange, // edge flag for snapping
}
```

The controller (`orbit_cam::collect_camera_input`/`orbit_cam`) reads only `OrbitCamInput`. It no longer reads `TouchTracker`, no longer references `MouseKeyTracker`, no longer queries Bevy raw input directly. This makes the controller fully decoupled from where deltas come from.

Touch input sources write to `OrbitCamInput` directly. The current `TouchInput::OneFingerOrbit | TwoFingerOrbit` selector and the gesture-classification logic move into the touch input source, which then folds its result into `OrbitCamInput` the same way the mouse source does.

### Layer 2: Pluggable input sources

`LagrangePlugin` gains constructor variants that pick which `Update` system populates `OrbitCamInput`:

```rust
LagrangePlugin                       // = LagrangePlugin::raw_input()  (back-compat default)
LagrangePlugin::raw_input()          // installs the current MouseMotion/MouseWheel/PinchGesture/Touch reader
LagrangePlugin::manual_input()       // installs no input system — caller populates OrbitCamInput themselves
LagrangePlugin::enhanced_input()     // feature-gated; installs the bevy_enhanced_input → OrbitCamInput bridge
```

Each variant only changes which input-population system is registered. The controller side is identical across all three.

The `enhanced_input` source is gated behind a `enhanced_input` cargo feature. It ships a default `OrbitCamInputContext` action set:

- `Action<Orbit>: Axis2D` — bound to MMB drag (matches today's raw default)
- `Action<Pan>: Axis2D` — bound to Shift+MMB drag (matches today's raw default)
- `Action<Zoom>: Axis1D` — bound to scroll wheel + pinch (matches today's raw default)

Consumers can rebind by replacing or extending the context — that is the point of using bevy_enhanced_input.

### Layer 3: User-input lifecycle events

Independent of input source. A small system reads `OrbitCamInput` each frame and emits edge-detection events when an action delta transitions between zero and nonzero:

```rust
OrbitInteractionBegin  { camera }   // orbit delta went 0 → nonzero
OrbitInteractionEnd    { camera }   // orbit delta stayed 0 for N idle frames after being nonzero
PanInteractionBegin    { camera }
PanInteractionEnd      { camera }
ZoomInteractionBegin   { camera }
ZoomInteractionEnd     { camera }
```

Same naming pattern as the existing programmatic-animation events (`AnimationBegin`/`AnimationEnd`, `ZoomBegin`/`ZoomEnd`). `Begin`/`End` are EntityEvents with `camera: Entity` as `#[event_target]`.

The `End` event uses an idle-frame debounce (default ~3 frames) so a single-frame zero gap inside a continuous gesture does not produce spurious End/Begin pairs. Constant tunable per camera or globally — to be decided during implementation.

These events fire regardless of which input source populated `OrbitCamInput`, so any consumer (UI affordances, gameplay reaction, save-on-idle) can observe them uniformly.

## OrbitCam field mode-dependence

`OrbitCam`'s input-config fields apply only to `raw_input` mode:

- `button_orbit`, `button_pan`, `button_zoom`, `button_zoom_axis`
- `modifier_orbit`, `modifier_pan`
- `input_control: Option<InputControl>` (touch + trackpad + zoom-direction)

In `enhanced_input` mode they are ignored. The doc on each field calls this out and points the reader at the action context for rebinding under enhanced_input. The honest alternative — making these fields drive the bevy_enhanced_input default bindings — would create a leaky synchronization layer between two binding systems with no win for the consumer. We keep the two binding systems separate and document the mode dependence.

## Phases

### Phase 1: Bring bevy_lagrange into the workspace

Before any API refactor, ingest `bevy_lagrange` into the `bevy_hana` workspace alongside `bevy_diegetic` so all subsequent work happens in-tree, with shared workspace dependencies, shared lints, and shared CI/nightly tooling.

**Goals**

- Match the existing `bevy_diegetic` workspace pattern from a `Cargo.toml` perspective: the crate at `crates/bevy_lagrange/` consumes `bevy = { workspace = true, default-features = false, features = [...] }` plus other workspace deps, inherits `workspace.package`, `workspace.lints`, etc. — the per-crate `Cargo.toml` becomes thin.
- Trim the Bevy feature set on the production path to the minimum bevy_lagrange actually needs, so workspace builds stay fast. Audit every `bevy::` import in `bevy_lagrange/src/` and turn each one into a feature flag entry. Today the standalone repo pulls in default Bevy features; the in-workspace version should not.
- Preserve the full git history from `~/rust/bevy_lagrange` so commits and blame survive the move.
- Keep nightly style runs operating on `bevy_lagrange` and `bevy_diegetic` in parallel.

**Steps**

1. **Ingest with history preserved** — from `~/rust/bevy_hana`:

   ```bash
   git remote add bevy_lagrange-source ~/rust/bevy_lagrange
   git fetch bevy_lagrange-source
   git subtree add --prefix=crates/bevy_lagrange bevy_lagrange-source main
   git remote remove bevy_lagrange-source
   ```

   `git subtree add` (without `--squash`) rewrites every `bevy_lagrange` commit so its paths live under `crates/bevy_lagrange/`, and merges that history into `bevy_hana`. `git log crates/bevy_lagrange` will still show the full original history; `git blame` works inside the moved files.

2. **Restructure `crates/bevy_lagrange/Cargo.toml` to match `bevy_diegetic`** — strip `[package].edition / license / readme / repository / version` in favor of `*.workspace = true` references. Move shared dependency versions (`bevy`, `bevy_egui`, etc.) up into `[workspace.dependencies]` in the root `Cargo.toml`. Replace per-crate `[lints]` with `workspace = true`.

3. **Minimize Bevy feature set on the production path** — audit `bevy_lagrange/src/` for every `use bevy::...` and translate the union into the smallest `features = [...]` list. Compare against `bevy_diegetic`'s feature list as a starting point and trim/extend as needed. Goal: workspace `cargo build` doesn't drag in Bevy's UI/audio/asset/sprite features unless bevy_lagrange genuinely uses them.

4. **Switch the workspace `bevy_lagrange` entry to `path + version`** — replace the registry-only `bevy_lagrange = "0.0.3"` in the workspace root `[workspace.dependencies]` with `bevy_lagrange = { path = "crates/bevy_lagrange", version = "0.0.3" }`. Consumers (`crates/bevy_diegetic/Cargo.toml` dev-dep and `crates/fairy_dust/Cargo.toml` regular dep) keep the `bevy_lagrange = { workspace = true }` form unchanged.

   Cargo uses the path during local workspace builds and emits the `version` constraint into the published manifest at `cargo publish` time, so end users from crates.io still get the registry release. This decouples semver per crate (the `version =` field controls what gets pinned in published manifests) while keeping a single source of truth during local development. This is the standard pattern used by bevy itself, tokio, and embassy.

   Bump the `version =` field at the workspace level whenever bevy_lagrange publishes a new release; every consumer picks up the new constraint at once. Each consuming crate still publishes its own release independently when it's ready to commit to the new bevy_lagrange constraint.

5. **Verify build + tests green** — `cargo build --workspace`, `cargo nextest run --workspace`, and the `world_text` example still launching cleanly. No behavior changes yet — Phase 1 is pure relocation.

6. **Update nightly style config** — `~/.claude/scripts/nightly/nightly-rust.conf` currently has `bevy_diegetic=bevy_hana/crates/bevy_diegetic`. Add `bevy_lagrange=bevy_hana/crates/bevy_lagrange` so the nightly clean/build/style-eval/style-fix flow processes both crates in parallel the same way. Confirm `style-fix-worktrees.sh` and friends pick up the new entry without further changes (they are config-driven).

7. **Retire the standalone `~/rust/bevy_lagrange` repo** — keep the directory on disk for archival but stop pushing to it. Future commits land in `bevy_hana` only. Decide later whether to keep publishing crates.io releases from `bevy_hana` or pause publication until the API stabilizes.

**Out of scope for Phase 1**

- API changes — Phase 1 is *pure ingestion*. The `OrbitCamInput` refactor, the input-source pluggability, and the interaction events all happen in Phase 2.
- Removing `bevy_egui` or `fit_overlay` features that bevy_lagrange currently exposes. Audit and trim as a separate concern.

### Phase 2: `bevy_lagrange 0.0.4` — input contract refactor + interaction events

All three architectural layers ship together as a single in-workspace bevy_lagrange version bump:

1. Add `OrbitCamInput` resource, public.
2. Refactor `orbit_cam::orbit_cam` to consume `OrbitCamInput` only.
3. Fold `TouchTracker` into the touch input source; remove `TouchTracker` as a public resource.
4. Split current `mouse_key_tracker` + touch reader into a `raw_input` cargo feature (default-on). `LagrangePlugin::raw_input()` installs it; bare `LagrangePlugin` aliases to this for back-compat.
5. Add `LagrangePlugin::manual_input()` — installs no input system.
6. Add `enhanced_input` cargo feature with `LagrangePlugin::enhanced_input()` and the default `OrbitCamInputContext`.
7. Add `Orbit/Pan/ZoomInteractionBegin/End` events + the edge-detection system.

Bundling rationale: no external users to migrate, easier to keep coherent, single doc/changelog pass.

### Phase 3: `fairy_dust` integration with the new input model

- Depend on the in-workspace `bevy_lagrange` with `enhanced_input` feature on.
- `with_orbit_cam_configured` pulls in `LagrangePlugin::enhanced_input()` instead of the bare `LagrangePlugin`.
- Document that consumers can override the default action context by replacing it before fairy_dust spawns the camera, or via a future fairy_dust capability if there is demand.
- Existing fairy_dust API keeps working — closure still mutates the spawned `OrbitCam`, but consumers should know that the input-config fields now do nothing under enhanced_input.
- Wire the `Orbit/Pan/ZoomInteractionBegin/End` events into the camera control panel for active-state highlighting (see also: planned 3-column Action / Mouse / Trackpad layout, which is its own follow-up).

## Open questions to resolve during implementation

1. **Idle-frame debounce constant for End events.** Pick a default; expose as a tunable on `OrbitCam` only if a use case appears.
2. **`OrbitButtonChange` semantics under enhanced_input.** The current snapping behavior in the controller depends on the exact frame an orbit binding is just-pressed/just-released. The enhanced_input source needs to set this flag from action `Started`/`Completed` events. Confirm parity.
3. **Where does the edge-detection system live in the schedule?** After `OrbitCamInput` is populated, before the controller applies it, so events fire the same frame the user starts/stops interacting. `PostUpdate` set `OrbitCamSystemSet` is the natural place; ordering inside that set needs to be explicit.
4. **`OrbitCam.input_control` doc deprecation strategy.** Marking the whole struct or just the input-config fields as `#[doc(alias = "raw_input only")]`, plus a top-of-struct doc note, may be enough. Avoid `#[deprecated]` because the fields are still correct under raw_input.
5. **README/docs update.** New top-level section explaining the three plugin variants, the `OrbitCamInput` contract, and the interaction-event pattern.

## Out of scope for `0.0.4`

- A built-in UI affordance crate (the `fairy_dust` camera control panel will consume the new events, but the events themselves are general-purpose).
- Rebinding UI for end users.
- Gamepad input source (would be another `Plugin::*_input()` variant, not a blocker for this release).
