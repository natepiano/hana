# Camera Binding Preset Design

## Goal

Add camera binding presets that give developers solid starting points without
hiding Bevy Enhanced Input. The first target is a gamepad orbit-camera preset,
but the same descriptor work should also support precision camera controls and
layered keyboard bindings. The public Lagrange API should describe camera
intent and routing, then lower those descriptors into ordinary BEI binding
entities with BEI modifiers and conditions.

This document covers option 1: extend the existing `OrbitCamBindings` descriptor
model with the BEI-aligned customization needed for a useful gamepad preset.
It does not propose a fully arbitrary BEI escape hatch.

## Current Model

`OrbitCamBindings` currently maps physical input to camera intent:

- `orbit`: held `Vec2`
- `pan`: held `Vec2`
- `zoom_smooth`: held `f32`
- `zoom_coarse`: impulse `f32`

Held bindings are split into two parts:

- `motion`: input value that contributes to the action
- `engagement`: input value that decides whether the action is active

For example:

```rust
OrbitCamHeldBinding::same(right_stick)
```

means the right stick both provides orbit value and engages orbit.

```rust
OrbitCamHeldBinding::new(left_stick, GamepadButton::LeftTrigger)
```

means the left stick provides pan value only while the left shoulder button is
held.

Internally, Lagrange lowers each binding into BEI entities:

- motion binding entity, attached to `OrbitCamOrbitAction`, `OrbitCamPanAction`,
  or `OrbitCamZoomSmoothAction`
- engagement binding entity, attached to the matching internal engagement
  action

The resolver then reads BEI action values and writes `OrbitCamInput`.

## Problem

Gamepad sticks emit normalized values in roughly `-1.0..1.0`. `OrbitCam`
currently interprets orbit and pan input as pixel-like deltas. A full stick
deflection therefore acts like about one pixel of input per frame before the
controller divides by the viewport size.

The example can compensate by increasing `OrbitCam::orbit_sensitivity`, but
that is a global camera multiplier. It does not model the input source. Mouse
motion, keyboard keys, trackpad scroll, and gamepad sticks do not naturally use
the same units.

The current descriptor also cannot express the natural slow-mode behavior:

- right stick orbits quickly
- right bumper plus right stick orbits slowly
- left stick pans quickly
- left bumper plus left stick pans slowly
- triggers zoom quickly
- same-side shoulder plus trigger zooms slowly

If fast and slow bindings are both installed today, both can fire at the same
time and add together.

## Design Direction

Keep Lagrange as a descriptor layer over BEI:

```text
OrbitCam binding descriptor
  -> BEI action entities owned by the camera
  -> BEI binding entities with Binding, modifiers, and conditions
  -> OrbitCamInput
```

Lagrange should continue to own:

- semantic camera actions: orbit, pan, zoom
- camera input routing and source attribution
- install and teardown when `OrbitCamInputMode` changes
- control-summary labels
- reflected descriptor support where practical

BEI should continue to own:

- physical input bindings
- value transforms such as scale, dead zone, negate, and swizzle
- binding-level conditions where they fit the model

## Proposed Public Shape

Add a gamepad preset:

```rust
OrbitCamInputMode::Preset(OrbitCamPreset::Gamepad)
```

The default gamepad preset should map:

- right stick: orbit
- left stick: pan
- right trigger / left trigger: zoom
- right bumper plus right stick: slow orbit
- left bumper plus left stick: slow pan
- right bumper plus right trigger: slow zoom in
- left bumper plus left trigger: slow zoom out

The preset should also opt into active gamepad routing:

```rust
CameraInputGamepadSelectionPolicy::Active
```

This selects the BEI gamepad device policy. It does not by itself decide which
camera should receive no-position gamepad input. Examples and apps should pair
the preset with an explicit route or with a routing fallback such as
`NoPositionFallback::OnlyEligibleCamera` when there is one eligible camera.

Add customization through a preset builder rather than forcing users to copy the
whole mapping:

```rust
let bindings = OrbitCamGamepadPreset::default()
    .customize()
    .orbit_scale(360.0)
    .slow_orbit_scale(90.0)
    .pan_scale(240.0)
    .slow_pan_scale(70.0)
    .zoom_scale(8.0)
    .slow_zoom_scale(2.0)
    .build()?;
```

