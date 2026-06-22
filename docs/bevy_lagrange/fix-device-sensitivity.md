# Device-specific camera sensitivity

> **Status: IMPLEMENTATION PLAN — phased, delegate-ready.** Builds per-device orbit-camera sensitivity in the binding/preset layer while preserving `OrbitCam` master gains, preset identity, reflected editing, and Fairy Dust camera-control behavior.

## Delegation Context
<!-- Shared across all phases. /plan:delegate prepends this to every dispatch. -->

- **Project:** `bevy_lagrange` + `fairy_dust` — orbit-camera input preset/binding sensitivity, reflected input-mode editing, and Fairy Dust camera-control display/preset helpers.
- **Stack:** Rust 2024 workspace; Bevy `0.19.0`; `bevy_enhanced_input` `0.26.0` with `reflect`; `bevy_lagrange` default feature `reflect-input-modes`; `fairy_dust` example helper uses Bevy/Fairy Dust camera-control panels.
- **Layout:** `crates/bevy_lagrange/src/input/bindings/` — presets, descriptors, validation, runtime binding storage; `crates/bevy_lagrange/src/input/adapter/` — install/inject/resolve adapter and BEI bindings; `crates/bevy_lagrange/src/input/{modes,lifecycle,control_summary}.rs` — mode reconciliation, reflected apply, reporting, summaries; `crates/bevy_lagrange/src/orbit_cam/` — `OrbitCam` helpers/globals; `crates/fairy_dust/src/{builder,camera_control_panel,orbit_cam}.rs` — camera builders and panel identity; `crates/bevy_lagrange/examples/` — preset/custom examples and stale unit-preset call sites.
- **Key files:** `docs/bevy_lagrange/fix-device-sensitivity.md` — source plan and invariants; `crates/bevy_lagrange/Cargo.toml` — crate features/deps; `Cargo.toml` — workspace deps/lints; `crates/bevy_lagrange/src/input/bindings/preset/enum_preset.rs` — `OrbitCamPreset` identity/lowering; `crates/bevy_lagrange/src/input/bindings/preset/{simple_mouse,blender_like,gamepad,simple_mouse_keyboard,blender_like_keyboard,keyboard}.rs` — typed preset settings and generated bindings; `crates/bevy_lagrange/src/input/bindings/{builder,descriptor,validate,error,mod}.rs` — binding API, descriptors, `InputBindingScale`, validation, runtime storage, inline tests; `crates/bevy_lagrange/src/input/adapter/{install,inject,resolve,mod}.rs` — BEI install scales, raw adapter staging, smooth-scroll selection, source attribution, adapter tests; `crates/bevy_lagrange/src/input/{modes,lifecycle,control_summary,mod}.rs` — reflected mode drafts/apply, transactionality, lifecycle debounce, control rows, public input exports; `crates/bevy_lagrange/src/{lib,orbit_cam/mod,orbit_cam/preset_helpers}.rs` — crate exports, `OrbitCam` master gains, preset/bindings bundle helpers; `crates/bevy_lagrange/src/animation.rs` — animation interruption tests for zero-sensitivity input; `crates/fairy_dust/src/builder/{sprinkle,primitive,studio_lighting}.rs` — preset helper overloads; `crates/fairy_dust/src/camera_control_panel/{snapshot,guidance,preset_switch,display}.rs` — preset labels, slow-mode hints, snapshot rows, cycle behavior/tests; `crates/fairy_dust/src/orbit_cam.rs` — Fairy Dust camera marker/installation helper; `crates/bevy_lagrange/examples/{input_preset_blender_like,input_custom,input_gamepad,input_keyboard,basic,animation,programmatic_control,zoom_to_fit,render_to_texture,swapped_axis,showcase/main,follow_target,focus_bounds,orthographic,pausing,viewports_windows}.rs` — example/docs unit-preset construction and sensitivity docs.
- **Build:** `cargo build --release --workspace --all-features --examples`
- **Test:** `cargo nextest run --all-features --workspace --tests`
- **Lint:** `cargo +nightly fmt --all --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo check --workspace --examples --all-features`; `cargo check -p bevy_lagrange --all-targets --no-default-features`; `taplo fmt --check`; `cargo mend --fail-on-warn`
- **Style:** `zsh ~/.claude/scripts/rust_style/load-rust-style.sh --project-root /Users/natemccoy/rust/bevy_lagrange_device_sensitivity`
- **Invariants:** Per-device sensitivity belongs in binding/preset lowering, not `OrbitCam`; `OrbitCam::{orbit,pan,zoom}_sensitivity` remain final master gains; preset identity must be setting-insensitive and preserved as `Preset / <kind>`; custom bindings stay `Bindings / Custom`; public wording uses “smooth scroll” for Bevy pixel-scroll input; `InputSensitivity` accepts finite non-negative values and `0.0` disables runtime participation/source attribution; do not make signed `InputBindingScale` non-negative; smooth-scroll sensitivity has one owner and must not double-scale; reflected apply and runtime preset reconciliation validate before replacing mode/resolved bindings/installations; Fairy Dust hide/show preserves tuned presets and explicit cycling constructs default target presets; do not rewrite source flags to express sensitivity.

## Phases

### Phase 1 — Add a compile-safe preset bridge API  · status: done (uncommitted)

#### Work Order

**Goal:** Add preset identity helpers, borrow-safe APIs, constructors, and `with_preset(...)` helpers while the current unit-variant preset enum still compiles.

**Spec:**
Add a bridge layer before changing `OrbitCamPreset` into payload-carrying variants.

`OrbitCamPreset` must expose setting-insensitive identity:

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Default)]
#[non_exhaustive]
pub enum OrbitCamPresetKind {
    #[default]
    SimpleMouse,
    BlenderLike,
    Keyboard,
    SimpleMouseKeyboard,
    BlenderLikeKeyboard,
    Gamepad,
}
```

Add `OrbitCamPresetKind::name(&self) -> &'static str` and `OrbitCamPreset::kind(&self) -> OrbitCamPresetKind`. Keep `PartialEq` on `OrbitCamPreset` as full value equality for future payload variants; kind equality is only for identity labels, comparisons, and preset cycling.

Make `OrbitCamPreset::name()` and `OrbitCamPreset::to_bindings()` borrow `&self` rather than consuming `self`. Update call sites that currently use `*preset`, by-value `const fn preset_mode_value(preset: OrbitCamPreset)`, or direct unit-variant matching for labels.

