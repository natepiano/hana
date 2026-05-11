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
- `LagrangePlugin` installs the enhanced-input plugin it depends on before registering
  camera input contexts, so apps do not need a second hidden setup step for camera
  input.

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
    state.rs               // public read-only interaction state
    routing.rs             // public routing config and logical surface metrics
    intent.rs              // public OrbitCamInput and typed deltas
    manual.rs              // public manual writer helper/query pattern
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
//! Start here:
//!
//! - Use [`OrbitCamControls::Preset`] when you want a built-in camera keymap.
//! - Use [`OrbitCamControls::Custom`] when your app has a keymap or gamepad binding UI.
//! - Use [`OrbitCamControls::Manual`] when your app wants to compute camera intent itself.
//!
//! ```rust
//! commands.spawn((Camera3d::default(), OrbitCam::default()));
//! ```
//!
//! ```rust
//! commands.spawn((
//!     Camera3d::default(),
//!     OrbitCam::default(),
//!     OrbitCamControls::Custom(my_keymap.to_orbit_cam_bindings()),
//! ));
//! ```
//!
//! ```rust
//! app.add_systems(
//!     PreUpdate,
//!     write_manual_camera_input.in_set(OrbitCamInputSet::WriteManual),
//! );
//! ```
//!
//! Preset and custom controls are resolved through `bevy_enhanced_input`.
//! Manual controls bypass enhanced input for that camera.
//!
//! Adapter-backed sources such as wheel-unit, pinch, touch, and smooth-scroll
//! policy are configured through [`OrbitCamBindings`], not through private
//! adapter actions.
//!
//! System-set and adapter details are lower-level integration points. Most users
//! should start with controls, bindings, and interaction events.
```

The public facade should re-export the semantic API from both `input` and the crate
root for convenience:

```rust
pub use input::{
    CameraInteractionEnded,
    CameraInteractionKind,
    CameraInteractionSources,
    CameraInteractionSourcesChanged,
    CameraInteractionStarted,
    CameraInputRouting,
    CameraInputRoutingConfig,
    CameraInputSurfaceMetrics,
    Orbit,
    OrbitCamBindings,
    OrbitCamControlPreset,
    OrbitCamControls,
    OrbitCamInteractionState,
    OrbitCamInput,
    OrbitCamInputContext,
    OrbitCamInputDisabled,
    OrbitCamInputSet,
    OrbitEngaged,
    ManualOrbitCamInput,
    ManualInputSource,
    Pan,
    PanEngaged,
    OrbitDelta,
    PanDelta,
    CoarseZoomDelta,
    SmoothZoomDelta,
    ZoomCoarse,
    ZoomEngaged,
    ZoomSmooth,
};
```

Do not re-export private source actions such as `OrbitFromSmoothScroll`,
`ZoomFromPinch`, or `TouchPan`.

Each public type in `bevy_lagrange::input` should carry a short rustdoc example for
its normal use. Keep the quick-start path at the top of `input/mod.rs`; put system-set
ordering, adapter internals, and validation details below the user-facing controls
overview.

## Camera Behavior

`OrbitCam` remains the camera behavior component. It owns:

- focus, yaw, pitch, radius, and targets;
- sensitivity and smoothing;
- bounds and clamping;
- upside-down behavior;
- animation behavior;
- time source;
- transform update behavior.

After this refactor, `OrbitCam` should not contain physical binding fields such as
mouse buttons, keyboard modifiers, touch behavior, trackpad behavior, or zoom
direction. Those belong to controls, bindings, adapter policy, or response
configuration.

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
app.add_plugins(EnhancedInputPlugin);
app.add_input_context::<OrbitCamInputContext>();
```

The plugin should own this setup. A minimal app that adds only `LagrangePlugin` should
have all enhanced-input resources and systems required by `OrbitCamInputContext`.

Add diagnostics for missing setup:

- `LagrangePlugin` should run a first-frame diagnostic that confirms enhanced input is
  installed and camera input contexts are registered.
- `OrbitCam` should have an `on_add` hook or equivalent one-time diagnostic path that
  warns when an `OrbitCam` exists but `LagrangePlugin` has not installed the input
  pipeline. The warning should say that camera input will not resolve until
  `LagrangePlugin` is added.

## Controls And Bindings

`OrbitCamControls` selects who owns user-input resolution for a camera.

```rust
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
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

Keep `OrbitCamControls::Custom(OrbitCamBindings)` as one variant rather than splitting
mode and bindings into separate components. That makes the invalid state
"custom mode without custom bindings" unrepresentable through the ordinary spawn API.

All public components and resources introduced by this refactor should derive
`Reflect` and register their reflected types. Because `OrbitCamControls` carries
`OrbitCamBindings`, the public binding spec must also be reflectable. Prefer
Lagrange-owned, reflectable binding recipes over storing arbitrary closures or opaque
trait objects in components/resources. If an advanced escape hatch cannot be reflected
honestly, keep it out of public component/resource state until it has a reflectable
descriptor or validation story.

If an `OrbitCam` has no explicit controls component, the required component default
should be `OrbitCamControls::Preset(OrbitCamControlPreset::SimpleMouse)`. This is the
most likely default for users who expect a mouse-oriented camera controller. Use
`BlenderLike` explicitly for editor-style workflows that want Blender's middle-mouse
orbit convention and trackpad behavior.

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
#[derive(Debug, Reflect)]
pub struct OrbitCamBindings {
    orbit: OrbitBindings,
    pan: PanBindings,
    zoom_smooth: SmoothZoomBindings,
    zoom_coarse: CoarseZoomBindings,
    wheel: OrbitCamWheelBinding,
    pinch: OrbitCamPinchBinding,
    touch: Option<TouchInput>,
    gamepad: GamepadSelectionPolicy,
    zoom_direction: ZoomDirection,
    button_drag_zoom: Option<ButtonDragZoomBinding>,
}

pub struct OrbitBindings(ActionBindingSet<Orbit>);
pub struct PanBindings(ActionBindingSet<Pan>);
pub struct SmoothZoomBindings(ActionBindingSet<ZoomSmooth>);
pub struct CoarseZoomBindings(ActionBindingSet<ZoomCoarse>);

pub struct ActionBindingSet<A: InputAction> {
    entries: Vec<ActionBindingEntry<A>>,
}

pub struct ActionBindingEntry<A: InputAction> {
    binding: BindingRecipe<A>,
    sources: CameraInteractionSources,
    route: BindingRoutePolicy,
    engagement: BindingEngagement,
}
```

Each action binding entry is typed by semantic action, not just output value. This
keeps invalid combinations such as pan bindings accidentally installed as orbit
bindings out of the ordinary API even though both actions output `Vec2`.

