# bevy_lagrange input refactor — `OrbitCamInput` + pluggable sources + interaction events

**Status:** plan, not yet implemented.
**Target version:** `bevy_lagrange 0.0.4`.
**Owner:** natepiano. We own bevy_lagrange. `0.0.3` is on crates.io but has no known external consumers, so the API can change before the next publish — `cargo-semver-checks` will still flag the breaking changes when we bump to `0.0.4`, and we either accept those warnings or yank `0.0.3` first (decided in Phase 1 step 7).

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

## Phases

### Phase 1: Bring bevy_lagrange into the workspace

Before any API refactor, ingest `bevy_lagrange` into the `bevy_hana` workspace alongside `bevy_diegetic` so all subsequent work happens in-tree, with shared workspace dependencies, shared lints, and shared CI/nightly tooling.

**Goals**

- Match the existing `bevy_diegetic` workspace pattern from a `Cargo.toml` perspective: the crate at `crates/bevy_lagrange/` consumes `bevy = { workspace = true, default-features = false, features = [...] }` plus other workspace deps, inherits `workspace.package`, `workspace.lints`, etc. — the per-crate `Cargo.toml` becomes thin.
- Trim the Bevy feature set on the production path to the minimum bevy_lagrange actually needs, so workspace builds stay fast. Audit every `bevy::` import in `bevy_lagrange/src/` and turn each one into a feature flag entry. Today the standalone repo pulls in default Bevy features; the in-workspace version should not.
- Preserve the full git history from `~/rust/bevy_lagrange` so commits and blame survive the move.
- Keep nightly style runs operating on `bevy_lagrange` and `bevy_diegetic` in parallel.

**Steps**

1. **Ingest with history preserved** — from `~/rust/bevy_hana`, use `git subtree add` directly against the local path (no named remote needed):

   ```bash
   git subtree add --prefix=crates/bevy_lagrange ~/rust/bevy_lagrange main
   ```

   `git subtree add` (without `--squash`) rewrites every `bevy_lagrange` commit so its paths live under `crates/bevy_lagrange/`, and merges that history into `bevy_hana`. After ingest, `git log --follow crates/bevy_lagrange/src/lib.rs` shows the full original commit history; `git blame` works inside the moved files.

   **Tags do not transfer.** The standalone bevy_lagrange repo carries 60+ historical tags, many of which refer to a prior project's history that was reused as a starting point. Tags from the source repo are intentionally not preserved — they don't refer to artifacts in the new repo's namespace. New `bevy_lagrange-X.Y.Z` tags will be created in the `bevy_hana` repo at publish time. Verify post-merge with `git log --follow crates/bevy_lagrange/src/lib.rs` before continuing.

2. **Restructure `crates/bevy_lagrange/Cargo.toml` to match `bevy_diegetic`** — strip `[package].edition / license / readme / repository / version` in favor of `*.workspace = true` references where the workspace already provides them. Update `[package].repository` to point at the bevy_hana repo URL (the source-of-truth move; see step 7). Move shared dependency versions (`bevy`, `bevy_egui`, etc.) up into `[workspace.dependencies]` in the root `Cargo.toml`. Replace per-crate `[lints]` with `workspace = true`. **Bump `[package].version` from `0.0.3-dev` to `0.0.4-dev`** immediately on ingest — the source repo is at `0.0.3-dev`, and the next published release will be `0.0.4`, so the in-tree version starts there.

3. **Minimize Bevy feature set on the production path** — audit `bevy_lagrange/src/` for every `use bevy::...` and translate the union into the smallest `features = [...]` list. Compare against `bevy_diegetic`'s feature list as a starting point and trim/extend as needed. Goal: workspace `cargo build` doesn't drag in Bevy's UI/audio/asset/sprite features unless bevy_lagrange genuinely uses them.

   **Verification gate** — the trim is not done until all of the following pass:

   - `cargo build -p bevy_lagrange --no-default-features` succeeds (catches anything that snuck in via a default).
   - `cargo build -p bevy_lagrange --all-features` succeeds (catches feature interactions, especially `fit_overlay` + `bevy_egui`).
   - `cargo nextest run --workspace` passes with zero new warnings.
   - The `fit_overlay` example still renders gizmos correctly (manual smoke test — no automated gizmo rendering test today).
   - The `world_text` example in `bevy_diegetic` still launches and orbits correctly.

   Required minimum bevy sub-features (starting list, refine during audit): `bevy_camera`, `bevy_core_pipeline`, `bevy_log`, `bevy_window`, `bevy_render`, `bevy_winit`, `bevy_input`. `fit_overlay` feature additionally needs `bevy_gizmos`. List the final set explicitly in the per-crate `Cargo.toml` and explain non-obvious entries with a comment.