Add constructors for all six preset identities before the payload enum exists:

```rust
impl OrbitCamPreset {
    pub fn simple_mouse() -> Self;
    pub fn blender_like() -> Self;
    pub fn keyboard() -> Self;
    pub fn simple_mouse_keyboard() -> Self;
    pub fn blender_like_keyboard() -> Self;
    pub fn gamepad() -> Self;
}
```

Add `OrbitCamInputMode::with_preset(preset: impl Into<OrbitCamPreset>) -> Self` and update `OrbitCam::simple_mouse()`, `OrbitCam::blender_like()`, `OrbitCam::gamepad()`, `OrbitCam::keyboard()`, `OrbitCam::simple_mouse_keyboard()`, `OrbitCam::blender_like_keyboard()`, and `OrbitCam::with_preset(preset)` to build through the helper path. Keep `OrbitCam::with_bindings(bindings)` as the app-owned custom binding constructor.

Migrate current source away from direct construction such as `OrbitCamPreset::BlenderLike` where the later payload change would break or drop settings. After this phase, remaining direct unit variants should be isolated to enum constructors or tests explicitly proving constructors return the current unit variants.

Update Fairy Dust preset-switch scaffolding enough that it no longer depends on `OrbitCamPreset: Copy + Eq`: preset cycling should compare by `preset.kind()` and construct targets through `OrbitCamPreset::simple_mouse()` / `OrbitCamPreset::blender_like()`.

**Files:**
- `crates/bevy_lagrange/src/input/bindings/preset/enum_preset.rs` — add `OrbitCamPresetKind`, borrowed methods, constructors, identity tests.
- `crates/bevy_lagrange/src/input/modes.rs` — add `OrbitCamInputMode::with_preset`, avoid by-value preset use.
- `crates/bevy_lagrange/src/input/control_summary.rs` — use borrowed `preset.to_bindings()` and `preset.kind().name()`.
- `crates/bevy_lagrange/src/orbit_cam/preset_helpers.rs` — add `OrbitCam::with_preset` and route preset helpers through constructors.
- `crates/bevy_lagrange/src/lib.rs` — export `OrbitCamPresetKind` and new helper API.
- `crates/fairy_dust/src/camera_control_panel/{snapshot,preset_switch,guidance}.rs` — remove duplicate unit-variant label matching and use kind/name helpers.
- `crates/fairy_dust/src/builder/{sprinkle,primitive,studio_lighting}.rs` — accept `impl Into<OrbitCamPreset>` for preset helpers where possible.
- `crates/bevy_lagrange/examples/{input_gamepad,input_keyboard,basic,animation,programmatic_control,zoom_to_fit,render_to_texture,swapped_axis,showcase/main,follow_target,focus_bounds,orthographic,pausing,viewports_windows}.rs` — replace public-facing unit-variant construction with constructors or `with_preset(...)` helpers where this phase makes it possible.

**Constraints from prior phases:** None.

**Acceptance gate:** `cargo check -p bevy_lagrange --all-targets --all-features` passes; `cargo check -p fairy_dust --all-targets --all-features` passes; tests prove `OrbitCamPreset::blender_like().kind().name() == "BlenderLike"`, `describe_orbit_cam_controls(&OrbitCamInputMode::with_preset(OrbitCamPreset::blender_like()))` still reports `Preset / BlenderLike`, and Fairy Dust visible/hidden preset cycling compares by kind rather than full preset value.

#### Retrospective

**What worked:**
- `OrbitCamPresetKind`, constructor helpers, borrowed `name`/`to_bindings`, `OrbitCamInputMode::with_preset`, `OrbitCam::with_preset`, and crate exports all shipped cleanly.
- Fairy Dust preset switching now compares `preset.kind()` and constructs cycle targets through `OrbitCamPreset::simple_mouse()` / `OrbitCamPreset::blender_like()`.

**What deviated from the plan:**
- The stale direct-construction sweep expanded into `crates/bevy_diegetic/examples`, `crates/bevy_liminal/examples`, and docs/as-built snippets because those public examples also used unit variants.
- The blind review caught stale docs after implementation; those docs and one Rust source comment were fixed directly.
- `cargo mend --fail-on-warn` did not run because the configured `kache` wrapper rejected the `cargo-mend` binary path; the wrapper was not disabled.

**Surprises:**
- The old direct preset construction appeared in public docs outside the files named by Phase 1.
- `cargo +nightly fmt --all --check` also enforced wrapping for the edited Rust source comment.

**Implications for remaining phases:**
- Later phases can rely on `OrbitCamPresetKind`, constructor helpers, `OrbitCamInputMode::with_preset`, `OrbitCam::with_preset`, and borrowed preset lowering as the bridge API.
- Preserve the constructor/helper form in future examples and docs; direct unit variants should stay isolated to constructor tests until the payload enum migration removes them.

#### Phase 1 Review

- Phase 2 now names adapter-backed singleton/vector storage as the canonical payload later phases consume, including smooth-scroll identity preservation.
- Phase 5 now includes Fairy Dust preset cycling as a compile dependency if payload constructors stop being const, and requires reflected preset apply to validate before replacing existing working input.
- Phase 7 is narrowed to payload-preservation behavior after Phase 1 already shipped builder overloads, kind labels, and kind-based cycling; its file scope now includes diegetic/liminal examples and public docs/as-built snippets.
- Phase 8 now carries the `cargo mend --fail-on-warn` blocker from Phase 1 as a validation dependency that must be resolved without clearing `RUSTC_WRAPPER`.

### Phase 2 — Add sensitivity value types and custom binding storage  · status: done (uncommitted)

#### Work Order

**Goal:** Add validated sensitivity types and per-binding API/storage without changing built-in preset behavior.

**Spec:**
Add orbit-camera sensitivity types:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub struct OrbitCamSensitivity {
    orbit: InputSensitivity,
    pan:   InputSensitivity,
    zoom:  InputSensitivity,
}

