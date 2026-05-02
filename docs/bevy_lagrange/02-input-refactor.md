# Phase 2: `bevy_lagrange 0.0.4` — input contract refactor + interaction events

**Status:** plan, not yet implemented.
**Depends on:** [Phase 1](./01-ingest.md) — `bevy_lagrange` must be in-workspace before this phase starts.
**Unblocks:** [Phase 3](./03-fairy-dust-adoption.md) — fairy_dust adopts the new input model.
**Owner:** natepiano.

## Why this phase exists

Today `bevy_lagrange`'s input handling is private and tightly coupled to a single source: `mouse_key_tracker` reads `ButtonInput<MouseButton>`, `MouseMotion`, `MouseWheel`, `PinchGesture` directly into a private `MouseKeyTracker` resource, and `TouchTracker` does the same for touch. The controller in `orbit_cam::orbit_cam` then consumes both.

Three concrete pain points:

- A consumer who wants to drive the camera from `bevy_enhanced_input` (or any other input system) cannot — there is no public input contract to populate.
- There are no events for direct-input interactions (orbit/pan/zoom start/stop). Only programmatic camera operations (`ZoomToFit`, `PlayAnimation`, etc.) emit `Begin`/`End` events. UI affordances that want to highlight "currently orbiting" must roll their own edge detection on `Changed<Transform>` or raw input.
- Touch input lives in a separate resource (`TouchTracker`) from mouse/key input, so any future input source has to write to two places.

We want bevy_lagrange to be a clean, pluggable camera-state crate with a public input contract, multiple shipped input sources, and lifecycle events for direct-input interactions. `bevy_enhanced_input` becomes the opinionated default in `fairy_dust` ([Phase 3](./03-fairy-dust-adoption.md)), but bevy_lagrange itself stays input-source-agnostic.

