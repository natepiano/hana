# OrbitCam Preset API

> **Status: IMPLEMENTATION PLAN — phased, delegate-ready.** Replaces the layer/profile model with concrete preset config structs, `OrbitCam::*` spawn helpers, and CapsLock slow mode for BlenderLike presets.

## Delegation Context

- **Project:** bevy_lagrange — Bevy orbit camera with pan, orbit, zoom-to-fit, queued animations, and trackpad support
- **Stack:** Rust, Bevy 0.19.0-rc.2, bevy_enhanced_input 0.26.0-rc.1
- **Layout:**
  - `crates/bevy_lagrange/src/input/bindings/` — validated binding specs, preset configs, builder, descriptors, validation, error types
  - `crates/bevy_lagrange/src/input/bindings/preset/` — module directory (Phase 1); `mod.rs`, `config.rs`, `enum_preset.rs`, `simple_mouse.rs`, `blender_like.rs`, `keyboard.rs`, `simple_mouse_keyboard.rs`, `blender_like_keyboard.rs`, `gamepad.rs`
  - `crates/bevy_lagrange/src/input/bindings/descriptor.rs` — binding descriptor types; `InputDeadZone` pattern is the model for `OrbitCamScalePolicy` (Phase 3)
  - `crates/bevy_lagrange/src/input/bindings/builder.rs` — `OrbitCamBindingsDescriptor` + `OrbitCamBindingsBuilder`; `profile` field removed in Phase 2
  - `crates/bevy_lagrange/src/input/bindings/validate.rs` — `validate_bindings`; scale/deadzone validation pattern
  - `crates/bevy_lagrange/src/input/bindings/error.rs` — `OrbitCamBindingsError` enum; used for preset validation errors
  - `crates/bevy_lagrange/src/input/bindings/mod.rs` — module exports; `OrbitCamBindings` struct; profile field removed in Phase 2
  - `crates/bevy_lagrange/src/input/routing/latches.rs` — `CameraInputSourceLatches`, `recover_unavailable_latches`, `clear_latches_on_mode_replaced`; extended for slow-mode latches in Phase 3
  - `crates/bevy_lagrange/src/input/routing/mod.rs` — routing dispatcher
  - `crates/bevy_lagrange/src/input/adapter/mod.rs` — input adapter; extended for slow-mode scaling in Phase 3
  - `crates/bevy_lagrange/src/input/mod.rs` — input subsystem root; export cleanup in Phase 2
  - `crates/bevy_lagrange/src/input/modes.rs` — `OrbitCamInputMode` and `OrbitCamInputModeDraft` enums
  - `crates/bevy_lagrange/src/orbit_cam/mod.rs` — `OrbitCam` component; re-exports from `preset_helpers` in Phase 4
  - `crates/bevy_lagrange/src/lib.rs` — crate root exports; profile/layer removals in Phase 2, preset config additions in Phase 4
  - `crates/bevy_lagrange/examples/input_preset_simple.rs` — updated in Phase 5
  - `crates/bevy_lagrange/examples/input_preset_blender_like.rs` — updated in Phase 5
  - `crates/bevy_lagrange/examples/input_gamepad.rs` — updated in Phase 5
  - `crates/bevy_lagrange/examples/input_keyboard.rs` — updated in Phase 5
  - `crates/bevy_lagrange/examples/input_manual.rs` — updated in Phase 5
- **Key files:**
  - `crates/bevy_lagrange/src/input/bindings/preset/enum_preset.rs` — `OrbitCamPreset` enum + `to_bindings()`; legacy types `OrbitCamBindingsProfile`, `OrbitCamPresetLayer`, `PresetLayerSet`, `OrbitCamPresetLayers` (all deleted in Phase 2)
  - `crates/bevy_lagrange/src/input/bindings/preset/blender_like.rs` — `OrbitCamBlenderLikePreset` with `zoom_mod_keys`, `slow_toggle_key`, `slow_scale` fields; validation in `build_into`
  - `crates/bevy_lagrange/src/input/bindings/preset/gamepad.rs` — `OrbitCamGamepadPreset` + `OrbitCamGamepadPresetBuilder`; scale/dead-zone validation
  - `crates/bevy_lagrange/src/input/bindings/preset/config.rs` — `pub(super) trait OrbitCamPresetConfig: Sized`
  - `crates/bevy_lagrange/src/input/bindings/descriptor.rs` — lines 24–32: `HeldBindingDescriptor`; lines 36–42: `ActionBindingDescriptor`; lines 137–190: `InputBindingModifiers`; lines 214–230: `InputDeadZone`
  - `crates/bevy_lagrange/src/input/bindings/builder.rs` — lines 36–53: `OrbitCamBindingsDescriptor` with `profile` field; line 147: `profile()` method
  - `crates/bevy_lagrange/src/input/bindings/validate.rs` — lines 44–83: `validate_bindings`
  - `crates/bevy_lagrange/src/input/bindings/error.rs` — all lines; `OrbitCamBindingsError` with `InvalidScale` (line 39), `InvalidDeadZone` (line 41)
  - `crates/bevy_lagrange/src/input/bindings/mod.rs` — lines 80–98: `OrbitCamBindings` struct; line 97: `profile` field; lines 166–169: `profile()` getter; lines 71–77: layer/profile exports
  - `crates/bevy_lagrange/src/input/routing/latches.rs` — lines 25–88: `CameraInputSourceLatches`; lines 67–83: `recover_unavailable_latches`; lines 90–95: `clear_latches_on_mode_replaced`
  - `crates/bevy_lagrange/src/input/modes.rs` — lines 43–51: `OrbitCamInputMode` `#[non_exhaustive]`; lines 84–90: `OrbitCamInputModeDraft` `#[non_exhaustive]`
  - `crates/bevy_lagrange/src/lib.rs` — line 87: `OrbitCamBindingsProfile`; lines 92–93: gamepad preset exports; lines 133–135: `OrbitCamPreset`, `OrbitCamPresetLayer`, `OrbitCamPresetLayers`; line 148: `PresetLayerSet`
