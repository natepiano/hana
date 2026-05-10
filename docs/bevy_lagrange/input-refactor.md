# `bevy_lagrange` input refactor

## Goal

Make `bevy_lagrange` opinionated about Bevy's action/context input model while
keeping camera behavior separate from physical input policy.

The target shape is:

- `OrbitCam` owns camera state, response scaling, smoothing, limits, animation behavior, and active-camera behavior.
- `bevy_enhanced_input` owns the public action model: actions, contexts, bindings, modifiers, conditions, and user keymaps.
- `bevy_lagrange` provides default camera controls as enhanced-input presets.
- `bevy_lagrange` keeps a narrow adapter for source details that enhanced input does not currently expose.
- The camera controller consumes one per-camera intent snapshot, not raw Bevy input and not binding policy.

## Design Rules

1. `OrbitCam` configures how the camera moves.
2. `bevy_lagrange::input` contains the public camera-input API.
3. `OrbitCamControls` configures who owns user-input resolution.
4. `OrbitCamBindings` is the public custom binding and adapter-policy spec.
5. Enhanced-input actions configure what user input means.
6. `OrbitCamInput` is the resolved per-frame camera intent.
7. Manual input uses helper methods and typed deltas, not raw field mutation.
8. App-level input disabling uses `OrbitCamInputDisabled`.
9. Transient blockers such as animation ignore and UI focus are internal library state.
10. Programmatic camera operations mutate camera state, targets, or animation queues; they do not write `OrbitCamInput`.
11. Preset and custom controls have one library-owned input writer per frame.
12. Manual controls mean the app writes `OrbitCamInput` and the library skips action resolution for that camera.

## Dependencies And Features

Use the simple feature surface:

- `bevy_enhanced_input` is a normal dependency of `bevy_lagrange`.
- `bitflags` is a direct dependency of `bevy_lagrange`.
- `bevy_egui` remains optional.
- `fit_overlay` remains optional.
- `OrbitCamControls::Manual` is a per-camera control mode, not a no-dependency build mode.

Declare both dependencies through workspace dependency entries and use those entries
from `crates/bevy_lagrange/Cargo.toml`. `bevy_enhanced_input` should be pinned to the
Bevy-compatible version the implementation targets so `bevy_lagrange` does not
silently rely on a transitive copy pulled in by another crate.

This plan assumes the current `bevy_enhanced_input` model:

- Contexts are regular components registered with `add_input_context`.
- Built-in `Binding` variants include keyboard, mouse button, mouse motion, mouse wheel, gamepad button, gamepad axis, any key, and none.
- Bindings can use custom `InputModifier` and `InputCondition` components.
- `ActionMock` can feed externally produced values through enhanced-input action timing, but active mocks skip input reading, conditions, and modifiers.
- The built-in `Binding` enum is closed, so user crates cannot add first-class raw binding sources.
- `PinchGesture`, `Touches`, and `MouseWheel::unit` are not represented with enough detail to preserve the current `bevy_lagrange` camera model purely through public bindings.

References:

- <https://docs.rs/bevy_enhanced_input/latest/bevy_enhanced_input/binding/enum.Binding.html>
- <https://docs.rs/bevy_enhanced_input/latest/bevy_enhanced_input/context/trait.InputContextAppExt.html>
- <https://docs.rs/bevy_enhanced_input/latest/bevy_enhanced_input/modifier/trait.InputModifier.html>
- <https://docs.rs/bevy_enhanced_input/latest/bevy_enhanced_input/condition/trait.InputCondition.html>
- <https://docs.rs/bevy_enhanced_input/latest/bevy_enhanced_input/action/mock/struct.ActionMock.html>
- <https://docs.rs/bevy_enhanced_input/latest/bevy_enhanced_input/context/struct.ExternallyMocked.html>

## Public Module Shape

Group the public input API under `bevy_lagrange::input` so the binding model is
discoverable.

```text
src/
  input/
    mod.rs                 // public overview docs and re-exports
    actions.rs             // public Orbit, Pan, ZoomCoarse, ZoomSmooth, OrbitEngaged
    bindings.rs            // public OrbitCamBindings and adapter binding policy
    context.rs             // public OrbitCamInputContext
    controls.rs            // public OrbitCamControls and presets
    events.rs              // public camera interaction lifecycle events
    intent.rs              // public OrbitCamInput and typed deltas
    disabled.rs            // public OrbitCamInputDisabled
    installation.rs        // private owned input-entity relationships and reconciliation
    adapter/
      mod.rs               // private adapter plugin and systems
      actions.rs           // pub(super) source actions only if needed
      wheel.rs
      touch.rs
      pinch.rs
```

`input/mod.rs` should explain the control modes at the top:

```rust
//! Camera input API.
//!
//! Most users choose one of three control modes:
//!
//! - [`OrbitCamControls::Preset`] for built-in bindings.
//! - [`OrbitCamControls::Custom`] for app-defined bindings through [`OrbitCamBindings`].
//! - [`OrbitCamControls::Manual`] for writing [`OrbitCamInput`] directly.
//!
//! Preset and custom controls are resolved through `bevy_enhanced_input`.
//! Manual controls bypass enhanced input for that camera.
//!
//! Adapter-backed sources such as wheel-unit, pinch, touch, and smooth-scroll
//! policy are configured through [`OrbitCamBindings`], not through private
//! adapter actions.
```

The public facade should re-export the semantic API from both `input` and the crate
root for convenience:

```rust
pub use input::{
    CameraInteractionEnded,
    CameraInteractionKind,
    CameraInteractionSources,
    CameraInteractionStarted,
    OrbitCamBindings,
    OrbitCamControlPreset,
    OrbitCamControls,
    OrbitCamInput,
    OrbitCamInputContext,
    OrbitCamInputDisabled,
    OrbitDelta,
    PanDelta,
    CoarseZoomDelta,
    SmoothZoomDelta,
};
```

Do not re-export private source actions such as `OrbitFromSmoothScroll`,
`ZoomFromPinch`, or `TouchPan`.

## Camera Behavior

`OrbitCam` remains the camera behavior component. It owns:

- focus, yaw, pitch, radius, and targets;
- sensitivity and smoothing;
- bounds and clamping;
- upside-down behavior;
- animation behavior;
- time source;
- transform update behavior.

Long term, `OrbitCam` should not contain physical binding fields such as mouse buttons,
keyboard modifiers, touch behavior, trackpad behavior, or zoom direction. Those belong
to controls, bindings, adapter policy, or response configuration.

`OrbitCam` should require:

```rust
#[require(
    OrbitCamInput,
    OrbitCamInputContext,
    OrbitCamControls,
)]
pub struct OrbitCam {
    // camera behavior fields
}
```

`LagrangePlugin` should register the context once:

```rust
app.add_input_context::<OrbitCamInputContext>();
```

## Controls And Bindings

`OrbitCamControls` selects who owns user-input resolution for a camera.

```rust
#[derive(Component, Clone, Debug)]
pub enum OrbitCamControls {
    Preset(OrbitCamControlPreset),
    Custom(OrbitCamBindings),
    Manual,
}

#[derive(Clone, Copy, Debug, Reflect)]
#[non_exhaustive]
pub enum OrbitCamControlPreset {
    BlenderLike,
    SimpleMouse,
}
```

`OrbitCamControls` should only derive `Reflect` if the chosen `OrbitCamBindings`
representation is also intentionally reflectable. Do not force custom bindings to use
weak type erasure just to make the control component reflect. If reflection is needed
before custom bindings can be reflected cleanly, reflect a smaller preset/manual
selection type and keep custom binding specs as ordinary Rust data.

If an `OrbitCam` has no explicit controls component, the required component default
should be `OrbitCamControls::Preset(OrbitCamControlPreset::BlenderLike)`.

Future-facing public policy enums should be `#[non_exhaustive]` unless the API is
intentionally closed. This applies especially to presets, wheel policy, pinch/touch
policy, routing, and interaction kind.

The modes mean:

| Mode | Meaning | Library writes `OrbitCamInput` |
|------|---------|--------------------------------|
| `Preset(BlenderLike)` | Build `OrbitCamBindings` from the Blender-like preset, install actions and adapter policy, and resolve input. | yes |
| `Preset(SimpleMouse)` | Build `OrbitCamBindings` from the simpler mouse preset, install actions and adapter policy, and resolve input. | yes |
| `Custom(bindings)` | Use the public camera context and resolver, but install the app-provided `OrbitCamBindings`. | yes |
| `Manual` | Do not install or resolve camera actions for this camera. The app writes `OrbitCamInput` through helper methods. | no |

Example spawns:

```rust
commands.spawn((
    Camera3d::default(),
    OrbitCam::default(),
    OrbitCamControls::Preset(OrbitCamControlPreset::BlenderLike),
));
```

```rust
commands.spawn((
    Camera3d::default(),
    OrbitCam::default(),
    OrbitCamControls::Custom(
        editor_keymap.to_orbit_cam_bindings(),
    ),
));
```

```rust
commands.spawn((
    Camera3d::default(),
    OrbitCam::default(),
    OrbitCamControls::Manual,
));
```

### `OrbitCamBindings`

`OrbitCamBindings` is a data spec that `bevy_lagrange` turns into enhanced-input
action entities and adapter policy. It should have private fields and be constructed
through local builder/spec APIs. The public API should either intentionally re-export
enhanced-input binding types as part of the `bevy_lagrange` semver surface or wrap
them behind Lagrange-specific constructors. The default should be to wrap where that
keeps the camera API stable and lets the implementation adapt to upstream changes.

It contains two kinds of configuration:

- ordinary enhanced-input bindings for public semantic actions;
- adapter policy for sources enhanced input does not currently describe richly enough.

Conceptual shape:

```rust
pub struct OrbitCamBindings {
    orbit: ActionBindingSpec<Vec2>,
    pan: ActionBindingSpec<Vec2>,
    zoom_smooth: ActionBindingSpec<f32>,
    zoom_coarse: ActionBindingSpec<f32>,
    orbit_engaged: ActionBindingSpec<bool>,
    wheel: OrbitCamWheelBinding,
    pinch: OrbitCamPinchBinding,
    touch: Option<TouchInput>,
    zoom_direction: ZoomDirection,
    button_drag_zoom: Option<ButtonDragZoomBinding>,
}
```

The actual type should prefer constructors and builders over public fields. Required
choices should use typestate builders where practical so invalid custom binding states
are not representable through ordinary Rust APIs. Runtime construction paths such as
reflection, deserialization, or future dynamic keymap loading still need validation.
Expose that validation as `try_build` or an equivalent checked constructor, and make
the resolver reject or clearly warn on adapter/public-binding conflicts.

Wheel policy needs a typestate builder, or an equivalent compile-time constrained API,
so custom users must intentionally choose adapter-owned wheel behavior or disabled
wheel behavior. Preset/custom controls should not expose raw `MouseWheel` binding
helpers.

Builder docs should explain the constraint:

```rust
/// Builder for custom orbit-camera bindings.
///
/// Wheel input is configured through [`OrbitCamWheelBinding`] instead of ordinary
/// enhanced-input mouse-wheel bindings because `bevy_lagrange` needs Bevy's wheel
/// unit information to distinguish line scroll from pixel scroll. Line scroll feeds
/// coarse zoom; pixel scroll may feed smooth zoom, pan, or orbit depending on the
/// selected camera policy.
///
/// The builder requires an explicit wheel choice so custom bindings cannot
/// accidentally route the same wheel event through both enhanced input and the
/// Lagrange wheel adapter.
```

Example custom keymap handoff:

```rust
pub struct EditorKeymap {
    pub orbit_button: MouseButton,
    pub pan_modifier: KeyCode,
    pub zoom_in_key: KeyCode,
    pub zoom_out_key: KeyCode,
}

impl EditorKeymap {
    pub fn to_orbit_cam_bindings(&self) -> OrbitCamBindings {
        OrbitCamBindings::builder()
            .orbit_drag(self.orbit_button)
            .pan_drag_with_key(self.orbit_button, self.pan_modifier)
            .zoom_keys(self.zoom_in_key, self.zoom_out_key)
            .wheel(OrbitCamWheelBinding::blender_like())
            .build()
    }
}
```

If the app changes its keymap at runtime, it should rebuild and replace
`OrbitCamControls::Custom(bindings)`. The control reconciler replaces the camera's
library-owned input installation, so the old custom bindings do not remain active.

Manual mode remains unrestricted: a manual user can read any Bevy input source and
write `OrbitCamInput` through the public helper methods.

### Input Installation Ownership

Preset and custom controls install private enhanced-input actions, bindings, adapter
state, and mock state for a camera. Those implementation entities are not scene
hierarchy children. Model their ownership with a private Bevy relationship rather
than `ChildOf`:

```rust
#[derive(Component)]
#[relationship(relationship_target = OrbitCamInputEntities)]
struct OrbitCamInputEntityOf(#[relationship] Entity);

#[derive(Component)]
#[relationship_target(relationship = OrbitCamInputEntityOf, linked_spawn)]
struct OrbitCamInputEntities(Vec<Entity>);
```