Each action binding entry also carries source metadata. A single semantic action can
have multiple entries, such as keyboard plus gamepad zoom. Resolve active sources from
the entries that actually triggered in the current frame, not from a broad
action-level union.

The implementation must preserve per-entry source attribution. It may do this by
installing one Lagrange-owned action instance per binding entry and attaching private
metadata to that installed entry:

```rust
pub(crate) struct InstalledCameraBinding {
    semantic_action: CameraSemanticAction,
    sources: CameraInteractionSources,
    route: BindingRoutePolicy,
    engagement: BindingEngagement,
}
```

The resolver reads each installed entry's per-frame action state and unions only the
sources whose entry actually triggered. Do not infer active sources from the final
merged enhanced-input action value, and do not report the union of every source that
could have triggered.

Source flags should be assigned at construction time:

```rust
OrbitActionBindingSpec::mouse_drag(MouseButton::Middle)      // MOUSE
OrbitActionBindingSpec::gamepad_axis(GamepadAxis::RightStick) // GAMEPAD
ZoomActionBindingSpec::keyboard_keys(KeyCode::Equal, KeyCode::Minus) // KEYBOARD
```

If the API exposes a low-level enhanced-input escape hatch, it must require an
explicit `CameraInteractionSources` argument:

```rust
ActionBindingSpec::from_enhanced_input(binding, sources)
```

This avoids inferring source flags from enhanced-input internals after the fact and
keeps lifecycle events useful for tooling.

Raw enhanced-input bindings added directly to the public semantic actions are not a
complete camera-resolution API unless they also carry Lagrange source metadata and
routing policy. The documented low-level escape hatch should therefore build
metadata-bearing binding specs or bundles rather than asking users to attach raw
bindings to camera action entities by hand.

Held controls should be modeled as one irreducible source-aware entry that installs
both movement and engagement state. Do not let motion and engagement drift into
unrelated custom bindings:

```rust
pub struct HeldActionBindingEntry<A: InputAction> {
    motion: BindingRecipe<Vec2>,
    engaged: BindingRecipe<bool>,
    sources: CameraInteractionSources,
}

impl<A: InputAction> HeldActionBindingEntry<A> {
    pub fn try_new(
        motion: BindingRecipe<Vec2>,
        engaged: BindingRecipe<bool>,
        sources: CameraInteractionSources,
    ) -> Result<Self, HeldActionBindingError>;

    pub fn motion(&self) -> &BindingRecipe<Vec2>;
    pub fn engagement(&self) -> &BindingRecipe<bool>;
}
```

The builder should construct that pair together and validate that paired motion and
engagement bindings have compatible sources, activation predicates, and route policy.
Do not expose public fields or unchecked constructors for held entries. Reflection,
deserialization, or dynamic keymap loading must go through the same validation path
before a held binding can be installed.

Keep the enhanced-input actions independent, but make the bindings API pair them.
This is necessary because held camera interactions often have motion and engagement
from different physical inputs:

```text
Orbit        <- MouseMotion
OrbitEngaged <- MouseButton::Middle
```

Advanced users who use the low-level escape hatch must still install held motion and
engagement through a metadata-bearing `HeldActionBindingEntry`; wiring `Orbit` and
`OrbitEngaged` separately is unsupported for library-resolved camera input.
Impulse bindings such as wheel, pinch, and smooth-scroll do not have a held phase and
must not bind `OrbitEngaged`, `PanEngaged`, or `ZoomEngaged`.

The actual type should prefer constructors and builders over public fields. Required
choices should use typestate builders where practical so invalid custom binding states
are not representable through ordinary Rust APIs. Runtime construction paths such as
reflection, deserialization, or future dynamic keymap loading still need validation.
Expose that validation as `try_build` or an equivalent checked constructor, and make
the resolver reject or clearly warn on adapter/public-binding conflicts.

The custom binding API should expose both high-level camera constructors and a
deliberate low-level path for advanced enhanced-input usage. Advanced users need a way
to attach enhanced-input modifiers and conditions such as deadzones, axis transforms,
chords, and app-specific predicates without bypassing source metadata or adapter
conflict validation.

Gamepad ownership is part of binding policy. Custom gamepad bindings should make
controller selection explicit:

```rust
pub enum GamepadSelectionPolicy {
    Any,
    Selected(Entity),
    Disabled,
}
```

Document how disconnected selected gamepads are handled. The default custom gamepad
example should use a selected gamepad when one is available, show a no-gamepad
fallback, and avoid accidentally letting every connected controller drive the camera.

Wheel policy needs a typestate builder, or an equivalent compile-time constrained API,
so custom users must intentionally choose adapter-owned wheel behavior or disabled
wheel behavior. Preset/custom controls should not expose raw `MouseWheel` binding
helpers.

The builder should make wheel policy compile-time mandatory:

```rust
OrbitCamBindings::builder()              // OrbitCamBindingsBuilder<WheelUnset>
    .orbit_drag(MouseButton::Middle)
    .wheel(OrbitCamWheelBinding::Disabled) // OrbitCamBindingsBuilder<WheelSet>
    .build()
```

`OrbitCamBindingsBuilder<WheelUnset>` should not expose `build`. Runtime construction
paths that cannot use typestate, such as reflection or dynamic keymap loading, must
use `try_build`.

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

### Binding Validation

`OrbitCamBindings` construction should have one strict validation path:

```rust
pub enum OrbitCamBindingsError {
    AdapterBindingConflict { source: CameraInteractionSources, action: CameraSemanticAction },
    HeldBindingWithoutEngagement { action: CameraSemanticAction },
    EngagementBindingForImpulse { action: CameraSemanticAction },
    HeldBindingSourceMismatch { action: CameraSemanticAction },
    AmbiguousWheelPolicy,
    MissingWheelPolicy,
}
```

`try_build` returns `Result<OrbitCamBindings, OrbitCamBindingsError>`. Convenience
`build` may panic with the same structured message, but examples should use the
non-panicking path where user keymaps are loaded dynamically. Runtime reconciliation
should re-check custom bindings on `Changed<OrbitCamControls>` and log a clear error
if reflection-loaded or dynamically generated bindings violate the same rules.

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

Use a custom relationship rather than `ChildOf` even though `ChildOf` can also provide
despawn cleanup. These entities are semantic input-installation entities, not scene or
UI hierarchy children. The custom relationship gives reconciliation a precise query
for "all private input entities owned by this camera" without mixing them with any
other child entities an app may attach to the camera.

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
They are part of the public semver surface and should be re-exported. Their entity
installation, private adapter source actions, and relationship wiring remain internal.
Most users should configure them through `OrbitCamBindings`, but advanced users may
need to name the semantic action types when integrating with enhanced-input tooling.

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
/// Low-level held-phase action consumed by the camera resolver.
///
/// Prefer [`OrbitCamBindings`] held-orbit constructors rather than binding this
/// action directly. The binding builder pairs orbit motion and engagement with
/// source metadata so latching and lifecycle events stay correct.
pub struct OrbitEngaged;