- **Build:** `cargo build -p bevy_lagrange`
- **Test:** `cargo nextest run -p bevy_lagrange`
- **Lint:** `cargo clippy -p bevy_lagrange && cargo +nightly fmt`
- **Style:** `zsh ~/.claude/scripts/rust_style/load-rust-style.sh --project-root /Users/natemccoy/rust/bevy_lagrange_presets`
- **Invariants:**
  - `OrbitCamInputMode` and `OrbitCamPreset` remain `#[non_exhaustive]`
  - `OrbitCamInputModeDraft` remains `#[non_exhaustive]`
  - All binding validation happens before `build()` returns `Ok`
  - Preset configs validate preset-specific invariants before descriptor construction — before any builder mutation
  - `OrbitCamScalePolicy` is a plain struct (mirrors `InputDeadZone`); validation at preset `build()` time, not at struct construction
  - Slow-mode latch is per-camera entity (mirrors `CameraInputSourceLatches` pattern in `routing/latches.rs`)
  - `clear_latches_on_mode_replaced` and `recover_unavailable_latches` must cover slow-mode latches
  - No public examples show exhaustive match over `#[non_exhaustive]` enums without a wildcard arm
  - `OrbitCamPresetBundle` does not exist — helpers return `impl Bundle`

## Phases

### Phase 1 — Preset module restructure + concrete preset config structs  · status: done

#### Work Order

**Goal:** Replace `preset.rs` with a `preset/` module directory and implement all concrete preset config structs with fluent setters, validation, and `OrbitCamPreset::to_bindings()` delegation. Slow-mode runtime behavior is excluded (Phase 3); `OrbitCamBlenderLikePreset` gets the `slow_scale` field and build-time validation only.

**Spec:**

Module structure — replace `crates/bevy_lagrange/src/input/bindings/preset.rs` with:

```
input/bindings/preset/
  mod.rs                     — re-export public preset API
  config.rs                  — sealed OrbitCamPresetConfig trait
  enum_preset.rs             — OrbitCamPreset enum + to_bindings()
  simple_mouse.rs            — OrbitCamSimpleMousePreset
  blender_like.rs            — OrbitCamBlenderLikePreset
  keyboard.rs                — OrbitCamKeyboardPreset
  simple_mouse_keyboard.rs   — OrbitCamSimpleMouseKeyboardPreset
  blender_like_keyboard.rs   — OrbitCamBlenderLikeKeyboardPreset
  gamepad.rs                 — OrbitCamGamepadPreset
```

Sealed trait (crate-private, in `preset/config.rs`):

```rust
trait OrbitCamPresetConfig: Sized {
    fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError>;
}
```

`Sized` bound is correct — trait is not intended for `dyn`; each concrete preset is consumed by value. Each concrete preset also exposes an inherent `build()` delegating to the trait so users never import the trait:

```rust
impl OrbitCamBlenderLikePreset {
    pub fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        <Self as OrbitCamPresetConfig>::build(self)
    }
}
```

Private `build_into` for composed preset reuse:

```rust
fn build_into(self, builder: OrbitCamBindingsBuilder) -> Result<OrbitCamBindingsBuilder, OrbitCamBindingsError>
```

Child configs validate their own invariants inside `build_into` BEFORE mutating the builder. If validation fails, return the error before any builder mutation (no partial state).

`OrbitCamPreset::to_bindings()` delegation (verbatim):

```rust
impl OrbitCamPreset {
    pub fn to_bindings(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        match self {
            Self::SimpleMouse => OrbitCamSimpleMousePreset::default().build(),
            Self::BlenderLike => OrbitCamBlenderLikePreset::default().build(),
            Self::Keyboard => OrbitCamKeyboardPreset::default().build(),
            Self::SimpleMouseKeyboard => OrbitCamSimpleMouseKeyboardPreset::default().build(),
            Self::BlenderLikeKeyboard => OrbitCamBlenderLikeKeyboardPreset::default().build(),
            Self::Gamepad => OrbitCamGamepadPreset::default().build(),
            _ => unreachable!(),
        }
    }
}
```

`OrbitCamBlenderLikePreset` struct and defaults:

```rust
pub struct OrbitCamBlenderLikePreset {
    zoom_mod_keys: ModKeys,
    slow_toggle_key: Option<KeyCode>,
    slow_scale: f32,
}

impl Default for OrbitCamBlenderLikePreset {
    fn default() -> Self {
        Self {
            zoom_mod_keys: ModKeys::CONTROL,
            slow_toggle_key: Some(KeyCode::CapsLock),
            slow_scale: 0.15,
        }
    }
}
```

Fluent setters on all preset configs consume and return `self`: `.zoom_mod_keys(ModKeys)`, `.slow_toggle_key(Option<KeyCode>)`, `.slow_scale(f32)`.

Validation invariants (enforce in each `build()` before descriptor construction; use existing `OrbitCamBindingsError` variants — no new error types for v0):

| Config | Invariants |
| --- | --- |
| `OrbitCamBlenderLikePreset` | `slow_scale` finite, > 0.0, ≤ 1.0 when slow mode enabled (`slow_toggle_key.is_some()`) |
| `OrbitCamGamepadPreset` | fast and slow orbit/pan/zoom scales finite and non-negative |
| `OrbitCamGamepadPreset` | slow scale ≤ corresponding fast scale |
| `OrbitCamGamepadPreset` | stick dead-zone lower and upper bounds finite, ordered, inside accepted input range |
| Composed presets | child validation runs inside `build_into` before builder mutation |

