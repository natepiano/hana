# Team Review Findings: `bevy_lagrange` Input Refactor

**Source document:** `docs/bevy_lagrange/input-refactor.md` (2157 lines)

**Reviewed by 5 expert agents covering:**
1. Correctness & Completeness
2. Architecture & Design
3. Risk & Failure Modes
4. Type System & Changeability
5. User Impact & Ergonomics

Findings are deduplicated across reviewers, ordered by severity, and tagged with `[source]` when one reviewer flagged it or `[consensus]` when multiple did.

---

## Critical

### F1 — Undefined core binding types (`BindingRecipe<A>`, `BindingEngagement`)
**Source:** Correctness

**Problem:** `BindingRecipe<A>` is referenced as a field of `ActionBindingEntry<A>` (lines 351, 415–416, 422–423) and of `HeldActionBindingEntry<A>` but its definition is never shown anywhere in the doc. Same for `BindingEngagement` (line 354, used by `ActionBindingEntry`). Implementers can't tell whether these are enums, structs, builders, newtypes around enhanced-input `Binding<T>`, or trait objects. The whole binding spec — public API, validation, reflection support — depends on what these types are.

**Impact:** Critical — the public API surface cannot be implemented without inventing these types, and different invention choices yield different validation, reflection, and ergonomics stories.

**Recommendation:** Add a definition section that shows both types in full, with their variants/fields and which paths construct them (builder, conversion, reflection). State whether they are public or `pub(crate)`. Show how each `BindingRecipe<A>` carries (or refers to) the underlying enhanced-input binding plus Lagrange source metadata.

---

### F2 — `apply_deferred` strategy is fragile and may not survive Bevy version drift
**Source:** Correctness + Risk [consensus]

**Problem:** Two related issues converge here:

1. The schedule wires `GateContexts` and `InjectAdapters` `.before(EnhancedInputSystems::Update)` with internal `.chain()` plus `apply_deferred`, and asserts "Any command-buffered entity, relationship, or context-activity changes needed by enhanced input must be visible before `EnhancedInputSystems::Update`" (lines 1698–1701). But Bevy's `.before()` does not force flushes from intermediate systems — a system in another configured chain can sit between the inner `apply_deferred` and the target set. The doc admits the risk but the configuration shown doesn't guarantee the structural property it claims.

2. `apply_deferred` itself is on a deprecation path in newer Bevy releases (replaced by automatic barriers and exclusive systems). Pinning the design to its explicit use ties scheduling correctness to an API that's likely to move.

**Impact:** Critical — context gating and adapter injection both depend on this barrier landing before enhanced input reads action state. If it doesn't, stale action values leak across route transitions and source attribution becomes flaky.

**Recommendation:** Make `gate_orbit_cam_input_contexts` and `inject_orbit_cam_adapter_values` exclusive (`&mut World`) systems, or wrap each behind an exclusive flusher. That makes the barrier structural without relying on `apply_deferred`. Also collapse the 7-set chain (see F4) so the structural ordering is easier to reason about. Document the minimum supported Bevy version and audit on each upgrade.

---

### F3 — Held-binding motion/engagement pairing is runtime-validated, but reflection can bypass it
**Source:** Type system + Risk + Correctness [consensus]

**Problem:** `HeldActionBindingEntry<A>::try_new` validates that motion and engagement bindings have compatible sources at construction (lines 414–430), and the doc requires "Reflection, deserialization, or dynamic keymap loading must go through the same validation path" (lines 434–435, 545–563). But `OrbitCamBindings` is also required to derive `Reflect` because it's wrapped in `OrbitCamControls` (lines 254–261). Deriving `Reflect` exposes raw field-level deserialization paths that don't route through `try_build`, so a scene/keymap file can produce an `OrbitCamBindings` with motion-without-engagement, or impulse bindings with engagement state, that only fails at the moment a held binding is installed — by which point the camera has already accepted the controls component.