#[derive(InputAction)]
#[action_output(bool)]
/// Low-level held-phase action consumed by the camera resolver.
///
/// Prefer [`OrbitCamBindings`] held-pan constructors rather than binding this action
/// directly. The binding builder pairs pan motion and engagement with source metadata
/// so latching and lifecycle events stay correct.
pub struct PanEngaged;

#[derive(InputAction)]
#[action_output(bool)]
/// Low-level held-phase action consumed by the camera resolver.
///
/// Prefer [`OrbitCamBindings`] held-zoom constructors rather than binding this action
/// directly. The binding builder pairs zoom motion and engagement with source
/// metadata so latching and lifecycle events stay correct.
pub struct ZoomEngaged;
```

`OrbitEngaged` exists because orbit motion and orbit interaction state are different
facts:

- `Orbit` is how much to rotate this frame.
- `OrbitEngaged` is whether the user's current control scheme is actively orbiting.

The controller needs the engagement edge to preserve the current orbit-drag latch,
including upside-down yaw behavior. A user can press the orbit control and hold still;
the motion delta is zero, but the interaction has still started.

Pan and zoom engagement are also semantic actions because held pan and held zoom can
be active with zero delta. Button-held pan and button-drag zoom must not infer
interaction phase only from movement. The resolver and adapter should derive
interaction state from action timing and source state for all interaction kinds.

The stable controller-facing representation is per-kind active sources in
`OrbitCamInput`, not the presence of a nonzero delta. Lifecycle events should be
derived from resolved active-source sets:

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

The binding builder should prevent held pan, orbit, or zoom bindings that can produce
motion but cannot report active state. Impulse bindings are separate: wheel, smooth
scroll, pinch, and gesture deltas are active only for the frame in which the event is
resolved.

Any orbit interaction start must update the controller's orbit-orientation latch
before applying orbit delta. This applies to held drag starts, impulse-only smooth
scroll orbit, and manual `orbit(...)` calls. Held orbit sources keep the latch until
release; impulse orbit sources sample the current camera orientation for that frame.

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

`OrbitCam::default()` should resolve to the mouse-oriented `SimpleMouse` preset.
`BlenderLike` remains the opinionated editor preset, but it should be explicit at the
spawn site so readers can see when a camera uses Blender-style controls.

### Wheel And Smooth Scroll

`OrbitCamWheelBinding` should make wheel and smooth-scroll policy explicit:

```rust
pub enum OrbitCamWheelBinding {
    Disabled,
    ZoomOnly,
    PlatformNatural,
    BlenderLike(BlenderLikeWheelBinding),
}