`OrbitCamBlenderLikeKeyboardPreset` composition:

```rust
pub struct OrbitCamBlenderLikeKeyboardPreset {
    pointer:  OrbitCamBlenderLikePreset,
    keyboard: OrbitCamKeyboardPreset,
}
```

Child-config setters replace the field entirely (no merge):

```rust
pub fn blender_like(mut self, preset: OrbitCamBlenderLikePreset) -> Self {
    self.pointer = preset; self
}
pub fn keyboard(mut self, preset: OrbitCamKeyboardPreset) -> Self {
    self.keyboard = preset; self
}
```

`OrbitCamBlenderLikeKeyboardPreset` inherits BlenderLike slow mode from its `pointer` child by default.

`OrbitCamPresetLayers` and `OrbitCamBindingsProfile` still exist in this phase but are no longer called by `to_bindings()`. They are deleted in Phase 2.

**Files:**
- `crates/bevy_lagrange/src/input/bindings/preset.rs` — delete; replaced by directory
- `crates/bevy_lagrange/src/input/bindings/preset/mod.rs` — new
- `crates/bevy_lagrange/src/input/bindings/preset/config.rs` — new; sealed trait
- `crates/bevy_lagrange/src/input/bindings/preset/enum_preset.rs` — new; `OrbitCamPreset` + `to_bindings()`
- `crates/bevy_lagrange/src/input/bindings/preset/simple_mouse.rs` — new
- `crates/bevy_lagrange/src/input/bindings/preset/blender_like.rs` — new
- `crates/bevy_lagrange/src/input/bindings/preset/keyboard.rs` — new
- `crates/bevy_lagrange/src/input/bindings/preset/simple_mouse_keyboard.rs` — new
- `crates/bevy_lagrange/src/input/bindings/preset/blender_like_keyboard.rs` — new
- `crates/bevy_lagrange/src/input/bindings/preset/gamepad.rs` — new
- `crates/bevy_lagrange/src/input/bindings/mod.rs` — update preset module path

**Constraints from prior phases:** None (first phase).

**Acceptance gate:** `cargo nextest run -p bevy_lagrange` green; `OrbitCamBlenderLikePreset::default().slow_scale(0.25).build()` returns `Ok`; `OrbitCamBlenderLikePreset::default().slow_scale(2.0).build()` returns `Err`; `OrbitCamGamepadPreset` slow > fast returns `Err`; `OrbitCamPreset::BlenderLike.to_bindings()` delegates to `OrbitCamBlenderLikePreset::default().build()`.

#### Retrospective

**What worked:** Module split clean; all 109 tests pass including 3 new acceptance gate tests; validation-before-mutation invariant held throughout; `OrbitCamPresetLayers::build_with_profile` rewritten to delegate to concrete preset `build_into` methods (behavioral equivalence verified by existing `composed_presets_use_layer_builder` test).

**What deviated from the plan:**
- `_ => unreachable!()` removed from `to_bindings()` — `#[non_exhaustive]` inside the defining crate makes the wildcard an unreachable-pattern warning; the spec included it for external-crate matches but that context is wrong here.
- New preset config types (`OrbitCamBlenderLikePreset`, `OrbitCamKeyboardPreset`, etc.) exported from `bindings/mod.rs`, `input/mod.rs`, and `lib.rs` in this phase — Phase 4 listed adding these to `lib.rs` as its work, but codex correctly added them here since they were already public-facing types.

**Surprises:**
- `OrbitCamSimpleMousePreset` and `OrbitCamKeyboardPreset` are unit structs (no fields) — composed preset setters (`simple_mouse()`, `keyboard()`) replace the whole child field rather than mutating it, which is consistent with the spec but worth noting for Phase 2's deletion scope.
- `OrbitCamPresetLayers::build_with_profile` (still alive, in `enum_preset.rs`) now delegates to concrete preset `build_into` methods; Phase 2 deletes it along with the rest of the layer types.