#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub struct InputSensitivity(pub f32);
```

`OrbitCamSensitivity::new()` defaults orbit, pan, and zoom to `1.0`. `OrbitCamSensitivity::uniform(value)` sets all three values to `value`. Add fluent `.orbit(f32)`, `.pan(f32)`, and `.zoom(f32)` setters. `InputSensitivity` validates finite and non-negative values; `0.0` means intentionally disabled. Reuse `OrbitCamBindingsError::InvalidScale` unless a specific new error is needed.

Keep `InputBindingScale` signed. Do not make `InputBindingScale` globally non-negative because existing scale modifiers encode direction and axis/unit mapping.

Thread sensitivity through descriptor and validated binding storage. Store sensitivity separately from signed scale until final lowering. The authored validated binding storage must preserve explicit zero-sensitivity entries for editor round-trips and export; runtime systems will use enabled-only views in later phases.

The Phase 2 wrapper/storage shape is the canonical payload consumed by later native and adapter lowering. It must represent singleton and vector-backed adapter sources without losing source identity, including mouse wheel, smooth scroll, touch, pinch, and button-drag zoom. Do not collapse duplicate smooth-scroll descriptors or erase the identity Phase 4 needs for smooth-scroll candidate selection, source attribution, and zero-sensitivity disable behavior.

Expose `.with_sensitivity(f32)` for individual custom bindings. For existing public binding structs with public fields, prefer wrapper descriptors over adding public fields. `OrbitCamMouseDrag`, `OrbitCamTrackpadScroll`, and `OrbitCamButtonDragZoom` already expose public fields; avoid breaking downstream struct literals and avoid removing `Eq` derives unless unavoidable.

Wrapper setters must keep modifier-order ergonomics. Calling `.with_sensitivity(...).with_mod_keys(...)` and `.with_mod_keys(...).with_sensitivity(...)` should either both work or the unsupported order should fail at compile time with examples consistently using the supported one. Prefer forwarding existing small setters on wrapper types so callers do not have to memorize terminal setter order.

Custom binding examples must remain the advanced story, not the main preset story:

```rust
let bindings = OrbitCamBindings::builder()
    .zoom(OrbitCamMouseWheelZoom.with_sensitivity(0.25))
    .zoom(OrbitCamButtonDragZoom::new(MouseButton::Middle).with_sensitivity(0.4))
    .build()?;