Changing `OrbitCamControls` replaces the whole private input installation:

```text
Changed<OrbitCamControls>
  -> finish active camera-input interactions
  -> clear OrbitCamInput for that camera
  -> clear the owner latch if that camera owns input
  -> despawn_related::<OrbitCamInputEntities>()
  -> install the new preset/custom input entities, or install nothing for Manual
```

The relationship owns structural cleanup. A scheduled reconciliation system owns the
semantic cleanup because it must emit interaction end events and clear stale intent
before any animation or controller system can consume input.

## Semantic Actions

The public enhanced-input actions are semantic, not device-specific.

```rust
#[derive(InputAction)]
#[action_output(Vec2)]
pub struct Orbit;

#[derive(InputAction)]
#[action_output(Vec2)]
pub struct Pan;

#[derive(InputAction)]
#[action_output(f32)]
pub struct ZoomCoarse;

#[derive(InputAction)]
#[action_output(f32)]
pub struct ZoomSmooth;

#[derive(InputAction)]
#[action_output(bool)]
pub struct OrbitEngaged;
```

`OrbitEngaged` exists because orbit motion and orbit interaction state are different
facts:

- `Orbit` is how much to rotate this frame.
- `OrbitEngaged` is whether the user's current control scheme is actively orbiting.

The controller needs the engagement edge to preserve the current orbit-drag latch,
including upside-down yaw behavior. A user can press the orbit control and hold still;
the motion delta is zero, but the interaction has still started.

Pan and zoom interaction phases should not be inferred only from nonzero movement.
Button-held pan and button-drag zoom can also be active with zero delta. The resolver
and adapter should derive interaction state from action timing and source state for
all interaction kinds.

The stable controller-facing representation is per-kind active sources in
`OrbitCamInput`, not the presence of a nonzero delta. Public semantic actions may add
dedicated engagement actions for pan or zoom if that is the cleanest way to preserve
held state through enhanced input, but lifecycle events should be derived from the
resolved active-source sets:

```text
orbit_delta + orbit_active_sources
pan_delta + pan_active_sources
coarse_zoom_delta + zoom_active_sources
smooth_zoom_delta + zoom_active_sources
```

This keeps these cases distinct:

```text
pan button held, pointer still       -> active pan, zero pan delta
mouse wheel event this frame         -> active zoom impulse, nonzero zoom delta
no user input for this camera        -> no active sources, zero deltas
```

## Presets

Preset docs should say whether the preset follows platform-natural pointing-device
expectations or 3D-viewport/editor expectations.

Apple's macOS pointing-device guidance treats scroll as content movement, pinch as
zoom, and rotate as content rotation. Blender-like controls intentionally map smooth
scroll to orbit because that is a 3D viewport convention, not because macOS generally
treats trackpad scroll that way.

### Preset Binding Table

| Operation | `BlenderLike` | `SimpleMouse` |
|-----------|---------------|---------------|
| Orbit drag | Middle mouse drag | Left mouse drag |
| Orbit engagement | Middle mouse held | Left mouse held |
| Pan drag | Shift + middle mouse drag | Right mouse drag |
| Coarse zoom | Line wheel | Line wheel |
| Smooth zoom | Pinch; smooth scroll with zoom modifier | Pixel wheel / smooth scroll; pinch |
| Smooth scroll without modifier | Orbit | Smooth zoom |
| Smooth scroll with pan modifier | Pan | Smooth zoom |
| Touch default | `TouchInput::OneFingerOrbit` | `TouchInput::OneFingerOrbit` |
| Zoom direction | normal | normal |
| Button-drag zoom | disabled unless configured | disabled unless configured |

`OrbitCam::default()` should resolve to the opinionated `BlenderLike` preset. This is
a deliberate behavior change from the current left-mouse/right-mouse/zoom-only default.
`SimpleMouse` is the migration path for users who want the older mouse-oriented feel.

### Wheel And Smooth Scroll

`OrbitCamWheelBinding` should make wheel and smooth-scroll policy explicit:

```rust
pub enum OrbitCamWheelBinding {
    Disabled,
    ZoomOnly,
    PlatformNatural,
    BlenderLike {
        pan_modifier: WheelModifier,
        zoom_modifier: WheelModifier,
    },
}

pub enum WheelModifier {
    Disabled,
    Key(KeyCode),
    Always,
}
```

Use builder constructors for Blender-like wheel policy so ambiguous states such as two
`Always` modifiers are rejected or unrepresentable. `Always` preserves the old
`Option<KeyCode>::None` behavior where a mode can be active without a key.

The preset adapter should preserve these behaviors unless a later implementation
explicitly changes the matrix and examples.

| Source | Policy | Modifier | Intent |
|--------|--------|----------|--------|
| `MouseWheel::Line` | any enabled wheel policy | any | `zoom.coarse += y` |
| `MouseWheel::Pixel` | `ZoomOnly` | any | `zoom.smooth += y * pixel_scale` |
| `MouseWheel::Pixel` | `PlatformNatural` | none | `pan += Vec2::new(x, y) * smooth_scroll_sensitivity` |
| `MouseWheel::Pixel` | `BlenderLike` | none | `orbit += Vec2::new(x, y) * smooth_scroll_sensitivity` |
| `MouseWheel::Pixel` | `BlenderLike` | pan modifier active | `pan += Vec2::new(x, y) * smooth_scroll_sensitivity` |
| `MouseWheel::Pixel` | `BlenderLike` | zoom modifier active | `zoom.smooth += y * pixel_scale` |

Line scroll is coarse zoom. Pixel scroll is smooth-scroll input. The event source flag
should use `SMOOTH_SCROLL`, not `TRACKPAD`, because Bevy does not guarantee the
physical device identity.

### Pinch

| Source | Policy | Modifier | Intent |
|--------|--------|----------|--------|
| `PinchGesture` | enabled pinch policy | no camera modifier | `zoom.smooth += pinch * pinch_scale` |
| `PinchGesture` | `BlenderLike` | pan or zoom modifier | ignored |

The Blender-like preset should keep the current behavior where pinch is suppressed
while the trackpad scroll modifiers are held.

### Zoom Direction And Button-Drag Zoom

`ZoomDirection` needs a new home outside `OrbitCam` physical input fields. Put it on
`OrbitCamBindings` or a closely related camera response config, and apply it uniformly
to every user-input zoom source:

- line wheel coarse zoom;
- pixel wheel / smooth-scroll zoom;
- pinch;
- touch pinch;
- button-drag zoom;
- keyboard or gamepad custom zoom actions.