pub struct BlenderLikeWheelBinding {
    pan_modifier: WheelModifier,
    zoom_modifier: WheelModifier,
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
Do not expose public data variants or public fields that bypass those constructors;
validated policies should be opaque anywhere builder invariants matter.

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
| `PinchGesture` | enabled pinch policy | any configured non-pinch camera modifier or held camera action active | ignored |

Pinch should keep the current conservative behavior where it is suppressed while any
configured non-pinch camera modifier or held camera action is active. This includes
ordinary orbit/pan modifiers as well as Blender-like trackpad pan/zoom modifiers.
Suppression is scoped to the camera receiving the pinch event. It is based on that
camera's resolved action/modifier state, not global raw key state. A modifier or held
action on a non-routed camera must not suppress pinch for the routed camera.

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

Adapter injection must be visible to enhanced input in the same frame. The schedule
must enforce this structurally, not as an implementation note. Prefer direct mutation
of existing action/mock components; if adapter injection uses `Commands` to insert or
update mock state, the `InjectAdapters` set must include an `apply_deferred` barrier
before `EnhancedInputSystems::Update`.

Camera actions should not consume app input by default. Set camera action/binding
consumption so app-owned enhanced-input contexts can still observe shared buttons,
motion, wheel, keyboard, and gamepad input. If a consuming camera binding is ever
needed, expose that as explicit binding policy along with context priority controls
and tests that cover an app context and camera context sharing the same binding.

Preset and custom controls should route wheel, pinch, touch, and smooth-scroll policy
through `OrbitCamBindings`. Users should not configure private adapter actions.

For any raw source handled by the adapter, the binding API should prevent or reject
equivalent public enhanced-input bindings in preset/custom modes. This prevents the
same physical event from being counted twice.

Adapter-backed policy types should expose modest builder hooks for advanced apps
without making private adapter actions public. The hooks should stay Lagrange-shaped:
they attach source metadata, validation, and adapter conflict checks automatically.

Examples:

```rust
OrbitCamPinchBinding::enabled()
    .with_deadzone(0.02)
    .with_condition(EditorViewportFocused);

OrbitCamWheelBinding::blender_like()
    .with_smooth_scroll_condition(BrushToolInactive);

TouchInput::one_finger_orbit()
    .with_condition(TouchViewportFocused);
```

These hooks should support common modifiers and conditions such as deadzones,
scale/sensitivity transforms, viewport-focus predicates, tool-mode predicates, and
custom app predicates. They should not require users to bind private adapter actions
directly.

## Camera Intent And Manual Input

`OrbitCamInput` is a per-camera frame snapshot. The controller reads it, applies camera
behavior, and the input pipeline clears or overwrites it each frame.

The snapshot stores movement deltas and active source sets separately. A helper call
marks an interaction active for that frame even if the delta is zero. This lets manual
and resolved controls represent "held but still" input without touching raw fields.

`OrbitCamInput` should expose read-only accessors to app code. Its fields should be
private or `pub(crate)`, and all mutation APIs should be `pub(crate)` except for the
manual writer. App systems can still query `&mut OrbitCamInput`, because it is a Bevy
component, but that mutable reference should not expose useful public setters or
fields. Library systems may use `pub(crate)` mutation APIs, while app-owned manual
writes go through `ManualOrbitCamInput`.

Manual users should not normally set value, source, and phase fields directly. The
public manual writer API should be method-based:

```rust
/// Source metadata for app-authored manual camera input.
///
/// Manual input always includes [`CameraInteractionSources::MANUAL`] because it
/// bypasses the Lagrange/enhanced-input camera resolver. The observed-device
/// constructors let apps preserve useful provenance when their manual system was
/// driven by keyboard, mouse, gamepad, touch, or another Bevy input source.
pub struct ManualInputSource(CameraInteractionSources);

// ManualInputSource should not derive Reflect and should not expose raw bit
// construction. It is only constructed through these methods so the MANUAL bit
// cannot be dropped by reflection or deserialization.
impl ManualInputSource {
    pub const fn manual() -> Self;
    pub const fn observed_mouse() -> Self;
    pub const fn observed_keyboard() -> Self;
    pub const fn observed_wheel() -> Self;
    pub const fn observed_smooth_scroll() -> Self;
    pub const fn observed_pinch() -> Self;
    pub const fn observed_touch() -> Self;
    pub const fn observed_gamepad() -> Self;

    pub const fn with_observed_mouse(self) -> Self;
    pub const fn with_observed_keyboard(self) -> Self;
    pub const fn with_observed_gamepad(self) -> Self;
}

impl ManualOrbitCamInputWriter<'_> {
    pub fn orbit_pixels(&mut self, x: f32, y: f32);
    pub fn pan_pixels(&mut self, x: f32, y: f32);
    pub fn zoom_coarse_amount(&mut self, amount: f32);
    pub fn zoom_smooth_amount(&mut self, amount: f32);

    pub fn orbit(
        &mut self,
        delta: impl Into<OrbitDelta>,
        source: ManualInputSource,
    );

    pub fn pan(
        &mut self,
        delta: impl Into<PanDelta>,
        source: ManualInputSource,
    );

    pub fn zoom_coarse(
        &mut self,
        delta: impl Into<CoarseZoomDelta>,
        source: ManualInputSource,
    );

    pub fn zoom_smooth(
        &mut self,
        delta: impl Into<SmoothZoomDelta>,
        source: ManualInputSource,
    );

    pub fn orbit_active(&mut self, source: ManualInputSource);
    pub fn pan_active(&mut self, source: ManualInputSource);
    pub fn zoom_active(&mut self, source: ManualInputSource);
}
```

The shorthand methods default to `ManualInputSource::manual()`. Use the explicit
methods when the app wants to preserve observed-device provenance such as
`MANUAL | KEYBOARD`.

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

Manual users provide value and manual source metadata. The library derives
interaction started/ended events from frame-to-frame active source sets. `orbit`,
`pan`, `zoom_coarse`, and `zoom_smooth` all mark the corresponding interaction active
for the frame. The `*_active` helpers exist for held controls that have no movement
this frame.

`ManualInputSource` always includes `CameraInteractionSources::MANUAL`. Observed
device constructors add source detail without losing provenance:

```text
ManualInputSource::manual()                 -> MANUAL
ManualInputSource::observed_keyboard()      -> MANUAL | KEYBOARD
ManualInputSource::observed_gamepad()       -> MANUAL | GAMEPAD
ManualInputSource::observed_smooth_scroll() -> MANUAL | SMOOTH_SCROLL
```

Manual writers should run in `OrbitCamInputSet::WriteManual`. The finalization system
runs after that set, clears blocked or stale input, emits lifecycle events, and then
hands finalized input to animation and controller systems.

Manual writes are valid only for cameras whose controls are
`OrbitCamControls::Manual`. Provide a public helper/query pattern that exposes only
manual cameras, and use it in examples:

```rust
fn manual_camera_input(mut cameras: ManualOrbitCamInput) {
    for mut camera in cameras.iter_mut() {
        camera.orbit_pixels(-4.0, 0.0);

        camera.pan(
            PanDelta::screen_pixels(0.0, 2.0),
            ManualInputSource::observed_keyboard(),
        );
    }
}
```

Manual mode bypasses automatic active-camera routing because the app has chosen to
write a specific camera's input directly. It still respects `OrbitCamInputDisabled`,
`BlockOnEguiFocus` when present, animation ignore blockers, and other finalization
rules. Preset/custom cameras should not be mutated by app systems in `WriteManual`;
debug builds should warn if a manual writer helper detects an attempted write to a
non-manual camera.

Manual screen-pixel orbit and pan deltas require logical surface metrics. In ordinary
window and viewport cases, `bevy_lagrange` should derive those metrics
programmatically from the camera render target, logical viewport, and window. Manual
users only need an explicit surface-metrics override for render-to-texture, offscreen
images, or custom editor surfaces whose input coordinate space is not the camera's
normal window viewport. If metrics cannot be derived or overridden, screen-pixel
manual input should warn and drop rather than guessing.

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

`CameraInteractionSources` should be the public reflected newtype. Keep raw bitflags
internal or behind conversions:

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect)]
pub struct CameraInteractionSources {
    bits: u32,
}
```

Expose associated constants, `contains`, `intersects`, and conversions to/from the
internal bitflags representation. The type must support the reflection traits needed
by the public reflected interaction events.

Define unknown-bit behavior explicitly. Public constructors should reject unknown
bits:

```rust
impl CameraInteractionSources {
    pub const fn from_bits(bits: u32) -> Option<Self>;
    pub const fn bits(self) -> u32;
}
```

Do not expose a public `from_bits_truncate`. Reflection/deserialization should
validate source bits rather than silently creating source sets no constructor could
produce.

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

#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct CameraInteractionSourcesChanged {
    #[event_target]
    pub camera: Entity,
    pub kind: CameraInteractionKind,
    pub previous_sources: CameraInteractionSources,
    pub current_sources: CameraInteractionSources,
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
Instead, emit `CameraInteractionSourcesChanged` whenever the active source set changes
without starting or ending the interaction as a whole.

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
For an impulse-only interaction, finalization emits `CameraInteractionStarted` and
`CameraInteractionEnded` in the same frame. The impulse exists only for that input
frame; it must not keep the semantic active-source set alive into the next frame just
so the event tracker can observe an empty transition later.

Concrete wheel trace:

```text
frame N:
  resolved zoom_active_sources = WHEEL
  previous zoom sources = empty
  emit CameraInteractionStarted { kind: Zoom, sources: WHEEL }
  controller may consume the zoom delta
  emit CameraInteractionEnded { kind: Zoom, sources: WHEEL }
  stored previous zoom sources for frame N+1 = empty

frame N+1:
  no wheel event
  resolved zoom_active_sources = empty
  no lifecycle event