```

**Files:**
- `crates/bevy_lagrange/src/input/bindings/descriptor.rs` — add `InputSensitivity`, sensitivity fields on binding descriptors/entries, validation helpers.
- `crates/bevy_lagrange/src/input/bindings/builder.rs` — accept wrapper descriptors and expose `.with_sensitivity(...)` on user-facing binding kinds.
- `crates/bevy_lagrange/src/input/bindings/held_binding.rs` — carry authored sensitivity through held binding descriptors.
- `crates/bevy_lagrange/src/input/bindings/validate.rs` — validate sensitivity independently from signed scale.
- `crates/bevy_lagrange/src/input/bindings/error.rs` — reuse or extend error reporting for invalid sensitivity.
- `crates/bevy_lagrange/src/input/bindings/mod.rs` — export new sensitivity types and wrappers.
- `crates/bevy_lagrange/src/lib.rs` — public exports for `OrbitCamSensitivity` and `InputSensitivity`.
- `crates/bevy_lagrange/examples/input_custom.rs` — add one readable `.with_sensitivity(...)` call without turning the example into a sensitivity reference.

**Constraints from prior phases:** Phase 1 shipped `OrbitCamPresetKind`, `OrbitCamPreset::{simple_mouse,blender_like,keyboard,simple_mouse_keyboard,blender_like_keyboard,gamepad}`, borrowed `OrbitCamPreset::{name,to_bindings}`, `OrbitCamInputMode::with_preset`, `OrbitCam::with_preset`, and public `OrbitCamPresetKind` exports. Keep those APIs stable, preserve `Preset / <kind>` identity labels through `preset.kind().name()`, and keep direct unit-variant construction isolated to constructor tests. This phase does not change built-in preset defaults or `OrbitCamPreset` variant shape.

**Acceptance gate:** Binding validation tests reject negative, `NaN`, and infinite sensitivity; accept `0.0`; default custom bindings generate the same validated data as before; adapter-backed authored entries preserve source identity for wheel, smooth scroll, touch, pinch, and button-drag zoom; `.with_mod_keys(...).with_sensitivity(...)` and the supported reverse order behave as documented; `input_custom.rs` still describes app-owned `Bindings / Custom` and uses “smooth scroll” in user-facing prose.

#### Retrospective

**What worked:**
- `InputSensitivity`, `OrbitCamSensitivity`, binding wrappers, validation, public exports, and custom-binding tests shipped without changing default preset behavior.
- Adapter-backed storage now preserves smooth-scroll identity with target plus authored index while keeping runtime sensitivity application for later phases.

**What deviated from the plan:**
- The implementation touched `crates/bevy_lagrange/src/input/adapter/{install,inject,mod}.rs` to keep adapter source identity explicit, even though those files were not in the original Phase 2 file list.
- `cargo mend --fail-on-warn` still did not run because `kache` rejects the `cargo-mend` binary path; `RUSTC_WRAPPER` was not cleared.

**Surprises:**
- `OrbitCamTouchBindingConfig` needed to become part of the public API so touch sensitivity can round-trip through builder/config accessors.
- `control_summary.rs` had to adapt to wrapper-backed trackpad and button-drag accessors even though it intentionally does not display sensitivity yet.

**Implications for remaining phases:**
- Phase 3 should compose native BEI `Scale` from `InputBindingEntry::sensitivity()` and existing signed scale without dropping disabled authored entries.
- Phase 4 should reuse the existing wrapper storage plus `TrackpadBindingCondition { target, index, mod_keys }` identity instead of introducing a second smooth-scroll identity scheme.
- Phase 8 should continue carrying the `cargo mend`/`kache` blocker as a validation dependency.

#### Phase 2 Review

- Phase 3 now names `adapter/resolve.rs` and `adapter/inject.rs` because native enabled-only views also affect pan-overrides-orbit and pinch suppression.
- Phase 4 now treats Phase 2's indexed smooth-scroll identity as the chosen rule, not an option, and requires duplicate same-binding sensitivity tests.
- Phase 4 now requires adapter-specific enabled-only accessors or predicates for trackpad, wheel, pinch, button-drag, and per-action touch sensitivity.
- Phase 5 now requires invalid preset replacement to preserve current-frame input and avoid replacement events before validation succeeds.
- Phase 7 now frames the cross-repo constructor/prose sweep as a regression check plus payload-related doc/example updates, because Phase 1 already performed the broad unit-variant migration.

### Phase 3 — Compose sensitivity into native enhanced-input bindings  · status: todo

#### Work Order

**Goal:** Apply custom binding sensitivity to native BEI binding installation while preserving signed scale direction and unit mapping.

**Spec:**
Native BEI bindings scale through the existing binding modifier path:

- `OrbitCamMouseDrag` lowers into an `OrbitCamHeldBinding`.
- `OrbitCamInputBinding` already supports `with_scale(...)`.
- `install.rs` already inserts BEI `Scale` modifiers when a descriptor entry carries a scale.

Store sensitivity separately from signed scale until final lowering. Then multiply any existing signed `InputBindingScale` by `InputSensitivity`. Do not replace the signed scale. This must handle uniform and per-axis scale values; a signed scale of `-2.0` with sensitivity `0.25` lowers to `-0.5`.

The composition must have one owner. The final installed `Scale` comes from one helper that combines signed scale and sensitivity, independent of whether the caller wrote `.with_scale(...).with_sensitivity(...)` or `.with_sensitivity(...).with_scale(...)`.

Keep authored zero-sensitivity entries in storage, but expose enabled-only iteration for native runtime installation and source aggregation. A zero-sensitivity held binding must not be installed as an active input path and must not participate in source selection, pan-overrides-orbit, pinch suppression, lifecycle events, or active control rows.

Enabled-only native accessors must be consumed everywhere native held/action entries affect runtime behavior: installer source masks, active control rows, lifecycle source/latch helpers, `resolve.rs` pan-overrides-orbit logic, and `inject.rs` pinch suppression logic.

**Files:**
- `crates/bevy_lagrange/src/input/bindings/descriptor.rs` — combine signed scale and sensitivity at the entry/modifier boundary.
- `crates/bevy_lagrange/src/input/bindings/held_binding.rs` — expose enabled-only held binding accessors.
- `crates/bevy_lagrange/src/input/bindings/action_set.rs` — route runtime iteration through enabled-only views while preserving authored storage.
- `crates/bevy_lagrange/src/input/bindings/validate.rs` — prove enabled-only runtime views and authored storage remain coherent.
- `crates/bevy_lagrange/src/input/adapter/install.rs` — install combined BEI `Scale` modifiers and skip disabled held entries.
- `crates/bevy_lagrange/src/input/adapter/inject.rs` — make pinch suppression consume enabled-only native held entries.
- `crates/bevy_lagrange/src/input/adapter/resolve.rs` — make pan-overrides-orbit consume enabled-only native held entries.
- `crates/bevy_lagrange/src/input/control_summary.rs` — describe effective enabled bindings, not disabled entries.
- `crates/bevy_lagrange/src/input/lifecycle.rs` — make held-source/latch helpers consume enabled-only views.

**Constraints from prior phases:** Phase 1 shipped the preset bridge APIs (`OrbitCamPresetKind`, constructor helpers, borrowed `name`/`to_bindings`, `OrbitCamInputMode::with_preset`, `OrbitCam::with_preset`) and moved public examples/docs to constructor form; do not reintroduce direct unit-variant construction outside constructor tests. Phase 2 introduced `InputSensitivity`, `InputBindingEntry::sensitivity()`, wrapper-backed adapter storage, and preserved authored disabled payload. Do not drop authored disabled entries from validated storage; runtime paths must use enabled-only accessors.

**Acceptance gate:** Tests prove signed native scale composes with sensitivity for positive, negative, uniform, and per-axis scale; setter order composes identically; zero-sensitive native held bindings are absent from installed actions, source aggregation, control summaries, lifecycle sources, pan-overrides-orbit behavior, and pinch suppression; default native bindings are unchanged.

### Phase 4 — Apply sensitivity to adapter-backed sources and smooth scroll  · status: todo

#### Work Order

**Goal:** Apply sensitivity before adapter-backed raw input becomes semantic camera intent, with one scaling owner per source.

**Spec:**
Adapter-backed sources stage raw Bevy input before BEI resolves semantic actions:

- wheel line scroll -> `adapter_zoom_coarse`;
- smooth pixel scroll -> `adapter_orbit`, `adapter_pan`, or `adapter_zoom_smooth`;
- pinch -> `adapter_zoom_smooth`;
- touch -> `adapter_orbit`, `adapter_pan`, and `adapter_zoom_smooth`;
- button-drag zoom -> `adapter_zoom_smooth`.

Apply each adapter sensitivity at the point where the source contribution is known, before the adapter contribution becomes semantic action input. Do not wait until `collect_camera_input`.

Smooth scroll is a hybrid adapter/BEI path and needs exactly one scaling owner. Keep injected custom input as raw pixel scroll, carry the selected binding's sensitivity through smooth-scroll target selection, and compose installed custom binding scale as:

- orbit/pan: `sensitivity`;
- zoom: `PIXEL_SCROLL_SCALE * sensitivity`, with existing zoom inversion still applied.

Do not also apply smooth-scroll sensitivity in injection or resolution.

Smooth-scroll selection must identify one concrete binding using the Phase 2 identity shape. `TrackpadScrollCandidate` and `TrackpadBindingCondition` carry `target`, authored `index`, and `mod_keys`; duplicate non-disabled smooth-scroll bindings with the same `(target, mod_keys)` are allowed. Among non-disabled candidates whose modifier keys match, prefer the highest modifier-key count, then zoom over pan over orbit, then the highest authored index within the same target. The selected candidate, installed condition, and installed scale must all refer to the same indexed binding after zero-sensitivity candidates are filtered.

Sensitivity must not change which target wins.

Existing adapter normalization constants remain source-unit conversion defaults:

- `PIXEL_SCROLL_SCALE` converts pixel-scroll zoom into smooth-zoom units;
- `PINCH_GESTURE_AMPLIFICATION` converts Bevy pinch gestures into smooth-zoom units;
- `TOUCH_PINCH_SCALE` converts two-finger touch pinch into smooth-zoom units;
- `BUTTON_ZOOM_SCALE` converts button-drag motion into smooth-zoom units.

User sensitivity multiplies the normalized source contribution. It does not replace these constants.

Wheel, pinch, touch, and button-drag zoom have distinct absent, enabled, and explicitly disabled states. Runtime getters used by input systems are enabled-only; export and descriptor round-trips preserve authored disabled singleton entries. Builder behavior for repeated singleton calls must be explicit, either rejecting duplicates or documenting last-write-wins, with tests for the chosen rule.

Add adapter-specific enabled-only accessors or predicates for trackpad, wheel, pinch, button-drag, and per-action touch sensitivity. Installer, injector, source attribution, and control-summary paths use enabled-only adapter views; authored getters remain available for export/editor round-trips.

`OrbitCamTouchBinding` can emit orbit, pan, and zoom from one policy. Its validated runtime config must carry `OrbitCamSensitivity` or explicit orbit/pan/zoom sensitivity fields. Zero sensitivity disables only that action's contribution and source attribution, not unrelated touch actions.

Apply sensitivity during binding/config lowering, not by changing `CameraInteractionSources` at resolution. Tuned wheel zoom still reports `WHEEL`, button-drag zoom still reports `MOUSE`, and pixel scroll still reports `SMOOTH_SCROLL`.

**Files:**
- `crates/bevy_lagrange/src/input/adapter/inject.rs` — keep raw staging raw and apply sensitivity only where source contribution is known.
- `crates/bevy_lagrange/src/input/adapter/resolve.rs` — carry selected smooth-scroll binding sensitivity and preserve target priority.
- `crates/bevy_lagrange/src/input/adapter/install.rs` — install smooth-scroll custom binding scale from selected binding sensitivity.
- `crates/bevy_lagrange/src/input/adapter/mod.rs` — adapter tests for wheel, smooth scroll, pinch, touch, button-drag zoom, source flags.
- `crates/bevy_lagrange/src/input/bindings/{builder,descriptor,validate,mod}.rs` — singleton config structs, touch action-shaped sensitivity, duplicate/last-write behavior.
- `crates/bevy_lagrange/src/input/routing/{snapshot,mod}.rs` — source attribution must remain source-specific.
- `crates/bevy_lagrange/src/input/control_summary.rs` — effective rows for enabled adapter-backed sources.

**Constraints from prior phases:** Phase 1 shipped the preset bridge APIs (`OrbitCamPresetKind`, constructor helpers, borrowed `name`/`to_bindings`, `OrbitCamInputMode::with_preset`, `OrbitCam::with_preset`) and moved public examples/docs to constructor form; do not reintroduce direct unit-variant construction outside constructor tests. Phase 2 introduced wrapper-backed adapter storage and indexed smooth-scroll identity via `TrackpadBindingCondition { target, index, mod_keys }`; do not replace that identity with duplicate rejection. Phase 3 established enabled-only native runtime views; add separate adapter enabled-only views for smooth-scroll target selection, installed conditions, active-modifier checks, pinch suppression, source aggregation, and summaries.

**Acceptance gate:** Tests prove smooth-scroll zoom resolves exactly to `delta.y * PIXEL_SCROLL_SCALE * sensitivity` with no second sensitivity factor; duplicate non-disabled smooth-scroll bindings with the same `(target, mod_keys)` but different sensitivities select the documented indexed binding and apply that binding's installed scale; cross-target priority remains modifier-count first, then zoom over pan over orbit; zero-sensitive wheel, pinch, touch orbit/pan/pinch, smooth scroll, and button-drag zoom produce no semantic delta, no source attribution, and no active input state; tuned wheel zoom scales while rows/source flags still report `WHEEL`.

### Phase 5 — Add payload-carrying presets and source-level preset setters  · status: todo

#### Work Order

**Goal:** Store tuned preset settings inside `OrbitCamPreset` while preserving preset identity and default behavior.

**Spec:**
Change `OrbitCamPreset` from unit variants to payload-carrying variants:

```rust
#[derive(Clone, Debug, PartialEq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamPreset {
    SimpleMouse(OrbitCamSimpleMousePreset),
    BlenderLike(OrbitCamBlenderLikePreset),
    Keyboard(OrbitCamKeyboardPreset),
    SimpleMouseKeyboard(OrbitCamSimpleMouseKeyboardPreset),
    BlenderLikeKeyboard(OrbitCamBlenderLikeKeyboardPreset),
    Gamepad(OrbitCamGamepadPreset),
}
```

Normal callers use constructors and conversions, not direct enum construction:

```rust
impl From<OrbitCamBlenderLikePreset> for OrbitCamPreset { /* ... */ }
impl From<OrbitCamSimpleMousePreset> for OrbitCamPreset { /* ... */ }
impl From<OrbitCamGamepadPreset> for OrbitCamPreset { /* ... */ }