`OrbitCamPreset::Gamepad` remains the zero-config enum preset. Tuned gamepad
controls build `OrbitCamBindings`, not a payload-bearing `OrbitCamPreset`.
Those bindings should carry enough profile metadata for control summaries and
examples to keep treating them as gamepad controls rather than opaque custom
bindings.

Keyboard camera bindings should be a reusable preset layer, not a separate
player-control system. `Keyboard` should cover orbit, pan, and zoom camera
intent, for example arrow keys or WASD for orbit/pan and plus/minus for zoom.
It should not include gameplay actions such as jump, move, crouch, sprint, or
interact.

Because `OrbitCamBindings` already supports multiple bindings per camera action,
keyboard and pointer-style bindings can coexist. The API should support both
zero-config composed presets and explicit layering:

```rust
OrbitCamInputMode::Preset(OrbitCamPreset::Keyboard)
OrbitCamInputMode::Preset(OrbitCamPreset::BlenderLikeKeyboard)
OrbitCamInputMode::Preset(OrbitCamPreset::SimpleMouseKeyboard)
```

Existing presets keep their current meaning. `SimpleMouse` and `BlenderLike`
remain pointer-style base presets. New composed presets are aliases over named
layers:

| Preset | Layers | Intended use |
| --- | --- | --- |
| `SimpleMouse` | `SimpleMouse` | mouse-first orbit camera |
| `BlenderLike` | `BlenderLike` | editor-style pointer controls |
| `Keyboard` | `Keyboard` | keyboard-only camera controls |
| `SimpleMouseKeyboard` | `SimpleMouse`, `Keyboard` | mouse plus keyboard camera controls |
| `BlenderLikeKeyboard` | `BlenderLike`, `Keyboard` | editor-style pointer plus keyboard controls |
| `Gamepad` | `Gamepad` | gamepad camera controls |

Layer composition should be the canonical implementation path, not a separate
future system. Enum aliases should lower through the same builder:

```rust
let bindings = OrbitCamPresetLayers::new()
    .with_blender_like()
    .with_keyboard()
    .build()?;
```

The composed presets above are convenience aliases for common layers, not a
separate input model.

## BEI-Aligned Binding Entries

Replace the current single transform field:

```rust
InputBindingEntry {
    binding,
    transform: InputBindingTransform,
}
```

with descriptors that can lower to BEI components:

```rust
InputBindingEntry {
    binding,
    modifiers: InputBindingModifiers,
}

OrbitCamHeldBinding {
    motion,
    engagement,
    gates: BindingGates,
}
```

The descriptors should stay cloneable and reflectable. They should not store
BEI component values or BEI action `Entity` handles directly, because those are
runtime install products that become stale when an input mode is reinstalled.

The modifier descriptor should be a canonical struct, not a free list:

```rust
InputBindingModifiers {
    dead_zone: Option<InputDeadZone>,
    scale: Option<InputScale>,
    axis_transform: InputAxisTransform,
}
```

The first supported modifier descriptors should be Lagrange-owned closed types
that lower to BEI components where possible:

- `Negate`
- `SwizzleAxis`
- `Scale`
- `DeadZone`
- `DeltaScale` or an equivalent Lagrange rate descriptor for time-normalized
  held inputs

The first supported condition descriptors should be symbolic gates. They should
include both gamepad buttons and keyboard keys so precision controls can be
expressed for gamepad and Blender-like/keyboard layers:

```rust
OrbitCamBindingGate {
    input: OrbitCamGateInput,
    polarity: OrbitCamGatePolarity,
}

enum OrbitCamGateInput {
    GamepadButton(GamepadButton),
    Key(KeyCode),
}

enum OrbitCamGatePolarity {
    Required,
    Blocked,
}
```

Required buttons engage a binding only while the button is pressed. Blocked
buttons suppress a binding while the button is pressed. This lets fast and slow
bindings coexist without adding together:

```text
right stick fast orbit: blocked by right bumper
right bumper slow orbit: required by right bumper
```