Additionally, the doc never specifies the failure path when reconciliation discovers invalid reflected bindings: panic? drop to default? log and disable?

**Impact:** Critical — modding, scene files, and any `from_reflect` deserialization can put cameras into states the type system claims are unrepresentable. Failure becomes a runtime crash or silent fallback.

**Recommendation:** Separate the wire representation from the runtime representation. Introduce a `OrbitCamBindingsDescriptor` (the reflectable / serializable side) and keep `OrbitCamBindings` as a validated opaque type with no public fields and no derived `Reflect`. Provide `TryFrom<OrbitCamBindingsDescriptor> for OrbitCamBindings` that runs the existing `try_build` logic. Reconciliation converts on `Changed<OrbitCamControls>`; conversion failure routes through a specified path (recommend: emit a structured event, leave previous bindings in place, log once per camera).

---

## Important

### F4 — 7-set PreUpdate chain mixes concerns and inflates the integration surface
**Source:** Architecture

**Problem:** `ReconcileControls → Route → GateContexts → InjectAdapters → ResolveActions → WriteManual → FinalizeInput` (lines 1566–1573). Each adds `apply_deferred`, dependency ordering against `EnhancedInputSystems::Update`/`Apply`, and a public name in `OrbitCamInputSet`. The split mixes structural cleanup (Reconcile), routing (Route), context lifecycle (Gate), adapter mocking (Inject), action resolution (Resolve), user surface (WriteManual), and finalization (Finalize). Future additions like Roll will pressure new sets.

**Impact:** Important — every additional set is a public-API integration point that downstream apps will reach into. The wider the surface, the harder it is to change.

**Recommendation:** Collapse to three public sets:
- `OrbitCamInputSet::PreInput` — reconcile, route, gate, inject (exclusive systems internally to avoid `apply_deferred` issues).
- `OrbitCamInputSet::WriteManual` — public user slot, unchanged.
- `OrbitCamInputSet::Finalize` — resolve, manual visibility, finalize.

Keep the internal phases as `pub(crate)` system names. The public ordering contract shrinks to "user code writes in `WriteManual`, observes in `Finalize`."

---

### F5 — `OrbitCamControls::Manual` overloads "control mode" with "input writer source"
**Source:** Architecture + Type system [consensus]

**Problem:** `OrbitCamControls::Manual` is data-less but means "the app writes `OrbitCamInput` directly." It sits in the same enum as `Preset(...)` and `Custom(bindings)` which mean "the library resolves user input." The doc itself acknowledges the resulting name overload with `CameraInputRouting` (lines 1343–1350). The split means library systems must filter every per-camera operation by control mode at query time; the type system can't enforce "preset/custom systems never touch a manual camera's `OrbitCamInput`" or "a manual writer never targets a preset camera."

**Impact:** Important — runtime filtering is the only safety net; a future maintainer who adds a per-camera system without the right `With<...>` filter will silently corrupt manual cameras (or be overridden by them).

**Recommendation:** Replace the enum with three marker components: `PresetControls(OrbitCamControlPreset)`, `CustomControls(OrbitCamBindings)`, `ManualControls`. Use `#[require]` on `OrbitCam` to default-insert `PresetControls(SimpleMouse)`. Library systems query `With<PresetControls>` or `With<CustomControls>`; manual writer helpers query `With<ManualControls>`. Compiler enforces mode-specific reach.

---

### F6 — Owner-latch and routing rules are underspecified across recovery edges
**Source:** Architecture + Risk + Correctness [consensus]