`0.0.3` is on crates.io but has no known external consumers, so the API can change before the next publish — `cargo-semver-checks` will still flag the breaking changes when we bump to `0.0.4`, and we accept those warnings (the yank decision is recorded in [Phase 1 step 7](./01-ingest.md#7-resolve-the-publication-source-of-truth-move)).

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

Touch input sources write to `OrbitCamInput` directly. The `TouchInput::OneFingerOrbit | TwoFingerOrbit` selector remains user-facing config — exposed as a constructor argument on the touch input source (e.g. `TouchInputSource::new(TouchInput::OneFingerOrbit)`), not as an internal constant — so consumers keep the same gesture-style choice they have today.

**Invariant:** `OrbitCamInput` is *only* populated by input-source systems. Programmatic camera operations (`PlayAnimation`, `ZoomToFit`, `LookAt`, etc.) never touch it. They mutate `OrbitCam.target_yaw/target_pitch/target_radius/target_focus` directly through the existing animation path. This invariant prevents double-emission of interaction events when programmatic and input-driven motion happen on the same frame, and it is asserted in Layer 3's edge-detector with a debug-build sanity check.

### Layer 2: Pluggable input sources

`LagrangePlugin` gains constructor variants that pick which `Update` system populates `OrbitCamInput`:

```rust
LagrangePlugin                       // = LagrangePlugin::raw_input()  (back-compat default)
LagrangePlugin::raw_input()          // installs the current MouseMotion/MouseWheel/PinchGesture/Touch reader
LagrangePlugin::manual_input()       // installs no input system — caller populates OrbitCamInput themselves
LagrangePlugin::enhanced_input()     // feature-gated; installs the bevy_enhanced_input → OrbitCamInput bridge
```

Each variant only changes which input-population system is registered. The controller side is identical across all three.

**Feature/constructor coupling:** the `enhanced_input` constructor lives behind a `enhanced_input` cargo feature. Cargo features are additive — a downstream crate can turn the feature on alongside `raw_input`. The plugin must not auto-register both readers. **Each `LagrangePlugin::*_input()` constructor registers exactly one input-source system; constructing two `LagrangePlugin`s in the same app panics with a clear message.** The cargo feature gates *availability* of the constructor, not auto-registration of its system.

The `enhanced_input` source ships a default `OrbitCamInputContext` action set:

- `Action<Orbit>: Axis2D` — bound to MMB drag (matches today's raw default)
- `Action<Pan>: Axis2D` — bound to Shift+MMB drag (matches today's raw default)
- `Action<Zoom>: Axis1D` — bound to scroll wheel + pinch (matches today's raw default)

Consumers can rebind by replacing or extending the context — that is the point of using bevy_enhanced_input.

### Layer 3: User-input lifecycle events

Independent of input source. A small system reads `OrbitCamInput` each frame and emits edge-detection events when an action delta transitions between zero and nonzero:

```rust
OrbitInteractionBegin  { camera }   // orbit delta went 0 → nonzero (immediate)
OrbitInteractionEnd    { camera }   // orbit delta stayed 0 for IDLE_FRAMES after being nonzero
PanInteractionBegin    { camera }
PanInteractionEnd      { camera }
ZoomInteractionBegin   { camera }
ZoomInteractionEnd     { camera }
```

Same naming pattern as the existing programmatic-animation events (`AnimationBegin`/`AnimationEnd`, `ZoomBegin`/`ZoomEnd`). `Begin`/`End` are EntityEvents with `camera: Entity` as `#[event_target]`.

**Edge asymmetry, pinned default:** `Begin` fires on the leading edge — the first frame any delta becomes nonzero. `End` is debounced: it fires after `IDLE_FRAMES` consecutive zero-delta frames following a nonzero run. This asymmetry exists because momentary zero gaps inside continuous gestures (between scroll events, mid-flick on trackpad) are normal; debouncing the end keeps a single gesture from emitting spurious End/Begin pairs, while leaving Begin instant gives UI affordances responsive feedback.

Default: `IDLE_FRAMES = 3`. Tunable per camera via a new field `OrbitCam.interaction_idle_frames: u8`. (3 frames at 60Hz ≈ 50ms — slightly slower than the typical pinch-rest gap on macOS, intentionally.)

These events fire regardless of which input source populated `OrbitCamInput` (raw, manual, enhanced). They never fire as a consequence of programmatic camera operations because the invariant in Layer 1 prevents `OrbitCamInput` from being touched by those code paths.

## OrbitCam field mode-dependence

`OrbitCam`'s input-config fields apply only to `raw_input` mode:

- `button_orbit`, `button_pan`, `button_zoom`, `button_zoom_axis`
- `modifier_orbit`, `modifier_pan`
- `input_control: Option<InputControl>` (touch + trackpad + zoom-direction)

In `enhanced_input` mode they are ignored. The doc on each field calls this out and points the reader at the action context for rebinding under enhanced_input. The alternative — making these fields drive the bevy_enhanced_input default bindings — would create a leaky synchronization layer between two binding systems with no win for the consumer. We keep the two binding systems separate and document the mode dependence.

[Phase 3](./03-fairy-dust-adoption.md) protects fairy_dust users from accidentally mutating these fields under enhanced_input mode by splitting the typestate API.

## Steps

All three architectural layers ship together as a single in-workspace `bevy_lagrange 0.0.4` version bump:

1. Add `OrbitCamInput` resource, public.
2. Refactor `orbit_cam::orbit_cam` to consume `OrbitCamInput` only.
3. Fold `TouchTracker` into the touch input source; remove `TouchTracker` as a public resource. `TouchInput::OneFingerOrbit | TwoFingerOrbit` becomes a constructor argument on the touch input source — exposed, not buried.
4. Split current `mouse_key_tracker` + touch reader into a `raw_input` cargo feature (default-on). `LagrangePlugin::raw_input()` installs it; bare `LagrangePlugin` aliases to this for back-compat.
5. Add `LagrangePlugin::manual_input()` — installs no input system.
6. Add `enhanced_input` cargo feature with `LagrangePlugin::enhanced_input()` and the default `OrbitCamInputContext`.
7. Add `Orbit/Pan/ZoomInteractionBegin/End` events + the edge-detection system. Default `IDLE_FRAMES = 3`; `OrbitCam.interaction_idle_frames: u8` field for per-camera tuning.
8. Assert the Layer 1 invariant: `OrbitCamInput` only written by input-source systems; debug-build assertion in the edge-detector when a programmatic-op frame happens to coincide with nonzero `OrbitCamInput`, so future regressions are caught.

Bundling rationale: no external users to migrate, easier to keep coherent, single doc/changelog pass.

## Open questions to resolve during implementation

1. **`OrbitButtonChange` semantics under enhanced_input.** The current snapping behavior in the controller depends on the exact frame an orbit binding is `just_pressed`/`just_released`. The `enhanced_input` source needs to set this flag from action `Started`/`Completed` events, which fire in different schedule positions than `ButtonInput::just_pressed` — expect a one-frame skew. Add a regression test that snaps work identically across the two input sources.
2. **Where does the edge-detection system live in the schedule?** After `OrbitCamInput` is populated, before the controller applies it, so events fire the same frame the user starts/stops interacting. `PostUpdate` set `OrbitCamSystemSet` is the natural place; ordering inside that set needs to be explicit.
3. **`OrbitCam.input_control` doc strategy.** Marking the input-config fields with a doc note ("applies under `raw_input` mode only; see `OrbitCamInputContext` for `enhanced_input`") is probably enough. Avoid `#[deprecated]` because the fields are still correct under raw_input.
4. **README update.** New top-level section explaining the three plugin variants, the `OrbitCamInput` contract, and the interaction-event pattern. Update the examples in `README.md` to show all three plugin variants.

## Out of scope for `0.0.4`

- A built-in UI affordance crate (the `fairy_dust` camera control panel will consume the new events, but the events themselves are general-purpose).
- Rebinding UI for end users.
- Gamepad input source (would be another `Plugin::*_input()` variant, not a blocker for this release).
- Any `fairy_dust` changes — those land in [Phase 3](./03-fairy-dust-adoption.md).

## Definition of done

- `OrbitCamInput` is the controller's only input-side dependency; `MouseKeyTracker` and `TouchTracker` are private/gone.
- Three plugin constructors (`raw_input`, `manual_input`, `enhanced_input`) work; tests cover each.
- `enhanced_input` ships behind a cargo feature with a default action context that matches today's raw bindings.
- Six interaction lifecycle events fire correctly with the documented Begin/End edge asymmetry; default `IDLE_FRAMES = 3`.
- Layer 1 invariant asserted in debug builds.
- Workspace `cargo nextest run --workspace` green; `cargo build` green for all feature combinations.
- `bevy_lagrange 0.0.4` ready to publish from the workspace.