Button-drag zoom should be represented as an explicit optional binding policy rather
than a leftover `OrbitCam` field:

```rust
pub struct ButtonDragZoomBinding {
    button: MouseButton,
    axis: ButtonDragZoomAxis,
    scale: f32,
}

#[derive(Clone, Copy, Debug, Reflect)]
#[non_exhaustive]
pub enum ButtonDragZoomAxis {
    X,
    Y,
    XY,
}
```

Button-drag zoom feeds smooth zoom. It should mark zoom active while the button is
held, even when pointer delta is zero, so interaction lifecycle events and animation
interruption behave like other held interactions.

### Touch

| Touch preset | Gesture | Intent |
|--------------|---------|--------|
| `OneFingerOrbit` | one-finger move | `orbit += motion` |
| `OneFingerOrbit` | two-finger move | `pan += midpoint_motion` |
| `OneFingerOrbit` | two-finger pinch | `zoom.smooth += pinch * touch_pinch_scale` |
| `TwoFingerOrbit` | one-finger move | `pan += motion` |
| `TwoFingerOrbit` | two-finger move | `orbit += midpoint_motion` |
| `TwoFingerOrbit` | two-finger pinch | `zoom.smooth += pinch * touch_pinch_scale` |

The touch adapter should track stable touch IDs. Changing touch arity ends the old
touch interaction and starts the new one:

- `0 -> 1` starts the one-finger operation.
- `1 -> 2` ends the one-finger operation and starts the two-finger operation.
- `2 -> 1` ends the two-finger operation and starts the one-finger operation.
- `2 -> 3+` ends the two-finger operation and starts no camera input.
- `3+ -> 2` starts the two-finger operation.

Two-finger rotation should stay computed internally but unused until camera roll is
designed.

## Adapter

The adapter exists to preserve source details that enhanced input does not currently
carry. It should be small, private, and easy to delete if upstream enhanced input gains
first-class line scroll, pixel scroll, pinch, touch, and gesture bindings.

The adapter should:

- use normal enhanced-input bindings where they are expressive enough;
- read `MouseWheel::unit`, `PinchGesture`, `RotationGesture` when roll exists, and `Touches` directly where needed;
- keep source-specific adapter actions private if `ActionMock` is useful for timing;
- avoid mocking public semantic actions that also have normal bindings;
- aggregate public semantic actions and private adapter contributions into `OrbitCamInput`;
- document any `ActionMock` use, because mocked actions skip normal input reading, conditions, and modifiers while active.

Preset and custom controls should route wheel, pinch, touch, and smooth-scroll policy
through `OrbitCamBindings`. Users should not configure private adapter actions.

For any raw source handled by the adapter, the binding API should prevent or reject
equivalent public enhanced-input bindings in preset/custom modes. This prevents the
same physical event from being counted twice.

## Camera Intent And Manual Input

`OrbitCamInput` is a per-camera frame snapshot. The controller reads it, applies camera
behavior, and the input pipeline clears or overwrites it each frame.

The snapshot stores movement deltas and active source sets separately. A helper call
marks an interaction active for that frame even if the delta is zero. This lets manual
and resolved controls represent "held but still" input without touching raw fields.

Manual users should not normally set value, source, and phase fields directly. The
public manual API should be method-based:

```rust
impl OrbitCamInput {
    pub fn orbit(
        &mut self,
        delta: impl Into<OrbitDelta>,
        sources: CameraInteractionSources,
    );

    pub fn pan(
        &mut self,
        delta: impl Into<PanDelta>,
        sources: CameraInteractionSources,
    );

    pub fn zoom_coarse(
        &mut self,
        delta: impl Into<CoarseZoomDelta>,
        sources: CameraInteractionSources,
    );

    pub fn zoom_smooth(
        &mut self,
        delta: impl Into<SmoothZoomDelta>,
        sources: CameraInteractionSources,
    );

    pub fn orbit_active(&mut self, sources: CameraInteractionSources);
    pub fn pan_active(&mut self, sources: CameraInteractionSources);
    pub fn zoom_active(&mut self, sources: CameraInteractionSources);
}
```

Typed deltas name the units:

```rust
pub struct OrbitDelta(Vec2);
pub struct PanDelta(Vec2);
pub struct CoarseZoomDelta(f32);
pub struct SmoothZoomDelta(f32);

impl OrbitDelta {
    pub const fn screen_pixels(x: f32, y: f32) -> Self;
}

impl PanDelta {
    pub const fn screen_pixels(x: f32, y: f32) -> Self;
}

impl CoarseZoomDelta {
    pub const fn amount(amount: f32) -> Self;
}

impl SmoothZoomDelta {
    pub const fn amount(amount: f32) -> Self;
}
```

Convenience conversions are fine if docs state the default unit:

```rust
impl From<Vec2> for OrbitDelta; // screen pixels
impl From<Vec2> for PanDelta;   // screen pixels
impl From<f32> for CoarseZoomDelta;
impl From<f32> for SmoothZoomDelta;
```

Manual example shape:

```rust
fn manual_camera_input(mut input: Single<&mut OrbitCamInput>) {
    input.orbit(
        OrbitDelta::screen_pixels(-4.0, 0.0),
        CameraInteractionSources::MANUAL,
    );

    input.pan_active(CameraInteractionSources::MANUAL);
}
```

Manual users provide value and source. The library derives interaction started/ended
events from frame-to-frame active source sets. `orbit`, `pan`, `zoom_coarse`, and
`zoom_smooth` all mark the corresponding interaction active for the frame. The
`*_active` helpers exist for held controls that have no movement this frame.

Manual writers should run in `OrbitCamInputSet::WriteManual`. The finalization system
runs after that set, clears blocked or stale input, emits lifecycle events, and then
hands finalized input to animation and controller systems.

`ZoomInput` uses camera-facing names:

- `coarse` is step-like zoom, usually line wheel input.
- `smooth` is continuous zoom, usually pixel scroll, pinch, or drag zoom.

Smooth zoom preserves the current pixel path semantics: it adjusts the target radius
and the current radius immediately so trackpad, pinch, and drag zoom feel responsive.
Coarse zoom drives the target radius and uses normal zoom smoothing.

## Interaction Events

Interaction lifecycle events live in `input/events.rs` and are re-exported from both
`bevy_lagrange::input` and the crate root.

They target the camera entity and carry both the interaction kind and source set.

```rust
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub enum CameraInteractionKind {
    Orbit,
    Pan,
    Zoom,
}
```