impl OrbitCam {
    pub fn with_preset(preset: impl Into<OrbitCamPreset>) -> impl Bundle { /* ... */ }
}

impl OrbitCamInputMode {
    pub fn with_preset(preset: impl Into<OrbitCamPreset>) -> Self { /* ... */ }
}
```

Provide associated constructors and `From<TypedPreset> for OrbitCamPreset` for all six preset identities: simple mouse, Blender-like, keyboard, simple mouse plus keyboard, Blender-like plus keyboard, and gamepad.

Constructors are the public path, but they do not have to remain `const` if payload construction prevents that. Update any existing `const fn` call sites that invoke `OrbitCamPreset::{simple_mouse,blender_like,...}()`; in particular, Fairy Dust `next_cycle_entry(...)` in `crates/fairy_dust/src/camera_control_panel/preset_switch.rs` can become a normal `fn` if constructors are no longer const.

Implement `Default` manually for `OrbitCamPreset` and every typed preset struct that needs default construction. Keep `#[reflect(Default)]` only where the manual default exists.

Add source-level traits:

```rust
pub trait MouseSensitivity {
    type Sensitivity;

    fn mouse_sensitivity(self, sensitivity: Self::Sensitivity) -> Self;
}

pub trait SmoothScrollSensitivity {
    type Sensitivity;

    fn smooth_scroll_sensitivity(self, sensitivity: Self::Sensitivity) -> Self;
}

pub trait GamepadSensitivity {
    type Sensitivity;

    fn gamepad_sensitivity(self, sensitivity: Self::Sensitivity) -> Self;
}
```

`OrbitCamBlenderLikePreset`, `OrbitCamSimpleMousePreset`, and composed pointer presets implement the mouse and smooth-scroll traits with `Sensitivity = OrbitCamSensitivity`. `OrbitCamGamepadPreset` implements the gamepad trait with `Sensitivity = OrbitCamSensitivity`. `OrbitCamKeyboardPreset` does not implement these source traits unless keyboard sensitivity becomes a real source-level concept later.

Prefer inherent forwarding setters on concrete preset types used in public examples so the advertised `.mouse_sensitivity(...)` and `.smooth_scroll_sensitivity(...)` snippets compile without trait import surprises. Keep source traits as the reusable API pattern.