The implementation should lower these symbolic descriptors during input
installation. Prefer BEI built-ins such as `Chord` and `BlockBy` where they fit.
Because those built-ins refer to action entities, Lagrange can spawn hidden
per-camera gate actions for the referenced buttons and then attach fresh BEI
conditions to the installed binding entities. Every helper action, helper
binding, and condition entity must carry `OrbitCamInputInstallationOf` and be
included in teardown.

During install, build a per-camera gate-action cache keyed by
`OrbitCamGateInput`. Spawn one hidden bool action and one binding per unique gate
input, reuse it for every binding that references the same gate, and include
all helper entities in the camera's installed entity list.

Required and blocked gates must be active for the full held duration, not only
for the press edge. If BEI built-ins such as `Chord` or `BlockBy` cannot prove
that behavior for held helper actions, Lagrange should use Lagrange-owned gate
conditions over the helper action state.

For held actions, conditions apply to both the motion descriptor and its
matching engagement descriptor by default. This keeps interaction lifecycle,
source attribution, and value contribution aligned. A blocked fast binding
should not report `has_orbit`, `has_pan`, or `has_zoom` simply because its
engagement binding is still active.

Validators should reject contradictory gates on one held binding, such as
requiring and blocking the same input. The gamepad preset builder should create
gated fast/slow pairs from one higher-level concept so the fast binding gets
the blocked gate and the slow binding gets the required gate together.

Validators should also reject invalid modifier values:

- non-finite scale values
- dead-zone thresholds outside the supported range
- dead-zone thresholds where lower is greater than or equal to upper
- duplicate modifiers when the descriptor shape permits only one

Preset-builder speed values should use semantic units, not unexplained raw
multipliers. For example:

- orbit rates: screen rotations per second at full input
- pan rates: screen fractions per second at the current focus depth
- zoom rates: radius ratios or radius units per second

If those rates cannot be represented by a static BEI `Scale`, Lagrange should
lower them through a Lagrange-owned rate descriptor or resolver/controller step
while still keeping the physical input binding and simple transforms in BEI.

Modifier order must be explicit. The first pass should use this normalized
lowering order for gamepad entries:

1. raw BEI binding value
2. dead zone
3. scale
4. intrinsic composite transform, such as swizzle or negate
5. conditions

Install-path tests should assert that generated BEI binding entities receive
the intended modifier and condition sequence.

## Composite Bindings

Convenience constructors should remain:

```rust
OrbitCamInputBinding::gamepad_axes_2d(GamepadAxis::RightStickX, GamepadAxis::RightStickY)
OrbitCamInputBinding::bidirectional_gamepad_buttons(
    GamepadButton::RightTrigger2,
    GamepadButton::LeftTrigger2,
)
```

Composite bindings should preserve logical composite intent until validation
and expansion. The flattened BEI entries need metadata for logical output axis
and sign so per-axis scale, swizzle, negate, and signed trigger zoom cannot be
applied to the wrong entry.

Composite bindings should apply held-binding gates to every expanded BEI entry
unless the API intentionally uses a signed single-button helper.

For a two-axis stick, first-pass dead-zone behavior is axial because
`GamepadAxes2d` expands into two one-dimensional BEI axis bindings. Radial stick
dead zones require a real two-axis value before expansion and are future work.

Scale should be expressible as either uniform scale or per-axis scale:

```rust
right_stick.with_scale(360.0)
right_stick.with_scale(Vec2::new(360.0, 260.0))
```

The descriptor can lower this to BEI `Scale` components on the expanded X and Y
binding entries. Concrete lowering is:

- X entry gets `Scale::splat(x_scale)`
- Y entry gets `Scale::splat(y_scale)` before `SwizzleAxis::YXZ`
- signed trigger entries apply their signed scalar consistently with negate

Slow zoom cannot rely only on one bidirectional trigger composite if each
direction needs a different shoulder condition. Add either per-expanded-entry
condition targeting or explicit signed single-button helpers. The simpler first
pass is a signed helper:

```rust
OrbitCamInputBinding::gamepad_button_axis(GamepadButton::RightTrigger2, 1.0)
OrbitCamInputBinding::gamepad_button_axis(GamepadButton::LeftTrigger2, -1.0)
```

That lets slow zoom build two conditioned entries without over-gating both
directions.

## Gamepad Preset Defaults