```

Expose the current active interaction state as a read-only component so editor tools
and examples do not have to reconstruct state from events:

```rust
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct OrbitCamInteractionState {
    pub orbit_sources: CameraInteractionSources,
    pub pan_sources: CameraInteractionSources,
    pub zoom_sources: CameraInteractionSources,
}
```

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

Blocking has two gates.

`GateContexts` acts on enhanced-input's state machine before
`EnhancedInputSystems::Update`. Preset and custom contexts that are disabled,
egui-blocked, animation-ignored, inactive, or unrouted should be deactivated or reset
so held-button state, action transition edges, condition timers, and stale action
values do not advance invisibly while the camera cannot consume input.

`FinalizeInput` acts on resolved per-frame intent after all input writers have run.
This includes preset/custom action resolution and user systems in
`OrbitCamInputSet::WriteManual`. It clears blocked intent, emits lifecycle events,
applies blockers that cannot be expressed inside enhanced input, and enforces owner
latch invariants. A blocked camera must not move, interrupt animation, or keep
guidance highlighted because of stale `OrbitCamInput`.

Both gates must consult `OrbitCamInputBlockerFlags`, the single computed source of
truth for blocker state. They must not re-derive egui, animation, disabled, or routing
blockers independently.

`BlockOnEguiFocus` should feed the internal UI-focus blocker. The blocker must preserve
current behavior:

- use `EguiWantsFocus::prev || EguiWantsFocus::curr` to avoid a one-frame leak;
- respect `EguiFocusIncludesHover`;
- collect egui focus state before input blocker computation;
- block context evaluation, adapter injection, action resolution, and finalized
  manual input from the same computed blocker state;
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
    /// Use the configured explicit camera entity.
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

Keep public routing configuration separate from internal resolved routing state. The
public API should express only the app's routing preference and explicit target. The
library should keep an internal resource for the resolved route:

```rust
pub struct CameraInputRoutingConfig {
    pub mode: CameraInputRouting,
    pub explicit_camera: Option<Entity>,
}

pub(crate) struct ResolvedOrbitCamInputRoute {
    routed_camera: Option<Entity>,
    held_owner: Option<Entity>,
    surface_metrics: CameraInputSurfaceMetrics,
    blockers: OrbitCamInputBlockerFlags,
}
```

The internal resolved route is rewritten every frame by routing systems and is the
only state that context gating, adapter injection, and manual finalization should
consult.

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
  route each event by available event metadata and current routing state
  mark the matching interaction active only for that frame
  emit start/end lifecycle events for that frame if no held source remains active
```

`MouseWheel` carries a window entity, so wheel and pixel scroll can route by event
window plus current cursor position. Bevy `PinchGesture`, `PanGesture`, and
`RotationGesture` are global deltas without window, cursor, touch ID, or phase
metadata. Route those global gesture impulses deterministically with this priority:

1. The current held owner, if a held camera interaction is active.
2. The explicit routing camera, if `CameraInputRouting::Explicit` is active.
3. The current cursor-hit camera, if the cursor is inside exactly one eligible camera
   viewport.
4. No camera input if routing is ambiguous or no eligible camera can be identified.

Document that precise multi-window gesture routing for global Bevy gesture events
requires explicit routing or an unambiguous cursor-hit camera.

No-position held sources such as keyboard or gamepad use the same routing family.
Their binding entries should declare that they have no pointer position. Automatic
routing should use:

1. The current held owner, if another held camera interaction is already active.
2. The explicit routing camera, if `CameraInputRouting::Explicit` is active.
3. The current cursor-hit camera, if exactly one eligible camera is under the cursor.
4. The only eligible camera, if there is exactly one routeable `OrbitCam`.
5. No camera input when ambiguous.

Store the routing-derived surface metrics for the selected camera alongside the
per-camera input context for the frame:

```rust
/// Surface sizes used to interpret screen-pixel camera input.
///
/// These values are logical pixels, not physical framebuffer pixels. Bevy cursor
/// positions and mouse motion are reported in logical window coordinates, so these
/// metrics must stay in the same coordinate space. On a Retina/high-DPI display,
/// do not multiply these values by the window scale factor.
pub struct CameraInputSurfaceMetrics {
    /// Size of the rendered camera view used for pan scaling.
    ///
    /// For a normal window camera this is usually [`Camera::logical_viewport_size`].
    /// For render-to-texture or editor panels, use the logical size of the surface the
    /// user is interacting with. Use image texel dimensions only when the image texel
    /// grid is intentionally the interaction surface.
    pub camera_view_size: Option<Vec2>,

    /// Size of the input surface used for orbit scaling.
    ///
    /// For a normal window camera this is usually [`Window::width`] and
    /// [`Window::height`], which are logical dimensions. For custom editor layouts,
    /// use the logical size of the panel or surface that produced the input delta.
    pub input_surface_size: Option<Vec2>,
}
```

Manual input, explicit routing, multi-window routing, and automatic cursor hit-testing
must all use the same per-camera logical metrics when converting screen-pixel deltas
into orbit or pan response.

In normal window and viewport cases, derive these metrics programmatically from the
camera's render target, logical viewport, and window. Manual camera input should not
force the user to provide metrics that Bevy already knows. Expose an explicit routing
override for render-to-texture, offscreen images, or custom editor surfaces where the
input surface is not the camera's window viewport:

```rust
CameraInputRoutingConfig::explicit(camera)
    .with_surface_metrics(CameraInputSurfaceMetrics {
        camera_view_size: Some(render_target_logical_size),
        input_surface_size: Some(panel_logical_size),
    });
```

Metric derivation should use this order:

1. Explicit `CameraInputSurfaceMetrics` on `CameraInputRoutingConfig`.
2. The selected camera's `Camera::logical_viewport_size` for `camera_view_size`.
3. The target window's logical `Window::width` and `Window::height` for
   `input_surface_size` when the camera renders to a window.
4. No metrics when the selected camera has a missing render target, missing window,
   zero-size viewport, image target without explicit metrics, or ambiguous custom
   surface.

Missing metrics are detected in finalization, where the routed camera entity and
input kind are known. Screen-pixel orbit or pan input without metrics should be
dropped with a structured warning that includes the camera entity and the missing
lookup. Do not fall back to physical framebuffer size or scale-factor-multiplied
values.

Enhanced-input action evaluation must also be gated to the latched active camera.
Inactive `OrbitCamInputContext` instances must not accumulate action state. The design
should use both sides of the invariant: deactivate or gate inactive contexts before
`EnhancedInputSystems::Update`, and reset their camera action state when route
ownership changes. A context that is inactive for routing must not read input in the
same frame, and a context that becomes active must not resume stale action values from
an earlier route.

If the owning camera becomes blocked, disabled, inactive, despawned, or otherwise
unavailable:

- emit `CameraInteractionEnded` for active interactions when possible;
- clear active interaction state;
- clear the owner.

Latch recovery must be deterministic. Clear the held owner immediately on camera
despawn, `OrbitCam` removal, controls replacement, `OrbitCamInputDisabled`, target
window close, application focus loss, or selected gamepad disconnect. Each frame,
reconcile the latch against the underlying held-source state that created it: if the
mouse button is no longer pressed, the touch ID is gone, or the selected gamepad is no
longer available, force the corresponding interaction ended event and clear the
latch. Do not use an idle-frame grace window for latch recovery.