**Problem:** Latch recovery is asserted at a high level (lines 1500–1512: "Clear ... immediately on camera despawn, OrbitCam removal, controls replacement, OrbitCamInputDisabled, target window close, application focus loss, or selected gamepad disconnect") but several concrete scenarios are not pinned down:
1. Multi-source holds — if mouse is latched to camera A and keyboard arrives, does keyboard route to A (held-owner rule) or to the cursor-hit camera? Per-source vs single-owner is ambiguous.
2. Gamepad reconnect — disconnect clears latch; does a same-frame press re-acquire through the routing fallback or wait?
3. Camera despawn during interaction — `On<Remove, OrbitCam>` observer firing order vs in-flight routing isn't specified; with mass despawns (scene teardown, `despawn_recursive`) the ended-event emission can be lost.
4. Multi-camera gamepad with `Selected(entity_a)` and `Selected(entity_b)` — disconnect of one, where does the next press go?

**Impact:** Important — these are the exact scenarios that break multi-camera editors and split-screen apps.

**Recommendation:** Replace the single owner latch with a per-source latch table (mouse, touch[id], gamepad[entity], keyboard) and document the rule: route by source-specific latch first, then by no-position fallback, then by hit-test. Make camera-removal cleanup a single explicit system that runs in `PreInput` and reads `RemovedComponents<OrbitCam>` rather than relying on observer ordering. Add the four scenarios above as explicit test cases in §Testing.

---

### F7 — Adapter/public-binding conflict detection is asserted but not specified
**Source:** Architecture

**Problem:** `OrbitCamBindingsError::AdapterBindingConflict` exists (line 551), and the doc says "the binding API should prevent or reject equivalent public enhanced-input bindings" (lines 879–880) for any raw source the adapter handles. But the conflict-detection algorithm is never specified: how does the validator know that a user's enhanced-input binding targets `MouseWheel`? Enhanced-input's `Binding` enum is closed; the validator presumably matches on variant. The doc doesn't show that match, doesn't enumerate which sources are adapter-owned, and doesn't say at what stage (build time, `try_build`, reconciliation) the check fires.

**Impact:** Important — double-counting wheel/pinch/touch events is a regression hazard whenever the low-level escape hatch is used.

**Recommendation:** Add a table mapping each adapter-owned source (`MouseWheel::Line`, `MouseWheel::Pixel`, `PinchGesture`, `Touches`) to the public binding variants that conflict with it, and show the match arm that detects them. Run the check inside `try_build` so conflicts surface before any installation. Note in rustdoc that the `from_enhanced_input` escape hatch is the only way to reach these sources, and it must route through the adapter.

---

### F8 — Preset/bindings boundary is implicit; reconciliation behavior may diverge
**Source:** Architecture

**Problem:** `OrbitCamControls::Preset(BlenderLike)` and `OrbitCamControls::Custom(bindings)` are treated as two paths, but the doc never states whether a preset is internally an `OrbitCamBindings` produced by a known constructor, or a separate code path that bypasses the binding builder. If they're separate paths, reconciliation, validation, and `try_build` rules can diverge subtly between presets (which presumably never fail validation) and custom bindings.

**Impact:** Important — drift between preset and custom code paths is a common bug source. It also blocks the natural "start from a preset, modify it" pattern.

**Recommendation:** Define presets explicitly as bindings constructors: `impl OrbitCamControlPreset { fn to_bindings(self) -> OrbitCamBindings }`. Internally, reconciliation always operates on `OrbitCamBindings` regardless of variant. This unifies code paths, lets users start from a preset and customize, and removes ambiguity in `Changed<OrbitCamControls>` handling.

---

### F9 — Lifecycle events vs blocking ordering is not stable
**Source:** Risk + Correctness [consensus]

**Problem:** Two ordering questions are unresolved:
1. Impulse interactions emit Started + Ended in the same frame (lines 1227, 1233). If the same frame's blocker activates (animation insertion, focus change), what wins — the lifecycle pair or the blocker's `Ended`?
2. `FinalizeInput` is in `PreUpdate`; animation can be inserted later by observers in `Update`. The doc adds a pre-controller guard in `PostUpdate` (lines 1727–1729) that re-checks animation state for `Ignore`, but lifecycle events were already emitted in `PreUpdate`. Tools subscribing to `CameraInteractionStarted` will see input that never reached the controller.