Initial defaults should prioritize usefulness over exact physical realism.
These values should be treated as tunable starting points:

- fast orbit: approximately one screen rotation in one to two seconds at full
  stick deflection
- slow orbit: about one quarter of fast orbit
- fast pan: large enough to reposition the focus comfortably around a small
  example object
- slow pan: about one quarter of fast pan
- fast zoom: comfortable continuous dolly speed
- slow zoom: about one quarter of fast zoom
- stick dead zone: enough to prevent drift on common controllers

The `input_gamepad` example should use the preset defaults first. Only promote
values after they feel correct on real hardware.

The control summary and example title bar should use condition-aware labels so
fast and slow controls do not collapse into duplicate rows. For gamepad display,
prefer Xbox-style labels in examples:

- `LS`
- `RS`
- `LB`
- `RB`
- `LT`
- `RT`

The preset should have expected control-summary rows for fast and slow orbit,
pan, and zoom.

`OrbitCamBindings` should carry camera-scoped profile metadata so tuned preset
bindings can still render useful summaries. A concrete starting point is:

```rust
OrbitCamBindingsProfile::Custom
OrbitCamBindingsProfile::GamepadPreset { customized: bool }
OrbitCamBindingsProfile::KeyboardPreset { customized: bool }
OrbitCamBindingsProfile::LayeredPreset {
    layers: PresetLayerSet,
}
```

The profile is descriptive metadata for camera controls only. It should help
control summaries choose labels such as `RS Orbit`, `RB+RS Slow Orbit`, or
`Arrow Keys Orbit`; it should not encode gameplay actions.

Profile propagation rules:

- raw `OrbitCamBindings::builder()` defaults to `Custom`
- preset and layer builders attach the appropriate preset profile
- typed tuning methods preserve preset profile metadata
- fully arbitrary edits either keep `Custom` or require an explicit profile
  override
- duplicate layers are rejected or made idempotent by `OrbitCamPresetLayers`

Every public descriptor that affects behavior, layer identity, profile
metadata, summaries, modifiers, or gates should derive `Reflect` and validate
through the same path as non-reflected construction. Runtime BEI components,
action entities, helper entities, and installed condition entities are excluded
from reflection.

## Preset Migration

The gamepad example should become preset-first:

```rust
OrbitCamInputMode::Preset(OrbitCamPreset::Gamepad)
```

and keep its explicit no-position routing fallback:

```rust
CameraInputRoutingConfig::cursor_hit_test()
    .with_no_position_fallback(NoPositionFallback::OnlyEligibleCamera)
```

Developers should choose between three levels:

- zero-config preset: `OrbitCamPreset::Gamepad`, `Keyboard`,
  `BlenderLikeKeyboard`, or `SimpleMouseKeyboard`
- tuned preset builder: change scales, dead zones, inversion, precision
  buttons, or preset layers while keeping the preset metadata
- fully custom `OrbitCamBindings`: use when the app owns a camera keymap that
  no preset layer expresses

Examples should follow that order: `input_gamepad` should show the preset first,
`input_keyboard` should become the keyboard preset example, and custom binding
examples should be clearly framed as escape hatches.

## Acceptance Tests

The implementation should include behavior-level tests for the committed
presets and descriptor features:

- `OrbitCamPreset::Gamepad.to_bindings()` sets
  `CameraInputGamepadSelectionPolicy::Active`
- default cursor-hit routing without an explicit route or no-position fallback
  produces no gamepad input
- explicit routing works with multiple eligible cameras
- `OnlyEligibleCamera` works with exactly one eligible camera
- multiple eligible cameras with no explicit route produce no gamepad input
- fast and slow orbit, pan, and zoom are mutually exclusive across multiple
  held frames
- blocked fast bindings do not set `has_orbit`, `has_pan`, or `has_zoom`
- blocked bindings do not emit misleading interaction source or lifecycle
  events
- replacing `OrbitCamInputMode` despawns helper gate actions, bindings, and
  conditions
- `RT` is positive zoom and `LT` is negative zoom
- slow values are the documented fraction of fast values
- `Keyboard`, `BlenderLikeKeyboard`, and `SimpleMouseKeyboard` resolve camera
  orbit, pan, and zoom from all included layers