Camera despawn and `OrbitCam` removal need an explicit cleanup path because scheduled
reconciliation may not see the camera after its components are gone. Add an observer
or lifecycle hook such as `On<Remove, OrbitCam>` that finishes active interactions,
clears owner state, clears interaction state, and lets the private linked relationship
handle structural input-entity cleanup.

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
    -> Apply deferred commands so context/activity/entity changes are visible
    -> Inject Lagrange adapter values for unsupported sources
    -> bevy_enhanced_input updates action state
    -> Resolve camera actions and adapter contributions into OrbitCamInput
    -> User systems write manual OrbitCamInput
    -> Finalize input: clear blocked/stale input, update interaction tracker, emit lifecycle events

Update:
  Programmatic camera animation requests are queued before camera input can reach the controller
  animation_input_interrupt reads OrbitCamInput
    -> Ignore clears input and lets animation continue
    -> Cancel cancels animation and keeps input
    -> Complete finishes animation and clears input
  process_camera_move_list advances remaining animations

PostUpdate:
  Pre-controller input guard re-checks animation ignore blockers
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
        OrbitCamInputSet::GateContexts,
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
        OrbitCamInputSet::GateContexts.before(EnhancedInputSystems::Update),
        OrbitCamInputSet::InjectAdapters.before(EnhancedInputSystems::Update),
        OrbitCamInputSet::ResolveActions.after(EnhancedInputSystems::Apply),
    ),
);

app.add_systems(
    PreUpdate,
    (
        reconcile_orbit_cam_controls,
        apply_deferred,
    )
        .chain()
        .in_set(OrbitCamInputSet::ReconcileControls),
);

app.add_systems(
    PreUpdate,
    (
        update_egui_focus_state,
        route_active_orbit_cam,
        reconcile_orbit_cam_input_latch,
        update_orbit_cam_input_blockers,
    )
        .chain()
        .in_set(OrbitCamInputSet::Route),
);

app.add_systems(
    PreUpdate,
    (
        gate_orbit_cam_input_contexts,
        apply_deferred,
    )
        .chain()
        .in_set(OrbitCamInputSet::GateContexts),
);

app.add_systems(
    PreUpdate,
    (
        inject_orbit_cam_adapter_values,
        apply_deferred,
    )
        .chain()
        .in_set(OrbitCamInputSet::InjectAdapters),
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
        queue_programmatic_camera_motion,
        animation_input_interrupt,
        process_camera_move_list,
    )
        .chain(),
);