**Impact:** Important — every tool that highlights live camera state from lifecycle events (the docs explicitly mention `fairy_dust` overlays) can desync from controller behavior.

**Recommendation:** Define a single rule: lifecycle events are emitted only for input that the controller will observe this frame. Move final event emission into the pre-controller guard, or queue events in `FinalizeInput` and flush them in the guard after the last blocker check. Add a test for the late-animation case.

---

### F10 — Pinch suppression depends on resolved modifier state but the read timing is ambiguous
**Source:** Risk

**Problem:** Pinch is suppressed by "that camera's resolved action/modifier state" (lines 783–788). `GateContexts` can deactivate or reset action state before `EnhancedInputSystems::Update`. If pinch arrives during a frame where the camera context was just reset (route change, focus regain), the adapter sees cleared modifier state even though the physical modifier key is held, and pinch fires when it shouldn't.

**Impact:** Important — accidental zoom during pan-modifier operations is the exact bug current pinch suppression was added to fix.

**Recommendation:** Have the adapter sample modifier state in `InjectAdapters` (before enhanced-input update wipes anything) and use that sampled snapshot for the suppression check, rather than reading post-gate action state.

---

### F11 — `OrbitCamInteractionState` is documented "read-only" but has public fields
**Source:** Risk + Type system [consensus]

**Problem:** Lines 1259–1265 define the component with public `orbit_sources`/`pan_sources`/`zoom_sources` fields. Apps can `Query<&mut OrbitCamInteractionState>` and overwrite them. The interaction tracker, lifecycle events, and owner latch all assume this component is authoritative — app mutation desyncs all three.

**Impact:** Important — silent footgun; bug reports will look like "events fire spuriously" with no obvious cause.

**Recommendation:** Make fields `pub(crate)` and expose getters. The library writes through internal methods that maintain the lifecycle invariants. App code reads via accessor methods.

---

### F12 — `ManualInputSource`'s "always includes MANUAL" invariant is documentation-only
**Source:** Type system

**Problem:** The doc says `ManualInputSource` cannot be reflected and cannot drop the MANUAL bit (lines 933–949). But the underlying `CameraInteractionSources` has `from_bits` (line 1149), so any code that converts from raw flags can produce a source set without MANUAL. As soon as the writer accepts `CameraInteractionSources` anywhere, the invariant escapes.

**Impact:** Important — provenance loss for manual writes invalidates the source-attribution claim of the whole lifecycle system.

**Recommendation:** Make `ManualInputSource` a sealed newtype that contains `CameraInteractionSources` and whose only conversion exposes that as `CameraInteractionSources` *with the MANUAL bit forced on*. Manual writer methods accept `ManualInputSource`, never raw sources.

---

### F13 — Impulse vs held distinction is runtime-validated, not type-encoded
**Source:** Type system

**Problem:** Impulse bindings must reject `OrbitEngaged`/`PanEngaged`/`ZoomEngaged` (line 450), enforced at `try_build` via `EngagementBindingForImpulse` (line 551). A future action added without thinking through its phase will silently pick up engagement state.

**Impact:** Important — every new action type is an opportunity to violate the rule.

**Recommendation:** Add a sealed `ActionPhase` trait with `Held` and `Impulse` markers; implement it on each `InputAction`. `HeldActionBindingEntry<A: HeldAction>` and `ImpulseActionBindingEntry<A: ImpulseAction>` become statically distinct; the compiler refuses to bind engagement to an impulse action.

---

### F14 — Default-spawn ergonomics: missing-plugin diagnostic fires after the user already shipped
**Source:** Ergonomics

**Problem:** The doc's quick-start (lines 106–108) shows `commands.spawn((Camera3d::default(), OrbitCam::default()));` but doesn't show the surrounding app setup. The diagnostic for "OrbitCam exists but LagrangePlugin is missing" is a warning (line 226–228) — easy to miss in development logs and impossible to surface to end users of the app.