- composed preset summaries include every layer family and no non-camera actions
- control summaries emit distinct fast and slow rows

## Non-Goals

This design does not try to expose arbitrary BEI bundles in the first pass.
That would require making Lagrange action entity handles public, solving
teardown ownership, and deciding how third-party binding entities participate
in camera routing.

This design also does not introduce Xbox-specific API names. The example can
use Xbox-style labels, but the preset should be named `Gamepad` unless hardware
testing shows a controller-specific mapping problem.

This design does not create a general gameplay input abstraction. Lagrange
descriptors and presets only create camera semantic actions such as orbit, pan,
and zoom. Player or free-camera actor actions such as jump, move, crouch,
sprint, and interact remain owned by the player/controller layer.

## Open Questions

- Should binding priority be added later beyond the accepted required/blocked
  gate model, or should priority stay out unless BEI lowering proves it is
  necessary?

## Team Review

Cycle 1 recorded these accepted refinements:

- active gamepad selection is device selection only; routing still needs an
  explicit route or no-position fallback
- button gates stay symbolic in descriptors and lower to fresh per-camera BEI
  actions and conditions during install
- required and blocked gates apply to both held motion and held engagement by
  default
- validators reject contradictory gates, and the preset builder should create
  fast/slow pairs atomically
- first-pass stick dead zones are axial; radial stick dead zones are future
  work unless a true two-axis descriptor is added
- modifier order is normalized and should be covered by install-path tests
- slow zoom needs signed single-button entries or per-expanded-entry condition
  targeting
- tuned gamepad controls build `OrbitCamBindings`; `OrbitCamPreset::Gamepad`
  remains the zero-config preset
- control summaries must include condition descriptors and use consistent
  gamepad labels

Second team review, cycle 1 input included one additional design constraint:

- keyboard camera bindings should be layerable with Blender-like and SimpleMouse
  controls, but this remains camera intent only and must not grow into player
  actions

Second team review, cycle 1 recorded these accepted refinements:

- route-neutral gamepad presets must document and test explicit routing or
  `OnlyEligibleCamera`; a camera-semantic gamepad latch is future work
- required/blocked gates must stay active across held frames, not only on press
  edges
- descriptors use Lagrange-owned closed modifier and condition types, lowered to
  BEI during install
- modifiers are canonicalized, not arbitrary duplicated BEI component payloads
- gates live on held bindings by default and are copied to both motion and
  engagement
- install builds a typed per-camera gate-action cache keyed by symbolic gate
  input
- composites preserve logical axis/sign metadata until expansion
- validation covers finite scale, valid dead-zone thresholds, duplicate
  modifiers, and gate contradictions
- behavior-level tests should cover the full gamepad preset, routing
  requirements, slow/fast exclusivity, signed zoom, and summary rows
- the gamepad example should become preset-first
- keyboard should exist as a preset layer and have composed convenience presets
  with Blender-like and SimpleMouse controls

Second team review, cycle 2 recorded these accepted refinements:

- `SimpleMouse` and `BlenderLike` keep their existing pointer-style meanings;
  composed presets are aliases over named layers
- `OrbitCamPresetLayers` is the canonical implementation path for composed
  presets
- profile metadata has an explicit data path from descriptor to validated
  bindings and summaries
- raw custom bindings default to `OrbitCamBindingsProfile::Custom`; typed preset
  tuning preserves preset metadata
- keyboard gates are first-pass support, not future work, because precision
  keyboard controls are part of the design intent
- modifiers are represented as a canonical struct, not a free vector
- gates are represented by normalized input plus polarity
- layer profiles are a validated set with duplicate behavior defined
- reflection covers every public descriptor that affects behavior or summaries;
  runtime BEI entities remain unreflected
- examples need a preset-first migration path and a naming table
- acceptance tests cover keyboard layers, gamepad routing negative cases,
  lifecycle/source correctness, and helper teardown

## Proposed User Decisions

### D1: Slow Zoom Chord Shape

Status: accepted

Decision: use same-side slow zoom chords: `RB + RT` for slow zoom in and
`LB + LT` for slow zoom out.

Impact: important. This determines the default preset and the labels users see
in the example.