4. **Switch the workspace `bevy_lagrange` entry to `path + version`** — replace the registry-only `bevy_lagrange = "0.0.3"` in the workspace root `[workspace.dependencies]` with `bevy_lagrange = { path = "crates/bevy_lagrange", version = "0.0.4-dev" }`. Consumers (`crates/bevy_diegetic/Cargo.toml` dev-dep and `crates/fairy_dust/Cargo.toml` regular dep) keep the `bevy_lagrange = { workspace = true }` form unchanged.

   **Version coordination rule:** the per-crate `[package].version` and the workspace-root `version =` constraint must always satisfy each other. Locally during development they are both `0.0.4-dev`. At publish time, both move together to `0.0.4`. If they ever drift (e.g. per-crate at `0.0.4-dev` while workspace constraint says `version = "0.0.3"`), `cargo publish` fails with a confusing version-mismatch error. The workspace dep entry is the single point of truth that consumers see; bump it in lockstep with the per-crate manifest version.

   Cargo uses the path during local workspace builds and emits the `version` constraint into the published manifest at `cargo publish` time, so end users from crates.io still get the registry release. This decouples semver per crate while keeping a single source of truth during local development. This is the standard pattern used by bevy itself, tokio, and embassy.

5. **Verify build + tests green** — `cargo build --workspace`, `cargo nextest run --workspace`, and the `world_text` example still launching cleanly. No behavior changes yet — Phase 1 is pure relocation.

6. **Update nightly style config** — `~/.claude/scripts/nightly/nightly-rust.conf` currently has `bevy_diegetic=bevy_hana/crates/bevy_diegetic`. Add `bevy_lagrange=bevy_hana/crates/bevy_lagrange` so the nightly clean/build/style-eval/style-fix flow processes both crates in parallel the same way. Confirm `style-fix-worktrees.sh` and friends pick up the new entry without further changes (they are config-driven).