Store source sensitivity on the leaf pointer or gamepad preset. Composed presets forward `mouse_sensitivity(...)` and `smooth_scroll_sensitivity(...)` into their pointer child. Replacing the pointer child with `.simple_mouse(...)` or `.blender_like(...)` replaces that child's tuning; changing `.keyboard(...)` leaves pointer tuning intact.

Source-level sensitivity lowering rules:

- For pointer presets, `mouse_sensitivity.orbit` and `.pan` apply to generated mouse-drag bindings.
- `mouse_sensitivity.zoom` applies to mouse-backed zoom such as line-wheel zoom and any mouse button-drag zoom owned by that preset.
- `smooth_scroll_sensitivity` applies only to Bevy pixel-scroll bindings.
- Pinch and touch stay at `1.0` unless a future source setter explicitly covers them.
- For gamepad presets, `gamepad_sensitivity` multiplies every generated gamepad binding for the matching action, including normal and gated slow entries, while preserving gates, `ControlSpeed`, and fast/slow ratios.

Existing gamepad scale knobs remain base source-unit mappings. `orbit_scale`, `slow_orbit_scale`, `pan_scale`, `slow_pan_scale`, `zoom_scale`, and `slow_zoom_scale` validate independently. `gamepad_sensitivity` validates as finite and non-negative, then multiplies final generated bindings. Sensitivity `0.0` disables all matching gamepad entries.

Runtime preset replacement must validate before clearing or replacing an existing installation. Reconciliation lowers and validates preset payloads before removing previous resolved bindings, action entities, installation entities, input mode, or current-frame `OrbitCamInput`. Invalid runtime preset replacement must not emit a replacement/success event before validation succeeds.

Reflected preset apply must follow the same validate-before-replace rule in this phase because `OrbitCamInputModeDraft::Preset(OrbitCamPreset)` will carry the payload enum as soon as Phase 5 changes it. Do not let reflected apply insert `Preset(tuned)` and report success before preset lowering validates; an invalid reflected preset must leave the previous mode, resolved bindings, installation, and action children intact.

**Files:**
- `crates/bevy_lagrange/src/input/bindings/preset/enum_preset.rs` — payload enum, constructors, `From` impls, kind/name/to_bindings, default.
- `crates/bevy_lagrange/src/input/bindings/preset/{simple_mouse,blender_like,gamepad,simple_mouse_keyboard,blender_like_keyboard,keyboard}.rs` — source sensitivity fields, forwarding setters, validation, generated binding lowering.
- `crates/bevy_lagrange/src/input/bindings/preset/mod.rs` — export source traits and typed preset APIs.
- `crates/bevy_lagrange/src/input/modes.rs` — validate preset lowering before replacing runtime input installations.
- `crates/bevy_lagrange/src/input/control_summary.rs` — use `preset.kind().name()` for identity and `preset.to_bindings()` on the full payload for rows.
- `crates/bevy_lagrange/src/orbit_cam/preset_helpers.rs` — route `OrbitCam::with_preset(...)` through payload constructors.
- `crates/bevy_lagrange/src/lib.rs` — export source traits and typed preset/sensitivity APIs.
- `crates/fairy_dust/src/camera_control_panel/preset_switch.rs` — keep kind-based cycling compiling if preset constructors stop being const.

**Constraints from prior phases:** Phase 1 shipped `OrbitCamPresetKind`, constructor helpers, borrowed `name`/`to_bindings`, `OrbitCamInputMode::with_preset`, `OrbitCam::with_preset`, public `OrbitCamPresetKind` exports, and Fairy Dust cycling that compares by `preset.kind()` while constructing target presets through helpers. Keep `PartialEq` as full value equality after payloads; use `kind()` only for labels, identity comparisons, and preset cycling. Phase 2-4 added custom binding sensitivity, enabled-only runtime views, and adapter sensitivity lowering. Preserve all default preset behavior when sensitivity is untouched.

**Acceptance gate:** Tests prove default `SimpleMouse`, `BlenderLike`, and `Gamepad` presets generate behaviorally unchanged bindings; tuned `BlenderLike` and `SimpleMouse` presets feed generated mouse and smooth-scroll bindings; `mouse_sensitivity.zoom` affects wheel/button-drag zoom but not pinch/touch; gamepad sensitivity scales normal and gated slow entries without losing `ControlSpeed`; `OrbitCam::with_preset(preset)` and `OrbitCamInputMode::with_preset(preset)` install `Preset(tuned)` and describe as `Preset / <kind>`; runtime insertion and reflected descriptor apply of an invalid typed preset over a working mode leave previous mode, resolved bindings, installation, action children, current-frame input, replacement/success events, and apply status correct; Fairy Dust preset cycling still compiles when constructors carry payloads.

### Phase 6 — Add reflected preset drafts, export, validation, and registration  · status: todo

#### Work Order

**Goal:** Let reflected/editor input-mode flows express tuned presets transactionally without falling back to `Bindings`.

**Spec:**
The reflected input-mode path mirrors the runtime shape under `reflect-input-modes`. Change `OrbitCamInputModeDraft::Preset` to hold `OrbitCamPresetDraft`, not runtime `OrbitCamPreset`.

Add reflected draft payloads:

```rust
#[derive(Clone, Debug, PartialEq, Reflect)]
#[non_exhaustive]
pub enum OrbitCamPresetDraft {
    SimpleMouse(OrbitCamSimpleMousePresetDraft),
    BlenderLike(OrbitCamBlenderLikePresetDraft),
    Keyboard(OrbitCamKeyboardPresetDraft),
    SimpleMouseKeyboard(OrbitCamSimpleMouseKeyboardPresetDraft),
    BlenderLikeKeyboard(OrbitCamBlenderLikeKeyboardPresetDraft),
    Gamepad(OrbitCamGamepadPresetDraft),
}

#[derive(Clone, Copy, Debug, PartialEq, Reflect)]
pub struct OrbitCamSensitivityDraft {
    pub orbit: f32,
    pub pan: f32,
    pub zoom: f32,
}

#[derive(Clone, Debug, PartialEq, Reflect)]
pub struct OrbitCamBlenderLikePresetDraft {
    pub mouse_sensitivity: OrbitCamSensitivityDraft,
    pub smooth_scroll_sensitivity: OrbitCamSensitivityDraft,
    pub zoom_mod_keys: ModKeys,
    pub slow_toggle_key: Option<KeyCode>,
    pub slow_toggle_mod_keys: ModKeys,
    pub slow_scale: f32,
}
```

