# FlyCam: incremental second-camera buildout

> **Status: ACTIVE PLAN + RUNNING AS-BUILT.** Adding a second camera kind (`FlyCam`) to `bevy_lagrange`, which is currently mono-mode (one camera: `OrbitCam`). Built top-down through the crate, one type/module at a time, refactoring `OrbitCam` into shared/specific parts as each step demands it. This doc is kept updated as we proceed.

## Goal

Editor needs a camera that can **turn away from the edited thing and walk forward into empty space to build something new** — decoupled look direction + translate-along-look. None of the existing modes do this:

- **OrbitCam** is pivot-locked (always faces the focus; can't turn away).
- **select + zoomToFit** needs an existing target (can't go to empty space).
- View-plane pan can't rotate.

So FlyCam is a genuinely new capability, not redundant with anything present.

## Framing decisions (established before any code)

- **FlyCam is a *sibling camera*, not an `OrbitCamInputMode` variant.** `OrbitCamInputMode` (Preset/Bindings/Manual) is *how input maps within an orbit camera*. Orbit state (`focus`/`radius`/`yaw`/`pitch` around a pivot) and fly state (free position + look direction) are different camera kinds with different state. Do **not** jam FlyCam into that enum.
- **The crate is mono-mode and `OrbitCam*`-prefixed throughout** (`OrbitCamInput`, `OrbitCamInputMode`, `OrbitCamSystemSet`, `OrbitCamBindings`, …). The naming encodes "there is one camera kind." Generalizing that naming is part of the work, but only when a second consumer makes it real.
- **Editor-level "which camera kind is active" is a new, orthogonal concept** the crate doesn't have yet. Name it now; leave it empty until a step forces it.
- **Renames (`OrbitCam*` → `Camera*`) are the user's to perform** via the editor's global-rename when a shared type is extracted. Claude flags the rename set; the user sweeps it.

## The recipe (the loop we repeat through the whole crate)

Starting at the top of the crate (`lib.rs`) and working down, for each module/type:

1. **Module structure.** Look at the current file/type. Ask: *what would live here if we now have two cameras?* Identify where `OrbitCam` and `FlyCam` each belong.
2. **Make it so.** Adjust the file and what it calls — move things — until the module is **structurally done** for two cameras (even if `FlyCam` is still empty/non-functional).
3. **Anchor-type analysis.** Look at the `OrbitCam`-side type itself. Identify what is **generic / trait-based** (shared across both cameras) vs **specific to each**.
4. **OrbitCam refactor first.** If we find a generic/trait or a refactoring that makes the next step work, do it **on `OrbitCam`** first.
5. **FlyCam analog.** Make the analogous new thing on the `FlyCam` side.
6. **Verify.** Run `/clippy` and `cargo nextest run -p bevy_lagrange` — both clean before moving on.
7. **Recurse.** Move to the next module/type and repeat: module structure → anchor-type analysis → OrbitCam refactor → FlyCam implementation → verify.

### Mirror in lockstep

Every OrbitCam-side change gets its FlyCam analog **in the same step**, not batched for later. When step 4 refactors or extracts something on the OrbitCam side, step 5 immediately mirrors the equivalent onto FlyCam before moving on. The two sides stay in sync as we descend, so FlyCam is never a big deferred catch-up — it grows one mirrored piece at a time.

**Same structure, not just same concept.** The FlyCam analog lands in the **structurally identical location** as its OrbitCam counterpart — same module, same file, same visibility — never a co-located spot picked for the new `fly_cam/` module. If `OrbitCamInputContext` lives in `input/context.rs`, then `FlyCamInputContext` lives in `input/context.rs` too, not `fly_cam/context.rs`. Two homes for one concept is divergence, and divergence is the failure mode this loop exists to prevent. Before writing the FlyCam side, find where the OrbitCam equivalent actually lives and put the analog there.

### Rule that keeps this from becoming the multi-day upfront design

Extract a shared trait/abstraction (or a rename) **only when the current step's FlyCam analog is the second consumer that makes it real** — never speculatively. No `CameraOperation` trait, no `OrbitCamInput`→`CameraInput` rename, no binding-system generalization until the step in front of us needs it.

## Traversal order (top-down)

1. ~~`lib.rs` — top-level plugin registration + public API.~~ **DONE** (see log).
2. (next) the `OrbitCam` struct itself — anchor-type analysis: shared vs specific.
3. … continue down the crate (input pipeline, bindings, presets, system sets, …) as the recipe recurses.

## Crate map (reference, as of start)

```
src/
├── lib.rs              top-level LagrangePlugin + public exports
├── orbit_cam/          OrbitCam component + controller (the one camera today)
├── input/              OrbitCam* input pipeline: intent, actions, axis_response,
│                       modes, bindings/, routing/, adapter/
├── events/             camera animation/motion events
├── fit/, fit_overlay/  fit-to-bounds + debug overlay
├── observers/          event observers
├── animation.rs, components.rs, constants.rs, projection.rs,
├── orbital_math.rs, system_sets.rs, touch.rs, enhanced_input.rs
```

Single top-level plugin `LagrangePlugin` (`lib.rs`) composes private sub-plugins (system sets, enhanced input, input modes, routing, adapter, lifecycle) and registers the `orbit_cam` system in `PostUpdate`.

## Running log (updated as we proceed)

<!-- One entry per step: what we asked, what we decided, what moved. Newest last. -->

### Step 1 — `lib.rs` structure for two cameras

**Asked:** what lives in `lib.rs` with two camera kinds, and where do `OrbitCam` / `FlyCam` live?

**Decided:**
- Two camera kinds → two camera modules: `orbit_cam/` (existing) and new `fly_cam/`.
- `LagrangePlugin` becomes a **composer**: shared infra + one per-camera plugin each.
  - **Shared** (stays in `LagrangePlugin::build`): `LagrangeEnhancedInputPlugin`, `LagrangeSystemSetsPlugin`, `ObserverPlugin`, touch tracking (`TouchTracker`/`Touches`/`PinchGesture` + `touch_tracker`), `animation::process_camera_move_list`, `ZoomOverlayPlugin` (feature-gated).
  - **OrbitCam-specific** → new `OrbitCamPlugin` (`orbit_cam/mod.rs`, `pub(super)`): the four `OrbitCam*` input sub-plugins (modes/routing/adapter/lifecycle) **and** the `orbit_cam` controller system. Per **fork (A)** — input pipeline is OrbitCam's for now; the generalized input layer gets pulled forward during descent.
  - **FlyCam** → new `FlyCamPlugin` (`fly_cam/mod.rs`, `pub(super)`): hollow `build` (empty).
- `OrbitCamPlugin` / `FlyCamPlugin` kept **private**; `LagrangePlugin` stays the sole public entry.

**Moved:**
- New `crates/bevy_lagrange/src/fly_cam/mod.rs` — `FlyCamPlugin` (empty).
- `orbit_cam/mod.rs` — added `OrbitCamPlugin` (absorbs the 4 input sub-plugins + `orbit_cam` controller registration); `orbit_cam` controller import dropped from `pub(crate)` to private (now internal to the module).
- `lib.rs` — `mod fly_cam;`; dropped the 4 input-plugin imports + `CameraUpdateSystems`/`TransformSystems` (now in `orbit_cam`); `LagrangePlugin::build` rewritten as composer; doc comment updated for two cameras.

**Behavior:** pure relocation, no functional change. `animation.rs` test harness builds its own app with the 4 input plugins directly — unaffected.

**Next:** step 2 — `OrbitCam` struct anchor-type analysis (shared vs fly-specific state).

### Step 1.1 — enhanced-input: per-kind contexts, delete trivial plugin

**Found:** `bevy_enhanced_input` isolates per entity. An input *context* type is registered once globally (`add_input_context::<Ctx>()`); each camera *entity* in Preset/Bindings mode then carries its own context instance + ~15 action entities + binding entities, all keyed to its `Entity` (`input/adapter/install.rs`). Manual-mode cameras have it stripped. `ContextActivity`/`ContextPriority` are per-entity-per-type.

**Multi-window decision:** cameras of different kinds run simultaneously (fly in one monitor, orbit in another); focus switches which one consumes input. So:
- **Two context types**, one per kind — `OrbitCamInputContext` (exists) + `FlyCamInputContext` (later). Per-entity isolation means simultaneous fly/orbit never interfere; no shared/generic context needed.
- The "switch" is the existing routing layer (`CameraInputRouting` / `ResolvedOrbitCamInputRoute`) picking the focused camera — **not** a same-entity mode swap. The parked "active camera kind" enum is unnecessary.

**Changed:**
- Context *type* registration is per-kind → `add_input_context::<OrbitCamInputContext>()` moved into `OrbitCamPlugin`. `FlyCamPlugin` gets `add_input_context::<FlyCamInputContext>()` when that type exists.
- `LagrangeEnhancedInputPlugin` became trivial (only the guarded `EnhancedInputPlugin` core add remained) → deleted `enhanced_input.rs`; inlined the guarded core add directly into `LagrangePlugin::build` as shared infra.

### Step 1.2 — FlyCam context mirror

First lockstep mirror: the OrbitCam side has `OrbitCamInputContext` (in `input/context.rs`), required by `OrbitCam`, registered in `OrbitCamPlugin`. Mirrored onto FlyCam:
- `fly_cam/context.rs` — `FlyCamInputContext` (ZST marker, same derives as `OrbitCamInputContext`).
- `fly_cam/mod.rs` — `pub struct FlyCam {}` with `#[require(FlyCamInputContext)]`; `FlyCamPlugin::build` registers `add_input_context::<FlyCamInputContext>()`.
- `lib.rs` — exports `FlyCam`, `FlyCamInputContext`.

`FlyCam` is still inert (no controller, no actions/bindings), but every `FlyCam` entity now carries its own enhanced-input context, registered exactly like the orbit side.

### Step 1.3 — shared input plugin; move `touch` into `input/`

`touch.rs` sat at crate root but is an input source — its only non-test consumer is `input/adapter/inject.rs`, and `touch_tracker` runs in `OrbitCamInputPhase::PreInput` before `AdapterInjection`. The gesture computation (`TouchTracker`/`TouchGestures`) is camera-agnostic shared infra; the OrbitCam-specific part (gesture→orbit-action mapping) already lives in `inject.rs`.

**Principle applied** (user): move misplaced things *fully* to where they belong, then make them right at the right time. The shared input layer should own its own registration; `lib.rs` reaching into input internals was itself one of the "wrong" things. Created the shared input plugin now — that's encapsulation/ownership (same move as `OrbitCamPlugin`), **not** speculative abstraction. The deferred thing is a shared input *trait*/context, which still waits for FlyCam to be the second consumer.

**Moved:**
- `src/touch.rs` → `src/input/touch.rs` (`git mv`, history preserved); `mod touch;` in `input/mod.rs`.
- New `pub(super) struct InputPlugin` (`input/mod.rs`) — owns the enhanced-input core (guarded `EnhancedInputPlugin`), bevy `Touches`/`PinchGesture`, `TouchTracker`, and the `touch_tracker` system. Re-exports `TouchTracker`/`TouchGestures` at `crate::input` (`pub(crate)`).
- `lib.rs` — dropped `mod touch`, the touch/enhanced-input registration, and the `Touches`/`PinchGesture`/`EnhancedInputPlugin`/`OrbitCamInputInternalSet` imports; `LagrangePlugin::build` now composes `InputPlugin` alongside system sets + observers. `animation::process_camera_move_list` stays (it's animation, not input).
- `use` paths updated: `inject.rs` → `crate::input::Touch*`; test modules (`animation.rs`, `adapter/mod.rs`) → `crate::input::`/`crate::input::touch::`.

**Behavior:** pure relocation, no functional change. Build + clippy clean, 174/174 tests pass. No FlyCam analog needed — `touch` is shared infra both kinds consume through the shared `InputPlugin`.

### Step 1.4 — fix context divergence (1.2 regression)

Step 1.2 mirrored the wrong way: `OrbitCamInputContext` lives in `input/context.rs`, but I put `FlyCamInputContext` in a new `fly_cam/context.rs`. Same concept, two homes — divergence. Fixed by converging both into `input/context.rs` (chosen over moving both into per-camera modules):
- `input/context.rs` — added `FlyCamInputContext` next to `OrbitCamInputContext`; `input/mod.rs` re-exports it.
- Deleted `fly_cam/context.rs`; `fly_cam/mod.rs` now `use crate::input::FlyCamInputContext;` for its `#[require]`.
- `lib.rs` — `FlyCamInputContext` re-exported via `input`, not `fly_cam`.

Drove the "**Same structure, not just same concept**" addition to the mirror rule above. Build + clippy clean, 174/174 pass.