```rust
/// Source categories that contributed to a camera interaction.
///
/// These are input paths, not guaranteed hardware identities. For example,
/// `SMOOTH_SCROLL` means Bevy reported pixel scroll input; it does not guarantee
/// the physical device was a trackpad.
pub struct CameraInteractionSources(...);
```

Use `bitflags` internally or as the wrapped representation:

```rust
bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct CameraInteractionSourceFlags: u32 {
        const MOUSE = 1 << 0;
        const KEYBOARD = 1 << 1;
        const WHEEL = 1 << 2;
        const SMOOTH_SCROLL = 1 << 3;
        const PINCH = 1 << 4;
        const TOUCH = 1 << 5;
        const GAMEPAD = 1 << 6;
        const MANUAL = 1 << 7;
    }
}
```

If direct bitflags reflection is brittle, expose a reflected newtype with constants,
`contains`, and `intersects`.

Do not include a `CUSTOM` source flag. Custom is a control mode, not an input source.
Custom keyboard bindings should report `KEYBOARD`; custom gamepad bindings should
report `GAMEPAD`; direct manual writes should report `MANUAL`.

Public events stay simple:

```rust
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct CameraInteractionStarted {
    #[event_target]
    pub camera: Entity,
    pub kind: CameraInteractionKind,
    pub sources: CameraInteractionSources,
}

#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct CameraInteractionEnded {
    #[event_target]
    pub camera: Entity,
    pub kind: CameraInteractionKind,
    pub sources: CameraInteractionSources,
}
```

Do not expose a public end reason yet.

Events are interaction-level, not per-source. Internally, a per-camera interaction
tracker should keep previous and current active source sets for orbit, pan, and zoom:

```text
previous active sources
current active sources
started sources = current - previous
ended sources = previous - current
```

Public events are emitted only when the interaction as a whole starts or ends:

```text
previous empty, current non-empty -> CameraInteractionStarted
previous non-empty, current empty -> CameraInteractionEnded
```

If another source joins while an interaction is already active, no second started event
is emitted. If one source ends while another remains active, no ended event is emitted.

If input becomes blocked while an interaction is active, emit `CameraInteractionEnded`
before suppressing further input so guidance overlays and editor tools do not get
stuck highlighted.

Source lifetime is deterministic:

| Source class | Examples | Lifecycle |
|--------------|----------|-----------|
| Held | mouse-button drags, touch contacts, engaged gamepad controls, manual active calls | starts when held state begins; ends when held state ends |
| Impulse | line wheel, pixel wheel / smooth scroll, pinch gesture delta, pan gesture delta | starts and ends in the frame where the event exists |

Owner latching comes only from held sources. Impulse sources are routed per event by
the event window and current pointer/touch position for that frame. Do not add an
idle-frame grace window for wheel, smooth-scroll, or pinch; presentation layers such
as `fairy_dust` may add visual highlight linger, but camera input semantics stay exact.
For an impulse-only interaction, finalization may emit `CameraInteractionStarted` and
`CameraInteractionEnded` in the same frame.

## Input Disabling And Blockers

Expose a small public app-level disable component:

```rust
#[derive(Component, Default, Reflect)]
#[reflect(Component)]
pub struct OrbitCamInputDisabled;
```

This is separate from `OrbitCamControls`. Disabling input does not replace the selected
preset, custom bindings, or manual mode.

Transient blockers remain internal library state:

- animation ignore;
- egui pointer/keyboard focus;
- inactive camera routing;
- unavailable owner camera.

No public enum should mix app-owned disabling with library-computed transient blockers.
Input is blocked if `OrbitCamInputDisabled` is present or any internal blocker is
active.

Blocking is enforced in the finalization system after all input writers have run.
This includes preset/custom action resolution and user systems in
`OrbitCamInputSet::WriteManual`. A blocked camera must not move, interrupt animation,
or keep guidance highlighted because of stale `OrbitCamInput`.

`BlockOnEguiFocus` should feed the internal UI-focus blocker. The blocker must preserve
current behavior:

- use `EguiWantsFocus::prev || EguiWantsFocus::curr` to avoid a one-frame leak;
- respect `EguiFocusIncludesHover`;
- collect egui focus state before input blocker computation;
- block adapter injection, action resolution, and finalized manual input;
- emit `CameraInteractionEnded` for active interactions before suppressing further input.

Keep the current egui scope unless a separate feature intentionally changes it:
any egui context wanting pointer or keyboard focus blocks cameras that opted into
`BlockOnEguiFocus`.

## Active Camera Routing

The default input path routes resolved user intent to one active camera. Automatic
routing hit-tests cursor or touch position against camera viewport rectangles and
camera order. Explicit routing remains available for render-to-texture and custom
editor layouts.

Rename routing to avoid overloading `Manual`:

```rust
pub enum CameraInputRouting {
    /// Choose the active camera from cursor/touch position and camera viewport rectangles.
    CursorHitTest,
    /// Use the camera entity supplied in `ActiveCameraData`.
    Explicit,
}
```

`CameraInputRouting::Explicit` is distinct from `OrbitCamControls::Manual`:

```text
CameraInputRouting::Explicit
  app chooses which camera receives input

OrbitCamControls::Manual
  app writes OrbitCamInput directly
```

Automatic routing should use an interaction owner latch:

```text
When a held camera interaction starts:
  latch the owning camera.

While any held interaction is active:
  route camera input to the latched owner,
  even if cursor/touch position crosses another viewport.

When all held interactions end:
  clear the owner and allow hit-testing again.
```

Impulse sources do not latch ownership because Bevy does not expose reliable
begin/end gesture phases for them:

```text
MouseWheel, pixel smooth-scroll, PinchGesture, PanGesture:
  route each event by its window and current pointer/touch position for that frame
  mark the matching interaction active only for that frame
  emit start/end lifecycle events for that frame if no held source remains active
```

Store the routing-derived viewport metrics for the selected camera alongside the
per-camera input context for the frame:

```rust
pub struct OrbitCamInputFrameContext {
    pub viewport_size: Option<Vec2>,
    pub window_size: Option<Vec2>,
}
```

Manual input, explicit routing, multi-window routing, and automatic cursor hit-testing
must all use the same per-camera metrics when converting screen-pixel deltas into
orbit or pan response.

Enhanced-input action evaluation must also be gated to the latched active camera.
Inactive `OrbitCamInputContext` instances must not accumulate action state. Prefer an
enhanced-input condition or context activation rule that prevents inactive camera
contexts from producing action transitions. If that is not available, reset inactive
camera action state deterministically.

If the owning camera becomes blocked, disabled, inactive, despawned, or otherwise
unavailable:

- emit `CameraInteractionEnded` for active interactions when possible;
- clear active interaction state;
- clear the owner.

