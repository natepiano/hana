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

### Do it now if it's correctly doable now

The counterweight to the rule above. When a step's analysis concludes a change is **correct** and **everything needed to make it correctly is already in front of us**, make the change *in that step* — do not park it. "It's not strictly about FlyCam," "we'll get to it later," "that's a different subsystem's call," and "it's orthogonal to the descent" are **not** reasons to leave a known-correct change unmade.

Deferral is legitimate in exactly one case: execution depends on a consumer or trigger that does **not exist yet** (you can't rename `OrbitCamInputPhase` correctly until FlyCam proves it uses the identical phases). Those go in *Pending decisions* with the explicit trigger — nowhere else.

So the two rules together draw one line: **act the moment a change becomes correctly doable — no earlier (no speculation), no later (no parking).** If the only thing stopping you is "later," later is now.

## Pending decisions (deferred, not yet executed)

Decisions we've *made* but are deliberately not acting on yet, because the FlyCam consumer that makes them real hasn't landed. Each is executed when the descent reaches the step that needs it. Renames are the user's to sweep via editor global-rename. **Newest last.**

| # | Decision | Trigger to execute | Stakes |
|---|----------|--------------------|--------|
| P1 | Rename `OrbitCamInputPhase` → shared `CameraInputPhase`. The phases (`PreInput`/`WriteManual`/`Finalize`) are anchored to bevy `InputSystems` + `EnhancedInputSystems` with zero orbit-specific content — one shared schedule structure both kinds run in, **not** a per-kind copy. | When FlyCam's controller / manual-input writer becomes the second consumer of these phases. | Public API break. |
| P2 | Rename `OrbitCamInputInternalSet` → `CameraInputInternalSet` (shared). Same reasoning as P1; `InputModes` is shared because **Preset/Bindings/Manual is a cratewide approach** — FlyCam uses the same three modes. | Same as P1. | `pub(crate)` — no API stakes. |
| P3 | Likely rename `OrbitCamInputMode` → `CameraInputMode` (Preset/Bindings/Manual is cratewide, both kinds map input the same three ways). Confirm the variants are identical for FlyCam before sweeping. | When FlyCam grows its input-mode handling. | Public API break. |

## Traversal order (top-down)

1. ~~`lib.rs` — top-level plugin registration + public API.~~ **DONE** (see log).
2. ~~`touch.rs` → `input/`; shared `InputPlugin`.~~ **DONE** (step 1.3).
3. ~~`system_sets.rs` — anchor-type analysis.~~ **DONE** (step 1.5): sets are **shared, not mirrored**; renames deferred → P1/P2.
4. ~~`projection.rs` + `fit_overlay/` placement.~~ **DONE** (step 1.6): both moved under `fit/`; `fit/mod.rs` is facade + `FitPlugin`.
5. ~~`orbital_math.rs` — anchor-type analysis.~~ **DONE** (step 1.7): split into shared `interpolation.rs` (root) + orbit-specific `orbit_cam/orbital_math.rs`.
6. (next) the `OrbitCam` struct itself — anchor-type analysis: shared vs specific.
7. … remaining root modules (`animation.rs`, `components.rs`, `events/`, `constants.rs`) then down the crate (input pipeline, bindings, presets, …) as the recipe recurses.

## Crate map (reference, as of start)

```
src/
├── lib.rs              top-level LagrangePlugin + public exports
├── orbit_cam/          OrbitCam component + controller (the one camera today),
│                       orbital_math.rs (spherical-coord transform math)
├── input/              OrbitCam* input pipeline: intent, actions, axis_response,
│                       modes, bindings/, routing/, adapter/
├── events/             camera animation/motion events
├── fit/                fit-to-bounds domain: mod.rs (FitPlugin facade),
│                       solve.rs (calculate_fit), projection.rs (geometry),
│                       overlay/ (debug overlay, feature-gated)
├── observers/          event observers
├── interpolation.rs    shared frame-rate-independent smoothing (both kinds)
├── animation.rs, components.rs, constants.rs,
├── system_sets.rs, touch.rs, enhanced_input.rs
```

(Crate map updated through step 1.7: `interpolation.rs` is shared at root, orbit-specific math is `orbit_cam/orbital_math.rs`; `projection.rs` + `fit_overlay/` under `fit/` (1.6); `touch.rs`/`enhanced_input.rs` already moved/deleted in steps 1.1/1.3 — listed as-of-start.)

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

### Step 1.5 — `system_sets` anchor-type analysis (no code change)

**Asked:** are the schedule sets orbit-specific, mirrored per-kind, or shared?

**Decided: shared, not mirrored.** `OrbitCamInputPhase` (`PreInput`/`WriteManual`/`Finalize`) is anchored entirely to bevy `InputSystems` + `EnhancedInputSystems` — no yaw/pitch/focus/pan/zoom. It's the temporal structure of any lagrange camera that consumes enhanced-input; both kinds run the same `PreUpdate` schedule against the same EI window. A `FlyCamInputPhase` would be the contexts-style divergence — there is no separate "fly PreInput" slot. The `OrbitCam` prefix is mono-mode naming. Same for `OrbitCamInputInternalSet`, including `InputModes`: **Preset/Bindings/Manual is a cratewide approach** (user-confirmed), so FlyCam uses the same three modes through the same set.

**No rename yet** (no-speculation rule): FlyCam is inert and references none of these sets. Recorded the certain-but-deferred renames as P1/P2 (and the related `OrbitCamInputMode` → `CameraInputMode` as P3) in *Pending decisions*. Executed when FlyCam's input pipeline becomes the second consumer.

### Step 1.6 — fold `projection.rs` + `fit_overlay/` into a `fit/` domain

**Asked:** are `projection.rs` and `fit_overlay/` correctly placed at crate root, or do they belong to a `fit/` domain?

**Decided (style-guide-driven):**
- `projection.rs` is camera-agnostic geometry consumed by the core fit solve **and** the overlay — both fit-family. It isn't overlay-only (so can't live in feature-gated `fit_overlay`) and isn't a root concern. → `fit/projection.rs`. One file, 327 lines, trips only the multiple-clusters split criterion (needs 2+) → **not** further split.
- `fit.rs` is the fit solve (`calculate_fit` → `FitSolution`); no single `Fit` type → it's a domain, not a module-name type. Module roots are table-of-contents only, so the 872-line solver can't live in `fit/mod.rs`. → `fit/solve.rs` (kept as **one** child; the solve→refine pipeline is one coupled algorithm, user-confirmed not to split).
- `fit_overlay/` independently consumes `projection` and does **not** depend on core `fit.rs` (and vice-versa) — a fit-family sibling, not an outside reacher. → `fit/overlay/`.
- `fit/mod.rs` is facade + **`FitPlugin`** (domain registration point per the bevy-plugin rule). `FitPlugin` feature-gates the overlay child internally, so the `fit_overlay` gate stops leaking into `lib.rs`. Core fit registers nothing (pure logic), so `FitPlugin` is thin today — correct home regardless, and where FlyCam-era fit registration lands.
- Renamed `ZoomOverlayPlugin` → **`FitOverlayPlugin`** (it's the fit overlay, not a zoom plugin; the mismatch got louder under `fit/overlay/`).

**Moved:** `projection.rs`→`fit/projection.rs`; `fit.rs`→`fit/solve.rs`; `fit_overlay/`→`fit/overlay/`; new `fit/mod.rs`. Path rewrites: `crate::projection`→`crate::fit::projection`, `crate::fit_overlay`→`crate::fit::overlay`, `super::constants`→`crate::constants` (solve/projection), overlay's `super::components::FitOverlay`→`crate::components::FitOverlay`. `fit/mod.rs` re-exports `calculate_fit`/`FitSolution` (`pub(crate)`) and `Edge` (gated, overlay-only) so consumer paths `crate::fit::*` stay stable. `lib.rs`: dropped `mod projection`/`mod fit_overlay`, added `FitPlugin` to the shared plugin tuple, re-pathed `FitTargetOverlayConfig` through `fit`.

**Behavior:** pure relocation, no functional change. Verified both feature states: default clippy clean + 174/174; `--features fit_overlay` clippy clean + 185/185. No FlyCam analog — `fit/` is shared infra both kinds consume.

### Step 1.7 — split `orbital_math.rs` into shared interpolation + orbit-specific math

**Asked:** is root-level `orbital_math.rs` one cohesive unit, or does it mix shared and orbit-specific concerns?

**Decided (two clusters, split):** the file held two unrelated function groups —
- **Orbit-pivot-specific:** `calculate_from_translation_and_focus` (world pos → yaw/pitch/radius), `update_orbit_transform` (spherical coords → `Transform`, incl. projection-coupled ortho `scale`/perspective near-clip), `sync_perspective_near_clip`. All assume a focus+radius+yaw/pitch orbit — meaningless for a FlyCam that owns its transform directly.
- **Shared:** `lerp_and_snap_f32` / `lerp_and_snap_position` (frame-rate-independent exponential smoothing with snap-to-target) + the private `approx_equal` they use. Camera-agnostic; a FlyCam smoothing its move/look wants exactly these.

FlyCam isn't a consumer yet, but the smoothing helpers are a real second-consumer-bound shared layer (same justification as the shared `InputPlugin` in 1.3), so the split is encapsulation, not speculation.

**Moved:**
- New `src/interpolation.rs` (root) — `lerp_and_snap_f32` / `lerp_and_snap_position` (`pub(crate)`), `approx_equal` (demoted to private — only the lerp fns use it), + their 3 test modules. Imports `crate::constants::{EPSILON, SMOOTHNESS_EXPONENT}`.
- `orbital_math.rs` → `orbit_cam/orbital_math.rs` (`git mv`) — the three orbit fns, demoted `pub(crate)` → `pub(super)` (only sibling `controller.rs` calls them) + their 2 test modules. Imports re-pathed `super::constants` → `crate::constants`.
- `lib.rs` — `mod orbital_math` → `mod interpolation`. `orbit_cam/mod.rs` — added `mod orbital_math;`.
- `controller.rs` — `use crate::orbital_math` → `use super::orbital_math` + `use crate::interpolation`; lerp call sites re-pointed `orbital_math::` → `interpolation::`.

**Behavior:** pure relocation, no functional change. Verified both feature states: default clippy clean + 174/174; `--features fit_overlay` clippy clean + 185/185. The shared-interpolation home is where FlyCam's smoothing lands without a second copy.

**Next:** step 5 — `OrbitCam` struct anchor-type analysis (shared camera state vs orbit-pivot-specific vs what FlyCam needs).