`OrbitCamSensitivityDraft` defaults all values to `1.0`. Conversion from draft sensitivity into runtime `OrbitCamSensitivity` is the validation boundary and rejects negative, `NaN`, and infinite values.

Implement `Default` manually for `OrbitCamPresetDraft` and every child draft that needs default construction. Keep `#[reflect(Default)]` only where the manual default exists.

Descriptor application is transactional. The same phase that introduces `OrbitCamPresetDraft` must include draft conversion, validate-before-insert behavior, rejection status/events, and tests that invalid tuned preset drafts leave runtime installation unchanged. Converting a draft preset validates sensitivity and runs `preset.to_bindings()` before mutating `OrbitCamInputMode`.

Preset export must preserve tuned preset identity and payload. Do not promise a blanket `From<&OrbitCamInputMode> for OrbitCamInputModeDraft` unless custom binding export is defined. Either add a full `TryFrom<&OrbitCamBindings> for OrbitCamBindingsDescriptor` export contract, or narrow runtime-to-draft export to preset/manual helpers plus tuned-preset round-trip tests. Preset export must not lower tuned presets to `Bindings`.

Composed reflected drafts mirror composed preset structure. Use nested child drafts, or an intentionally flattened equivalent, for `SimpleMouseKeyboard` and `BlenderLikeKeyboard` so pointer sensitivity, keyboard settings, and Blender-like slow-mode settings survive editor round-trips.

Define descriptor ownership after apply. `OrbitCamInputModeDescriptor` is a mutable draft component; after successful apply it can become stale if Rust code, Fairy Dust preset cycling, or another system changes `OrbitCamInputMode` directly. Treat descriptors as write-only apply requests, or add a sync/export path that updates or removes stale descriptors without reapply loops. Test direct mode changes and preset cycling before export.

Under `reflect-input-modes`, register `OrbitCamInputModeDescriptor`, `OrbitCamInputModeDraft`, `OrbitCamPresetDraft`, all child draft structs, `OrbitCamSensitivityDraft`, `OrbitCamSensitivity`, `InputSensitivity`, and runtime preset/kind types with Bevy's type registry.

**Files:**
- `crates/bevy_lagrange/src/input/modes.rs` — reflected draft enum, apply conversion, transactionality, descriptor ownership behavior.
- `crates/bevy_lagrange/src/input/bindings/preset/{enum_preset,simple_mouse,blender_like,gamepad,simple_mouse_keyboard,blender_like_keyboard,keyboard}.rs` — draft structs and conversion helpers, feature-gated as needed.
- `crates/bevy_lagrange/src/input/bindings/{builder,descriptor,mod}.rs` — custom binding export only if fully defined.
- `crates/bevy_lagrange/src/input/mod.rs` — feature-gated exports for new draft types.
- `crates/bevy_lagrange/src/lib.rs` — feature-gated public exports and type registration in plugin setup.
- `crates/bevy_lagrange/Cargo.toml` — feature coverage if new cfg wiring is needed.

**Constraints from prior phases:** Phase 1 established the bridge API shape: labels come from `preset.kind().name()`, lowering borrows through `preset.to_bindings()`, and new code should use constructor/helper form instead of direct unit variants. Phase 5 introduced payload presets and validate-before-replace runtime lowering. Reuse the same fallible preset validation/lowering entrypoint for reflected apply.

**Acceptance gate:** With default features, tests prove reflected input-mode descriptors can apply a tuned Blender-like preset without falling back to `Bindings`, reflected preset drafts can construct tuned presets without Rust fluent setters, invalid reflected preset drafts leave previous mode/resolved bindings/installation/action children unchanged, tuned `BlenderLikeKeyboard` draft apply/export preserves pointer sensitivity and slow-mode fields, registered type lookup finds every new draft/runtime type, and direct mode changes/preset cycling do not leave stale descriptors that reapply unexpectedly. `cargo check -p bevy_lagrange --all-targets --no-default-features` still passes for runtime preset APIs.

### Phase 7 — Preserve tuned presets in Fairy Dust panels and examples  · status: todo

#### Work Order

**Goal:** Update Fairy Dust and user-facing examples so tuned presets display as built-in presets and do not reset until an explicit preset change.

**Spec:**
Fairy Dust builder overloads, snapshot labels, explicit guidance helpers, and preset cycling must preserve payload presets. Phase 1 already changed builder helpers such as `with_orbit_cam_preset` and related bundle overloads to accept `impl Into<OrbitCamPreset>` and insert via `OrbitCamInputMode::with_preset`; keep that bridge behavior and focus this phase on payload preservation, tuned preset display, tests, and public examples/docs.

Use `preset.kind().name()` only for identity labels, comparisons, and cycle targets. Use `preset.to_bindings()` on the full preset value for rows, slow-mode hints, summaries, and guidance snapshots so tuned payload values are not reset to defaults.

Preset cycling is by preset kind, not by full preset value. Hiding or showing the panel must not reset a tuned preset. An explicit cycle to another preset intentionally constructs the target preset's default settings.

Slow-mode UI should follow effective enabled controls. Prefer defining effective slow mode as "slow mode plus at least one enabled contribution it can scale"; then hide the slow-mode row and clear active slow-mode display state when all slow-scaled controls are disabled. If the implementation intentionally keeps slow mode visible independently, document that behavior and test it.

Update `input_preset_blender_like.rs` as the primary user-facing example. It should tune Blender-like mouse and smooth-scroll sensitivities while keeping the existing face-panel/control-panel scaffolding. Put named sensitivity constants near `spawn_camera`, make the module doc comment name the tuned mouse and smooth-scroll values, and use only the tuned-preset helper in `spawn_camera`. Update module docs and section header to say the example attaches a tuned Blender-like preset with `OrbitCam::with_preset`; avoid stale prose that says it attaches `OrbitCam::blender_like` or raw `OrbitCamInputMode::Preset` rows. Keep control summaries labeled as `Preset / BlenderLike`.

Leave `input_custom.rs` focused on app-owned bindings. It should include one readable `.with_sensitivity(...)` binding example, but not become a sensitivity reference. User-facing prose there should call Bevy pixel-scroll input "smooth scroll" even if low-level type names still say `OrbitCamTrackpadScroll`.

Re-run the Phase 1 constructor/helper migration as a regression check, then update examples and docs only where payload-carrying presets change behavior or prose. Search for stale `OrbitCamPreset::BlenderLike`-style unit construction, tuned-example prose that still says `OrbitCam::blender_like`, and user-facing `trackpad` wording where the public concept should be `smooth scroll`.