## Scheduling

Add a root-level `system_sets` module and a dedicated plugin called by
`LagrangePlugin`.

```rust
mod system_sets;

pub(crate) struct LagrangeSystemSetsPlugin;
```

The module-level docs should include the ordering diagram because the system sets are
the integration contract between Bevy input, enhanced input, adapters, animation, and
the camera controller.

```text
PreUpdate:
  Bevy input has collected raw device state
    -> Reconcile changed OrbitCamControls and replace private input installations
    -> Route active camera and update internal input blockers
    -> Gate active camera context/action evaluation
    -> Inject Lagrange adapter values for unsupported sources
    -> bevy_enhanced_input updates action state
    -> Resolve camera actions and adapter contributions into OrbitCamInput
    -> User systems write manual OrbitCamInput
    -> Finalize input: clear blocked/stale input, update interaction tracker, emit lifecycle events

Update:
  animation_input_interrupt reads OrbitCamInput
    -> Ignore clears input and lets animation continue
    -> Cancel cancels animation and keeps input
    -> Complete finishes animation and clears input
  process_camera_move_list advances remaining animations

PostUpdate:
  OrbitCam controller reads OrbitCamInput
    -> Camera transform targets are updated
    -> OrbitCamInput is cleared
    -> Transform propagation
    -> Camera update systems
```

The exact enhanced-input set names should come from the dependency, but the ordering
shape should be:

```rust
app.configure_sets(
    PreUpdate,
    (
        OrbitCamInputSet::ReconcileControls,
        OrbitCamInputSet::Route,
        OrbitCamInputSet::InjectAdapters,
        OrbitCamInputSet::ResolveActions,
        OrbitCamInputSet::WriteManual,
        OrbitCamInputSet::FinalizeInput,
    )
        .chain(),
);

app.configure_sets(
    PreUpdate,
    (
        OrbitCamInputSet::ReconcileControls.after(InputSystems),
        OrbitCamInputSet::InjectAdapters.before(EnhancedInputSystems::Update),
        OrbitCamInputSet::ResolveActions.after(EnhancedInputSystems::Apply),
    ),
);

app.add_systems(
    PreUpdate,
    reconcile_orbit_cam_controls.in_set(OrbitCamInputSet::ReconcileControls),
);

app.add_systems(
    PreUpdate,
    (
        update_egui_focus_state,
        route_active_orbit_cam,
        update_orbit_cam_input_blockers,
    )
        .chain()
        .in_set(OrbitCamInputSet::Route),
);

app.add_systems(
    PreUpdate,
    inject_orbit_cam_adapter_values.in_set(OrbitCamInputSet::InjectAdapters),
);

app.add_systems(
    PreUpdate,
    resolve_orbit_cam_actions.in_set(OrbitCamInputSet::ResolveActions),
);

app.add_systems(
    PreUpdate,
    finalize_orbit_cam_input.in_set(OrbitCamInputSet::FinalizeInput),
);

app.add_systems(
    Update,
    (
        animation_input_interrupt,
        process_camera_move_list,
    )
        .chain(),
);

app.add_systems(
    PostUpdate,
    (
        orbit_cam,
        clear_orbit_cam_input,
    )
        .chain()
        .in_set(OrbitCamSystemSet)
        .before(TransformSystems::Propagate)
        .before(CameraUpdateSystems),
);
```

`WriteManual` is a public slot for user systems. `bevy_lagrange` does not normally add
systems to it:

```rust
app.add_systems(
    PreUpdate,
    my_manual_camera_input.in_set(OrbitCamInputSet::WriteManual),
);
```

Keep the public scheduling surface explicit but small:

```rust
pub enum OrbitCamInputSet {
    ReconcileControls,
    Route,
    InjectAdapters,
    ResolveActions,
    WriteManual,
    FinalizeInput,
}

pub struct OrbitCamSystemSet;
```

`ReconcileControls` handles structural replacement when `OrbitCamControls` changes.
`FinalizeInput` is the last semantic gate before any animation or controller system
can observe input. It clears blocked manual/preset/custom input, emits lifecycle
events, updates interaction state, and clears the owner latch when needed.

## Animation And Programmatic Motion

`AnimationConflictPolicy` and `CameraInputInterruptBehavior` remain separate policy
axes:

| Situation | Existing policy | Input behavior |
|-----------|-----------------|----------------|
| New programmatic animation arrives while another animation is active. | `AnimationConflictPolicy` | Does not inspect or modify input blockers. |
| User input arrives during animation and policy is `Ignore`. | `CameraInputInterruptBehavior::Ignore` | Clear `OrbitCamInput`; animation continues. |
| User input arrives during animation and policy is `Cancel`. | `CameraInputInterruptBehavior::Cancel` | Cancel/remove animation, emit existing cancelled events, keep `OrbitCamInput` so user control applies this frame. |
| User input arrives during animation and policy is `Complete`. | `CameraInputInterruptBehavior::Complete` | Complete/jump animation, emit existing completion events, clear `OrbitCamInput` for this frame. |

Resolved `OrbitCamInput` is the user-input interrupt signal. Animation interruption
should not depend on detecting later target mutation.

Programmatic camera operations do not write `OrbitCamInput` and do not emit camera
input lifecycle events. They continue to use existing events such as `ZoomToFit`,
`PlayAnimation`, `ZoomBegin`, `ZoomEnd`, `AnimationBegin`, and `AnimationEnd`.

## Examples

Each supported controls mode should have a small example named after the control type.
The controls examples should use `fairy_dust` so the camera window can show live
guidance text that reacts to `CameraInteractionStarted` and
`CameraInteractionEnded`.

Recommended new examples:

- `examples/controls_blender_like.rs`
- `examples/controls_simple_mouse.rs`
- `examples/controls_custom_keyboard.rs`
- `examples/controls_custom_gamepad.rs`
- `examples/controls_manual.rs`

Each controls example should:

- spawn one `OrbitCam`;
- install exactly one controls mode;
- show orbit, pan, and zoom guidance text in the camera view;
- highlight the relevant guidance text while the interaction is active;
- display or log the interaction source flags so mouse, wheel, smooth-scroll, pinch,
  touch, keyboard, gamepad, and manual paths can be verified.

`fairy_dust` needs a data-driven camera guidance panel that examples can configure
with rows. The panel should highlight active rows from lifecycle events and optionally
display source flags.

Conceptual API:

```rust
CameraGuidance::for_preset(OrbitCamControlPreset::BlenderLike)
CameraGuidance::for_preset(OrbitCamControlPreset::SimpleMouse)
CameraGuidance::custom([
    CameraGuidanceRow::new(CameraInteractionKind::Orbit, "Right stick"),
    CameraGuidanceRow::new(CameraInteractionKind::Pan, "Left stick + L2"),
    CameraGuidanceRow::new(CameraInteractionKind::Zoom, "R2 / L2"),
])
```

The guidance panel may visually retain highlights for a short presentation-friendly
duration after an impulse interaction ends. That linger belongs to the display layer;
the camera input lifecycle events stay deterministic.

The custom keyboard example should show the app-owned keymap pattern:

```rust
let bindings = editor_keymap.to_orbit_cam_bindings();
commands.entity(camera).insert(OrbitCamControls::Custom(bindings));
```

The custom gamepad example should start from one documented mapping:

```text
right stick -> orbit
left stick + left bumper -> pan
triggers -> smooth zoom
deadzone -> binding/modifier layer
```

The gamepad example should also explain controller-selection assumptions for
multi-controller apps and include a visible no-gamepad fallback.

### Legacy API Migration Table

| Existing API / behavior | New home |
|-------------------------|----------|
| `OrbitCam::input_control = None` used to stop user camera input temporarily | Add `OrbitCamInputDisabled` when the selected controls should be preserved; use `OrbitCamControls::Manual` only when the app takes over writing `OrbitCamInput`. |
| Default left/right mouse controls | `OrbitCamControls::Preset(OrbitCamControlPreset::SimpleMouse)`. |
| `TrackpadBehavior::ZoomOnly` | `OrbitCamWheelBinding::ZoomOnly`. |
| `TrackpadBehavior::BlenderLike` | `OrbitCamWheelBinding::BlenderLike` through preset or custom bindings. |
| `modifier_pan: None` / `modifier_zoom: None` in Blender-like trackpad config | `WheelModifier::Always`, represented through builder APIs that reject ambiguous combinations. |
| `ZoomDirection::Reversed` | `OrbitCamBindings::zoom_direction(ZoomDirection::Reversed)` or equivalent response config, applied uniformly to every user-input zoom source. |
| `button_zoom` | `ButtonDragZoomBinding`. |
| `ButtonZoomAxis::{X, Y, XY}` | `ButtonDragZoomAxis::{X, Y, XY}`. |
| `TouchInput::OneFingerOrbit` / `TwoFingerOrbit` | Touch adapter policy inside `OrbitCamBindings`. |
| Keyboard control examples that mutate targets directly | `OrbitCamControls::Custom(OrbitCamBindings)` for user input, or existing programmatic camera APIs for non-user camera motion. |
| Manual active-camera resource setup for render-to-texture | `CameraInputRouting::Explicit` plus per-camera frame metrics. |

### Example Migration Notes

- `basic.rs` should remain the smallest working camera example. It should use
  `LagrangePlugin + OrbitCam::default()` to demonstrate the zero-config default,
  which resolves to the opinionated preset controls.
- `advanced.rs` should be renamed to `custom_bindings.rs`. It should demonstrate
  `OrbitCamControls::Custom(OrbitCamBindings)` with custom action bindings plus
  custom wheel, pinch, and touch adapter policy.
- `keyboard_controls.rs` should be retired. Keyboard-as-user-input should be shown
  through `custom_bindings.rs` or a focused custom controls example, while
  programmatic camera movement is covered by zoom, look, fit, and animation examples.
- `egui.rs` should remain the focused UI integration example. It should pair a normal
  controls preset with `BlockOnEguiFocus` and demonstrate that egui pointer/keyboard
  focus blocks camera interactions without replacing the selected controls.
- `pausing.rs` should remain the `TimeSource::Real` example. It should demonstrate
  keeping camera smoothing responsive while virtual time is paused. Migrate it by
  replacing raw `input_control` setup with the default preset or an explicit
  `OrbitCamControls::Preset(OrbitCamControlPreset::BlenderLike)`.
- `render_to_texture.rs` should remain the explicit active-camera routing example.
  It should demonstrate controlling a camera that renders to an image rather than a
  window viewport, so automatic cursor hit-testing cannot choose it. Migrate
  `CameraInputDetection::{Automatic, Manual}` to
  `CameraInputRouting::{CursorHitTest, Explicit}` with doc comments explaining that
  `CursorHitTest` chooses from cursor/touch position and camera viewport rectangles,
  while `Explicit` uses the camera entity supplied in `ActiveCameraData`.
- `viewports_windows.rs` should remain the automatic multi-window/multi-viewport
  routing example. Its code comments should explain cursor/touch hit-testing, camera
  order, and the interaction owner latch that keeps held interactions attached to the
  camera where they started until the held source ends. It should also demonstrate
  that wheel, smooth-scroll, and pinch impulses route deterministically per event.
- `animation.rs` and the showcase animation controls should remain the animation
  policy examples. They should demonstrate `CameraInputInterruptBehavior::{Ignore,
  Cancel, Complete}` and `AnimationConflictPolicy`, with resolved `OrbitCamInput`
  acting as the user-input interrupt signal.
- `zoom_to_fit` should remain the programmatic camera event example. It should keep
  teaching `ZoomToFit`, `ZoomBegin`, `ZoomEnd`, and related animation events as
  separate from user-input interaction lifecycle events.
  As cleanup, collapse the current `zoom_to_fit/main.rs` plus `constants.rs`
  directory example back into a single `zoom_to_fit.rs` file with its constants
  integrated. Single-file examples no longer need a separate constants module.
- `follow_target.rs`, `focus_bounds.rs`, `orthographic.rs`, and `swapped_axis.rs`
  should remain camera behavior examples rather than input examples. They should use
  the default controls unless the demonstrated camera behavior specifically requires
  a different preset.
- `controls_blender_like.rs` should show the Blender-like preset with `fairy_dust`
  guidance text that highlights orbit, pan, and zoom rows from camera interaction
  lifecycle events.
- `controls_simple_mouse.rs` should show the simpler mouse-oriented preset and make
  its differences from Blender-like controls visible in the guidance text.
- `controls_custom_keyboard.rs` should show keyboard controls through
  `OrbitCamControls::Custom(OrbitCamBindings)`, not by mutating camera targets
  directly.
- `controls_custom_gamepad.rs` should show gamepad axes/buttons through
  `OrbitCamControls::Custom(OrbitCamBindings)`, including deadzone/axis guidance and
  a visible no-gamepad fallback.
- `controls_manual.rs` should show direct `OrbitCamInput` writes through helper
  methods and typed deltas, with `CameraInteractionSources::MANUAL`.

## Testing Strategy