7. **Resolve the publication source-of-truth move.** Concrete steps:

   - **Archive the GitHub repo** at `github.com/natepiano/bevy_lagrange` via the GitHub UI (Settings → "Archive this repository"). This makes it read-only and signals the move to anyone who finds it.
   - **Update `[package].repository`** in the in-tree `crates/bevy_lagrange/Cargo.toml` to point at the bevy_hana repo URL. Done as part of step 2.
   - **Decide on `0.0.3` yank.** Two options: (a) leave `0.0.3` published (it works, no urgent reason to retract); (b) `cargo yank --version 0.0.3 bevy_lagrange` if the API differences from `0.0.4` are large enough that we want to discourage new adopters from picking it up. Recommend (a) — yank is for security/correctness issues, not API churn.
   - **Keep the `~/rust/bevy_lagrange` directory on disk for archival.** Stop pushing to its origin. Future commits land in `bevy_hana` only.
   - **Per-crate CHANGELOG** — `bevy_lagrange/CHANGELOG.md` stays per-crate (it's part of what crates.io users see). Add an entry at the top: `## [Unreleased] — moved into bevy_hana workspace; will be published as 0.0.4 from there.`

**Out of scope for Phase 1**

- API changes — Phase 1 is *pure ingestion*. The `OrbitCamInput` refactor, the input-source pluggability, and the interaction events all happen in Phase 2.
- Removing `bevy_egui` or `fit_overlay` features that bevy_lagrange currently exposes. Audit and trim as a separate concern.

### Phase 2: `bevy_lagrange 0.0.4` — input contract refactor + interaction events

All three architectural layers ship together as a single in-workspace bevy_lagrange version bump:

1. Add `OrbitCamInput` resource, public.
2. Refactor `orbit_cam::orbit_cam` to consume `OrbitCamInput` only.
3. Fold `TouchTracker` into the touch input source; remove `TouchTracker` as a public resource. `TouchInput::OneFingerOrbit | TwoFingerOrbit` becomes a constructor argument on the touch input source — exposed, not buried.
4. Split current `mouse_key_tracker` + touch reader into a `raw_input` cargo feature (default-on). `LagrangePlugin::raw_input()` installs it; bare `LagrangePlugin` aliases to this for back-compat.
5. Add `LagrangePlugin::manual_input()` — installs no input system.
6. Add `enhanced_input` cargo feature with `LagrangePlugin::enhanced_input()` and the default `OrbitCamInputContext`.
7. Add `Orbit/Pan/ZoomInteractionBegin/End` events + the edge-detection system. Default `IDLE_FRAMES = 3`; `OrbitCam.interaction_idle_frames: u8` field for per-camera tuning.
8. Assert the Layer 1 invariant: `OrbitCamInput` only written by input-source systems; debug-build assertion in the edge-detector when a programmatic-op frame happens to coincide with nonzero `OrbitCamInput`, so future regressions are caught.

Bundling rationale: no external users to migrate, easier to keep coherent, single doc/changelog pass.

### Phase 3: `fairy_dust` integration with the new input model

- Depend on the in-workspace `bevy_lagrange` with `enhanced_input` feature on.
- **Split the camera-configuration capability into two state-gated methods**, mirroring bevy_lagrange's mode split, so the type system prevents the silent-no-op trap of mutating `OrbitCam.button_orbit` under enhanced_input mode:

  ```rust
  // SprinkleBuilder<NoOrbitCam> → SprinkleBuilder<WithOrbitCam>
  fn with_orbit_cam_configured(self, FnOnce(&mut OrbitCam)) -> ...      // raw_input mode
  fn with_orbit_cam_actions(self, FnOnce(&mut OrbitCamInputContext)) -> ... // enhanced_input mode
  ```

  Both transition to `WithOrbitCam`. `with_orbit_cam_actions` is the opinionated default for `bevy_hana` examples (chains `LagrangePlugin::enhanced_input()`); `with_orbit_cam_configured` stays for consumers who want raw_input.

- Wire the `Orbit/Pan/ZoomInteractionBegin/End` events into the camera control panel for active-state highlighting (see also: planned 3-column Action / Mouse / Trackpad layout, which is its own follow-up).

## Open questions to resolve during implementation

1. **`OrbitButtonChange` semantics under enhanced_input.** The current snapping behavior in the controller depends on the exact frame an orbit binding is `just_pressed`/`just_released`. The `enhanced_input` source needs to set this flag from action `Started`/`Completed` events, which fire in different schedule positions than `ButtonInput::just_pressed` — expect a one-frame skew. Add a regression test that snaps work identically across the two input sources.
2. **Where does the edge-detection system live in the schedule?** After `OrbitCamInput` is populated, before the controller applies it, so events fire the same frame the user starts/stops interacting. `PostUpdate` set `OrbitCamSystemSet` is the natural place; ordering inside that set needs to be explicit.
3. **`OrbitCam.input_control` doc deprecation strategy.** Marking the whole struct or just the input-config fields as `#[doc(alias = "raw_input only")]`, plus a top-of-struct doc note, may be enough. Avoid `#[deprecated]` because the fields are still correct under raw_input.
4. **README/docs update.** New top-level section explaining the three plugin variants, the `OrbitCamInput` contract, the interaction-event pattern, and the `with_orbit_cam_configured` vs `with_orbit_cam_actions` split in fairy_dust.

## Out of scope for `0.0.4`

- A built-in UI affordance crate (the `fairy_dust` camera control panel will consume the new events, but the events themselves are general-purpose).
- Rebinding UI for end users.
- Gamepad input source (would be another `Plugin::*_input()` variant, not a blocker for this release).