**Implications for remaining phases:**
- Phase 2: `OrbitCamPresetLayers` is in `preset/enum_preset.rs` (not a separate file); delete the whole struct and its methods from that file. Also delete `OrbitCamPresetLayer`, `PresetLayerSet`, and `OrbitCamBindingsProfile` from `enum_preset.rs`. The `composed_presets_use_layer_builder` test in `bindings/mod.rs` exercises `OrbitCamPresetLayers` directly — remove it in Phase 2.
- Phase 4: Preset config type exports to `lib.rs` (the table in Phase 4's Spec) are already present — Phase 4 only needs to add `OrbitCam::*` spawn helpers in `orbit_cam/preset_helpers.rs`.

### Phase 1 Review

- **Phase 2 Work Order updated:** Added 5 preset files (`blender_like.rs`, `simple_mouse.rs`, `keyboard.rs`, `simple_mouse_keyboard.rs`, `gamepad.rs`) to Files — all call `.profile()` and must be updated before `OrbitCamBindingsProfile` is deleted. Added `.profile()` removal ordering guidance, named both affected tests explicitly (`composed_presets_use_layer_builder`, `presets_validate_through_shared_path`), added constraint that `_ => unreachable!()` arm is absent from `to_bindings()`, and tightened acceptance gate greps.
- **Phase 3 Work Order updated:** Added `OrbitCamBindings.slow_mode: Option<OrbitCamSlowMode>` field spec (completing the implicit data-flow path from `build()` to adapter); added `bindings/mod.rs`, `builder.rs`, `validate.rs`, `routing/mod.rs` to Files; updated acceptance gate criterion 1 to be unit-testable against `OrbitCamBindings.slow_mode`.
- **Phase 4 Work Order updated:** Removed the 6-type export table (all six types already exported in Phase 1); removed `lib.rs` from Files.
- **Phase 5 Work Order updated:** Replaced ambiguous gamepad/keyboard helper mention with explicit tuple path guidance; removed the `input_keyboard.rs` "custom bindings" stale-text task (text not present in the file).
- **Phase 5 Work Order updated (user decision):** `input_preset_simple.rs` drops all scaffolding (`apply_example_orbit_cam_limits`, `FairyDustOrbitCam`, `SimpleMouseCamera`, explicit field overrides) — it is the canonical one-liner demo, showing exactly what using a preset looks like and nothing else.

---

### Phase 2 — Profile and layer plumbing removal  · status: done

#### Work Order

**Goal:** Delete `OrbitCamBindingsProfile`, `OrbitCamPresetLayers`, `OrbitCamPresetLayer`, `PresetLayerSet`, and the `profile` field from all binding types. Label model is now `OrbitCamInputMode` only.

**Spec:**

Remove `profile` from bindings:
- `OrbitCamBindings` (`bindings/mod.rs` line 97): remove `profile` field and `profile()` getter (lines 166–169)
- `OrbitCamBindingsDescriptor` (`builder.rs` lines 36–53): remove `profile` field
- `OrbitCamBindingsBuilder` (`builder.rs` line 147): remove `profile()` method
- `validate_bindings` (`validate.rs` lines 44–83): remove profile copy-through

Remove from exports:
- `input/bindings/mod.rs` lines 71–77: remove `OrbitCamBindingsProfile`, `OrbitCamPresetLayer`, `OrbitCamPresetLayers`, `PresetLayerSet`
- `input/mod.rs`: remove same
- `lib.rs` line 87: remove `OrbitCamBindingsProfile`; lines 133–135: remove `OrbitCamPresetLayer`, `OrbitCamPresetLayers`; line 148: remove `PresetLayerSet`

Delete dead types from `preset/enum_preset.rs`:
- `OrbitCamPresetLayers`, `OrbitCamPresetLayer`, `PresetLayerSet`, `OrbitCamBindingsProfile` — Phase 1 rewrote `to_bindings()` to not use them; delete entirely from that file.

Remove `.profile()` calls from all six concrete preset files — do this BEFORE removing `OrbitCamBindingsProfile` from the type system, otherwise all six files fail to compile simultaneously:
- `preset/blender_like.rs` (lines 103–108): remove `.profile(OrbitCamBindingsProfile::LayeredPreset { layers: PresetLayerSet::blender_like() })` call
- `preset/simple_mouse.rs`: remove `.profile(...)` call
- `preset/keyboard.rs`: remove `.profile(...)` call
- `preset/simple_mouse_keyboard.rs`: remove `.profile(...)` call
- `preset/blender_like_keyboard.rs`: remove `.profile(...)` call
- `preset/gamepad.rs`: remove `.profile(...)` call

Update `describe_orbit_cam_controls` so labels derive only from `OrbitCamInputMode`:
- `Preset(preset)` → preset input
- `Bindings(bindings)` → custom bindings
- `Manual` → manual input
- Tuned preset bindings (from e.g. `OrbitCamBlenderLikePreset::build()`) become `Bindings` and display as custom bindings

Update `OrbitCamInputModeDraft::Bindings(OrbitCamBindingsDescriptor)`: remove profile/layer fields from descriptor reflection. Reflected mode drafts remain three concepts: preset, bindings, manual.

Remove profile and layer assertions from tests. Named tests to update or remove:
- `composed_presets_use_layer_builder` in `bindings/mod.rs` — exercises `OrbitCamPresetLayers` directly; remove the entire test
- `presets_validate_through_shared_path` in `bindings/mod.rs` (lines 228–239) — asserts `.profile()` returns `OrbitCamBindingsProfile::KeyboardPreset` and `OrbitCamBindingsProfile::GamepadPreset`; remove those profile assertions

**Files:**
- `crates/bevy_lagrange/src/input/bindings/mod.rs` — remove `profile` field, `profile()` getter, layer/profile exports
- `crates/bevy_lagrange/src/input/bindings/builder.rs` — remove `profile` field from `OrbitCamBindingsDescriptor`; remove `profile()` from builder
- `crates/bevy_lagrange/src/input/bindings/validate.rs` — remove profile copy-through
- `crates/bevy_lagrange/src/input/bindings/preset/enum_preset.rs` — delete `OrbitCamPresetLayers`, `OrbitCamPresetLayer`, `PresetLayerSet`, `OrbitCamBindingsProfile`
- `crates/bevy_lagrange/src/input/bindings/preset/blender_like.rs` — remove `.profile()` call from `OrbitCamPresetConfig::build`
- `crates/bevy_lagrange/src/input/bindings/preset/simple_mouse.rs` — remove `.profile()` call
- `crates/bevy_lagrange/src/input/bindings/preset/keyboard.rs` — remove `.profile()` call
- `crates/bevy_lagrange/src/input/bindings/preset/simple_mouse_keyboard.rs` — remove `.profile()` call
- `crates/bevy_lagrange/src/input/bindings/preset/blender_like_keyboard.rs` — remove `.profile()` call
- `crates/bevy_lagrange/src/input/bindings/preset/gamepad.rs` — remove `.profile()` call
- `crates/bevy_lagrange/src/input/mod.rs` — remove layer/profile exports
- `crates/bevy_lagrange/src/lib.rs` — remove lines 87, 133–135, 148 (profile/layer exports)
- `crates/bevy_lagrange/src/input/describe.rs` (or equivalent) — update label derivation from `OrbitCamInputMode`
- Tests — remove profile/layer assertions

**Constraints from prior phases:**
- Phase 1 rewrote `OrbitCamPreset::to_bindings()` to delegate to concrete preset configs; `OrbitCamPresetLayers` is no longer called by `to_bindings()` and is safe to delete.
- `OrbitCamPresetLayers`, `OrbitCamPresetLayer`, `PresetLayerSet`, `OrbitCamBindingsProfile` all live in `preset/enum_preset.rs` — delete them all from that file.
- `OrbitCamPresetLayers::build_with_profile` still exists (delegates to concrete preset `build_into`); delete it with the rest.
- The `composed_presets_use_layer_builder` test in `crates/bevy_lagrange/src/input/bindings/mod.rs` exercises `OrbitCamPresetLayers` directly — remove it in this phase.
- Preset config type exports (`OrbitCamBlenderLikePreset`, `OrbitCamKeyboardPreset`, etc.) are already in `bindings/mod.rs`, `input/mod.rs`, and `lib.rs` from Phase 1; Phase 2 does not need to add them, only remove the profile/layer exports.
- `to_bindings()` in `enum_preset.rs` has NO `_ => unreachable!()` wildcard arm — Phase 1 correctly removed it (it is an unreachable-pattern warning inside the defining crate for `#[non_exhaustive]`). Do not add it back.

**Acceptance gate:** `cargo nextest run -p bevy_lagrange` green; `rg 'OrbitCamBindingsProfile' crates/bevy_lagrange/src/` returns nothing; `rg 'OrbitCamPresetLayers' crates/bevy_lagrange/src/` returns nothing; `rg '\.profile(' crates/bevy_lagrange/src/input/bindings/` returns nothing; `profile` field absent from `OrbitCamBindings`; `describe_orbit_cam_controls` test covers all three `OrbitCamInputMode` variants; `composed_presets_use_layer_builder` test absent from `bindings/mod.rs`.

#### Retrospective

**What worked:** All 4 dead types deleted cleanly; 109 tests pass; both reviewers APPROVE with no findings.

**What deviated from the plan:**
- Spec named `describe.rs (or equivalent)` — actual files are `control_summary.rs` (label logic) and `constants.rs` (label constant values); the "(or equivalent)" hedge was correct.
- `OrbitCamGamepadPresetBuilder.customized: bool` also removed — field was only used to populate `OrbitCamBindingsProfile::GamepadPreset { customized }`; dead once the profile type was deleted; private field so no public API change.
- `preset_mode_value()` helper deleted — was returning per-variant strings ("SimpleMouse", "BlenderLike", etc.); now all presets display uniformly as "preset input".

**Surprises:**
- `OrbitCamControlSummary.mode_value` is now purely variant-keyed, not preset-name-keyed — "preset input" for all `Preset(_)` variants regardless of which preset.

**Implications for remaining phases:**
- Phase 3: label system lives in `control_summary.rs` + `constants.rs` (not `describe.rs`); no Phase 3 work affects this.
- Phase 4: `lib.rs` profile/layer exports confirmed removed; Phase 4 only adds `preset_helpers.rs`.

### Phase 2 Review

- **Phase 3 Work Order updated:** Added explicit `slow_scale → OrbitCamScalePolicy` translation note; clarified variant-agnostic slow-mode applies to any `Bindings` mode; extended observer signature note (`ResMut<OrbitCamSlowModeLatches>` second parameter); added system-set (`OrbitCamInputInternalSet::Routing`) note to `routing/mod.rs` entry; added `input/mod.rs` and `lib.rs` to Files for re-exporting `OrbitCamSlowMode`/`OrbitCamScalePolicy`; clarified acceptance gate criterion 1 as a test to add. User decision: removed "embed in each descriptor type" instruction — `OrbitCamScalePolicy` lives only in `OrbitCamSlowMode`, adapter reads it uniformly (Option A over per-source embedding).
- **Phase 4 Work Order updated:** Fixed contradictory Constraints sentence (exports already present since Phase 1, Phase 4 does not touch `lib.rs`); updated acceptance gate to verify `preset_helpers.rs` exists rather than a vacuous re-export check.
- **Phase 5 Work Order updated:** Strengthened Phase 3 dependency note; added `input_custom.rs` compile-verify note; clarified `input_preset_simple.rs` Files entry as a full rewrite of the ~335-line file.

---

### Phase 3 — BlenderLike CapsLock slow mode  · status: done

#### Work Order

**Goal:** Add `OrbitCamScalePolicy`, the per-camera slow-mode latch, and BlenderLike CapsLock slow mode. All seven BlenderLike input sources apply slow scaling when the latch is active.

**Spec:**

New type `OrbitCamScalePolicy` — plain struct, validated at preset `build()` time (mirrors `InputDeadZone` at `descriptor.rs` lines 214–230; NOT a newtype; NOT validated at construction):

```rust
pub struct OrbitCamScalePolicy {
    pub normal: f32,
    pub slow: f32,
}
```

Home: `crates/bevy_lagrange/src/input/bindings/descriptor.rs` alongside `InputDeadZone`. `OrbitCamScalePolicy` is NOT embedded in individual descriptor types — it lives only in `OrbitCamSlowMode`. The adapter reads `bindings.slow_mode.as_ref().map(|s| &s.scale)` and applies it uniformly across all seven scaled sources:

| Source | Slow coverage |
| --- | --- |
| Mouse drag orbit | scaled |
| Trackpad orbit | scaled |
| Mouse drag pan | scaled |
| Trackpad pan | scaled |
| Wheel zoom | scaled |
| Trackpad zoom | scaled |
| Pinch zoom | scaled |

Adapter applies scale uniformly — single function, no per-source duplication:

```rust
fn apply_scale(value: f32, policy: &OrbitCamScalePolicy, slow_active: bool) -> f32 {
    value * if slow_active { policy.slow } else { policy.normal }
}
```

Slow-mode latch — per-camera entity, same pattern as `CameraInputSourceLatches` (`routing/latches.rs` lines 25–88):
- Add a parallel resource for slow-mode latches keyed by camera `Entity`
- Latch toggles on key EDGE (key-press event — not key-repeat, not OS CapsLock text state)
- Extend `recover_unavailable_latches` (lines 67–83) to clear slow latches for despawned cameras; add `mut slow_latches: ResMut<OrbitCamSlowModeLatches>` as a second parameter
- Extend `clear_latches_on_mode_replaced` observer (lines 90–95) to reset slow latch to `off` when a camera's mode/bindings no longer reference slow mode; extend observer signature by adding `mut slow_latches: ResMut<OrbitCamSlowModeLatches>` as a second parameter

Slow-mode descriptor emitted by `OrbitCamBlenderLikePreset::build()`:

```rust
pub struct OrbitCamSlowMode {
    pub toggle_key: KeyCode,
    pub scale: OrbitCamScalePolicy,
}
```

`OrbitCamBlenderLikePreset::build()` emits `OrbitCamSlowMode` when `slow_toggle_key.is_some()`. Explicit translation: `slow_scale: f32` becomes `OrbitCamScalePolicy { normal: 1.0, slow: self.slow_scale }` when constructing `OrbitCamSlowMode` inside `build_into`.

`OrbitCamBindings` and `OrbitCamBindingsDescriptor` each gain a `slow_mode: Option<OrbitCamSlowMode>` field. `OrbitCamBindingsBuilder` gains a `slow_mode(OrbitCamSlowMode) -> Self` method. Inside `OrbitCamBlenderLikePreset`'s `OrbitCamPresetConfig::build` impl, call `builder.slow_mode(OrbitCamSlowMode { toggle_key, scale })` before `.build()` when `slow_toggle_key.is_some()`. The adapter reads `OrbitCamBindings.slow_mode` to determine the toggle key and arm the latch system.

Slow mode is variant-agnostic: both `OrbitCamInputMode::Preset(BlenderLike)` and `OrbitCamInputMode::Bindings(built_from_blender_like)` produce bindings with `slow_mode.is_some()`. The adapter reads from `OrbitCamBindings.slow_mode` regardless of which mode variant is active — no branch on `OrbitCamInputMode`.

Slow mode applies ONLY to the camera whose input context receives the routed toggle.

`OrbitCamBlenderLikeKeyboardPreset` inherits slow mode from `pointer: OrbitCamBlenderLikePreset` automatically.

**Files:**
- `crates/bevy_lagrange/src/input/bindings/descriptor.rs` — add `OrbitCamScalePolicy` and `OrbitCamSlowMode` structs (no embedding in individual descriptor types)
- `crates/bevy_lagrange/src/input/bindings/mod.rs` — add `slow_mode: Option<OrbitCamSlowMode>` field to `OrbitCamBindings`; add `pub use descriptor::OrbitCamScalePolicy` and `pub use descriptor::OrbitCamSlowMode` following the `InputDeadZone` re-export pattern
- `crates/bevy_lagrange/src/input/mod.rs` — re-export `OrbitCamScalePolicy` and `OrbitCamSlowMode`
- `crates/bevy_lagrange/src/lib.rs` — re-export `OrbitCamScalePolicy` and `OrbitCamSlowMode`
- `crates/bevy_lagrange/src/input/bindings/builder.rs` — add `slow_mode: Option<OrbitCamSlowMode>` field to `OrbitCamBindingsDescriptor`; add `slow_mode(OrbitCamSlowMode) -> Self` builder method
- `crates/bevy_lagrange/src/input/bindings/validate.rs` — pass `slow_mode` through validation into `OrbitCamBindings`
- `crates/bevy_lagrange/src/input/bindings/preset/blender_like.rs` — update `OrbitCamPresetConfig::build` to call `builder.slow_mode(...)` when `slow_toggle_key.is_some()`
- `crates/bevy_lagrange/src/input/routing/latches.rs` — add slow-mode latch resource; extend `recover_unavailable_latches` and `clear_latches_on_mode_replaced`
- `crates/bevy_lagrange/src/input/routing/mod.rs` — register the new key-press edge system in `OrbitCamInputInternalSet::Routing` (bindings are installed by the `Installation` set, which runs before `Routing`)
- `crates/bevy_lagrange/src/input/adapter/mod.rs` — add `apply_scale`; apply slow scaling for all seven sources

**Constraints from prior phases:**
- Phase 1 added `OrbitCamBlenderLikePreset` with `slow_scale: f32` field, fluent setter, and validation. Phase 3 adds the descriptor plumbing from `build()` to the adapter.
- Phase 2 removed `OrbitCamBindingsProfile`; bindings no longer carry profile metadata.

**Acceptance gate:** `cargo nextest run -p bevy_lagrange` green including: (1) NEW TEST (add alongside the `slow_mode` field): `OrbitCamBlenderLikePreset::default().build()` returns `OrbitCamBindings` with `slow_mode.is_some()`; (2) latch toggles on CapsLock press edge, not repeat; (3) mode change resets slow latch to off; (4) camera despawn clears slow latch; (5) `slow_scale(2.0).build()` returns `Err`; (6) non-BlenderLike preset produces `OrbitCamBindings` with `slow_mode == None`.

#### Retrospective

**What worked:** `AdapterScale<'_>` wrapper cleanly unifies 7-source scaling — `Copy`, zero-cost, single borrow of the policy; 120 tests pass including all 6 acceptance criteria. Both reviewers APPROVE with no findings.

**What deviated from the plan:**
- `AdapterScale<'_>` wrapper struct added in `adapter/mod.rs` (not in spec) — internal only (`pub(super)`), avoids threading `policy + slow_active` as separate args through every inject/resolve function. `apply_scale` is still present with the required signature.
- `validate_scale_policy` validates `policy.slow > policy.normal → Err` (relative constraint) rather than a flat `slow ≤ 1.0`. Since `normal` is always 1.0 in practice, the behavior is identical — but a caller using the public builder with `normal=2.0, slow=1.5` would pass validation (valid relative slowdown). The preset-level `validate()` in `blender_like.rs` still enforces `slow_scale ≤ 1.0` as a hard cap.
- `OrbitCamScalePolicy` and `OrbitCamSlowMode` are not `Copy` — `Reflect` trait blanket prevents `Copy` on structs with non-Copy fields; codex removed `Copy` rather than adding an `allow`.

**Implications for remaining phases:**
- Phase 4: unaffected — spawn helpers (`preset_helpers.rs`) have no dependency on slow mode infrastructure.
- Phase 5: `input_preset_blender_like.rs` can now demonstrate CapsLock slow mode correctly; Phase 3's latch system is in place.

### Phase 3 Review

- **Phase 4 Work Order updated:** Fixed "re-exports the impl block" wording — `mod preset_helpers;` extends `OrbitCam` directly, no `pub use` needed. Added `OrbitCam` doc-comment update to Files. Added `OrbitCamBindings` not-`Copy` constraint. Tightened acceptance gate to include `OrbitCam::with_bindings(...)` compile + mode-variant check.
- **Phase 5 Work Order updated:** Clarified `input_preset_simple.rs` rewrite scope — strip everything including `fairy_dust::sprinkle_example()`, retain only `App::new() + DefaultPlugins + spawn_camera`. Clarified `input_preset_blender_like.rs` as a targeted update (retain face panels, update spawn only, add marker comment). Fixed `input_manual.rs` — has field overrides, must use explicit tuple path. Fixed `input_custom.rs` — do not change it, confirm coexistence. Added note that `input_gamepad.rs`/`input_keyboard.rs` may need zero changes. User decision (Finding 5): approved Option A for `input_preset_blender_like.rs` — retain face-panel showcase, update spawn to `OrbitCam::with_bindings(bindings)`, add comment on `BlenderLikeCamera` marker that it is only required by the fairy_dust example library.

---

### Phase 4 — OrbitCam spawn helpers + public exports  · status: todo

#### Work Order

**Goal:** Add `OrbitCam::blender_like()`, `OrbitCam::simple_mouse()`, and all other preset/bindings/manual helpers returning `impl Bundle`. Export concrete preset config types from the crate root.

**Spec:**

New file: `crates/bevy_lagrange/src/orbit_cam/preset_helpers.rs`

All helpers return `impl Bundle` — a `(OrbitCam, OrbitCamInputMode)` tuple. No named `OrbitCamPresetBundle` type exists or is exported:

```rust
impl OrbitCam {
    pub fn simple_mouse() -> impl Bundle {
        (OrbitCam::default(), OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouse))
    }
    pub fn blender_like() -> impl Bundle {
        (OrbitCam::default(), OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike))
    }
    pub fn gamepad() -> impl Bundle {
        (OrbitCam::default(), OrbitCamInputMode::Preset(OrbitCamPreset::Gamepad))
    }
    pub fn keyboard() -> impl Bundle {
        (OrbitCam::default(), OrbitCamInputMode::Preset(OrbitCamPreset::Keyboard))
    }
    pub fn simple_mouse_keyboard() -> impl Bundle {
        (OrbitCam::default(), OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouseKeyboard))
    }
    pub fn blender_like_keyboard() -> impl Bundle {
        (OrbitCam::default(), OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLikeKeyboard))
    }
    pub fn with_bindings(bindings: OrbitCamBindings) -> impl Bundle {
        (OrbitCam::default(), OrbitCamInputMode::Bindings(bindings))
    }
    pub fn manual() -> impl Bundle {
        (OrbitCam::default(), OrbitCamInputMode::Manual)
    }
}
```

Common spawn path:

```rust
commands.spawn((
    Transform::from_xyz(0.0, 1.5, 5.0),
    OrbitCam::blender_like(),
));
```

When `OrbitCam` fields need configuring, use the explicit tuple ("I know what I'm doing" path — no helper):

```rust
commands.spawn((
    Transform::from_xyz(0.0, 1.5, 5.0),
    OrbitCam { target_focus: Vec3::Y, target_radius: 8.0, ..default() },
    OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLike),
));
```

`orbit_cam/mod.rs` adds `mod preset_helpers;` — no explicit `pub use` is needed because the `impl OrbitCam` block in `preset_helpers.rs` extends the type directly and its methods are visible wherever `OrbitCam` is in scope.

Also update the `OrbitCam` component doc-comment in `orbit_cam/mod.rs` to replace any raw `OrbitCam::default()` spawn pattern with the new one-liner helper form (e.g. `OrbitCam::blender_like()`).

Public exports to `lib.rs`: all six preset config types were exported from `lib.rs` in Phase 1 and are already accessible at `bevy_lagrange::OrbitCamBlenderLikePreset` etc. Phase 4 has no `lib.rs` work.

**Files:**
- `crates/bevy_lagrange/src/orbit_cam/preset_helpers.rs` — new; all `OrbitCam::*` helpers
- `crates/bevy_lagrange/src/orbit_cam/mod.rs` — add `mod preset_helpers;`; update `OrbitCam` doc-comment to use helper form

**Constraints from prior phases:**
- Phase 1 added all concrete preset config types under `input/bindings/preset/`.
- Phase 2 cleaned up `lib.rs` profile/layer exports. All six preset config type exports were added to `lib.rs` in Phase 1 and are already present; Phase 4 does not touch `lib.rs`.
- Phase 3 added `OrbitCamBindings.slow_mode`; `OrbitCamBindings` is not `Copy` (Reflect prevents it). `OrbitCam::with_bindings(bindings)` must take ownership of `bindings`.
- `OrbitCamPreset`, `OrbitCamBindings`, `OrbitCamInputMode` all available in scope.

**Acceptance gate:** `cargo nextest run -p bevy_lagrange` green; `OrbitCam::blender_like()`, `OrbitCam::simple_mouse()`, `OrbitCam::with_bindings(bindings)`, and `OrbitCam::manual()` compile; `OrbitCam::with_bindings(OrbitCamPreset::SimpleMouse.to_bindings().unwrap())` compiles and produces `OrbitCamInputMode::Bindings(_)`; `crates/bevy_lagrange/src/orbit_cam/preset_helpers.rs` exists; `rg 'OrbitCamPresetBundle' crates/bevy_lagrange/src/lib.rs` returns nothing.

---

### Phase 5 — Example updates  · status: todo

#### Work Order

**Goal:** Update the six input examples to use the new preset API. `input_preset_simple.rs` becomes the canonical one-liner demo. `input_preset_blender_like.rs` becomes the primary tuned-preset demo.

**Spec:**

`input_preset_simple.rs` — canonical one-liner demo; drop all scaffolding (`apply_example_orbit_cam_limits`, `FairyDustOrbitCam`, `SimpleMouseCamera`, explicit `OrbitCam` field overrides). The example shows exactly what using a preset looks like and nothing else:

```rust
fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Transform::from_xyz(0.0, 1.5, 5.0),
        OrbitCam::simple_mouse(),
    ));
}
```

`input_preset_blender_like.rs` — primary example for preset input modification; retain the face-panel showcase; update the spawn only:
- Build from `OrbitCamBlenderLikePreset::default().slow_scale(0.25).build()?`
- Spawn with `OrbitCam::with_bindings(bindings)` (replaces the old `OrbitCamInputMode::Preset(PRESET)` spawn)
- Document that CapsLock toggles slow orbit, pan, and zoom
- Explain this is BlenderLike-derived even though it displays as custom bindings at runtime
- Include a disable-slow comment: `OrbitCamBlenderLikePreset::default().slow_toggle_key(None).build()?`
- Add a comment on `BlenderLikeCamera` (the face-panel marker component) that it is only required by the fairy_dust example library to drive the interactive cube faces; production code does not need it

`input_gamepad.rs` and `input_keyboard.rs` — both configure `OrbitCam` fields (radius, focus) and call `apply_example_orbit_cam_limits`; use the explicit tuple path: `(OrbitCam { ..default() }, OrbitCamInputMode::Preset(OrbitCamPreset::Gamepad))` / `Keyboard`. `OrbitCam::gamepad()` / `OrbitCam::keyboard()` helpers return `(OrbitCam::default(), ...)` and cannot be composed with field overrides.

`input_manual.rs` — has explicit `OrbitCam` field overrides (`focus`, `yaw`, `pitch`, `radius`) and a custom `CameraGuidance` component; `OrbitCam::manual()` cannot accommodate these. Use the explicit tuple path: `(OrbitCam { ..., ..default() }, OrbitCamInputMode::Manual, ...)`.

`input_custom.rs` — already uses `OrbitCamInputMode::Bindings(bindings.0.clone())` directly (the "I know what I'm doing" path) and has `OrbitCam` field overrides. Do NOT change it. Confirm it compiles unchanged after Phase 4 adds `OrbitCam::with_bindings()` — both forms coexist.

**Files:**
- `crates/bevy_lagrange/examples/input_preset_simple.rs` — full rewrite: strip everything including `fairy_dust::sprinkle_example()`, face panels, `apply_example_orbit_cam_limits`, `FairyDustOrbitCam`, `SimpleMouseCamera`; retain only `App::new().add_plugins(DefaultPlugins)` + `spawn_camera` system; the example is intentionally minimal — a blank orbitable scene is correct
- `crates/bevy_lagrange/examples/input_preset_blender_like.rs` — targeted update: retain face-panel showcase; replace spawn to use `OrbitCamBlenderLikePreset::default().slow_scale(0.25).build()?` + `OrbitCam::with_bindings(bindings)`; add marker comment; add CapsLock and disable-slow comments
- `crates/bevy_lagrange/examples/input_gamepad.rs` — already on the explicit tuple path; may need zero changes; verify it compiles cleanly after Phase 4
- `crates/bevy_lagrange/examples/input_keyboard.rs` — already on the explicit tuple path; may need zero changes; fix any stale text
- `crates/bevy_lagrange/examples/input_manual.rs` — update spawn to use explicit tuple path (already has field overrides, cannot use `OrbitCam::manual()`)

**Constraints from prior phases:**
- Phase 1 added `OrbitCamBlenderLikePreset` with `.slow_scale()` and `.slow_toggle_key()` setters.
- Phase 3 implemented CapsLock slow mode and must be fully tested before Phase 5 ships `input_preset_blender_like.rs` — the CapsLock demonstration is only functionally correct after Phase 3's latch system is in place.
- Phase 4 added `OrbitCam::simple_mouse()`, `OrbitCam::with_bindings()`, and all helpers.

**Acceptance gate:** `cargo nextest run -p bevy_lagrange` green; all five examples compile; `input_preset_simple.rs` contains `OrbitCam::simple_mouse()`; `input_preset_blender_like.rs` builds from `OrbitCamBlenderLikePreset::default()` and spawns via `OrbitCam::with_bindings()`.