app.add_systems(
    PostUpdate,
    (
        guard_orbit_cam_input_before_controller,
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
    GateContexts,
    InjectAdapters,
    ResolveActions,
    WriteManual,
    FinalizeInput,
}

pub struct OrbitCamSystemSet;
```

`ReconcileControls` handles structural replacement when `OrbitCamControls` changes.
`GateContexts` activates only the routed or latched camera input context, deactivates
or resets inactive contexts, and clears stale action state when ownership changes.
This set is the chosen inactive-context handling path: it owns action-state hygiene
via context deactivation/reset before `EnhancedInputSystems::Update`, rather than
leaving inactive contexts running and trying to repair their output later.
Route resolution, latch reconciliation, blocker computation, and context gating must
run as one chained sequence from the perspective of enhanced input; `GateContexts`
must not read a route that disagrees with the held-owner latch.
Any command-buffered entity, relationship, or context-activity changes needed by
enhanced input must be visible before `EnhancedInputSystems::Update`; use explicit
`apply_deferred` barriers or exclusive systems rather than relying on later schedule
boundaries.
`FinalizeInput` is the last semantic gate before any animation or controller system
can observe input. It clears blocked manual/preset/custom input, emits lifecycle
events, updates interaction state, and clears the owner latch when needed.

## Animation And Programmatic Motion

`AnimationConflictPolicy` and `CameraInputInterruptBehavior` remain separate policy
axes:

| Situation | Existing policy | Input behavior |
|-----------|-----------------|----------------|
| New programmatic animation arrives while another animation is active. | `AnimationConflictPolicy` | Does not inspect or modify input blockers. |
| User input arrives during animation and policy is `Ignore`. | `CameraInputInterruptBehavior::Ignore` | `FinalizeInput` treats the active animation as an input blocker before lifecycle events are emitted; animation continues and input is not observable. |
| User input arrives during animation and policy is `Cancel`. | `CameraInputInterruptBehavior::Cancel` | `animation_input_interrupt` cancels/removes animation, emits existing cancelled events, and keeps finalized input so user control applies this frame. |
| User input arrives during animation and policy is `Complete`. | `CameraInputInterruptBehavior::Complete` | `animation_input_interrupt` completes/jumps animation, emits existing completion events, and clears input for this frame. |

Finalized `OrbitCamInput` is the user-input interrupt signal for `Cancel` and
`Complete`. `Ignore` is different: active animation plus `Ignore` is an input blocker
inside `FinalizeInput` before started/ended input lifecycle events are emitted.
Finalization should check the authoritative animation state directly, such as
`CameraMoveList` plus the camera's interrupt policy, so observer-driven animation
insertion/removal cannot leave a one-frame stale blocker. Animation interruption
should not depend on detecting later target mutation.

Programmatic animation requests that should affect input in the same frame must be
queued before camera input can reach the controller. If an animation can be inserted
after `FinalizeInput`, run a pre-controller guard in `PostUpdate` before `orbit_cam`
that re-checks authoritative animation state and clears blocked input for `Ignore`.
`Cancel` and `Complete` remain handled by `animation_input_interrupt` for finalized
input.

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
  touch, keyboard, gamepad, and manual paths can be verified through
  `OrbitCamInteractionState` or `CameraInteractionSourcesChanged`.

`fairy_dust` needs a data-driven camera guidance panel that examples can configure
with rows. The panel should highlight active rows from lifecycle events and optionally
display source flags.

Conceptual API:

```rust
CameraGuidance::for_preset(OrbitCamControlPreset::BlenderLike)
CameraGuidance::for_preset(OrbitCamControlPreset::SimpleMouse)
CameraGuidance::custom([
    CameraGuidanceRow::new(CameraInteractionKind::Orbit, "Right stick")
        .when_sources(CameraInteractionSources::GAMEPAD),
    CameraGuidanceRow::new(CameraInteractionKind::Pan, "Left stick + L2")
        .when_sources(CameraInteractionSources::GAMEPAD),
    CameraGuidanceRow::new(CameraInteractionKind::Zoom, "Pinch")
        .when_sources(CameraInteractionSources::PINCH),
    CameraGuidanceRow::new(CameraInteractionKind::Zoom, "Wheel")
        .when_sources(CameraInteractionSources::WHEEL),
])
```

Rows match by interaction kind and, when provided, source predicate. This lets one
guidance panel distinguish wheel zoom, pinch zoom, keyboard zoom, gamepad zoom, and
manual zoom without highlighting every zoom row at once.

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

`fairy_dust` camera setup also needs to move with the refactor. Existing helpers that
only mutate `OrbitCam` are not enough because controls now live in separate
components. Provide either a closure over `EntityCommands` or builder methods such as:

```rust
with_orbit_cam_controls(OrbitCamControls::Preset(OrbitCamControlPreset::BlenderLike))
with_camera_guidance(CameraGuidance::for_preset(OrbitCamControlPreset::BlenderLike))
```

Examples should be able to insert custom/manual controls and guidance rows on the
spawned camera without reaching around the helper.

### Legacy API Migration Table

This refactor is a breaking input API change. Remove the legacy `OrbitCam` raw-input
fields outright rather than keeping a compatibility shim that maps old fields into
`OrbitCamControls`. The migration table documents the replacement concepts, but the
old fields should not remain functional alongside the new controls model.

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
| Manual active-camera resource setup for render-to-texture | `CameraInputRouting::Explicit` plus logical `CameraInputSurfaceMetrics`. |

### Example Migration Notes

- `basic.rs` should remain the smallest working camera example. It should use
  `LagrangePlugin + OrbitCam::default()` to demonstrate the zero-config default,
  which resolves to the mouse-oriented `SimpleMouse` preset. Its comments should
  state that `BlenderLike` is available for editor-style workflows.
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
  while `Explicit` uses the camera entity supplied by the public routing config.
  It should also demonstrate `CameraInputSurfaceMetrics` for render-to-texture:
  metrics are logical pixels; image texel dimensions should only be used when the app
  intentionally treats the image texel grid as the interaction surface.
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
  should remain camera behavior examples rather than input examples. They should
  use the default controls unless the demonstrated camera behavior specifically
  requires a different preset.
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
  methods and typed deltas, with `ManualInputSource::manual()` and at least one
  observed-device source such as `ManualInputSource::observed_keyboard()`. Its
  guidance text should make the resulting `MANUAL | KEYBOARD` source set visible.

## Testing Strategy

Prefer ECS-only tests for the input refactor. Most behavior can be validated with an
`App`, the input systems/plugins, spawned camera entities, synthetic input messages,
and event/message readers. Avoid requiring renderer or GPU setup unless a test
specifically covers rendered output.

Core ECS-only tests:

- default `OrbitCam` receives `OrbitCamControls::Preset(SimpleMouse)` through the
  required component path;
- `Preset -> Manual` despawns related `OrbitCamInputEntities` and installs no new
  library-owned input entities;
- `Preset -> Custom` replaces old related entities rather than accumulating bindings;
- replacing controls during an active interaction emits `CameraInteractionEnded` and
  clears stale `OrbitCamInput`;
- owner latch recovery clears held ownership on despawn, `OrbitCam` removal, controls
  replacement, input disable, target-window close, application focus loss, selected
  gamepad disconnect, or missing underlying held-source state;
- `OrbitCamInputDisabled`, egui focus blockers, inactive routing, and animation ignore
  clear manual and preset/custom input before animation or controller systems observe it;
- systems in `OrbitCamInputSet::WriteManual` are visible to `FinalizeInput` in the
  same frame;
- manual writer helpers expose only `OrbitCamControls::Manual` cameras, and manual
  writes cannot override preset/custom resolved input;
- manual shorthand helpers such as `orbit_pixels` and `pan_pixels` write with
  `ManualInputSource::manual()`;
- manual writer helpers take `ManualInputSource`, always include `MANUAL`, and can
  add observed-device source flags without allowing arbitrary source sets;
- manual zero-delta active helpers emit started/ended lifecycle events correctly;
- manual screen-pixel orbit and pan writes use automatically derived logical surface
  metrics when possible, and missing metrics are detected by the manual writer or
  finalizer instead of silently producing incorrect scaling;
- surface metrics are documented and tested as logical pixels, including a high-DPI
  case where physical framebuffer size differs from logical window size;
- surface metric derivation covers normal window cameras, render-to-texture explicit
  overrides, multi-window routing, zero-size viewports, missing windows, and image
  targets without explicit metrics;
- held pan/zoom/orbit bindings cannot be built without corresponding engagement
  state;
- reflected or dynamically loaded held bindings go through validation and reject
  motion-without-engagement or source/condition mismatches;
- impulse bindings reject `OrbitEngaged`, `PanEngaged`, and `ZoomEngaged` because
  wheel, pinch, and smooth-scroll do not have a held phase;
- custom binding specs carry source metadata, and source flags do not need to be
  inferred from enhanced-input internals;
- custom binding specs are action-typed, so orbit/pan and smooth/coarse zoom bindings
  cannot be swapped through the ordinary builder API;
- per-binding source attribution survives enhanced-input action merging, so
  keyboard-plus-gamepad bindings for the same action report only the source that
  actually triggered;
- held sources latch the owner until release, while impulse wheel/pinch/smooth-scroll
  events route independently per event;
- global Bevy gesture impulses without window metadata route by the documented
  fallback policy and produce no input when ambiguous;
- per-camera logical surface metrics are used for orbit and pan scaling under
  explicit and cursor-hit-test routing;
- `ReconcileControls` and `GateContexts` changes are visible to
  `EnhancedInputSystems::Update` in the same frame;
- adapter values inserted through command-buffered mock state are visible to enhanced
  input in the same frame because the barrier is structural;
- disabled, egui-blocked, animation-ignored, inactive, and unrouted preset/custom
  contexts are gated or reset before `EnhancedInputSystems::Update`;
- two cameras can swap routing without the inactive camera retaining stale
  enhanced-input action state;
- `App::new().add_plugins(LagrangePlugin)` installs the enhanced-input plugin and
  registers `OrbitCamInputContext` without additional app setup;
- spawning `OrbitCam` without `LagrangePlugin` produces a one-time diagnostic warning
  that input will not resolve;
- `CameraInteractionSources::from_bits` rejects unknown bits, reflection validates
  source bits, and `ManualInputSource` cannot be constructed without `MANUAL`;
- camera actions do not consume app-owned enhanced-input bindings by default;
- adapter/public-binding conflicts are rejected or reported by `OrbitCamBindings`
  validation;
- binding validation returns structured errors for adapter/public-binding conflicts
  and missing mandatory wheel policy;
- `CameraInteractionSourcesChanged` and `OrbitCamInteractionState` report source-set
  changes while an interaction remains active;
- impulse-only interactions such as line wheel emit started and ended in the same
  frame and do not remain active into the next frame;
- each binding entry carries its own source metadata, and keyboard-plus-gamepad
  bindings for the same action report only the source that actually triggered;
- gamepad selection policy covers any gamepad, selected gamepad, disconnect fallback,
  and multi-controller behavior;
- `ZoomDirection::Reversed` applies uniformly to coarse wheel, smooth scroll, pinch,
  touch pinch, button-drag zoom, keyboard, and gamepad zoom paths;
- touch pinch applies `touch_pinch_scale` before the shared zoom response path;
- pinch is suppressed while any configured non-pinch camera modifier or held camera
  action is active;
- pinch suppression is scoped to the routed camera, not global raw modifier state;
- egui click/drag focus tests preserve the current `prev || curr` leak prevention,
  including the frame focus is requested;
- `CameraInputInterruptBehavior::{Ignore, Cancel, Complete}` preserve their exact
  input, animation-event, and controller-consumption behavior on the frame an
  animation starts, completes, is cancelled, or is replaced;
- dependency validation confirms `bevy_lagrange` uses the workspace-pinned
  `bevy_enhanced_input` and `bitflags` versions without duplicate direct versions,
  and confirms `bevy_kana`'s `input` feature is removed or resolves to the same
  enhanced-input version.
- workspace consumers, especially `crates/bevy_diegetic/examples/*`, compile after
  legacy `OrbitCam` input fields move into controls and bindings.

## Migration Plan

1. Add workspace-pinned `bevy_enhanced_input` as a normal `bevy_lagrange` dependency and have `LagrangePlugin` install the enhanced-input plugin.
2. Add `bitflags = { workspace = true }` as a direct `bevy_lagrange` dependency.
3. Audit `bevy_kana`'s `input` feature. Remove it if unused by `bevy_lagrange`, or validate that it resolves to the same `bevy_enhanced_input` version as the direct dependency.
4. Add the public `input` module with actions, context, controls, bindings, intent, disabled input, interaction state, manual writing, and interaction events.
5. Add `OrbitCamInput`, typed deltas, active-source fields, and helper methods for manual input.
6. Add `OrbitCamInputContext` as a required component on `OrbitCam` and register it in `LagrangePlugin` after enhanced input is installed.
7. Add `OrbitCamControls::{Preset, Custom(OrbitCamBindings), Manual}`.
8. Add `OrbitCamBindings`, private fields, action-typed local builder/spec types with per-binding source metadata, typestate wheel ownership, engagement invariants, gamepad selection policy, metadata-bearing low-level enhanced-input escape hatches, and runtime validation.
9. Add `ZoomDirection`, `ButtonDragZoomBinding`, touch policy, pinch policy, and wheel policy as binding/adapter configuration.
10. Add the private `OrbitCamInputEntityOf` / `OrbitCamInputEntities` relationship and control reconciliation.
11. Add the private adapter module for wheel units, smooth scroll, pinch, and touch.
12. Add source-aware interaction tracking, `CameraInteractionStarted`, `CameraInteractionEnded`, `CameraInteractionSourcesChanged`, and `OrbitCamInteractionState`.
13. Replace public runtime gating with `OrbitCamInputDisabled` plus internal transient blockers.
14. Rename `CameraInputDetection` to `CameraInputRouting` with `CursorHitTest` and `Explicit`.
15. Implement public routing configuration, internal resolved routing state, held-source owner latching, deterministic latch recovery, per-event impulse routing, no-position source routing, global gesture fallback routing, logical surface metrics, and inactive-context gating/reset before enhanced-input update.
16. Add the root-level `system_sets` module and `LagrangeSystemSetsPlugin` with `ReconcileControls`, `GateContexts`, `WriteManual`, and `FinalizeInput`.
17. Add `animation_input_interrupt` and use finalized `OrbitCamInput` as the user-input interrupt signal for `Cancel` and `Complete`; treat `Ignore` as a finalization and pre-controller blocker.
18. Remove physical binding fields from `OrbitCam` as a breaking change and move their replacement concepts into presets and adapter configuration.
19. Update egui blocking to feed internal UI-focus blockers before finalization.
20. Add the `fairy_dust` camera guidance panel and component-insertion camera setup needed by the controls examples.
21. Add the controls examples with `fairy_dust` visual feedback.
22. Migrate existing examples according to the example migration notes.
23. Migrate workspace consumers, especially `crates/bevy_diegetic/examples/*`, away from legacy `OrbitCam` input fields.
24. Add missing-plugin diagnostics and first-frame setup validation.
25. Add ECS-only tests for scheduling, reconciliation, routing, blockers, lifecycle events, legacy behavior preservation, interrupt policies, workspace consumers, and dependency versioning.

## Changelog-Style Summary

### Breaking

- Remove legacy raw-input fields from `OrbitCam`; configure user input through
  `OrbitCamControls`, `OrbitCamBindings`, and `OrbitCamInputDisabled`.
- Replace `CameraInputDetection::{Automatic, Manual}` with
  `CameraInputRouting::{CursorHitTest, Explicit}`.

### Added

- Add enhanced-input based orbit-camera controls with preset, custom, and manual control modes.
- Add source-aware camera interaction lifecycle events, source-change events, and read-only interaction state.
- Add `ManualInputSource` so manual camera input always reports `MANUAL` and may include observed device provenance.
- Add logical `CameraInputSurfaceMetrics` for explicit routing, render-to-texture, and custom editor input surfaces.
- Add structured binding validation and missing-plugin diagnostics for common setup mistakes.
- Add control-mode examples with `fairy_dust` guidance that highlights active camera interactions and source flags.

### Changed

- Change the default controls model to `OrbitCamControls::Preset(SimpleMouse)` and
  make `BlenderLike` an explicit editor-style preset.
- Change camera input routing to use `CameraInputRouting::{CursorHitTest, Explicit}` with internal resolved routing state.
- Change custom bindings to be action-typed and source-aware so lifecycle events can distinguish mouse, wheel, smooth-scroll, pinch, touch, keyboard, gamepad, and manual input.
- Change render-to-texture routing to use explicit routing plus logical surface metrics instead of manually populating `ActiveCameraData`.
- Change examples and workspace consumers to configure controls through `OrbitCamControls` and `OrbitCamBindings`.

### Removed

- Remove legacy raw-input fields from `OrbitCam` as a breaking change.
- Remove the old `CameraInputDetection::{Automatic, Manual}` API in favor of `CameraInputRouting::{CursorHitTest, Explicit}`.
- Remove the old keyboard-controls pattern that mutates camera targets directly for user input.

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