**Impact:** Important — first-experience pit. A user copying the quick-start without `LagrangePlugin` gets a non-functional camera and a log line they may never see.

**Recommendation:** Change the diagnostic from `warn!` to a one-time `error!` plus a panic-on-startup option (defaultable, controllable through `LagrangePlugin` settings). Expand every quick-start example to include the full minimal `App` with `LagrangePlugin`.

---

### F15 — Mandatory wheel-policy choice without a recommended default
**Source:** Ergonomics

**Problem:** Typestate forces every custom-bindings user to call `.wheel(...)` before `.build()` works (lines 486–513). Good for safety. But the doc gives no guidance on which value to pick; the example uses `OrbitCamWheelBinding::blender_like()` without saying why.

**Impact:** Important — every custom-bindings author dives into wheel-policy docs before they can compile.

**Recommendation:** Either (a) provide `.wheel_for(OrbitCamControlPreset::SimpleMouse)` / `.wheel_for(OrbitCamControlPreset::BlenderLike)` shortcuts that pick the preset's policy, or (b) write the `MissingWheelPolicy` error message to recommend `ZoomOnly` as the safe default and link to the policy table.

---

### F16 — Render-to-texture migration teaches three new concepts simultaneously
**Source:** Ergonomics

**Problem:** `render_to_texture.rs` migration (lines 1862–1871) hands the user `CameraInputRouting::Explicit` + `CameraInputRoutingConfig` + `CameraInputSurfaceMetrics` + the note that manual mode is *not* the right answer. The relationships among these are scattered through the §Active Camera Routing section.

**Impact:** Important — multi-window/editor adoption depends on this example. If users guess wrong they end up in `Manual` mode unnecessarily.

**Recommendation:** Add a "Render-to-texture walkthrough" subsection inside `input/mod.rs` rustdoc and the design doc that shows the full pattern in one place: explicit routing → optional surface-metrics override → keep preset/custom controls. Cross-link from §Manual Input ("if you're here for RTT, see §RTT instead").

---

### F17 — Migration table doesn't show the "temporarily pause input" pattern
**Source:** Ergonomics

**Problem:** Lines 1830–1831 map `input_control = None` to "OrbitCamInputDisabled when preserving controls; Manual when taking over." But the common case is "pause for a menu and resume" — the migration table doesn't show the insert/remove pair, and the design's manual mode section emphasizes manual is for full takeover, leaving the pause case under-documented.

**Impact:** Important — it's the most common reason existing apps touch input today.

**Recommendation:** Add a worked example to the migration table showing `insert(OrbitCamInputDisabled)` and the matching `remove::<OrbitCamInputDisabled>()`, and mention it in the §Input Disabling rustdoc.

---

### F18 — Gamepad selection pushes connection tracking onto every gamepad consumer
**Source:** Ergonomics

**Problem:** `GamepadSelectionPolicy::Selected(Entity)` (lines 469–474) requires the app to know which gamepad entity to pass and to swap it on connect/disconnect. The doc says the library handles latch cleanup on disconnect, but doesn't show the recommended app-side pattern for "use first connected gamepad, fall back if it disconnects."

**Impact:** Important — without a recipe, apps will either use `Any` (multiple controllers fight) or reinvent disconnect handling.

**Recommendation:** Add a helper system or `GamepadSelectionPolicy::first_connected()` variant that internally tracks the first connected gamepad. Document in rustdoc that disconnects are reconciled by the library, and provide a worked example in `controls_custom_gamepad.rs`.

---

## Minor

### F19 — Private `OrbitCamInputEntityOf` relationship is heavier than the use case needs
**Source:** Architecture

**Problem:** Lines 567–586 introduce a custom relationship purely for "all private input entities owned by this camera." The same query is available with `ChildOf` + a private marker component or a `HashMap<Entity, Vec<Entity>>` resource.