Prefer ECS-only tests for the input refactor. Most behavior can be validated with an
`App`, the input systems/plugins, spawned camera entities, synthetic input messages,
and event/message readers. Avoid requiring renderer or GPU setup unless a test
specifically covers rendered output.

Core ECS-only tests:

- default `OrbitCam` receives `OrbitCamControls::Preset(BlenderLike)` through the
  required component path;
- `Preset -> Manual` despawns related `OrbitCamInputEntities` and installs no new
  library-owned input entities;
- `Preset -> Custom` replaces old related entities rather than accumulating bindings;
- replacing controls during an active interaction emits `CameraInteractionEnded` and
  clears stale `OrbitCamInput`;
- `OrbitCamInputDisabled`, egui focus blockers, inactive routing, and animation ignore
  clear manual and preset/custom input before animation or controller systems observe it;
- systems in `OrbitCamInputSet::WriteManual` are visible to `FinalizeInput` in the
  same frame;
- manual zero-delta active helpers emit started/ended lifecycle events correctly;
- held sources latch the owner until release, while impulse wheel/pinch/smooth-scroll
  events route independently per event;
- per-camera viewport metrics are used for orbit and pan scaling under explicit and
  cursor-hit-test routing;
- adapter/public-binding conflicts are rejected or reported by `OrbitCamBindings`
  validation;
- `ZoomDirection::Reversed` applies uniformly to coarse wheel, smooth scroll, pinch,
  touch pinch, button-drag zoom, keyboard, and gamepad zoom paths;
- touch pinch applies `touch_pinch_scale` before the shared zoom response path;
- egui click/drag focus tests preserve the current `prev || curr` leak prevention;
- dependency validation confirms `bevy_lagrange` uses the workspace-pinned
  `bevy_enhanced_input` and `bitflags` versions without duplicate direct versions.

## Migration Plan

1. Add workspace-pinned `bevy_enhanced_input` as a normal `bevy_lagrange` dependency.
2. Add `bitflags = { workspace = true }` as a direct `bevy_lagrange` dependency.
3. Add the public `input` module with actions, context, controls, bindings, intent, disabled input, and interaction events.
4. Add `OrbitCamInput`, typed deltas, active-source fields, and helper methods for manual input.
5. Add `OrbitCamInputContext` as a required component on `OrbitCam` and register it in `LagrangePlugin`.
6. Add `OrbitCamControls::{Preset, Custom(OrbitCamBindings), Manual}`.
7. Add `OrbitCamBindings`, private fields, local builder/spec types, typestate wheel ownership, and runtime validation.
8. Add `ZoomDirection`, `ButtonDragZoomBinding`, touch policy, pinch policy, and wheel policy as binding/adapter configuration.
9. Add the private `OrbitCamInputEntityOf` / `OrbitCamInputEntities` relationship and control reconciliation.
10. Add the private adapter module for wheel units, smooth scroll, pinch, and touch.
11. Add source-aware interaction tracking and `CameraInteractionStarted` / `CameraInteractionEnded`.
12. Replace public runtime gating with `OrbitCamInputDisabled` plus internal transient blockers.
13. Rename `CameraInputDetection` to `CameraInputRouting` with `CursorHitTest` and `Explicit`.
14. Implement held-source owner latching, per-event impulse routing, per-camera frame metrics, and inactive-context gating.
15. Add the root-level `system_sets` module and `LagrangeSystemSetsPlugin` with `ReconcileControls`, `WriteManual`, and `FinalizeInput`.
16. Add `animation_input_interrupt` and use finalized `OrbitCamInput` as the user-input interrupt signal.
17. Move physical binding fields off `OrbitCam` into presets and adapter configuration.
18. Update egui blocking to feed internal UI-focus blockers before finalization.
19. Add the `fairy_dust` camera guidance panel needed by the controls examples.
20. Add the controls examples with `fairy_dust` visual feedback.
21. Migrate existing examples according to the example migration notes.
22. Add ECS-only tests for scheduling, reconciliation, routing, blockers, lifecycle events, legacy behavior preservation, and dependency versioning.
23. Deprecate and then remove the legacy raw-input fields after examples and downstream code have moved.

## Final Architecture

```text
Preset controls
  -> OrbitCamControlPreset
      -> OrbitCamBindings
          -> private input installation relationship
              -> public enhanced-input actions + private adapter policy
                  -> OrbitCamInput
                      -> FinalizeInput
                          -> OrbitCam controller

Custom controls
  -> OrbitCamBindings supplied by the app
      -> private input installation relationship
          -> public enhanced-input actions + private adapter policy
              -> OrbitCamInput
                  -> FinalizeInput
                      -> OrbitCam controller

Manual controls
  -> app writes OrbitCamInput through helper methods in OrbitCamInputSet::WriteManual
      -> FinalizeInput
          -> OrbitCam controller

Programmatic camera operations
  -> OrbitCam state, targets, or animation queues
      -> OrbitCam controller
```

The default path is action-centered. The adapter keeps today's richer wheel,
smooth-scroll, pinch, and touch behavior without making a second public input model.
Manual users can bypass enhanced input for a camera by writing `OrbitCamInput`, but
presets and custom controls keep camera input inside the same action/context
architecture used by the rest of the app.

## Future Cleanup

### Roll

Roll is a natural future camera interaction because platform gesture systems can
produce rotation gestures. Bevy already exposes `RotationGesture`, and the current
touch tracker computes two-finger rotation even though the controller does not use it.

Roll should not be added as part of the initial input refactor. It requires extending
the camera behavior model, not just adding another input action.

Candidate future additions:

- `Roll` semantic action.
- `CameraInteractionKind::Roll`.
- `OrbitCamInput::roll`.
- `roll` and `target_roll` camera state.
- `roll_lower_limit` and `roll_upper_limit`.
- `roll_sensitivity` and `roll_smoothness`.

`CameraInteractionKind` should be non-exhaustive so `Roll` can be added later without
forcing downstream exhaustive matches to break.

### Angle State

Adding roll would create another set of parallel angle fields. Before adding those
fields, consider grouping angle state into a reusable type.

```rust
pub struct OrbitAngle {
    pub current: Option<f32>,
    pub target: f32,
    pub limits: AngleLimits,
}

pub struct AngleLimits {
    pub lower: Option<f32>,
    pub upper: Option<f32>,
}
```

Then `OrbitCam` could carry:

```rust
pub yaw: OrbitAngle,
pub pitch: OrbitAngle,
pub roll: OrbitAngle,
```

This would make yaw, pitch, and future roll state easier to document and harder to
update inconsistently. It is a camera-state cleanup, not a prerequisite for the input
refactor.