**Files:**
- `crates/fairy_dust/src/builder/{sprinkle,primitive,studio_lighting,title_bar,camera_home}.rs` — preserve existing `impl Into<OrbitCamPreset>` bridge behavior through payload-carrying presets.
- `crates/fairy_dust/src/camera_control_panel/{snapshot,guidance,preset_switch,display}.rs` — payload-preserving labels, slow-mode hints, rows, kind-based cycling, tuned preservation tests.
- `crates/fairy_dust/src/orbit_cam.rs` — camera marker/installation helper adjustments if builder paths require them.
- `crates/bevy_lagrange/examples/input_preset_blender_like.rs` — primary tuned preset example.
- `crates/bevy_lagrange/examples/input_custom.rs` — one custom binding sensitivity example and public wording cleanup.
- `crates/bevy_lagrange/examples/{input_gamepad,input_keyboard,basic,animation,programmatic_control,zoom_to_fit,render_to_texture,swapped_axis,showcase/main,follow_target,focus_bounds,orthographic,pausing,viewports_windows}.rs` — regression-check constructor/helper form and update payload-related prose only where needed.
- `crates/bevy_diegetic/examples/` and `crates/bevy_liminal/examples/` — regression-check public example call sites that used unit preset variants during Phase 1.
- `crates/bevy_lagrange/README.md` and `crates/fairy_dust/README.md` — update only if they contain stale preset construction or public wording affected by payload presets.
- `docs/bevy_lagrange/as-built/`, `docs/fairy_dust/`, and `docs/bevy_diegetic/` — update only snippets affected by payload presets or stale public wording.

**Constraints from prior phases:** Phase 1 changed Fairy Dust preset cycling to compare by `preset.kind()` and construct fresh target presets through helpers; do not depend on `OrbitCamPreset: Copy + Eq` for identity. Phase 5 introduced payload presets and source setters. Phase 6 defined reflected descriptor ownership; Fairy Dust direct mode changes or preset cycling must not cause stale reflected descriptors to reapply unexpectedly.

**Acceptance gate:** Fairy Dust tests prove a tuned Blender-like preset inserted through builder helpers displays `Preset / BlenderLike`, preserves tuning and slow-mode hint through hide/show, and only resets to a default target preset on explicit cycle. `input_preset_blender_like.rs` compile-checks with tuned `OrbitCam::with_preset(preset)`. `input_custom.rs` remains `Bindings / Custom`. `rg` checks over `crates` and `docs` find no stale unit-variant construction outside constructors/tests and no stale tuned-example prose saying `OrbitCam::blender_like`.

### Phase 8 — Close out lifecycle, animation, and full verification  · status: todo

#### Work Order

**Goal:** Prove disabled sensitivity, lifecycle state, animation interruption, feature gates, and the full workspace are correct after the migration.

**Spec:**
Mode changes that disable active sources must flush debounced lifecycle state. When reconciliation removes or disables a source, old `OrbitCamInteractionState` reports, settle deadlines, latches, panel highlights, and animation-interrupt state clear on the next frame rather than waiting for the normal debounce window.

Zero-sensitive input must not cancel, complete, or ignore-clear an active animation as user input. The same source with nonzero sensitivity still interrupts according to `CameraInputInterruptBehavior`.

Controller tests only need to confirm global `OrbitCam` sensitivity remains a final multiplier. Device-specific tests should stay at the input boundary and not move calibration logic into camera math.

Run the migration closeout checks at each major boundary if any previous phase left them incomplete:

- runtime sensitivity storage;
- preset kind/payload API and internal call-site migration;
- Fairy Dust, examples, and docs migration;
- reflected drafts/export/apply/type registration;
- default-feature and no-default-features validation.

Final verification commands:

```text
cargo +nightly fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --all-features --workspace --tests
cargo check --workspace --examples --all-features
cargo check -p bevy_lagrange --all-targets --no-default-features
taplo fmt --check
cargo mend --fail-on-warn
```

Use `cargo +nightly fmt` in this repo, not plain `cargo fmt`. Use `cargo nextest run` for tests. Do not clear `RUSTC_WRAPPER`.

Phase 1 and Phase 2 could not run `cargo mend --fail-on-warn` because the configured `kache` wrapper rejected the `cargo-mend` binary path. Treat that as a validation dependency to resolve or rerun when fixed; do not work around it by clearing `RUSTC_WRAPPER`.

**Files:**
- `crates/bevy_lagrange/src/input/lifecycle.rs` — debounce flush/state clearing tests.
- `crates/bevy_lagrange/src/animation.rs` — animation interruption tests for zero and nonzero sensitivity.
- `crates/bevy_lagrange/src/orbit_cam/mod.rs` and `crates/bevy_lagrange/src/orbit_cam/controller.rs` — only if existing global sensitivity tests need final-master-gain assertions.
- `crates/bevy_lagrange/src/input/{adapter/mod,control_summary,modes}.rs` — final integration tests for effective enabled controls, mode reconciliation, and reflected apply.
- `crates/fairy_dust/src/camera_control_panel/{snapshot,preset_switch}.rs` — final panel highlight/slow-mode/cycling assertions if not already covered.
- `Cargo.toml`, `crates/bevy_lagrange/Cargo.toml`, `taplo.toml`, `rustfmt.toml` — verification/config inputs only if failures require narrow fixes.

**Constraints from prior phases:** Phase 1 bridge APIs (`OrbitCamPresetKind`, constructor helpers, borrowed `name`/`to_bindings`, `OrbitCamInputMode::with_preset`, `OrbitCam::with_preset`) are the stable public path and public examples/docs should keep constructor/helper form. All implementation phases must leave default behavior unchanged when sensitivity is `1.0`; zero sensitivity disables runtime participation but authored payload remains round-trippable; custom bindings label as custom; tuned presets label by kind and lower from full payload; reflected and runtime preset replacement validate before destroying working installations.

**Acceptance gate:** Added tests cover active source -> apply zero-sensitive tuned preset -> next frame has no interaction state, no panel highlight, no latch, and no animation interrupt; nonzero tuned input still interrupts according to `CameraInputInterruptBehavior`; default bindings produce unchanged values; invalid `NaN`, infinite, or negative sensitivity is rejected; every final verification command listed in the Spec passes, including `cargo mend --fail-on-warn` once the `kache`/`cargo-mend` wrapper-path issue is resolved without clearing `RUSTC_WRAPPER`.