**Impact:** Minor — works, but adds machinery (`Reflect` derive, macro use) for a problem `With<PrivateInputEntity>` already solves.

**Recommendation:** Use `ChildOf` plus a `pub(crate) struct OrbitCamInputEntity;` marker. Reconciliation queries children filtered by the marker. Drop the custom relationship.

---

### F20 — Missing surface metrics fail silently to a `warn!` log
**Source:** Architecture + Risk [consensus]

**Problem:** Lines 1075, 1480–1483 say screen-pixel input drops with "a structured warning" when metrics can't be derived. In a release build with default log filter, that warning is invisible. Manual-mode users get no feedback that their input is being dropped.

**Impact:** Minor — debuggability issue.

**Recommendation:** Emit a per-camera one-time `error!` (not `warn!`) plus a public `CameraInputMetricsMissing` event so apps can surface the failure in their own UI/logs. Run a startup diagnostic for any `OrbitCam` whose metrics can't be derived.

---

### F21 — `HeldBindingSourceMismatch` semantics are not defined
**Source:** Architecture

**Problem:** The error variant exists (line 552) but the doc doesn't say what counts as a "mismatch." Same source-flag category? Same modifier set? Same activation predicate?

**Impact:** Minor — the implementer has to invent the rule.

**Recommendation:** Define explicitly: motion and engagement must share at least one source-flag category (MOUSE/KEYBOARD/TOUCH/GAMEPAD). Document accepted vs rejected pair examples.

---

### F22 — `Camera::is_active` mid-frame toggling and routing
**Source:** Correctness

**Problem:** Routing is computed in `Route` and locked for the frame, but the doc doesn't say what happens if `Camera::is_active` flips between then and `FinalizeInput`.

**Impact:** Minor — uncommon outside multi-pass renderers and editor layouts.

**Recommendation:** State the rule: routing is locked at `Route`; subsequent activity flips block input via the inactive blocker rather than re-routing. Add a test.

---

### F23 — Global gesture fallback silently produces no input
**Source:** Risk

**Problem:** Global `PinchGesture`/`PanGesture` with no window metadata fall through to "no camera input" (lines 1406–1408). Users will see "my pinch doesn't work" with no log.

**Impact:** Minor — workaround is explicit routing.

**Recommendation:** Log a once-per-camera-frame debug message identifying which gesture was ambiguously routed and which cameras were eligible.

---

### F24 — Manual writer's shorthand/verbose split has no recommended pattern
**Source:** Ergonomics

**Problem:** `orbit_pixels(x, y)` vs `orbit(OrbitDelta::screen_pixels(x, y), ManualInputSource::observed_keyboard())` — no doc on when to pick which.

**Impact:** Minor — users default to shorthand and miss provenance.

**Recommendation:** Add one rustdoc paragraph: "shorthand for prototyping and tests; verbose form when you want source attribution to flow into `CameraInteractionStarted` for editor overlays or analytics."

---

### F25 — Error-message text for `OrbitCamBindingsError` variants is not designed
**Source:** Ergonomics

**Problem:** Variants are precise but the doc never specifies the text users will see.

**Impact:** Minor — variants are recoverable enough that someone will figure them out, but every new custom-bindings author hits these first.

**Recommendation:** Specify the `Display` text for each variant in the doc, including a one-line fix suggestion.

---

### F26 — Public re-export list is large with no discoverability hierarchy
**Source:** Ergonomics

**Problem:** Lines 139–171 re-export 25+ public types. `input/mod.rs` has a quick-start but no structured "quick-start vs advanced" outline.

**Impact:** Minor — discoverability.

**Recommendation:** Restructure `input/mod.rs` rustdoc into "Quick start / Observing interactions / Advanced (RTT, manual) / Reference" subsections so users can skip what they don't need.

---

### F27 — `ActionBindingSet<A>` phantom parameter does no work if newtypes are removed
**Source:** Type system

**Problem:** The compiler safety from line 360 ("pan bindings can't be installed as orbit bindings") comes from the newtypes `OrbitBindings`/`PanBindings`, not from the generic `<A>`. If the newtypes are inlined in a future refactor, the generic provides no protection.

**Impact:** Minor — only a concern under future refactors.

**Recommendation:** Document in code that the newtype wrappers are the safety mechanism, not the generic parameter. Or commit fully to types by making the generic carry a `PhantomData<A>` used in trait bounds elsewhere.

---

### F28 — Test checklist misses controls-replacement during Ignore-policy animation
**Source:** Correctness

**Problem:** The test list covers "controls change during interaction" and "interrupt policies preserve behavior" separately, not in combination.

**Impact:** Minor — edge case.

**Recommendation:** Add the combined test: animation active under `Ignore`, controls replaced mid-frame, verify `CameraInteractionEnded` fires once and input is cleared.

---

### F29 — Adding a new interaction kind (Roll) updates many sites
**Source:** Type system

**Problem:** Adding Roll requires touching `CameraInteractionKind`, `OrbitCamInput`, `OrbitCamInteractionState`, internal tracker, all preset tables, and `ManualOrbitCamInput`. Doc acknowledges this in §Future Cleanup but doesn't reduce the cost.

**Impact:** Minor — predictable cost; doc already non-exhaustively marks the enum.

**Recommendation:** Out of scope for this refactor. If Roll lands, consider a trait-based `CameraInteractionKind` with associated `Action` type so the tracker becomes generic.

---

## Summary Table

| ID  | Severity  | Title | Source |
|-----|-----------|-------|--------|
| F1  | critical  | Undefined `BindingRecipe<A>` / `BindingEngagement` | Correctness |
| F2  | critical  | `apply_deferred` strategy fragile + version risk | Correctness + Risk |
| F3  | critical  | Held-binding pairing bypassable via reflection | Type + Risk + Correctness |
| F4  | important | 7-set PreUpdate chain | Architecture |
| F5  | important | `OrbitCamControls::Manual` overload | Architecture + Type |
| F6  | important | Owner-latch recovery underspecified | Architecture + Risk + Correctness |
| F7  | important | Adapter/binding conflict algorithm unspecified | Architecture |
| F8  | important | Preset vs bindings boundary implicit | Architecture |
| F9  | important | Lifecycle/blocking event ordering | Risk + Correctness |
| F10 | important | Pinch suppression modifier-state timing | Risk |
| F11 | important | `OrbitCamInteractionState` public fields | Risk + Type |
| F12 | important | `ManualInputSource` MANUAL-bit invariant only documented | Type |
| F13 | important | Impulse vs held distinction not type-encoded | Type |
| F14 | important | Missing-plugin diagnostic too quiet | Ergonomics |
| F15 | important | Mandatory wheel policy without default | Ergonomics |
| F16 | important | RTT migration teaches three concepts at once | Ergonomics |
| F17 | important | "Pause input" pattern missing from migration | Ergonomics |
| F18 | important | Gamepad selection lacks recipe | Ergonomics |
| F19 | minor     | Custom relationship overkill | Architecture |
| F20 | minor     | Surface metrics fail silently | Architecture + Risk |
| F21 | minor     | `HeldBindingSourceMismatch` semantics undefined | Architecture |
| F22 | minor     | `Camera::is_active` mid-frame behavior | Correctness |
| F23 | minor     | Global gesture fallback silent | Risk |
| F24 | minor     | Manual writer API split unguided | Ergonomics |
| F25 | minor     | Error message text not designed | Ergonomics |
| F26 | minor     | Re-export list lacks hierarchy | Ergonomics |
| F27 | minor     | Phantom `<A>` parameter | Type |
| F28 | minor     | Test gap: controls-change during Ignore animation | Correctness |
| F29 | minor     | Adding new interaction kind is multi-site | Type |
