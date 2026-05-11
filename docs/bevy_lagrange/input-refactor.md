# `bevy_lagrange` input refactor

## Goal

Make `bevy_lagrange` opinionated about Bevy's action/context input model while
keeping camera behavior separate from physical input policy.

The target shape is:

- `OrbitCam` owns camera state, response scaling, smoothing, limits, animation behavior, and active-camera behavior.
- `bevy_enhanced_input` owns the public action model: actions, contexts, bindings, modifiers, conditions, and user keymaps.
- `bevy_lagrange` provides default camera input modes as enhanced-input presets.
- `bevy_lagrange` keeps a narrow adapter for source details that enhanced input does not currently expose.
- The camera controller consumes one per-camera intent snapshot, not raw Bevy input and not binding policy.

## Design Rules

1. `OrbitCam` configures how the camera moves.
2. `bevy_lagrange::input` contains the public camera-input API.
3. Mutually exclusive input-mode components configure who owns user-input resolution.
4. `OrbitCamBindings` is the public custom binding and adapter-policy spec.
5. Enhanced-input actions configure what user input means.
6. `OrbitCamInput` is the semantic per-frame camera input consumed by the controller.
7. Manual input uses helper methods and typed deltas, not raw field mutation.
8. App-level input disabling uses `CameraInputDisabled`.
9. Transient blockers such as animation ignore and UI focus are internal library state.
10. Programmatic camera operations mutate camera state, targets, or animation queues; they do not write `OrbitCamInput`.
11. Preset and custom input modes have one library-owned input writer per frame.
12. Manual input mode means the app writes `OrbitCamInput` and the library skips action resolution for that camera.

## Naming Conventions

Use prefixes to show whether an API belongs to the current `OrbitCam` controller or
to shared Lagrange camera-input infrastructure that should also fit a future
`FreeCam`.

| Prefix | Meaning | Examples |
|--------|---------|----------|
| `OrbitCam*` | Orbit-controller state, bindings, input modes, lifecycle events, or scheduling. These names can mention orbit/pan/zoom concepts directly. | `OrbitCamInput`, `OrbitCamBindings`, `OrbitCamPreset`, `OrbitCamInteractionStarted`, `OrbitCamInteractionState`, `OrbitCamInputPhase` |
| `CameraInput*` | Shared Lagrange-managed camera-input infrastructure. These names should not assume orbit/pan/zoom and do not mean "any Bevy camera." | `CameraInputRouting`, `CameraInputRoutingConfig`, `CameraInputSurfaceMetrics`, `CameraInputDisabled`, `CameraInputMetricsMissing` |
| `CameraInteractionSources` | Shared source-attribution flags usable by current and future camera controllers. | `CameraInteractionSources::MOUSE`, `CameraInteractionSources::GAMEPAD`, `CameraInteractionSources::MANUAL` |

Do not use `Camera*` only because a type happens to mention a Bevy camera. Use the
generic prefix when the type is intended to survive additional camera controllers.
Use the `OrbitCam*` prefix when the type is coupled to `OrbitCam` behavior,
configuration, lifecycle events, or resolved orbit/pan/zoom intent. A future
`FreeCam` should get its own controller-specific lifecycle event and kind types, such
as `FreeCamInteractionStarted` and `FreeCamInteractionKind`, while reusing
`CameraInteractionSources` for device/source attribution.

Enhanced-input action marker types should end in `Action`. Do not use bare operation
names such as `Orbit` or `Pan` for zero-sized marker types. Use names such as
`OrbitCamOrbitAction`, `OrbitCamPanAction`, `OrbitCamZoomCoarseAction`, and
`OrbitCamZoomSmoothAction` so signatures distinguish action markers from `OrbitCam`,
`OrbitDelta`, interaction kinds, and binding collections.

Source-policy types follow the same prefix rule. Use `OrbitCam*` when the policy
encodes orbit-camera action semantics, even if the type lives inside
`OrbitCamBindings`: `OrbitCamWheelBinding`, `OrbitCamBlenderLikeWheelBinding`,
`OrbitCamWheelModifier`, `OrbitCamButtonDragZoomBinding`, and
`OrbitCamButtonDragZoomAxis`. Touch policy also uses this controller prefix:
`OrbitCamTouchBinding`. Use `CameraInput*` when the policy is shared device
infrastructure that can apply unchanged to multiple camera controllers:
`CameraInputGamepadSelectionPolicy`. Do not introduce bare public names such as
`WheelBinding`, `WheelModifier`, `TouchInput`, `ButtonDragZoomBinding`, or
`GamepadSelectionPolicy`; those names do not make ownership clear in imports,
rustdoc, reflected descriptors, or validation errors.

## Locked Decisions

These decisions are settled for the initial refactor. Future reviews should treat
them as constraints unless implementation proves one is unworkable.

- `bevy_enhanced_input` is a normal dependency installed by `LagrangePlugin`.
- Keep all reflected editor/keymap support in `bevy_lagrange` behind a default-on
  `reflect-input-modes` feature.
- Keep three mutually exclusive input-mode marker components:
  `OrbitCamPreset`, `OrbitCamBindings`, and `OrbitCamManual`.
  Use the observer shim for tidy mutations and `PreInput` validation as the
  deterministic authority until native Bevy mutually exclusive components can replace
  the shim. Do not reopen a single `OrbitCamInputMode` enum component for the initial
  refactor; this marker-component shape is a locked public API decision.
- `OrbitCam::default()` resolves to the stable `SimpleMouse` preset. `BlenderLike`
  remains explicit editor-style configuration.
- Use one progressive `OrbitCamBindings` builder. Do not add a second simple/custom
  builder surface.
- Keep adapter internals private and replaceable. Do not add an adapter feature gate
  or a separate pure-enhanced-input control path. Public wheel, pinch, touch, and
  smooth-scroll policy types describe camera behavior, not adapter mechanics.
- Keep engagement actions such as `OrbitCamOrbitEngagedAction`,
  `OrbitCamPanEngagedAction`, and `OrbitCamZoomEngagedAction` private. Public UI and
  editor code observes interaction events and `OrbitCamInteractionState`.
- Use controller-specific interaction lifecycle events for orbit-camera behavior:
  `OrbitCamInteractionStarted`, `OrbitCamInteractionEnded`,
  `OrbitCamInteractionSourcesChanged`, and `OrbitCamInteractionKind`.
- Keep `CameraInteractionSources` as the only public source-set type. Back it with
  private bitflags, expose named source constants and set operations, and do not expose
  public raw-bit constructors in the initial API. Manual writes use branded
  `ManualInputSource`.
- Default no-position keyboard/gamepad routing to no input unless a latch, explicit
  route, or unambiguous cursor-hit camera identifies the target. Single-camera
  fallback requires explicit opt-in.
- Keep `orbit_pixels` and `pan_pixels` as `()` shorthand methods. Missing logical
  metrics report through `CameraInputMetricsMissing` and a one-time error log during
  finalization.
- Do not expose a public route/latch diagnostics resource in the initial refactor.
  Start with rate-limited debug logs and add a public diagnostics API only from a
  concrete in-tree or user-driven need.
- Keep internal controller ordering sets private. The public scheduling surface is
  `OrbitCamInputPhase::{PreInput, WriteManual, Finalize}`, primarily
  `WriteManual` for manual-input writers. Do not expose a separate public controller
  system set in the initial refactor.
- Use `CameraInputDisabled` as the shared app-level pause marker for camera input.
- Do not add a legacy compatibility layer for removed raw `OrbitCam` input fields.
  This is an intentional breaking cleanup.
- Keep supported input modes as separate named examples rather than one
  parameterized input-mode example.

## Dependencies And Features

Use the simple feature surface:

- `bevy_enhanced_input` is a normal dependency of `bevy_lagrange`.
- `bitflags` is a direct dependency of `bevy_lagrange`.
- Reflected descriptor/editor support is a default-on feature, tentatively
  `reflect-input-modes`. Keep it in `bevy_lagrange`, not a separate crate. Disabling it
  removes `OrbitCamInputModeDescriptor`, `OrbitCamBindingsDescriptor`, descriptor
  apply systems, apply-status components, and related reflected editor/keymap
  registration. It does not remove preset input modes, custom runtime bindings, manual
  input, routing, lifecycle events, or the enhanced-input adapter.
- `bevy_egui` remains optional.
- `fit_overlay` remains optional.
- `OrbitCamManual` is a per-camera input mode, not a no-dependency build mode.
- `LagrangePlugin` installs the enhanced-input plugin it depends on before registering
  camera input contexts, so apps do not need a second hidden setup step for camera
  input.

Declare both dependencies through workspace dependency entries and use those entries
from `crates/bevy_lagrange/Cargo.toml`. `bevy_enhanced_input` should be a direct
`bevy_lagrange` dependency with an explicit Bevy-compatible minimum and maximum
version range, and the workspace root should pin the same range so no transitive copy
from another crate can silently select an incompatible enhanced-input API.
Document the exact supported Bevy version whose scheduling semantics the input
pipeline targets. For the current workspace, that is Bevy `0.18.1`. Audit schedule
barrier APIs on every Bevy upgrade, especially any replacement for explicit deferred
application or exclusive-system behavior.

Keep enhanced-input assumptions behind an internal integration boundary. The concrete
name can change during implementation, but the module should isolate:

- plugin setup and duplicate-plugin guards;
- `add_input_context` registration;
- action/context entity installation;
- enhanced-input binding descriptors and system-set names;
- action/mock write paths used by adapter-backed sources.

Conceptual shape:

```rust
pub(crate) trait EnhancedInputCameraAdapter {
    fn ensure_plugin(app: &mut App);
    fn register_camera_context(app: &mut App);
    fn install_bindings(world: &mut World, camera: Entity, bindings: &OrbitCamBindings);
}
```

The implementation should keep this trait or equivalent internal module private. Its
purpose is not to abstract Bevy away from users; it is to make an upstream
enhanced-input API change local to one integration layer.

This plan assumes the current `bevy_enhanced_input` model:

- Contexts are regular components registered with `add_input_context`.
- Built-in `Binding` variants include keyboard, mouse button, mouse motion, mouse wheel, gamepad button, gamepad axis, any key, and none.
- Bindings can use custom `InputModifier` and `InputCondition` components.
- `ActionMock` can feed externally produced values through enhanced-input action timing, but active mocks skip input reading, conditions, and modifiers.
- The built-in `Binding` enum is closed, so user crates cannot add first-class raw binding sources.
- `PinchGesture`, `Touches`, and `MouseWheel::unit` are not represented with enough detail to preserve the current `bevy_lagrange` camera model purely through public bindings.

Add an enhanced-input integration test that compiles and exercises the pinned API
surface: context registration, expected system-set ordering, normal binding
installation, and adapter/mock contribution if mocks are used. Run this test on every
Bevy or `bevy_enhanced_input` upgrade.
Also add a startup diagnostic in strict mode that verifies the expected ordering
resources, context registration, plugin setup, and one known enhanced-input API marker
were installed by `LagrangePlugin`. Bevy does not expose a general runtime schedule
proof, so the diagnostic should fail loud for missing setup, missing context
registration, unexpected enhanced-input API shape, or missing Lagrange set
configuration; the ECS ordering tests remain the authoritative guard for barrier
semantics.

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
    actions.rs             // public OrbitCamOrbitAction, OrbitCamPanAction, OrbitCamZoomCoarseAction, OrbitCamZoomSmoothAction
    bindings.rs            // public OrbitCamBindings and adapter binding policy
    context.rs             // public OrbitCamInputContext
    modes/
      mod.rs               // public input-mode docs and re-exports
      components.rs        // public mutually exclusive input-mode components
      descriptors.rs       // public reflectable draft input modes for editors/keymaps
      exclusive.rs         // private observer shim until native Bevy exclusivity
      installation.rs      // private owned input-entity relationships
      reconcile.rs         // private input-mode reconciliation systems
    events.rs              // public camera interaction lifecycle events
    state.rs               // public read-only interaction state
    routing.rs             // public routing config and logical surface metrics
    intent.rs              // public OrbitCamInput and typed deltas
    manual.rs              // public manual writer helper/query pattern
    disabled.rs            // public CameraInputDisabled
    adapter/
      mod.rs               // private adapter plugin and systems
      actions.rs           // pub(super) source actions only if needed
      wheel.rs
      touch.rs
      pinch.rs
```

`input/mod.rs` should explain the input-mode components at the top:

```rust
//! Camera input API.
//!
//! # Quick Start
//!
//! - Use [`OrbitCamPreset`] when you want a built-in camera keymap.
//! - Use [`OrbitCamBindings`] when your app has a keymap or gamepad binding UI.
//! - Use [`OrbitCamManual`] when your app wants to compute camera intent itself.
//!
//! ```rust
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(LagrangePlugin)
//!     .add_systems(Startup, setup)
//!     .run();
//!
//! fn setup(mut commands: Commands) {
//! commands.spawn((Camera3d::default(), OrbitCam::default()));
//! }
//! ```
//!
//! ```rust
//! commands.spawn((
//!     Camera3d::default(),
//!     OrbitCam::default(),
//!     my_keymap.to_orbit_cam_bindings(),
//! ));
//! ```
//!
//! ```rust
//! app.add_systems(
//!     PreUpdate,
//!     write_manual_camera_input.in_set(OrbitCamInputPhase::WriteManual),
//! );
//! ```
//!
//! Preset and custom input modes are resolved through `bevy_enhanced_input`.
//! Manual input mode bypasses enhanced input for that camera.
//!
//! Adapter-backed sources such as wheel-unit, pinch, touch, and smooth-scroll
//! policy are configured through [`OrbitCamBindings`], not through private
//! adapter actions.
//!
//! # Components
//!
//! [`OrbitCam`] requires [`OrbitCamInput`], [`OrbitCamInputContext`], and
//! [`OrbitCamPreset`]. A camera therefore receives the stable
//! [`OrbitCamPreset::SimpleMouse`] default unless the app inserts
//! [`OrbitCamBindings`] or [`OrbitCamManual`]. Those three input-mode components are
//! mutually exclusive; inserting one removes the others before input is routed for
//! the frame.
//!
//! # Binding Invariants
//!
//! Custom bindings are built through [`OrbitCamBindings`]. Held camera bindings must
//! pair movement and engagement through held constructors, impulse controls such as
//! wheel and pinch must not bind engagement actions, every binding entry carries
//! source metadata, and adapter-owned sources such as mouse wheel must be configured
//! through Lagrange adapter policy rather than raw enhanced-input mouse-wheel
//! bindings.
//!
//! # Routing And Ownership
//!
//! Cursor-hit routing chooses the camera under the cursor or touch position.
//! Explicit routing chooses the configured camera. Held sources latch to the camera
//! where they started until release; impulse sources such as wheel, smooth scroll,
//! pinch, and global gestures route independently for the frame in which they occur.
//! Ambiguous global gestures are dropped with a rate-limited debug log.
//!
//! # Observing Interactions
//!
//! Use [`OrbitCamInteractionStarted`], [`OrbitCamInteractionEnded`],
//! [`OrbitCamInteractionSourcesChanged`], or [`OrbitCamInteractionState`] when editor
//! UI needs to react to orbit, pan, or zoom activity.
//!
//! # Connecting Input To Behavior
//!
//! Bindings describe what the user did. [`OrbitCam`] describes how the camera
//! responds. For example, a gamepad binding can report `GAMEPAD` zoom intent while
//! `OrbitCam` still owns zoom sensitivity, smoothing, radius limits, and animation
//! interruption policy. Prefer changing bindings when the physical control changes;
//! prefer changing `OrbitCam` when the response should feel different.
//!
//! # Advanced
//!
//! Render-to-texture and custom editor surfaces use [`CameraInputRouting::Explicit`]
//! plus optional [`CameraInputSurfaceMetrics`]. Manual input is for apps that compute
//! camera intent directly; it is not required just to choose an offscreen camera.
//!
//! System-set and adapter details are lower-level integration points. Most users
//! should start with input modes, bindings, and interaction events.
```

The split between input modes, `bindings.rs`, and descriptors should be
explicit in module docs:

- `modes/components.rs` owns validated runtime input-mode components that camera input systems trust.
- `modes/exclusive.rs` owns the temporary observer-based mutual-exclusion invariant.
- `modes/reconcile.rs` owns conversion from mode components into private enhanced-input installations.
- `bindings.rs` owns validated runtime binding specs and their builders.
- `modes/descriptors.rs` owns reflected draft configuration, apply events, and persisted
  apply status for editors, scene files, and keymap tools.

The public facade should re-export the semantic API from both `input` and the crate
root for convenience:

```rust
pub use input::{
    OrbitCamInteractionEnded,
    OrbitCamInteractionKind,
    CameraInteractionSources,
    OrbitCamInteractionSourcesChanged,
    OrbitCamInteractionStarted,
    CameraInputDisabled,
    CameraInputMetricsMissing,
    CameraInputRouting,
    CameraInputRoutingConfig,
    CameraInputSurfaceMetrics,
    CameraInputGamepadSelectionPolicy,
    OrbitCamOrbitAction,
    OrbitCamPanAction,
    OrbitCamZoomCoarseAction,
    OrbitCamZoomSmoothAction,
    OrbitCamBindings,
    OrbitCamBindingsDescriptor,
    OrbitCamBlenderLikeWheelBinding,
    OrbitCamButtonDragZoomAxis,
    OrbitCamButtonDragZoomBinding,
    OrbitCamPreset,
    OrbitCamPinchBinding,
    OrbitCamInputModeApplied,
    OrbitCamInputModeRejected,
    OrbitCamInputModeApplyState,
    OrbitCamInputModeApplyStatus,
    OrbitCamInputModeDescriptor,
    OrbitCamInteractionState,
    OrbitCamInput,
    OrbitCamInputContext,
    OrbitCamInputPhase,
    OrbitCamManual,
    OrbitCamWheelBinding,
    OrbitCamWheelModifier,
    OrbitCamManualInput,
    ManualInputSource,
    OrbitCamTouchBinding,
    ZoomDirection,
    OrbitDelta,
    PanDelta,
    CoarseZoomDelta,
    SmoothZoomDelta,
};
```

Do not re-export private engagement or source actions such as
`OrbitCamOrbitEngagedAction`, `OrbitCamPanEngagedAction`,
`OrbitCamZoomEngagedAction`, `OrbitFromSmoothScroll`, `ZoomFromPinch`, or `TouchPan`.

Each public type in `bevy_lagrange::input` should carry a short rustdoc example for
its normal use. Keep the quick-start path at the top of `input/mod.rs`; put system-set
ordering, adapter internals, and validation details below the user-facing input modes
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
direction. Those belong to input modes, bindings, adapter policy, or response
configuration.

`OrbitCam` should require:

```rust
#[require(
    OrbitCamInput,
    OrbitCamInputContext,
    OrbitCamPreset,
)]
pub struct OrbitCam {
    // camera behavior fields
}
```

`OrbitCamInputContext` is the `bevy_enhanced_input` context installed for an
`OrbitCam`. Do not shorten it to `OrbitCamContext`: that would blur enhanced-input
wiring with camera behavior, routing, editor, or viewport context.

`LagrangePlugin` should register the context once:

```rust
app.add_plugins(EnhancedInputPlugin);
app.add_plugins(OrbitCamInputModeInvariantPlugin);
app.add_input_context::<OrbitCamInputContext>();
```

The plugin should own this setup. A minimal app that adds only `LagrangePlugin` should
have all enhanced-input resources and systems required by `OrbitCamInputContext`.
Guard plugin setup so workspace-composed apps can add `LagrangePlugin` from multiple
modules without double-installing enhanced input. If Bevy exposes an
`is_plugin_added::<EnhancedInputPlugin>()` equivalent, use it before adding
`EnhancedInputPlugin`; otherwise use an internal setup marker resource and emit a
one-time warning if setup is requested again.

Add diagnostics for missing setup:

- `LagrangePlugin` should run a first-frame diagnostic that confirms enhanced input is
  installed and camera input contexts are registered.
- `OrbitCam` should have an `on_add` hook or equivalent one-time diagnostic path that
  emits a one-time `error!` when an `OrbitCam` exists but `LagrangePlugin` has not installed the input
  pipeline. The warning should say that camera input will not resolve until
  `LagrangePlugin` is added.
- `LagrangePlugin` should expose a diagnostic setting that can panic on missing setup
  during startup for tests and strict application builds. The default should be an
  error log, not a panic.

## Input Modes And Bindings

The active input mode is represented by three mutually exclusive components. Exactly
one input-mode component should be present on every `OrbitCam`:

```rust
#[derive(Component, Clone, Copy, Debug, Reflect)]
#[reflect(Component)]
#[non_exhaustive]
pub enum OrbitCamPreset {
    BlenderLike,
    SimpleMouse,
}

#[derive(Component, Default, Debug, Reflect)]
#[reflect(Component)]
/// Manual input mode for an [`OrbitCam`].
///
/// This means the app writes [`OrbitCamInput`] through [`OrbitCamManualInput`].
/// It does not choose which camera receives ordinary routed input; use
/// [`CameraInputRouting::Explicit`] for explicit routing.
pub struct OrbitCamManual;

impl Default for OrbitCamPreset {
    fn default() -> Self {
        Self::SimpleMouse
    }
}
```

`OrbitCamPreset`, `OrbitCamBindings`, and `OrbitCamManual`
are one exclusive family. This is the same marker-component state-machine pattern
used in `hana::movable::state`: adding one mode removes the other modes. Keep the
invariant code isolated in `input/modes/exclusive.rs` so it can be replaced with
native Bevy mutually exclusive components when the supported Bevy version provides
them.

Temporary observer shim:

```rust
pub(crate) struct OrbitCamInputModeInvariantPlugin;

impl Plugin for OrbitCamInputModeInvariantPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_preset_mode_added);
        app.add_observer(on_bindings_mode_added);
        app.add_observer(on_manual_mode_added);
    }
}

fn on_preset_mode_added(
    added: On<Add, OrbitCamPreset>,
    mut commands: Commands,
) {
    commands
        .entity(added.entity)
        .remove::<OrbitCamBindings>()
        .remove::<OrbitCamManual>();
}

fn on_bindings_mode_added(
    added: On<Add, OrbitCamBindings>,
    mut commands: Commands,
) {
    commands
        .entity(added.entity)
        .remove::<OrbitCamPreset>()
        .remove::<OrbitCamManual>();
}

fn on_manual_mode_added(
    added: On<Add, OrbitCamManual>,
    mut commands: Commands,
) {
    commands
        .entity(added.entity)
        .remove::<OrbitCamPreset>()
        .remove::<OrbitCamBindings>();
}
```

The invariant module enforces at-most-one mode. Required components provide the normal
at-least-one default at spawn. If app code removes every input-mode component from
an existing `OrbitCam`, the pre-input invariant check should restore
`OrbitCamPreset::default()` and log a diagnostic. Use
`CameraInputDisabled` to pause input without changing the selected mode.
Keep the three input-mode components rather than collapsing them into one enum
component. This preserves the query ergonomics of separate mode surfaces, mirrors the
existing marker-state pattern used elsewhere in the workspace, and maps cleanly onto
future native Bevy mutually exclusive components.
The single-enum alternative is intentionally rejected for the initial refactor. The
temporary part is only the observer shim used to maintain exclusivity until Bevy's
native mutually exclusive components are available.

Also add an explicit validation/finalization pass in `OrbitCamInputPhase::PreInput`.
The observer shim keeps common insertions tidy, but `PreInput` is the deterministic
authority before routing and enhanced-input context evaluation:

- observer removals are command-deferred and are not the correctness boundary;
- no library input reconciliation or action resolution may trust input-mode exclusivity
  until the exclusive `PreInput` system has flushed pending mode changes and enforced
  the invariant;
- if more than one input-mode component is present, choose the most recently added
  mode when that information is available, otherwise use a documented precedence of
  `Manual > Bindings > Preset`;
- remove the non-selected modes before reconciliation;
- emit a debug panic or test-only panic when strict diagnostics are enabled, and emit
  a one-time warning in normal builds;
- if no mode remains, insert `OrbitCamPreset::default()` and warn.

When native Bevy mutually exclusive components become available in the supported Bevy
version, replace `modes/exclusive.rs` with the native registration while preserving
the public marker component names and the `PreInput` invariant test coverage.

All public components and resources introduced by this refactor should derive
`Reflect` and register their reflected types when reflected input modes are enabled. The
three input-mode components are the validated runtime state, while
`OrbitCamInputModeDescriptor` is the mutable reflected draft component for editors,
scene files, and keymap tools. Do not make reflected field mutation of custom bindings
the runtime-authoritative path. A reflect client may temporarily create incomplete
draft data while the user is editing; the camera should continue using the last valid
input-mode component until the descriptor validates and is applied.
`OrbitCamBindings` is both the validated binding data and the custom input-mode
component. It must be reflectable as a component, but its binding internals should use
opaque/custom reflection or an equivalent non-editable field strategy. Reflected
editing of custom bindings goes through `OrbitCamBindingsDescriptor`.

Prefer Lagrange-owned, reflectable binding recipes over storing arbitrary closures or
opaque trait objects in components/resources. If an advanced escape hatch cannot be
reflected honestly, keep it out of public component/resource state until it has a
reflectable descriptor or validation story.

If an `OrbitCam` has no explicit input-mode component, the required component default
should be `OrbitCamPreset::SimpleMouse`. This is the most likely default for users who
expect a mouse-oriented camera controller. Insert `OrbitCamPreset::BlenderLike`
explicitly for editor-style workflows that want Blender's middle-mouse orbit
convention and trackpad behavior.
Treat `SimpleMouse` as a stable default once this breaking refactor lands. Do not
change the behavior of `OrbitCam::default()` in a later minor release; add a new preset
variant and require an explicit opt-in instead.

Future-facing public policy enums should be `#[non_exhaustive]` unless the API is
intentionally closed. This applies especially to presets, wheel policy, pinch/touch
policy, routing, and interaction kind.

The modes mean:

| Mode | Meaning | Library writes `OrbitCamInput` |
|------|---------|--------------------------------|
| `OrbitCamPreset::BlenderLike` | Build `OrbitCamBindings` from the Blender-like preset, install actions and adapter policy, and resolve input. | yes |
| `OrbitCamPreset::SimpleMouse` | Build `OrbitCamBindings` from the simpler mouse preset, install actions and adapter policy, and resolve input. | yes |
| `OrbitCamBindings` | Use the public camera context and resolver, but install the app-provided bindings. | yes |
| `OrbitCamManual` | Do not install or resolve camera actions for this camera. The app writes `OrbitCamInput` through helper methods. | no |

Library systems should use component queries rather than matching a mode enum:

```rust
Query<..., With<OrbitCamPreset>>
Query<..., With<OrbitCamBindings>>
Query<..., With<OrbitCamManual>>
```

That keeps preset/custom resolution and manual writing on separate query surfaces.

Example spawns:

```rust
commands.spawn((
    Camera3d::default(),
    OrbitCam::default(),
    OrbitCamPreset::BlenderLike,
));
```

```rust
let bindings = editor_keymap.to_orbit_cam_bindings();

commands.spawn((
    Camera3d::default(),
    OrbitCam::default(),
    bindings,
));
```

```rust
commands.spawn((
    Camera3d::default(),
    OrbitCam::default(),
    OrbitCamManual,
));
```

### Reflected Input Mode Drafts

With the default-on `reflect-input-modes` feature, editor tooling, scene files, and
keymap UIs get a mutable reflected representation of camera input modes. That
representation should be separate from the validated runtime component:

```rust
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct OrbitCamInputModeDescriptor {
    pub mode: OrbitCamInputMode,
}

#[derive(Clone, Debug, Reflect)]
#[non_exhaustive]
pub enum OrbitCamInputMode {
    Preset(OrbitCamPreset),
    Bindings(OrbitCamBindingsDescriptor),
    Manual,
}

#[derive(Clone, Debug, Reflect)]
pub struct OrbitCamBindingsDescriptor {
    // Reflectable draft binding recipes and adapter policy.
}

impl TryFrom<OrbitCamBindingsDescriptor> for OrbitCamBindings {
    type Error = OrbitCamBindingsError;

    fn try_from(descriptor: OrbitCamBindingsDescriptor) -> Result<Self, Self::Error> {
        validate_bindings(&descriptor)
    }
}
```

`OrbitCamInputModeDescriptor` is editable draft state, not the source the controller
trusts. It may be temporarily invalid while a tool mutates fields one at a time, so do
not force it through typestate constructors. The runtime systems consume the exclusive
input-mode components, which are only changed after descriptor validation succeeds.
Scenes and editor files should serialize `OrbitCamInputModeDescriptor`, not
`OrbitCamBindings` internals. If a keymap or scene format uses Serde, implement
`Serialize`/`Deserialize` for the descriptor types; if it uses Bevy scene reflection,
register equivalent reflected serialization. Runtime load/apply validates the
descriptor in `PreInput` and changes the runtime input-mode component only after
validation succeeds.

The internal apply step runs automatically on `Changed<OrbitCamInputModeDescriptor>` in
`OrbitCamInputPhase::PreInput` before input-mode
reconciliation:

```text
Changed<OrbitCamInputModeDescriptor>
  -> try_build a validated input-mode component insertion
      -> success: insert exactly one input-mode component, emit OrbitCamInputModeApplied,
         set OrbitCamInputModeApplyStatus.state to OrbitCamInputModeApplyState::Applied
      -> rejection: keep previous input-mode component, emit OrbitCamInputModeRejected,
         set OrbitCamInputModeApplyStatus.state to OrbitCamInputModeApplyState::Rejected with the error
```

Expose both events for reactive app code and a persisted status component for
reflect/inspector clients:

```rust
#[derive(Event, Clone, Debug)]
pub struct OrbitCamInputModeApplied {
    pub camera: Entity,
}

#[derive(Event, Clone, Debug)]
pub struct OrbitCamInputModeRejected {
    pub camera: Entity,
    pub error: OrbitCamBindingsError,
}

#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component)]
pub struct OrbitCamInputModeApplyStatus {
    pub state: OrbitCamInputModeApplyState,
    pub last_error: Option<OrbitCamBindingsError>,
    pub last_applied_frame: Option<u64>,
}

#[derive(Clone, Debug, Reflect)]
pub enum OrbitCamInputModeApplyState {
    Applied,
    Rejected,
}
```

The rejection path must be explicit: leave the currently installed input-mode
component and private input installation in place, update
`OrbitCamInputModeApplyStatus`, emit `OrbitCamInputModeRejected`, and log a clear
diagnostic. Do not silently fall back to a preset and do not partially install an
invalid custom binding draft.
Descriptor apply, validation, mode exclusivity, private installation replacement, and
old-installation cleanup all run inside the same exclusive `PreInput` structural
boundary. Do not split descriptor apply into an ordinary command-buffered system whose
commands become visible only after reconciliation. A descriptor changed in a frame with
input events must leave exactly one private installation observable by enhanced input
for that frame.

`OrbitCamInputModeApplyStatus` is point-in-time descriptor feedback, not a complete
statement about the current runtime mode. Editor tools should compare
`last_applied_frame` with their own edit/apply bookkeeping or query the current
input-mode component when they need to know whether an applied descriptor is still
the active runtime configuration.
Do not clear `OrbitCamInputModeApplyStatus` just because
`OrbitCamInputModeDescriptor` is removed. The status reports the last descriptor apply
attempt. Removing the draft descriptor does not roll back the validated runtime
input-mode component. Editor tools that need current truth should query the active
input-mode component directly.
Bevy change detection coalesces multiple descriptor field mutations in the same frame
into one `Changed<OrbitCamInputModeDescriptor>` apply attempt. Editors that need
per-edit validation should run the same validator directly against their draft before
writing the component; component change detection reports only the final descriptor
state for the frame.

### `OrbitCamBindings`

`OrbitCamBindings` is a data spec that `bevy_lagrange` turns into enhanced-input
action entities and adapter policy. It should have private fields and be constructed
through local builder/spec APIs. The public API should either intentionally re-export
enhanced-input binding types as part of the `bevy_lagrange` semver surface or wrap
them behind Lagrange-specific constructors. The default should be to wrap where that
keeps the camera API stable and lets the implementation adapt to upstream changes.
Do not add public or planned private witness-wrapper types just to prove validation at
the field level in the initial design. The runtime safety boundary is private fields
plus one shared validator used by every construction path. Add internal wrapper types
later only if the implementation becomes clearer with them.
`OrbitCamBindings` is the validated runtime representation. Do not derive
field-by-field reflection for it if that exposes unchecked internals. Reflected
editing should happen through `OrbitCamBindingsDescriptor`; converting a descriptor
into `OrbitCamBindings` must run the same validation as the builder. If the runtime
type needs to be registered for `OrbitCamBindings` reflection, use Bevy's
supported opaque/custom reflection path rather than making raw binding fields mutable
through reflection.
The reflected runtime shape should be read-only or opaque. A future implementation
may wrap the runtime value in a `ValidatedOrbitCamBindings` newtype internally if that
makes the descriptor-to-runtime authority boundary clearer, but public reflected
mutation must always go through `OrbitCamBindingsDescriptor`.

It contains two kinds of configuration:

- ordinary enhanced-input bindings for public semantic actions;
- adapter policy for sources enhanced input does not currently describe richly enough.

Conceptual shape:

```rust
#[derive(Component, Debug, Reflect)]
#[reflect(Component)]
pub struct OrbitCamBindings {
    orbit: OrbitCamOrbitActionBindings,
    pan: OrbitCamPanActionBindings,
    zoom_smooth: OrbitCamZoomSmoothActionBindings,
    zoom_coarse: OrbitCamZoomCoarseActionBindings,
    wheel: OrbitCamWheelBinding,
    pinch: OrbitCamPinchBinding,
    touch: Option<OrbitCamTouchBinding>,
    gamepad: CameraInputGamepadSelectionPolicy,
    zoom_direction: ZoomDirection,
    button_drag_zoom: Option<OrbitCamButtonDragZoomBinding>,
}

pub struct OrbitCamOrbitActionBindings(ActionBindingSet<OrbitCamOrbitAction>);
pub struct OrbitCamPanActionBindings(ActionBindingSet<OrbitCamPanAction>);
pub struct OrbitCamZoomSmoothActionBindings(
    ActionBindingSet<OrbitCamZoomSmoothAction>,
);
pub struct OrbitCamZoomCoarseActionBindings(
    ActionBindingSet<OrbitCamZoomCoarseAction>,
);

mod sealed {
    pub trait Sealed {}
}

/// Marker trait for `bevy_lagrange` camera actions.
///
/// This trait is sealed and cannot be implemented outside `bevy_lagrange`.
pub trait CameraSemanticAction: InputAction + sealed::Sealed {}
pub trait HeldCameraAction: CameraSemanticAction {}
pub trait ImpulseCameraAction: CameraSemanticAction {}

pub struct ActionBindingSet<A: CameraSemanticAction> {
    entries: Vec<ActionBindingEntry<A>>,
}

pub struct ActionBindingEntry<A: CameraSemanticAction> {
    binding: BindingRecipe,
    sources: CameraInteractionSources,
    route: BindingRoutePolicy,
    engagement: BindingEngagement,
    action: PhantomData<A>,
}

#[derive(Clone, Debug, Reflect)]
pub enum BindingRecipe {
    Key(KeyCode),
    MouseButton(MouseButton),
    MouseMotion(MouseMotionRecipe),
    GamepadButton(GamepadButton),
    GamepadAxis(GamepadAxisRecipe),
    EnhancedInput(EnhancedInputBindingRecipe),
}

#[derive(Clone, Copy, Debug, Reflect)]
pub enum BindingEngagement {
    Impulse,
    Held,
}

#[derive(Clone, Debug, Reflect)]
pub struct EnhancedInputBindingRecipe {
    binding: EnhancedInputBindingDescriptor,
    modifiers: Vec<EnhancedInputModifierDescriptor>,
    conditions: Vec<EnhancedInputConditionDescriptor>,
}

```

Each action binding entry is typed by semantic action, not just output value. This
keeps invalid combinations such as pan bindings accidentally installed as orbit
bindings out of the ordinary API even though both actions output `Vec2`.
The `OrbitCamOrbitActionBindings`, `OrbitCamPanActionBindings`,
`OrbitCamZoomSmoothActionBindings`, and `OrbitCamZoomCoarseActionBindings` newtype
wrappers are the safety mechanism; do not inline them into raw `ActionBindingSet<A>`
fields in a future refactor unless an equivalent type-level guard remains.
Use sealed marker traits such as `CameraSemanticAction`, `HeldCameraAction`, and
`ImpulseCameraAction` so only Lagrange camera action types can appear in camera
binding entries. Do not allow arbitrary `InputAction` implementors in
`ActionBindingEntry<A>`.
Use the standard `sealed::Sealed` module pattern and say in rustdoc that downstream
crates cannot implement these traits. Binding errors should expose stable action names
for programmatic handling without requiring external code to inspect sealed action
types:

```rust
impl OrbitCamBindingsError {
    pub fn action_name(&self) -> Option<&'static str>;
}
```

Installed action entities should also be typed so installation cannot hand a pan
entity to the orbit resolver by mistake:

```rust
pub(crate) struct OrbitActionEntity(Entity);
pub(crate) struct PanActionEntity(Entity);
pub(crate) struct ZoomCoarseActionEntity(Entity);
pub(crate) struct ZoomSmoothActionEntity(Entity);
```

`BindingRecipe` is the public reflectable descriptor of the underlying enhanced-input
binding, modifiers, and conditions. It does not infer source metadata or semantic
action; the semantic action comes from the enclosing `ActionBindingSet<A>`, while
source metadata lives on `ActionBindingEntry` and `HeldActionBindingEntry` so the
resolver can report only sources that actually triggered.

Use distinct constructors and entry types for held and impulse bindings instead of a
separate action-phase trait. `HeldActionBindingEntry<A>` should be available only
through held constructors, while impulse constructors should produce ordinary
`ActionBindingEntry<A>` values with `BindingEngagement::Impulse`. Runtime-loaded
descriptors still pass through shared validation so a future impulse action cannot
accidentally bind an engagement action.

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
OrbitCamOrbitActionBindingSpec::mouse_drag(MouseButton::Middle)      // MOUSE
OrbitCamOrbitActionBindingSpec::gamepad_axis(GamepadAxis::RightStick) // GAMEPAD
OrbitCamZoomActionBindingSpec::keyboard_keys(KeyCode::Equal, KeyCode::Minus) // KEYBOARD
```

Do not expose a general public raw enhanced-input escape hatch that installs arbitrary
bindings directly. Low-level enhanced-input use must still produce complete
metadata-bearing Lagrange entries and pass normal validation. For example, a typed
constructor may accept enhanced-input descriptors plus explicit source metadata:

```rust
HeldActionBindingEntry::<OrbitCamOrbitAction>::from_enhanced_input_pair(
    motion_binding,
    engagement_binding,
    CameraInteractionSources::MOUSE,
)

ActionBindingEntry::<OrbitCamZoomSmoothAction>::from_enhanced_input_impulse(
    binding,
    CameraInteractionSources::GAMEPAD,
)
```

This avoids inferring source flags from enhanced-input internals after the fact and
keeps lifecycle events useful for tooling.

Raw enhanced-input bindings added directly to the public semantic actions are not a
complete camera-resolution API unless they also carry Lagrange source metadata and
routing policy. The documented low-level path should therefore build
metadata-bearing binding specs or bundles rather than asking users to attach raw
bindings to camera action entities by hand. Any truly raw unsupported hook should be
`#[doc(hidden)]`, explicitly named as unsupported, and excluded from examples.

Held bindings should be modeled as one irreducible source-aware entry that installs
both movement and engagement state. Do not let motion and engagement drift into
unrelated custom bindings:

```rust
pub struct HeldActionBindingEntry<A: HeldCameraAction> {
    motion: BindingRecipe,
    engaged: BindingRecipe,
    sources: CameraInteractionSources,
    action: PhantomData<A>,
}

pub struct HeldActionBindingBuilder<A, Motion, Engagement> {
    action: PhantomData<A>,
    motion: Motion,
    engagement: Engagement,
}

pub struct Unset;
pub struct Set<T>(T);

impl<A: HeldCameraAction> HeldActionBindingBuilder<A, Unset, Unset> {
    pub fn new() -> Self;
}

impl<A: HeldCameraAction, Engagement> HeldActionBindingBuilder<A, Unset, Engagement> {
    pub fn motion(
        self,
        motion: BindingRecipe,
    ) -> HeldActionBindingBuilder<A, Set<BindingRecipe>, Engagement>;
}

impl<A: HeldCameraAction, Motion> HeldActionBindingBuilder<A, Motion, Unset> {
    pub fn engagement(
        self,
        engagement: BindingRecipe,
    ) -> HeldActionBindingBuilder<A, Motion, Set<BindingRecipe>>;
}

impl<A: HeldCameraAction> HeldActionBindingBuilder<A, Set<BindingRecipe>, Set<BindingRecipe>> {
    pub fn build(
        self,
        sources: CameraInteractionSources,
    ) -> Result<HeldActionBindingEntry<A>, HeldActionBindingError>;
}
```

The builder rustdoc should include the typestate diagram before showing the generic
type parameters:

```text
HeldActionBindingBuilder<A, Unset, Unset>
  -> .motion(...)     -> HeldActionBindingBuilder<A, Set<Motion>, Unset>
  -> .engagement(...) -> HeldActionBindingBuilder<A, Set<Motion>, Set<Engagement>>
  -> .build(...)      -> HeldActionBindingEntry<A>
```

Keep this builder out of the quick-start path. Public examples should prefer
`OrbitCamBindings::builder().held_mouse_orbit(...)` and similar shorthand methods.

The builder should construct that pair together and validate that paired motion and
engagement bindings have compatible sources, activation predicates, and route policy.
`HeldActionBindingEntry` should be opaque: do not expose public fields, unchecked
constructors, or accessors that allow app code to split the motion and engagement
halves and rebuild an inconsistent pair. Reflection, deserialization, or dynamic
keymap loading must go through the same validation path before a held binding can be
installed.

Keep the enhanced-input actions independent, but make the bindings API pair them.
This is necessary because held camera interactions often have motion and engagement
from different physical inputs:

```text
OrbitCamOrbitAction <- MouseMotion
OrbitCamOrbitEngagedAction <- MouseButton::Middle
```

Advanced users who use the low-level escape hatch must still install held motion and
engagement through a metadata-bearing `HeldActionBindingEntry`. The held constructor
installs the private engagement action internally; direct engagement-action wiring is
not public API.
Impulse bindings such as wheel, pinch, and smooth-scroll do not have a held phase and
must not provide an engagement half.

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

Keep this as one ergonomic `OrbitCamBindings` builder path, not separate "simple" and
"advanced" builders. Common operations should be easy methods on the same builder,
while advanced methods remain available when needed:

```rust
let bindings = OrbitCamBindings::builder()
    .orbit_mouse(MouseButton::Left)
    .pan_mouse(MouseButton::Right)
    .zoom_keys(KeyCode::Equal, KeyCode::Minus)
    .wheel_from_preset(OrbitCamPreset::SimpleMouse)
    .build();
```

More precise methods should compose on that same path:

```rust
let bindings = OrbitCamBindings::builder()
    .held_mouse_orbit(MouseButton::Middle)
    .held_mouse_pan(MouseButton::Middle, KeyCode::ShiftLeft)
    .gamepad_orbit(GamepadAxis::RightStick)
    .with_modifier(EditorViewportFocused)
    .wheel(OrbitCamWheelBinding::blender_like())
    .build();
```

Do not add a second `OrbitCamSimpleBindings` builder. If a common customization feels
too heavy, improve the main builder's method names, defaults, typestate, and examples
instead.

Do not add a separate mid-level helper API for "simple custom" bindings. The one
`OrbitCamBindings` builder should be progressive enough to cover the ladder from
light rebinds to advanced enhanced-input descriptors:

```rust
// 1. Preset
OrbitCamPreset::SimpleMouse;

// 2. Preset swap
OrbitCamPreset::BlenderLike;

// 3. Light custom
let bindings = OrbitCamBindings::builder()
    .orbit_mouse(MouseButton::Left)
    .pan_mouse(MouseButton::Right)
    .zoom_keys(KeyCode::Equal, KeyCode::Minus)
    .wheel_from_preset(OrbitCamPreset::SimpleMouse)
    .build();

// 4. Full custom
let bindings = OrbitCamBindings::builder()
    .held_mouse_orbit(MouseButton::Middle)
    .gamepad_orbit(GamepadAxis::RightStick)
    .wheel(OrbitCamWheelBinding::blender_like())
    .build();

// 5. Manual
OrbitCamManual;
```

Builder rustdoc should include this decision tree before introducing lower-level
held-entry, source-metadata, or adapter-conflict terminology.

Gamepad ownership is shared camera-input device policy, not orbit-camera action
policy. Custom gamepad bindings should make controller selection explicit:

```rust
pub enum CameraInputGamepadSelectionPolicy {
    Any,
    Selected(Entity),
    Disabled,
}
```

Held gamepad pairs must carry the same `CameraInputGamepadSelectionPolicy` on the
motion and engagement halves. A selected-gamepad axis paired with an any-gamepad
button is invalid because selection changes can break held ownership mid-gesture.
If the selected gamepad changes or disconnects during an active held interaction,
latch reconciliation emits `OrbitCamInteractionEnded` for the old owner before any
new selected gamepad can acquire the source.
Document how disconnected selected gamepads are handled. The default custom gamepad
example should use a selected gamepad when one is available, show a no-gamepad
fallback, and avoid accidentally letting every connected controller drive the camera.

Wheel policy needs a typestate builder, or an equivalent compile-time constrained API,
so custom users must intentionally choose adapter-owned wheel behavior or disabled
wheel behavior. Preset/custom input modes should not expose raw `MouseWheel` binding
helpers.
Low-level enhanced-input descriptor methods that could conflict with Lagrange's wheel
adapter should only be available in a builder state where the adapter-owned wheel
policy has been disabled. The ordinary builder path should make the conflict
unrepresentable where practical, and the shared validator remains the fallback for
reflected or dynamically loaded descriptors.

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
Do not add `build_with_wheel_disabled()` or another one-call escape hatch that hides
the wheel decision. Prototype code should still make the choice visible with
`.wheel(OrbitCamWheelBinding::Disabled)` or `.wheel_from_preset(...)`.

Provide preset shortcuts so custom users do not need to study wheel policy before the
first compile:

```rust
OrbitCamBindings::builder()
    .orbit_drag(MouseButton::Middle)
    .wheel_from_preset(OrbitCamPreset::SimpleMouse)
    .build();
```

`MissingWheelPolicy` should recommend `OrbitCamWheelBinding::ZoomOnly` as the safe
manual choice and `wheel_from_preset(...)` as the easiest preset-matching choice.
`wheel_from_preset(preset)` copies only that preset's wheel policy into a custom
binding builder. It does not switch the whole camera to the preset. Users who want the
entire preset should use `OrbitCamPreset::to_bindings()` or insert the preset
component directly.

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

Builder rustdoc should be progressive: start with common mouse, keyboard, gamepad, and
wheel methods, then introduce held-entry terminology, source metadata, low-level
enhanced-input descriptors, and adapter conflict rules only in later sections.

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
`bindings`. The input-mode reconciler replaces the camera's
library-owned input installation, so the old custom bindings do not remain active.

Manual input remains unrestricted: a manual user can read any Bevy input source and
write `OrbitCamInput` through the public helper methods.

### Binding Invariants

Public docs for `OrbitCamBindings` should list the binding rules before introducing
low-level types. Users should not need to discover these rules from failed validation.

| Rule | Example | Fix |
|------|---------|-----|
| Choose exactly one wheel policy. | Custom bindings omit wheel setup. | Call `.wheel_from_preset(OrbitCamPreset::SimpleMouse)` or `.wheel(OrbitCamWheelBinding::Disabled)`. |
| Use held constructors for drags and held gamepad controls. | Mouse motion is bound without a held mouse button. | Use `.held_mouse_orbit(...)`, `.held_mouse_pan(...)`, or the matching gamepad held constructor. |
| Do not provide engagement state for impulses. | Wheel zoom attempts to add a held engagement half. | Configure wheel, pinch, smooth-scroll, or touch through adapter policy. |
| Keep held motion and engagement in the same source family. | Mouse motion plus gamepad button. | Use one mouse-held pair or one gamepad-held pair. |
| Keep held motion and engagement on compatible route policies. | Cursor-routed mouse motion plus global engagement. | Use a constructor that records the same route policy for both halves. |
| Preserve source metadata per binding entry. | Keyboard and gamepad both feed zoom. | Let each entry carry `KEYBOARD` or `GAMEPAD`; do not infer from the merged action value. |
| Do not double-bind adapter-owned raw sources. | `Binding::MouseWheel` plus enabled Lagrange wheel policy. | Configure wheel through `OrbitCamWheelBinding` or disable Lagrange wheel policy. |
| Use descriptors for reflected/dynamic edits. | A keymap UI mutates runtime `OrbitCamBindings` fields. | Edit `OrbitCamBindingsDescriptor`, then validate and apply. |

### Binding Validation

`OrbitCamBindings` construction should have one strict validation path:

```rust
#[derive(Clone, Debug, Reflect)]
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
should re-check custom bindings on `Changed<OrbitCamBindings>`.
Descriptor-driven reflection must validate before inserting
`OrbitCamBindings`; on rejection it leaves the previous runtime input-mode
component in place, emits `OrbitCamInputModeRejected`, updates
`OrbitCamInputModeApplyStatus`, and logs a clear error.

All construction paths must share one validation implementation:

```rust
fn validate_bindings(
    descriptor: &OrbitCamBindingsDescriptor,
) -> Result<OrbitCamBindings, OrbitCamBindingsError>;
```

The typestate builder may make common invalid states unrepresentable, but its final
`build`/`try_build` path should still call this same validation function. Reflection,
deserialization, dynamic keymaps, and preset constructors should also pass through it
or through a validated `OrbitCamBindings` value produced by it.
Reflection and dynamic keymap paths should reject `MissingWheelPolicy`; they should
not silently default it. Defaults belong in presets and explicit builder shortcuts
such as `wheel_from_preset(...)`, not in descriptor validation.

`HeldBindingSourceMismatch` means the motion binding and engagement binding do not
share a compatible source category, route policy, or activation predicate. Accepted
examples:

```text
MouseMotion + MouseButton::Middle -> MOUSE
GamepadAxis::RightStick + GamepadButton::RightStick -> GAMEPAD
```

Rejected examples:

```text
MouseMotion + GamepadButton::South
MouseMotion routed by cursor + MouseButton routed globally
MouseMotion with Shift condition + MouseButton without the same condition
```

Compatibility should be documented as a small matrix:

| Motion half | Engagement half | Result |
|-------------|-----------------|--------|
| cursor-routed mouse motion | same mouse button and same modifier predicates | valid |
| cursor-routed mouse motion | global mouse button with no cursor route metadata | reject |
| gamepad axis scoped to selected gamepad | button from the same selected gamepad | valid |
| gamepad axis scoped to selected gamepad | keyboard key or any-gamepad button | reject |
| gamepad axis scoped to selected gamepad | button scoped to a different selected gamepad or `Any` | reject |
| motion with a condition/deadzone predicate | engagement with the same activation predicate family | valid |
| motion with a condition/deadzone predicate | engagement without that predicate | reject |

Route policy must be stored on the binding entry, not inferred from the binding recipe
alone. `try_build` can reject incompatible held pairs when both motion and engagement
entries carry route metadata. If a future low-level enhanced-input descriptor cannot
provide enough information until installation, reconciliation should reject it through
the same `HeldBindingSourceMismatch` error, emit `OrbitCamInputModeRejected`, and
leave the previous runtime input mode installed.

Adapter conflict validation should run in `try_build` before installation:

| Adapter-owned source | Conflicting public binding |
|----------------------|----------------------------|
| `MouseWheel::Line` | raw enhanced-input `Binding::MouseWheel` |
| `MouseWheel::Pixel` | raw enhanced-input `Binding::MouseWheel` |
| `PinchGesture` | any future raw pinch binding once enhanced input exposes it |
| `Touches` | any future raw touch binding once enhanced input exposes it |

Conceptual validation:

```rust
match binding_recipe.binding() {
    EnhancedInputBindingDescriptor::MouseWheel if wheel_policy.is_enabled() => {
        Err(OrbitCamBindingsError::AdapterBindingConflict {
            source: CameraInteractionSources::WHEEL | CameraInteractionSources::SMOOTH_SCROLL,
            action,
        })
    }
    _ => Ok(()),
}
```

Suggested `Display` text:

| Error | Message |
|-------|---------|
| `AdapterBindingConflict` | "binding conflicts with Lagrange's {source} adapter for {action}; configure this source through OrbitCamBindings adapter policy instead" |
| `HeldBindingWithoutEngagement` | "{action} is a held binding but has no engagement binding; use the held_* builder constructor" |
| `EngagementBindingForImpulse` | "{action} is an impulse binding and cannot have an engagement action" |
| `HeldBindingSourceMismatch` | "{action} motion and engagement bindings do not share compatible source, route, or condition policy" |
| `AmbiguousWheelPolicy` | "wheel policy is ambiguous; choose one line/pixel wheel policy" |
| `MissingWheelPolicy` | "custom bindings must choose a wheel policy; use wheel_from_preset(SimpleMouse), wheel_from_preset(BlenderLike), ZoomOnly, or Disabled" |

Actionable remediation text should accompany these messages in `Display` or in a
structured diagnostic helper. For example:

- `HeldBindingWithoutEngagement`: "Use `.held_mouse_orbit(...)`,
  `.held_mouse_pan(...)`, or the matching gamepad held constructor so motion and held
  state are installed together."
- `EngagementBindingForImpulse`: "Wheel, pinch, smooth-scroll, and gesture bindings
  are impulses; configure them through wheel, pinch, touch, or adapter policy rather
  than an engagement action."
- `HeldBindingSourceMismatch`: "Motion and engagement must share the same source
  family and routing policy. For mouse drag, use a held mouse constructor; for
  gamepad, use a held gamepad constructor."

Public rustdoc should include an "Error Reference" table for the same variants. Each
entry should name:

- the validation rule that failed;
- the affected camera action when known;
- the constructor or builder method that fixes the common case;
- a short dynamic-keymap example that returns or displays the structured error.

Every warning or error log from descriptor apply and reconciliation should include
the camera entity, attempted input mode, and `OrbitCamBindingsError` display text.

### Input Installation Ownership

Preset and custom input modes install private enhanced-input actions, bindings, adapter
state, and mock state for a camera. Those implementation entities are not scene
hierarchy children. Model their ownership with a private Bevy relationship rather
than `ChildOf`:

```rust
#[derive(Component)]
#[relationship(relationship_target = OrbitCamInputInstallation)]
struct OrbitCamInputInstallationOf(#[relationship] Entity);

#[derive(Component)]
#[relationship_target(relationship = OrbitCamInputInstallationOf, linked_spawn)]
struct OrbitCamInputInstallation(Vec<Entity>);
```

Use a custom relationship rather than `ChildOf` even though `ChildOf` can also provide
despawn cleanup. These entities are semantic input-installation entities, not scene or
UI hierarchy children. The custom relationship gives reconciliation a precise query
for "all private input entities owned by this camera" without mixing them with any
other child entities an app may attach to the camera.
`OrbitCamInputInstallation` lives on the camera as the input installation record.
`OrbitCamInputInstallationOf(camera)` lives on private enhanced-input, binding, and
adapter entities that belong to that camera's input installation.
Add a private helper for tests and debug tools so the relationship graph is inspectable
without making the relationship public:

```rust
pub(crate) fn installed_input_entities(world: &World, camera: Entity) -> Vec<Entity>;
```

Changing the active input-mode component replaces the whole private input
installation:

```text
Added/Changed<OrbitCamPreset>
Added/Changed<OrbitCamBindings>
Added<OrbitCamManual>
RemovedComponents<OrbitCamPreset | OrbitCamBindings | OrbitCamManual>
  -> finish active camera-input interactions
  -> clear OrbitCamInput for that camera
  -> clear the owner latch if that camera owns input
  -> despawn_related::<OrbitCamInputInstallation>()
  -> install the new preset/custom input entities, install nothing for manual mode,
     or restore the default preset if no mode remains
```

The relationship owns structural cleanup. A scheduled reconciliation system owns the
semantic cleanup because it must emit interaction end events and clear stale intent
before any animation or controller system can consume input.

## Semantic Actions

The public enhanced-input actions are semantic, not device-specific. The public action
surface names user intent that app tooling may reasonably inspect: orbit movement, pan
movement, coarse zoom, and smooth zoom. Their entity installation, engagement actions,
private adapter source actions, and relationship wiring remain internal.
Most users should configure actions through `OrbitCamBindings`.
Action marker names end in `Action` because they are zero-sized enhanced-input tags,
not per-frame values. `OrbitDelta`, `PanDelta`, `OrbitCamInteractionKind::Orbit`, and
`OrbitCamOrbitAction` should read as different roles.

```rust
#[derive(InputAction)]
#[action_output(Vec2)]
pub struct OrbitCamOrbitAction;

#[derive(InputAction)]
#[action_output(Vec2)]
pub struct OrbitCamPanAction;

/// Discrete step zoom, usually line-wheel scroll or key/button zoom.
#[derive(InputAction)]
#[action_output(f32)]
pub struct OrbitCamZoomCoarseAction;

/// Continuous zoom delta, usually trackpad pixel scroll, pinch, or drag zoom.
#[derive(InputAction)]
#[action_output(f32)]
pub struct OrbitCamZoomSmoothAction;
```

Private engagement actions are resolver plumbing:

```rust
pub(crate) struct OrbitCamOrbitEngagedAction;
pub(crate) struct OrbitCamPanEngagedAction;
pub(crate) struct OrbitCamZoomEngagedAction;
```

`OrbitCamOrbitEngagedAction` exists internally because orbit motion and orbit
interaction state are different facts:

- `OrbitCamOrbitAction` is how much to rotate this frame.
- `OrbitCamOrbitEngagedAction` is whether the user's current control scheme is
  actively orbiting.

The controller needs the engagement edge to preserve the current orbit-drag latch,
including upside-down yaw behavior. A user can press the orbit control and hold still;
the motion delta is zero, but the interaction has still started.

Pan and zoom engagement are also semantic actions because held pan and held zoom can
be active with zero delta. Button-held pan and button-drag zoom must not infer
interaction phase only from movement. The resolver and adapter should derive
interaction state from action timing and source state for all interaction kinds.
Keep these engagement actions private. Client UI does not need to name them to show
which input source is active; public consumers should use
`OrbitCamInteractionStarted`, `OrbitCamInteractionEnded`,
`OrbitCamInteractionSourcesChanged`, and `OrbitCamInteractionState`.
On release, engagement state is authoritative for ending held interactions. A zero
motion delta while engagement remains true is still an active held interaction; a
release edge should end the interaction in that frame even if the motion action
reports zero. Tests should cover mouse-button release and ensure exactly one ended
event is emitted without a one-frame lag.

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
| Touch default | `OrbitCamTouchBinding::OneFingerOrbit` | `OrbitCamTouchBinding::OneFingerOrbit` |
| Zoom direction | normal | normal |
| Button-drag zoom | disabled unless configured | disabled unless configured |

`OrbitCam::default()` should resolve to the mouse-oriented `SimpleMouse` preset.
`BlenderLike` remains the opinionated editor preset, but it should be explicit at the
spawn site so readers can see when a camera uses Blender-style controls.

Presets should be implemented as binding constructors, not as a separate resolver
path:

```rust
impl OrbitCamPreset {
    pub fn to_bindings(self) -> OrbitCamBindings;
}
```

Reconciliation should always operate on an `OrbitCamBindings` value internally. This
keeps preset and custom validation, installation, source attribution, and adapter
policy on the same code path, and lets users start from a preset and customize it.

### Wheel And Smooth Scroll

`OrbitCamWheelBinding` should make wheel and smooth-scroll policy explicit:

```rust
pub enum OrbitCamWheelBinding {
    Disabled,
    ZoomOnly,
    PlatformNatural,
    BlenderLike(OrbitCamBlenderLikeWheelBinding),
}

pub struct OrbitCamBlenderLikeWheelBinding {
    pan_modifier: OrbitCamWheelModifier,
    zoom_modifier: OrbitCamWheelModifier,
}

pub enum OrbitCamWheelModifier {
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
Because the internal context-gating phase may deactivate or reset enhanced-input
action state before the adapter runs, pinch suppression should use a modifier snapshot
captured only for the source-routed camera after routing/blocker computation and the
context-gating decision are known, but before that routed camera's relevant action
state is reset. Inactive or non-routed cameras must not contribute to the snapshot.
The adapter should read that snapshot during the internal adapter-injection phase
rather than relying on post-reset action state.
Store the snapshot in a private per-frame resource written inside the exclusive
`PreInput` phase after route/gating scope is known, before adapter injection:

```rust
pub(crate) struct PinchSuppressionSnapshot {
    camera: Entity,
    is_suppressed: bool,
}
```

The resource should be keyed by camera when multiple cameras are routeable. The pinch
adapter is the only reader. Tests should cover modifier-held pinch suppression,
modifier-release pinch activation, suppression scoped to the routed camera, and a
modifier held on a non-routed camera not suppressing pinch on the routed camera.

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
pub struct OrbitCamButtonDragZoomBinding {
    button: MouseButton,
    axis: OrbitCamButtonDragZoomAxis,
    scale: f32,
}

#[derive(Clone, Copy, Debug, Reflect)]
#[non_exhaustive]
pub enum OrbitCamButtonDragZoomAxis {
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
- `n -> 0` for any active touch arity emits one ended transition for the previous
  touch operation and never synthesizes an intermediate one-finger frame.

Two-finger rotation should stay computed internally but unused until camera roll is
designed.

## Adapter

The adapter is a structured input-policy shim. It preserves source details that
enhanced input does not currently carry and encodes current camera policy for
wheel-unit dispatch, pinch suppression, touch arity, and smooth-scroll routing. Keep
it private and narrow, but do not describe it as a trivial temporary workaround.
Do not add an `enhanced-input-adapters` feature or a separate "pure enhanced-input"
control path. The public API should stay at the camera-policy level:
`OrbitCamWheelBinding`, `OrbitCamPinchBinding`, `OrbitCamTouchBinding`, and related binding
policy types describe what camera input should do, not how unsupported raw sources are
implemented today.
When upstream enhanced input gains first-class line scroll, pixel scroll, pinch,
touch, or gesture bindings, migrate one source at a time: run the adapter and upstream
path side by side in tests, confirm equivalent `OrbitCamInput` output and lifecycle
events, then remove the private adapter path for that source while preserving the
public camera-policy API.

The adapter should:

- use normal enhanced-input bindings where they are expressive enough;
- read `MouseWheel::unit`, `PinchGesture`, `RotationGesture` when roll exists, and `Touches` directly where needed;
- keep source-specific adapter actions private if `ActionMock` is useful for timing;
- avoid mocking public semantic actions that also have normal bindings;
- aggregate public semantic actions and private adapter contributions into `OrbitCamInput`;
- document any `ActionMock` use, because mocked actions skip normal input reading, conditions, and modifiers while active.

Keep a private read-only diagnostics snapshot for tests and debug logging:

```rust
pub(crate) struct AdapterDiagnostics {
    camera: Entity,
    route_allowed: bool,
    injected_sources: CameraInteractionSources,
    dropped_sources: CameraInteractionSources,
}
```

Do not expose this as a public resource in the initial API. If adapter debugging needs
surface in editor tooling later, add a separate debug-feature-gated public diagnostics
type with an explicit use case.

Adapter injection must be visible to enhanced input in the same frame. The schedule
must enforce this structurally, not as an implementation note. Prefer direct mutation
of existing action/mock components; if adapter injection uses `Commands` to insert or
update mock state, the internal adapter-injection phase must run in an exclusive
system or use the Bevy-version-supported structural barrier before
`EnhancedInputSystems::Update`.

Adapter injection must respect the same route and gating decision as enhanced-input
context evaluation. The internal context-gating phase should publish a private marker
or frame-local table equivalent to:

```rust
pub(crate) struct OrbitCamInputContextGated {
    camera: Entity,
    allowed: bool,
}
```

The adapter and resolver should consult that same decision and should assert in debug
that they are not injecting or resolving values for a gated camera. Adapter mocks or
externally injected values must be cleared when a context is gated off, so wheel,
pinch, touch, or smooth-scroll values cannot leak into inactive cameras after a route
swap.

Camera actions should not consume app input by default. Set camera action/binding
consumption so app-owned enhanced-input contexts can still observe shared buttons,
motion, wheel, keyboard, and gamepad input. If a consuming camera binding is ever
needed, expose that as explicit binding policy along with context priority controls
and tests that cover an app context and camera context sharing the same binding.

Preset and custom input modes should route wheel, pinch, touch, and smooth-scroll policy
through `OrbitCamBindings`. Users should not configure private adapter actions.

Public API docs for adapter-backed policy types should have an "Adapter Policies"
section:

| Policy type | Purpose |
|-------------|---------|
| `OrbitCamWheelBinding` | Chooses disabled, zoom-only, platform-natural, or Blender-like line/pixel wheel behavior. |
| `OrbitCamBlenderLikeWheelBinding` | Configures the OrbitCam-specific smooth-scroll split between orbit, pan, and zoom. |
| `OrbitCamWheelModifier` | Names the modifier rule for one Blender-like wheel branch. |
| `OrbitCamPinchBinding` | Enables pinch zoom and optional modifier/condition policy. |
| `OrbitCamTouchBinding` | Chooses one-finger/two-finger orbit and pan interpretation plus touch pinch behavior. |
| `OrbitCamButtonDragZoomBinding` | Maps a held button plus pointer movement into smooth zoom. |
| `OrbitCamButtonDragZoomAxis` | Chooses the pointer axis used by button-drag smooth zoom. |
| `CameraInputGamepadSelectionPolicy` | Chooses which physical gamepad may feed a camera binding. |

These are public camera input policies even though the adapter implementation is
private.

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

OrbitCamTouchBinding::one_finger_orbit()
    .with_condition(TouchViewportFocused);
```

These hooks should support common modifiers and conditions such as deadzones,
scale/sensitivity transforms, viewport-focus predicates, tool-mode predicates, and
custom app predicates. They should not require users to bind private adapter actions
directly.

## Camera Intent And Manual Input

`OrbitCamInput` is a per-camera semantic input snapshot for one frame. It is not raw
device input: routing, bindings, modifiers, adapter policy, and source attribution
have already been applied. The controller reads it, applies camera behavior, and the
input pipeline clears or overwrites it each frame.

The snapshot stores movement deltas and active source sets separately. A helper call
marks an interaction active for that frame even if the delta is zero. This lets manual
and resolved input represent "held but still" input without touching raw fields.
Do not add a cross-frame held-phase enum to `OrbitCamInput`. It is a per-frame input
value; held/ending phase is derived and stored by `OrbitCamInteractionState` plus the
serialized lifecycle queue.

`OrbitCamInput` should expose read-only accessors to app code. Its fields should be
private or `pub(crate)`, and all mutation APIs should be `pub(crate)` except for the
manual writer. App systems can still query `&mut OrbitCamInput`, because it is a Bevy
component, but that mutable reference should not expose useful public setters or
fields. Library systems may use `pub(crate)` mutation APIs, while app-owned manual
writes go through `OrbitCamManualInput`.
Use an internal write-token guard for any direct mutation methods so future setters do
not accidentally become an app-facing bypass:

```rust
pub(crate) struct OrbitCamInputWriteToken;

impl OrbitCamInput {
    pub fn orbit_delta(&self) -> Vec2;
    pub fn pan_delta(&self) -> Vec2;
    pub fn zoom_coarse_delta(&self) -> f32;
    pub fn zoom_smooth_delta(&self) -> f32;

    pub(crate) fn set_orbit_delta(
        &mut self,
        token: OrbitCamInputWriteToken,
        delta: Vec2,
    );
}
```

`OrbitCamInputWriteToken` is not a user-facing API. Library systems and
`OrbitCamManualInputWriter` can construct it internally; external app code can query
`OrbitCamInput` for reading but cannot call mutation methods directly.

Manual users should not normally set value, source, and phase fields directly. The
public manual writer API should be method-based:

| API shape | Source attribution | Use when |
|-----------|--------------------|----------|
| `orbit_pixels`, `pan_pixels`, `zoom_*_amount` | Defaults to `ManualInputSource::manual()` / `MANUAL`. | Prototypes, tests, simple app-authored camera motion. |
| `orbit`, `pan`, `zoom_coarse`, `zoom_smooth` with `ManualInputSource` | Preserves `MANUAL` plus observed source flags such as `KEYBOARD` or `GAMEPAD`. | Editor overlays, guidance UI, analytics, or debugging need source provenance. |
| `*_active` with `ManualInputSource` | Marks a held interaction active without a delta and preserves provenance. | A manual control is held but has no movement this frame. |

```rust
/// Source metadata for app-authored manual camera input.
///
/// This is not an input mode and does not route input. It only records provenance for
/// writes made through [`OrbitCamManualInput`].
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

    pub const fn into_sources(self) -> CameraInteractionSources;
}

impl OrbitCamManualInputWriter<'_> {
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
`ManualInputSource::into_sources` must force the `MANUAL` bit on even if the internal
representation changes; manual writer methods never accept raw
`CameraInteractionSources`.

Document the intended split in rustdoc: use shorthand methods for prototypes, tests,
and simple app-authored motion; use explicit `ManualInputSource` methods when source
attribution should flow into camera interaction events for editor overlays,
analytics, or debugging.
Put the explicit-source methods under an "Advanced: source attribution" rustdoc
heading so simple manual users can start with `orbit_pixels`, `pan_pixels`, and zoom
shorthands without learning provenance rules first.

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
for the frame. The `*_active` helpers exist for held interactions that have no movement
this frame.

`ManualInputSource` always includes `CameraInteractionSources::MANUAL`. Observed
device constructors add source detail without losing provenance:

```text
ManualInputSource::manual()                 -> MANUAL
ManualInputSource::observed_keyboard()      -> MANUAL | KEYBOARD
ManualInputSource::observed_gamepad()       -> MANUAL | GAMEPAD
ManualInputSource::observed_smooth_scroll() -> MANUAL | SMOOTH_SCROLL
```

Manual writers should run in `OrbitCamInputPhase::WriteManual`. The finalization system
runs after that set, clears blocked or stale input, queues lifecycle events, and then
hands finalized input to animation and controller systems.

Manual writes are valid only for cameras with `OrbitCamManual`. Provide a
public helper/query pattern that exposes only manual cameras, and use it in examples:

```rust
fn manual_camera_input(mut cameras: OrbitCamManualInput) {
    for mut camera in cameras.iter_mut() {
        camera.orbit_pixels(-4.0, 0.0);

        camera.pan(
            PanDelta::screen_pixels(0.0, 2.0),
            ManualInputSource::observed_keyboard(),
        );
    }
}
```

`OrbitCamManual` bypasses automatic active-camera routing because the app has
chosen to write a specific camera's input directly. It still respects
`CameraInputDisabled`,
`BlockOnEguiFocus` when present, animation ignore blockers, and other finalization
rules. Preset/custom cameras should not be mutated by app systems in `WriteManual`;
debug builds should warn if a manual writer helper detects an attempted write to a
non-manual camera.
Finalization should also debug-assert the contract: if `OrbitCamInput` was written by
the manual writer path, the camera must have `OrbitCamManual`. This catches
future internal setters or query helpers that accidentally bypass the manual-only
query surface.

Manual screen-pixel orbit and pan deltas require logical surface metrics. In ordinary
window and viewport cases, `bevy_lagrange` should derive those metrics
programmatically from the camera render target, logical viewport, and window. Manual
users only need an explicit surface-metrics override for render-to-texture, offscreen
images, or custom editor surfaces whose input coordinate space is not the camera's
normal window viewport. If metrics cannot be derived or overridden, screen-pixel
manual input should emit `CameraInputMetricsMissing`, log a per-camera one-time
`error!`, and drop rather than guessing.
Metrics are derived once per frame during route resolution and cached on
`ResolvedOrbitCamInputRoute` or equivalent per-camera frame state. Finalization uses
that cached logical-metrics snapshot. If a window resize occurs mid-frame, the new
size is picked up on the next routing pass rather than changing conversions halfway
through input finalization.
The shorthand call-site contract should be explicit in rustdoc:
`orbit_pixels`/`pan_pixels` record intent for the frame and return `()`. The input may
be dropped during finalization if logical surface metrics cannot be derived. Apps that
need synchronous error handling should use an explicit metrics-aware helper if one is
added later; otherwise listen for `CameraInputMetricsMissing`.
Manual input rustdoc should include the async failure pattern for render-to-texture:
subscribe to `CameraInputMetricsMissing`, display the missing logical metric in editor
UI, and add explicit `CameraInputSurfaceMetrics` for the offscreen image or panel.
Do not add `try_orbit_pixels` or `try_pan_pixels` to the default shorthand API in the
initial refactor. The default path should stay ergonomic; metrics failures are
reported through `CameraInputMetricsMissing` and a one-time error log because metrics
are resolved from frame/routing state.

`ZoomInput` uses camera-facing names:

- `coarse` is step-like zoom, usually line wheel input.
- `smooth` is continuous zoom, usually pixel scroll, pinch, or drag zoom.

Keep the public action names `OrbitCamZoomCoarseAction` and
`OrbitCamZoomSmoothAction`; explain discrete step versus continuous delta in rustdoc
rather than renaming to step/continuous.
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
pub enum OrbitCamInteractionKind {
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

Use `bitflags` as a private implementation detail:

```rust
bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub(crate) struct CameraInteractionSourceBits: u32 {
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

`CameraInteractionSources` should be the only public reflected source-set type. Keep
raw bitflags internal:

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect)]
pub struct CameraInteractionSources(CameraInteractionSourceBits);
```

Expose associated constants plus `contains`, `intersects`, `union`, and bit-or
operator ergonomics. Do not expose a public flags type, unchecked constructor,
`bits()`, or `from_bits(...)` in the initial API. Add raw-bit access later only for a
concrete serialization, reflection, BRP, or FFI caller, and keep that path checked.
The type must support the reflection traits needed by the public reflected interaction
events without making the private bit representation directly mutable.
Keep source constants public because event consumers need readable matching code:

```rust
if event.sources.contains(CameraInteractionSources::GAMEPAD) {
    // highlight gamepad guidance
}

if event.sources.intersects(CameraInteractionSources::MOUSE | CameraInteractionSources::WHEEL) {
    // highlight pointer guidance
}
```

Public `CameraInteractionSources` constants should support ordinary `const` unions so
apps can name reusable groups without raw bits:

```rust
const POINTER_SOURCES: CameraInteractionSources =
    CameraInteractionSources::MOUSE.union(CameraInteractionSources::WHEEL);
```

Define the public set API explicitly:

```rust
impl CameraInteractionSources {
    pub const MOUSE: Self;
    pub const KEYBOARD: Self;
    pub const WHEEL: Self;
    pub const SMOOTH_SCROLL: Self;
    pub const PINCH: Self;
    pub const TOUCH: Self;
    pub const GAMEPAD: Self;
    pub const MANUAL: Self;

    pub const fn empty() -> Self;
    pub const fn contains(self, other: Self) -> bool;
    pub const fn intersects(self, other: Self) -> bool;
    pub const fn union(self, other: Self) -> Self;
}

impl BitOr for CameraInteractionSources { ... }
impl BitOrAssign for CameraInteractionSources { ... }
```

Because there is no public raw-bit constructor, ordinary callers can only compose
known source constants. Reflection/deserialization should use a custom or opaque
representation and should not expose the private bitflags field as directly mutable.

Do not include a `CUSTOM` source flag. Custom is a input mode, not an input source.
Custom keyboard bindings should report `KEYBOARD`; custom gamepad bindings should
report `GAMEPAD`; direct manual writes should report `MANUAL`.
`CameraInteractionSources` itself does not require `MANUAL`: ordinary mouse input is
just `MOUSE`, and ordinary gamepad input is just `GAMEPAD`. The `MANUAL` invariant
belongs to `ManualInputSource`, which is the only public path for manual writer source
metadata.

Public events stay simple:

```rust
#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct OrbitCamInteractionStarted {
    #[event_target]
    pub camera: Entity,
    pub kind: OrbitCamInteractionKind,
    pub sources: CameraInteractionSources,
}

#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct OrbitCamInteractionEnded {
    #[event_target]
    pub camera: Entity,
    pub kind: OrbitCamInteractionKind,
    pub sources: CameraInteractionSources,
}

#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct OrbitCamInteractionSourcesChanged {
    #[event_target]
    pub camera: Entity,
    pub kind: OrbitCamInteractionKind,
    pub previous_sources: CameraInteractionSources,
    pub current_sources: CameraInteractionSources,
}

impl OrbitCamInteractionSourcesChanged {
    pub fn added_sources(&self) -> CameraInteractionSources;
    pub fn removed_sources(&self) -> CameraInteractionSources;
}

#[derive(EntityEvent, Reflect)]
#[reflect(Event, FromReflect)]
pub struct CameraInputMetricsMissing {
    #[event_target]
    pub camera: Entity,
    pub missing: CameraInputMetricKind,
}
```

`CameraInputMetricsMissing` intentionally does not carry
`OrbitCamInteractionKind`. It is shared Lagrange-managed input infrastructure; the
event reports which metric is unavailable for the camera, while controller-specific
logs can mention whether orbit or pan input was dropped.

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
previous empty, current non-empty -> OrbitCamInteractionStarted
previous non-empty, current empty -> OrbitCamInteractionEnded
```

If another source joins while an interaction is already active, no second started event
is emitted. If one source ends while another remains active, no ended event is emitted.
Instead, emit `OrbitCamInteractionSourcesChanged` whenever the active source set changes
without starting or ending the interaction as a whole.
Multi-source held interactions use source-set transitions, not one event per source:

```text
Frame N:   orbit active from MOUSE              -> Started(MOUSE)
Frame N+1: GAMEPAD joins orbit                  -> SourcesChanged(MOUSE | GAMEPAD)
Frame N+2: MOUSE releases, GAMEPAD remains held -> SourcesChanged(GAMEPAD)
Frame N+3: GAMEPAD releases                     -> Ended(GAMEPAD)
```

If input becomes blocked while an interaction is active, emit `OrbitCamInteractionEnded`
before suppressing further input so guidance overlays and editor tools do not get
stuck highlighted.

Lifecycle events should describe input that the controller will observe this frame.
`Finalize` should compute and queue lifecycle events after ordinary blockers have
cleared. A pre-controller guard should run after late animation/blocker changes and
before `orbit_cam`; it flushes queued events only if input still reaches the
controller, or replaces them with the needed ended events when a late blocker suppresses
input. This prevents editor overlays from highlighting input that was dropped by a
late `CameraInputInterruptBehavior::Ignore` animation.

All lifecycle changes should pass through one serialized internal queue so route
cleanup, input-mode reconciliation, despawn cleanup, blocker finalization, and the
pre-controller guard cannot emit duplicate or contradictory events:

```rust
pub(crate) struct OrbitCamInputLifecycleQueue {
    // Deduplicated by camera + kind + transition for the current frame.
}

pub(crate) enum LifecycleState {
    Inactive,
    Active(CameraInteractionSources),
    ImpulsePair(CameraInteractionSources),
}
```

The queue should expose transition methods rather than open-coded event pushes. Adding
a future interaction kind should require calling the same transition API, not copying
the deduplication procedure.
The queue is idempotent for duplicate transitions in one frame. Two systems that both
observe `Inactive -> Active` for the same camera/kind/source set produce one started
event. Two cleanup paths that both observe the same active interaction ending produce
one ended event. If same-frame transitions conflict, the terminal state after all
ordered input phases wins and the queued events must leave
`OrbitCamInteractionState` consistent with that terminal state.

The finalization invariant is:

1. Resolve current active sources for each camera and interaction kind.
2. Compare current sources against `OrbitCamInteractionState` previous sources.
3. Queue lifecycle events and source-change events.
4. Update stored previous sources for the next frame.

Impulse-only interactions are a paired write in that same critical section:
`OrbitCamInteractionStarted` and `OrbitCamInteractionEnded` are queued together, then the
stored previous source set is left empty for the next frame. A late blocker in the
pre-controller guard may cancel the queued started event or replace queued transitions
with an ended event as needed, but it must leave the queue balanced for each camera
and interaction kind.
The pre-controller guard uses the same lifecycle queue before flushing events. If a
late blocker appears after `Finalize`, the guard queues the needed ended transition or
cancels still-queued started/source-change events so observers see the same input the
controller is allowed to consume.

Source lifetime is deterministic:

| Source class | Examples | Lifecycle |
|--------------|----------|-----------|
| Held | mouse-button drags, touch contacts, engaged gamepad controls, manual active calls | starts when held state begins; ends when held state ends |
| Impulse | line wheel, pixel wheel / smooth scroll, pinch gesture delta, pan gesture delta | starts and ends in the frame where the event exists |

Owner latching comes only from held sources. Impulse sources are routed per event by
the event window and current pointer/touch position for that frame. Do not add an
idle-frame grace window for wheel, smooth-scroll, or pinch; presentation layers such
as `fairy_dust` may add visual highlight linger, but camera input semantics stay exact.
For an impulse-only interaction, the lifecycle queue emits `OrbitCamInteractionStarted` and
`OrbitCamInteractionEnded` in the same frame. The impulse exists only for that input
frame; it must not keep the semantic active-source set alive into the next frame just
so the event tracker can observe an empty transition later.

Concrete wheel trace:

```text
frame N:
  resolved zoom_active_sources = WHEEL
  previous zoom sources = empty
  emit OrbitCamInteractionStarted { kind: Zoom, sources: WHEEL }
  controller may consume the zoom delta
  emit OrbitCamInteractionEnded { kind: Zoom, sources: WHEEL }
  stored previous zoom sources for frame N+1 = empty

frame N+1:
  no wheel event
  resolved zoom_active_sources = empty
  no lifecycle event
```

Interaction event rustdoc should include a short cheat sheet:

```text
mouse drag:
  press/hold -> Started(Orbit, MOUSE)
  move       -> no lifecycle event, state remains active
  release    -> Ended(Orbit, MOUSE)

wheel:
  wheel tick -> Started(Zoom, WHEEL), Ended(Zoom, WHEEL) in the same frame

mouse drag + wheel:
  drag start -> Started(Orbit, MOUSE)
  wheel tick -> Started(Zoom, WHEEL), Ended(Zoom, WHEEL)
  drag end   -> Ended(Orbit, MOUSE)
```

Example UI handlers should prefer `OrbitCamInteractionState` for "is highlighted
now?" and use lifecycle events for edge-triggered reactions. That works for both held
and impulse sources.

Expose the current active interaction state as a read-only component so editor tools
and examples do not have to reconstruct state from events:

```rust
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct OrbitCamInteractionState {
    orbit_sources: CameraInteractionSources,
    pan_sources: CameraInteractionSources,
    zoom_sources: CameraInteractionSources,
}

impl OrbitCamInteractionState {
    pub const fn orbit_sources(&self) -> CameraInteractionSources;
    pub const fn pan_sources(&self) -> CameraInteractionSources;
    pub const fn zoom_sources(&self) -> CameraInteractionSources;
}
```

The fields are internal because this component is the library's authoritative
interaction tracker. App code reads it through accessors; library systems mutate it
through internal methods that keep lifecycle events and owner latches consistent.

## Input Disabling And Blockers

Expose a small public app-level disable component:

```rust
#[derive(Component, Default, Reflect)]
#[reflect(Component)]
pub struct CameraInputDisabled;
```

This is separate from the mutually exclusive input-mode components. Disabling input
does not replace the selected preset, custom bindings, or manual mode.

Common pause/resume pattern:

```rust
commands.entity(camera).insert(CameraInputDisabled);

// Later, when the menu or modal closes:
commands.entity(camera).remove::<CameraInputDisabled>();
```

Use `CameraInputDisabled` for temporary pauses such as menus, modal tools, and UI
capture. Use `OrbitCamManual` only when the app takes over writing camera
intent itself.
It is valid but usually redundant to have both `CameraInputDisabled` and
`OrbitCamManual` on a camera: `OrbitCamManual` selects who writes input, while
`CameraInputDisabled` temporarily suppresses whatever input mode is selected.

Transient blockers remain internal library state:

- animation ignore;
- egui pointer/keyboard focus;
- inactive camera routing;
- unavailable owner camera.

No public enum should mix app-owned disabling with library-computed transient blockers.
Input is blocked if `CameraInputDisabled` is present or any internal blocker is
active.

Blocking has two gates.

The internal pre-input phase acts on enhanced-input's state machine before
`EnhancedInputSystems::Update`. Preset and custom contexts that are disabled,
egui-blocked, animation-ignored, inactive, or unrouted should be deactivated or reset
so held-button state, action transition edges, condition timers, and stale action
values do not advance invisibly while the camera cannot consume input.

`Finalize` acts on resolved per-frame intent after all input writers have run.
This includes preset/custom action resolution and user systems in
`OrbitCamInputPhase::WriteManual`. It clears blocked intent, emits lifecycle events,
applies blockers that cannot be expressed inside enhanced input, and enforces owner
latch invariants. A blocked camera must not move, interrupt animation, or keep
guidance highlighted because of stale `OrbitCamInput`.

Both gates must consult `OrbitCamInputBlockers`, the single computed source of truth
for blocker state. They must not re-derive egui, animation, disabled, or routing
blockers independently.
Compute those blockers once in the exclusive `PreInput` phase and store the per-camera
result for the frame. Context gating, adapter injection, action resolution, manual
finalization, and the pre-controller guard read that stored value. If a blocker source
changes after `PreInput`, the late guard may suppress input for safety, but it must not
re-route or re-enable input that was gated off earlier in the frame.

`BlockOnEguiFocus` should feed the internal UI-focus blocker. The blocker must preserve
current behavior:

- use `EguiWantsFocus::prev || EguiWantsFocus::curr` to avoid a one-frame leak;
- respect `EguiFocusIncludesHover`;
- collect egui focus state before input blocker computation;
- block context evaluation, adapter injection, action resolution, and finalized
  manual input from the same computed blocker state;
- emit `OrbitCamInteractionEnded` for active interactions before suppressing further input.

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
    ///
    /// This still routes ordinary preset/custom camera input. It does not make the
    /// app write [`OrbitCamInput`] directly; use [`OrbitCamManual`] plus
    /// [`OrbitCamManualInput`] for manual writes.
    Explicit,
}
```

`CameraInputRouting::Explicit` is distinct from `OrbitCamManual`:

```text
CameraInputRouting::Explicit
  app chooses which camera receives input

OrbitCamManual
  app writes OrbitCamInput directly
```

Keep public routing configuration separate from internal resolved routing state. The
public API should express only the app's routing preference and explicit target. The
library should keep an internal resource for the resolved route:

```rust
pub struct CameraInputRoutingConfig {
    pub mode: CameraInputRouting,
    pub explicit_camera: Option<Entity>,
    pub no_position_fallback: NoPositionFallback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[non_exhaustive]
pub enum NoPositionFallback {
    /// Do not route keyboard/gamepad input unless a latch, explicit route,
    /// or unambiguous cursor-hit camera already identifies the camera.
    NoInput,
    /// Route to the only routeable OrbitCam when there is exactly one.
    OnlyEligibleCamera,
}

pub(crate) struct ResolvedOrbitCamInputRoute {
    routed_camera: Option<Entity>,
    held_latches: CameraInputSourceLatches,
    surface_metrics: CameraInputSurfaceMetrics,
    blockers: OrbitCamInputBlockers,
}

pub(crate) struct CameraInputSourceLatches {
    mouse: Option<OrbitCamInputOwnerLatch>,
    keyboard: Option<OrbitCamInputOwnerLatch>,
    gamepads: HashMap<Entity, OrbitCamInputOwnerLatch>,
    touches: HashMap<TouchId, OrbitCamInputOwnerLatch>,
}
```

`CameraInputRoutingConfig` is a public resource. Mutations in any schedule take effect
at the next `OrbitCamInputPhase::PreInput` route phase. Once routing is resolved for a
frame, later `Update` or `PostUpdate` mutations do not retroactively change the camera
that receives that frame's input.

The internal resolved route is rewritten every frame by routing systems and is the
only state that context gating, adapter injection, and manual finalization should
consult.
Expose the derivation as an internal resolver function or builder rather than hiding
it inside unrelated systems:

```rust
impl ResolvedOrbitCamInputRoute {
    pub(crate) fn resolve(
        world: &World,
        config: &CameraInputRoutingConfig,
        previous_latches: &CameraInputSourceLatches,
    ) -> Self;
}
```

Do not implement a simple `From<&CameraInputRoutingConfig>` conversion, because
resolution depends on world state: cursor position, windows, viewports, camera
activity, blockers, gamepads, and previous latches.

Use explicit latch newtypes and named operations rather than mutating bare
`Option<Entity>` values:

```rust
pub(crate) struct OrbitCamInputOwnerLatch(Entity);

impl CameraInputSourceLatches {
    pub(crate) fn acquire_for_held_interaction(
        &mut self,
        source: CameraInteractionSources,
        camera: Entity,
    );

    pub(crate) fn release(&mut self, source: CameraInteractionSources);
}
```

Automatic routing should use per-source interaction owner latches:

```text
When a held camera interaction starts:
  latch the owning camera for that source.

While that source remains held:
  route that source's camera input to its latched owner,
  even if cursor/touch position crosses another viewport.

When that source ends:
  clear that source's latch and allow hit-testing/fallback routing again.
```

Route by source-specific latch first, then no-position fallback, then hit-test. This
lets a mouse drag remain attached to camera A while a selected gamepad can continue
driving camera B. At the start of the route phase, validate existing latches before
applying fallback: if a latched camera is despawned, disabled, inactive, missing
`OrbitCam`, or otherwise unavailable, clear that latch and immediately reroute the
still-held source through the explicit route, fallback, or hit-test rules in the same
phase. Do not leave stale ownership for one extra frame.

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

1. The matching source latch, if that held source already owns a camera.
2. The explicit routing camera, if `CameraInputRouting::Explicit` is active.
3. The current cursor-hit camera, if the cursor is inside exactly one eligible camera
   viewport.
4. No camera input if routing is ambiguous or no eligible camera can be identified.

Document that precise multi-window gesture routing for global Bevy gesture events
requires explicit routing or an unambiguous cursor-hit camera. "Unambiguous" means the
cursor is inside exactly one eligible viewport on one focused window. If multiple
focused windows or viewports can plausibly own a global gesture, drop the gesture for
camera input and log at `debug!`.
If a global gesture impulse is dropped because routing is ambiguous, emit a
rate-limited debug log that names the gesture and the eligible cameras. This keeps the
semantics deterministic while giving users a clue to use explicit routing.
Keep routing diagnostics as logs for now rather than exposing a public debug resource.
Use rate-limited `debug!` logs for:

- ambiguous no-position or global gesture routing;
- source latch acquire/release;
- latch recovery after despawn, disable, focus loss, or disconnected input source;
- routed input dropped because the selected camera is blocked or missing metrics.

Do not expose per-source latch owners as a public resource until implementation proves
that logs are insufficient.
Do not add `OrbitCamInputRouteDebug`, `OrbitCamInputDiagnostics`, or another public
debug resource in the initial refactor. Public diagnostic resources become semver
surface and can freeze internals too early. Start with rate-limited logs; add a
designed read-only diagnostics API later only when an in-tree example, editor need, or
user PR shows what data should be stable.

No-position held sources such as keyboard or gamepad use the same routing family.
Their binding entries should declare that they have no pointer position. Automatic
routing should use:

1. The matching source latch, if that held source already owns a camera.
2. The explicit routing camera, if `CameraInputRouting::Explicit` is active.
3. The current cursor-hit camera, if exactly one eligible camera is under the cursor.
4. The configured `NoPositionFallback`:
   - `NoInput` drops the input.
   - `OnlyEligibleCamera` routes only when there is exactly one routeable `OrbitCam`.

The default should be `NoPositionFallback::NoInput` so keyboard or gamepad input never
silently controls an offscreen or unrelated camera. Single-camera apps that want
keyboard/gamepad input even when the cursor is outside the viewport can opt in:

```rust
commands.insert_resource(
    CameraInputRoutingConfig::cursor_hit_test()
        .with_no_position_fallback(NoPositionFallback::OnlyEligibleCamera),
);
```

Store surface metrics per camera for the frame, not only for the routed camera.
Routed preset/custom input uses the routed camera's metrics; manual-mode systems may
write to a camera that is not currently routed, so finalization must be able to derive
that camera's metrics independently.

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Reflect)]
#[non_exhaustive]
pub enum CameraInputMetricKind {
    CameraViewSize,
    InputSurfaceSize,
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

Missing metrics are detected in finalization, where the affected camera entity and
missing metric kind are known. Screen-pixel orbit or pan input without metrics should
be dropped, emit a per-camera one-time `error!`, and emit
`CameraInputMetricsMissing` so applications can surface the failure in editor UI or
diagnostics. Do not fall back to physical framebuffer size or scale-factor-multiplied
values.

Enhanced-input action evaluation must also be gated to the source-latched or routed active camera.
Inactive `OrbitCamInputContext` instances must not accumulate action state. The design
should use both sides of the invariant: deactivate or gate inactive contexts before
`EnhancedInputSystems::Update`, and reset their camera action state when route
ownership changes. A context that is inactive for routing must not read input in the
same frame, and a context that becomes active must not resume stale action values from
an earlier route.

If the owning camera becomes blocked, disabled, inactive, despawned, or otherwise
unavailable:

- emit `OrbitCamInteractionEnded` for active interactions when possible;
- clear active interaction state;
- clear the owner.

Routing is locked during the internal route phase of `OrbitCamInputPhase::PreInput` for
the frame. If `Camera::is_active` or equivalent camera activity changes later in the
frame, do not re-route to a different camera. Treat the originally routed camera as
blocked by inactive-camera state, clear or suppress its input through
finalization/pre-controller guard, and allow normal routing to choose again on the
next frame.

Latch recovery must be deterministic. Clear affected source latches immediately on
camera despawn, `OrbitCam` removal, input-mode replacement, `CameraInputDisabled`,
target window close, application focus loss, or selected gamepad disconnect. Each
frame, reconcile each source latch against the underlying held-source state that
created it: if the mouse button is no longer pressed, the touch ID is gone, the
selected gamepad is no longer available, or the target window is synchronously known
to be unfocused/closed, force the corresponding interaction ended event and clear the
latch. Do not rely only on platform focus/window events; those events can feed
diagnostics, but the route phase should inspect current window state where Bevy exposes
it. Do not use an idle-frame grace window for latch recovery.
Model latch transitions explicitly:

```rust
pub(crate) enum LatchState {
    Unlocked,
    Locked(Entity),
    ReleasedPendingCleanup(Entity),
}
```

The routing phase may acquire `Unlocked -> Locked`. Cleanup and source-release
reconciliation move `Locked -> ReleasedPendingCleanup`, queue the needed lifecycle
transition, and then clear to `Unlocked` in the same finalization sequence. The
controller must never observe `ReleasedPendingCleanup`.

Camera despawn and `OrbitCam` removal need an explicit cleanup path. Prefer a system
in the early input phase that reads `RemovedComponents<OrbitCam>` and clears matching
source latches, interaction state, and private input entities before routing for the
frame. Observers may supplement this, but lifecycle cleanup should not depend on
observer ordering during scene teardown or recursive despawn.
Ordering for this cleanup is part of the input contract: relationship-based despawn or
input-mode replacement commands are flushed at the start of the internal `PreInput`
exclusive phase, then the `RemovedComponents<OrbitCam>` cleanup runs before routing,
context gating, adapter injection, and `EnhancedInputSystems::Update`. The cleanup
system owns semantic cleanup for removed cameras: clear source latches, clear
interaction state, queue a single ended lifecycle transition when appropriate, and
remove private input entities. Observers may perform local structural cleanup, but
they must not be the only path that releases latches or queues ended events.

If a selected gamepad disconnects and reconnects in the same frame, the disconnect
clears its source latch first. Any same-frame press after reconnect reacquires through
the normal source-specific fallback rules; it does not inherit the stale latch.
Detect selected-gamepad availability synchronously during `PreInput` by consulting the
current gamepad collection, not only by waiting for connection events. Connection
events can feed diagnostics, but latch recovery should use the authoritative current
set for the frame.

## Scheduling

Add a root-level `system_sets` module and a dedicated plugin called by
`LagrangePlugin`.

```rust
mod system_sets;

pub(crate) struct LagrangeSystemSetsPlugin;
```

This refactor moves input resolution into `PreUpdate` while keeping the camera
controller in `PostUpdate`. Existing `LagrangePlugin` `PostUpdate` controller systems
should be reconciled with this schedule by splitting input collection/resolution from
controller application, not by leaving user-input resolution inside the controller
system.

The module-level docs should include the ordering diagram because the system sets are
the integration contract between Bevy input, enhanced input, adapters, animation, and
the camera controller.

```text
PreUpdate:
  Bevy input has collected raw device state
    -> Apply changed OrbitCamInputModeDescriptor drafts to exclusive input-mode components
    -> Enforce input-mode exclusivity and restore the default mode if none remains
    -> Reconcile changed input-mode components and replace private input installations
    -> Route active camera and update internal input blockers
    -> Gate active camera context/action evaluation
    -> Flush or directly apply structural changes needed by enhanced input
    -> Inject Lagrange adapter values for unsupported sources
    -> bevy_enhanced_input updates action state
    -> Resolve camera actions and adapter contributions into OrbitCamInput
    -> User systems write manual OrbitCamInput
    -> Finalize input:
         1. recover source latches and clear blocked/stale input
         2. resolve current active sources
         3. compare against previous active sources
         4. queue lifecycle/source-change events
         5. update interaction tracker for the next frame

Update:
  Programmatic camera animation requests are queued before camera input can reach the controller
  animation_input_interrupt reads OrbitCamInput
    -> Ignore clears input and lets animation continue
    -> Cancel cancels animation and keeps input
    -> Complete finishes animation and clears input
  process_camera_move_list advances remaining animations

PostUpdate:
  Pre-controller input guard re-checks animation ignore blockers and flushes queued lifecycle events
  OrbitCam controller reads OrbitCamInput
    -> Camera transform targets are updated
    -> OrbitCamInput is cleared
    -> Transform propagation
    -> Camera update systems
```

The exact enhanced-input set names should come from the dependency, but the ordering
shape should keep the public scheduling surface small:

```rust
app.configure_sets(
    PreUpdate,
    (
        OrbitCamInputPhase::PreInput,
        OrbitCamInputPhase::WriteManual,
        OrbitCamInputPhase::Finalize,
    )
        .chain(),
);

app.configure_sets(
    PreUpdate,
    (
        OrbitCamInputPhase::PreInput.after(InputSystems),
        OrbitCamInputPhase::PreInput.before(EnhancedInputSystems::Update),
        OrbitCamInputPhase::Finalize.after(EnhancedInputSystems::Apply),
    ),
);

app.add_systems(
    PreUpdate,
    orbit_cam_pre_input_exclusive.in_set(OrbitCamInputPhase::PreInput),
);

app.add_systems(
    PreUpdate,
    (
        resolve_orbit_cam_actions,
        finalize_orbit_cam_input,
    )
        .chain()
        .in_set(OrbitCamInputPhase::Finalize),
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
        .before(TransformSystems::Propagate)
        .before(CameraUpdateSystems),
);
```

`orbit_cam_pre_input_exclusive` is the structural boundary for descriptor apply,
input-mode exclusivity, reconciliation, removal cleanup, routing, context gating,
and command-buffered adapter setup. It should either mutate the world directly through
exclusive world access or explicitly flush its own commands before returning. The
correctness boundary is this exclusive phase, not an ordinary `apply_deferred` system
placed nearby in the schedule.
This boundary has explicit invariants:

1. Descriptor apply and input-mode exclusivity finish before reconciliation.
2. Removed `OrbitCam` and removed/replaced input modes are semantically cleaned up
   before routing.
3. Route resolution, source-latch reconciliation, and blocker computation produce one
   per-camera frame snapshot.
4. Context gating uses that snapshot and resets inactive action state before
   `EnhancedInputSystems::Update`.
5. Adapter injection reads the same route/gating snapshot and clears adapter mock
   state for gated cameras before `EnhancedInputSystems::Update`.
6. No system outside this boundary can make command-buffered context, relationship,
   or adapter state visible to enhanced input by relying on a nearby deferred flush.

`WriteManual` is a public slot for user systems. `bevy_lagrange` does not normally add
systems to it:

```rust
app.add_systems(
    PreUpdate,
    my_manual_camera_input.in_set(OrbitCamInputPhase::WriteManual),
);
```

Keep the public scheduling surface explicit but small:

```rust
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub enum OrbitCamInputPhase {
    PreInput,
    WriteManual,
    Finalize,
}
```

`OrbitCamInputPhase` is implemented as Bevy system-set variants, but the public name
describes the role users depend on: phases of orbit-camera input processing.
`PreInput` owns the internal descriptor-apply, input-mode exclusivity, reconcile,
route, latch, blocker, context-gating, and adapter-injection phases. Those finer
internal phases should stay `pub(crate)` so downstream apps do not depend on them as
public scheduling slots.
The controller ordering around `orbit_cam` should also stay internal; expose a named
post-update controller set only after a concrete integration needs to order against
the controller as a whole.
Do not expose unstable internal phase sets or add a hook registry preemptively. The
public integration points are input-mode components, `OrbitCamBindings`,
`CameraInputRoutingConfig`, `OrbitCamInputPhase::WriteManual`, interaction events, and
read-only interaction state. Add a new public hook or system set only when there is a
concrete in-tree need or an external PR with a specific integration problem.
Context gating inside `PreInput` is the chosen inactive-context handling path: it owns
action-state hygiene via deactivation/reset before `EnhancedInputSystems::Update`,
rather than leaving inactive contexts running and trying to repair their output later.
Route resolution, latch reconciliation, blocker computation, and context gating must
run as one chained sequence from the perspective of enhanced input; context gating
must not read a route that disagrees with the source-latch table.
Context deactivation/reset should use direct exclusive-world mutation where Bevy's API
allows it. If a specific enhanced-input operation requires commands, those commands
must be flushed inside the exclusive boundary before enhanced input runs.
Any command-buffered entity, relationship, or context-activity changes needed by
enhanced input must be visible before `EnhancedInputSystems::Update`. Use exclusive
systems, or the Bevy-version-supported equivalent structural barrier, for
reconciliation, context gating, and command-buffered adapter injection. Do not rely on
a nearby ordinary `apply_deferred` system as the correctness boundary; audit this
ordering on each supported Bevy upgrade.
All input phases are sequenced with explicit ordering. App systems that mutate
`OrbitCamInput`, routing, bindings, or input-mode components should run in the public
phase intended for that mutation and should not spawn parallel tasks that mutate those
same ECS values concurrently with `OrbitCamInputPhase::*`.
Strict diagnostics should confirm at startup that the Lagrange input sets were
configured and that the enhanced-input update/apply sets are ordered relative to
`PreInput` and `Finalize` as expected for the supported Bevy/enhanced-input versions.
`Finalize` is the last public input set before any animation or controller system
can observe input. It clears blocked manual/preset/custom input, queues lifecycle
events, updates interaction state, and clears source latches when needed. The
pre-controller guard flushes queued lifecycle events after the final blocker check.
Latch recovery and lifecycle event emission are atomic within `Finalize`: if one
source latch is released while another source remains held on another camera, only the
released source's camera gets its ended/source-change event, and the remaining source
continues with its existing owner. The controller must not observe a half-cleared
latch table.

## Animation And Programmatic Motion

`AnimationConflictPolicy` and `CameraInputInterruptBehavior` remain separate policy
axes:

| Situation | Existing policy | Input behavior |
|-----------|-----------------|----------------|
| New programmatic animation arrives while another animation is active. | `AnimationConflictPolicy` | Does not inspect or modify input blockers. |
| User input arrives during animation and policy is `Ignore`. | `CameraInputInterruptBehavior::Ignore` | `Finalize` treats the active animation as an input blocker before lifecycle events are emitted; animation continues and input is not observable. |
| User input arrives during animation and policy is `Cancel`. | `CameraInputInterruptBehavior::Cancel` | `animation_input_interrupt` cancels/removes animation, emits existing cancelled events, and keeps finalized input so user control applies this frame. |
| User input arrives during animation and policy is `Complete`. | `CameraInputInterruptBehavior::Complete` | `animation_input_interrupt` completes/jumps animation, emits existing completion events, and clears input for this frame. |

Finalized `OrbitCamInput` is the user-input interrupt signal for `Cancel` and
`Complete`. `Ignore` is different: active animation plus `Ignore` is an input blocker
inside `Finalize` before started/ended input lifecycle events are emitted.
Finalization should check the authoritative animation state directly, such as
`CameraMoveList` plus the camera's interrupt policy, so observer-driven animation
insertion/removal cannot leave a one-frame stale blocker. Animation interruption
should not depend on detecting later target mutation.

Programmatic animation requests that should affect input in the same frame must be
queued before camera input can reach the controller. If an animation can be inserted
after `Finalize`, run a pre-controller guard in `PostUpdate` before `orbit_cam`
that re-checks authoritative animation state, clears blocked input for `Ignore`, and
flushes or cancels queued lifecycle events so tools only observe input that reaches
the controller. `Cancel` and `Complete` remain handled by `animation_input_interrupt`
for finalized input.
Input-mode replacement during an active `Ignore` animation is atomic from the input
consumer's perspective: clear `OrbitCamInput`, queue at most one `Ended` transition
for each previously active interaction, remove the old private installation, and
install the new private installation inside the same input structural boundary. Do not
emit a new `Started` for the replacement bindings until a later frame produces
observable input that is not blocked by the animation.

Programmatic camera operations do not write `OrbitCamInput` and do not emit camera
input lifecycle events. They continue to use existing events such as `ZoomToFit`,
`PlayAnimation`, `ZoomBegin`, `ZoomEnd`, `AnimationBegin`, and `AnimationEnd`.

## Examples

Each supported input mode should have a small example file named after the mode type.
Keep these as separate examples rather than consolidating them into one parameterized
`input_modes.rs` example. The input-mode examples should use `fairy_dust` so the
camera window can show live guidance text that reacts to `OrbitCamInteractionStarted`
and `OrbitCamInteractionEnded`.

Planned separate examples:

- `examples/orbit_cam_preset_blender_like.rs`
- `examples/orbit_cam_preset_simple_mouse.rs`
- `examples/orbit_cam_bindings_keyboard.rs`
- `examples/orbit_cam_bindings_gamepad.rs`
- `examples/orbit_cam_manual.rs`

Each input-mode example should:

- spawn one `OrbitCam`;
- install exactly one input-mode component;
- show orbit, pan, and zoom guidance text in the camera view;
- highlight the relevant guidance text while the interaction is active;
- display or log the interaction source flags so mouse, wheel, smooth-scroll, pinch,
  touch, keyboard, gamepad, and manual paths can be verified through
  `OrbitCamInteractionState` or `OrbitCamInteractionSourcesChanged`.

`fairy_dust` needs a data-driven camera guidance panel that examples can configure
with rows. The panel should highlight active rows from lifecycle events and optionally
display source flags.

Conceptual API:

```rust
CameraGuidance::for_preset(OrbitCamPreset::BlenderLike)
CameraGuidance::for_preset(OrbitCamPreset::SimpleMouse)
CameraGuidance::custom([
    CameraGuidanceRow::new(OrbitCamInteractionKind::Orbit, "Right stick")
        .when_sources(CameraInteractionSources::GAMEPAD),
    CameraGuidanceRow::new(OrbitCamInteractionKind::Pan, "Left stick + L2")
        .when_sources(CameraInteractionSources::GAMEPAD),
    CameraGuidanceRow::new(OrbitCamInteractionKind::Zoom, "Pinch")
        .when_sources(CameraInteractionSources::PINCH),
    CameraGuidanceRow::new(OrbitCamInteractionKind::Zoom, "Wheel")
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
commands.entity(camera).insert(bindings);
```

`orbit_cam_preset_simple_mouse.rs` should be source-level simple enough to copy into a new
app:

```rust
commands.spawn((
    Camera3d::default(),
    OrbitCam::default(),
    OrbitCamPreset::SimpleMouse,
    CameraGuidance::for_preset(OrbitCamPreset::SimpleMouse),
));
```

`orbit_cam_bindings_keyboard.rs` should show the smallest complete custom binding path:

```rust
let bindings = OrbitCamBindings::builder()
    .orbit_keys(KeyCode::ArrowLeft, KeyCode::ArrowRight, KeyCode::ArrowUp, KeyCode::ArrowDown)
    .pan_keys(KeyCode::KeyA, KeyCode::KeyD, KeyCode::KeyW, KeyCode::KeyS)
    .zoom_keys(KeyCode::Equal, KeyCode::Minus)
    .wheel(OrbitCamWheelBinding::Disabled)
    .build();

commands.entity(camera).insert(bindings);
```

The exact method names can change during implementation, but the example should show
one validated custom binding construction, one input-mode insertion, and one
`fairy_dust` guidance panel that highlights keyboard-sourced interactions.

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
only mutate `OrbitCam` are not enough because input modes now live in mutually exclusive
components. Provide either a closure over `EntityCommands` or a generic bundle-based
builder method such as:

```rust
with_orbit_cam_input_mode(OrbitCamPreset::BlenderLike)
with_camera_guidance(CameraGuidance::for_preset(OrbitCamPreset::BlenderLike))
```

Examples should be able to insert bindings/manual input modes and guidance rows on the
spawned camera without reaching around the helper.

### Render-To-Texture Walkthrough

Render-to-texture is explicit routing, not manual input. Keep these concepts separate:

- Explicit routing tells `bevy_lagrange` which camera receives Bevy's input stream.
- Surface metrics tell `bevy_lagrange` how logical screen-pixel movement maps to that
  camera's rendered surface.
- Manual input mode tells `bevy_lagrange` that the app itself writes orbit/pan/zoom
  intent.

Use this decision tree:

```text
Does the app compute orbit/pan/zoom deltas itself?
  yes -> OrbitCamManual + OrbitCamManualInput
  no  -> preset/custom input modes

Does automatic cursor hit-testing know which camera surface is under the pointer?
  yes -> CameraInputRouting::CursorHitTest
  no  -> CameraInputRouting::Explicit(camera)

Does the camera render to a custom image/panel/offscreen surface?
  yes -> provide logical CameraInputSurfaceMetrics
  no  -> let bevy_lagrange derive metrics from the camera/window
```

The app still uses preset or bindings input modes for render-to-texture; it only tells
Lagrange which camera receives input and what logical input surface should
scale screen-pixel movement.

```rust
commands.entity(render_texture_camera).insert((
    OrbitCam::default(),
    OrbitCamPreset::BlenderLike,
));

commands.insert_resource(
    CameraInputRoutingConfig::explicit(render_texture_camera)
        .with_surface_metrics(CameraInputSurfaceMetrics {
            camera_view_size: Some(render_target_logical_size),
            input_surface_size: Some(editor_panel_logical_size),
        }),
);
```

Use this pattern when the camera renders to an image, texture, or editor panel that
automatic cursor hit-testing cannot discover. Do not switch to `OrbitCamManual`
unless the app is computing orbit/pan/zoom intent directly.

### Legacy API Migration Table

This refactor is a breaking input API change. Remove the legacy `OrbitCam` raw-input
fields outright rather than keeping a compatibility shim that maps old fields into
the new input-mode components. The migration table documents the replacement
concepts, but the old fields should not remain functional alongside the new input
model.
Do not add `OrbitCamLegacyInputCompat` or a one-release compatibility component. This
is an intentional breaking cleanup while `bevy_lagrange` has no external users.

| Existing API / behavior | New home |
|-------------------------|----------|
| `OrbitCam::input_control = None` used to stop user camera input temporarily | Add `CameraInputDisabled` when the selected input mode should be preserved; use `OrbitCamManual` only when the app takes over writing `OrbitCamInput`. |
| Pause camera input for a menu, modal, or tool overlay | `commands.entity(camera).insert(CameraInputDisabled)`; resume with `remove::<CameraInputDisabled>()`. |
| Default left/right mouse controls | `OrbitCamPreset::SimpleMouse`. |
| `TrackpadBehavior::ZoomOnly` | `OrbitCamWheelBinding::ZoomOnly`. |
| `TrackpadBehavior::BlenderLike` | `OrbitCamWheelBinding::BlenderLike` through preset or custom bindings. |
| `modifier_pan: None` / `modifier_zoom: None` in Blender-like trackpad config | `OrbitCamWheelModifier::Always`, represented through builder APIs that reject ambiguous combinations. |
| `ZoomDirection::Reversed` | `OrbitCamBindings::zoom_direction(ZoomDirection::Reversed)` or equivalent response config, applied uniformly to every user-input zoom source. |
| `button_zoom` | `OrbitCamButtonDragZoomBinding`. |
| `ButtonZoomAxis::{X, Y, XY}` | `OrbitCamButtonDragZoomAxis::{X, Y, XY}`. |
| `OrbitCamTouchBinding::OneFingerOrbit` / `TwoFingerOrbit` | Touch adapter policy inside `OrbitCamBindings`. |
| Keyboard control examples that mutate targets directly | `OrbitCamBindings` for user input, or existing programmatic camera APIs for non-user camera motion. |
| Manual active-camera resource setup for render-to-texture | `CameraInputRouting::Explicit` plus logical `CameraInputSurfaceMetrics`. |
| Keyboard/gamepad input in single-camera apps when the cursor is outside the viewport | `CameraInputRoutingConfig::cursor_hit_test().with_no_position_fallback(NoPositionFallback::OnlyEligibleCamera)`. |

### Migration Examples

Legacy Blender-like trackpad behavior with a pan modifier and always-on zoom:

```rust
// Before:
orbit_cam.trackpad_behavior = TrackpadBehavior::BlenderLike {
    modifier_pan: Some(KeyCode::ShiftLeft),
    modifier_zoom: None,
};

// After:
commands.entity(camera).insert(OrbitCamPreset::BlenderLike);
```

If the app needs the same policy inside a custom binding:

```rust
let bindings = OrbitCamBindings::builder()
    .orbit_drag(MouseButton::Middle)
    .wheel(OrbitCamWheelBinding::blender_like()
        .with_pan_modifier(OrbitCamWheelModifier::Key(KeyCode::ShiftLeft))
        .with_zoom_modifier(OrbitCamWheelModifier::Always))
    .build();

commands.entity(camera).insert(bindings);
```

Legacy temporary input pause:

```rust
// Before:
orbit_cam.input_control = None;

// After:
commands.entity(camera).insert(CameraInputDisabled);

// Resume:
commands.entity(camera).remove::<CameraInputDisabled>();
```

Keyboard plus gamepad user input should become `OrbitCamBindings`, not direct camera
target mutation:

```rust
let bindings = OrbitCamBindings::builder()
    .zoom_keys(KeyCode::Equal, KeyCode::Minus)
    .gamepad(CameraInputGamepadSelectionPolicy::Selected(gamepad))
    .gamepad_orbit(GamepadAxis::RightStick)
    .gamepad_smooth_zoom(GamepadAxis::RightTrigger, GamepadAxis::LeftTrigger)
    .wheel_from_preset(OrbitCamPreset::SimpleMouse)
    .build();

commands.entity(camera).insert(bindings);
```

Legacy button-drag zoom:

```rust
let bindings = OrbitCamBindings::builder()
    .orbit_drag(MouseButton::Middle)
    .button_drag_zoom(OrbitCamButtonDragZoomBinding {
        button: MouseButton::Right,
        axis: OrbitCamButtonDragZoomAxis::Y,
        scale: 1.0,
    })
    .wheel_from_preset(OrbitCamPreset::SimpleMouse)
    .build();

commands.entity(camera).insert(bindings);
```

### Example Migration Notes

- `basic.rs` should remain the smallest working camera example. It should use
  `LagrangePlugin + OrbitCam::default()` to demonstrate the zero-config default,
  which resolves to the mouse-oriented `SimpleMouse` preset. Its comments should
  state that `BlenderLike` is available for editor-style workflows.
- `advanced.rs` should be renamed to `custom_bindings.rs`. It should demonstrate
  `OrbitCamBindings` with custom action bindings plus
  custom wheel, pinch, and touch adapter policy.
- `keyboard_controls.rs` should be retired. Keyboard-as-user-input should be shown
  through `custom_bindings.rs` or a focused bindings example, while
  programmatic camera movement is covered by zoom, look, fit, and animation examples.
- `egui.rs` should remain the focused UI integration example. It should pair a normal
  input preset with `BlockOnEguiFocus` and demonstrate that egui pointer/keyboard
  focus blocks camera interactions without replacing the selected input mode.
- `pausing.rs` should remain the `TimeSource::Real` example. It should demonstrate
  keeping camera smoothing responsive while virtual time is paused. Migrate it by
  replacing raw `input_control` setup with the default preset or an explicit
  `OrbitCamPreset::BlenderLike`.
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
  use the default input mode unless the demonstrated camera behavior specifically
  requires a different preset.
- `orbit_cam_preset_blender_like.rs` should show the Blender-like preset with `fairy_dust`
  guidance text that highlights orbit, pan, and zoom rows from camera interaction
  lifecycle events.
- `orbit_cam_preset_simple_mouse.rs` should show the simpler mouse-oriented preset and make
  its differences from Blender-like controls visible in the guidance text.
- `orbit_cam_bindings_keyboard.rs` should show keyboard bindings through
  `OrbitCamBindings`, not by mutating camera targets
  directly.
- `orbit_cam_bindings_gamepad.rs` should show gamepad axes/buttons through
  `OrbitCamBindings`, including deadzone/axis guidance and
  a visible no-gamepad fallback.
- `orbit_cam_manual.rs` should show direct `OrbitCamInput` writes through helper
  methods and typed deltas, with `ManualInputSource::manual()` and at least one
  observed-device source such as `ManualInputSource::observed_keyboard()`. Its
  guidance text should make the resulting `MANUAL | KEYBOARD` source set visible.

Keep these as separate named examples rather than one parameterized `input_modes.rs`
example. The filenames should match the supported input modes so users can find
the relevant setup quickly. Share small helper functions for scene setup and guidance
rows where useful, but do not hide the input-mode setup behind a CLI flag.

## Testing Strategy

Prefer ECS-only tests for the input refactor. Most behavior can be validated with an
`App`, the input systems/plugins, spawned camera entities, synthetic input messages,
and event/message readers. Avoid requiring renderer or GPU setup unless a test
specifically covers rendered output.

Core ECS-only tests:

- default `OrbitCam` receives `OrbitCamPreset::SimpleMouse` through the
  required component path;
- inserting one input-mode component removes the other input-mode components;
- if multiple input-mode components are present before `PreInput`, validation
  removes all but the selected mode, emits the configured diagnostic, and reconciles
  only one input installation;
- inserting all three input modes in one frame leaves exactly one selected mode by
  `PreInput` completion, regardless of observer command-defer timing;
- removing every input-mode component from an `OrbitCam` restores
  `OrbitCamPreset::default()` and logs a diagnostic;
- valid `OrbitCamInputModeDescriptor` changes insert the expected exclusive
  input-mode component, emit `OrbitCamInputModeApplied`, and set
  `OrbitCamInputModeApplyStatus.state` to `OrbitCamInputModeApplyState::Applied`;
- invalid `OrbitCamInputModeDescriptor` changes leave the previous input-mode
  component and private input installation in place, emit `OrbitCamInputModeRejected`,
  and set `OrbitCamInputModeApplyStatus.state` to
  `OrbitCamInputModeApplyState::Rejected` with the validation error;
- `OrbitCamInputModeApplyStatus` remains point-in-time descriptor feedback when the
  descriptor is removed or the runtime input-mode component is changed directly;
- multiple same-frame mutations to `OrbitCamInputModeDescriptor` coalesce to one
  apply attempt for the final descriptor state;
- descriptor apply plus private installation replacement exposes exactly one private
  installation to enhanced input in the same frame an input event arrives;
- `OrbitCamPreset -> OrbitCamManual` despawns related
  `OrbitCamInputInstallation` and installs no new library-owned input entities;
- `OrbitCamPreset -> OrbitCamBindings` replaces old related entities
  rather than accumulating bindings;
- replacing input modes during an active interaction emits `OrbitCamInteractionEnded` and
  clears stale `OrbitCamInput`;
- source-latch recovery clears held ownership on despawn, `OrbitCam` removal, input-mode
  replacement, input disable, target-window close, application focus loss, selected
  gamepad disconnect, or missing underlying held-source state;
- latch recovery follows the explicit `LatchState` transitions and the controller
  never observes `ReleasedPendingCleanup`;
- per-source latches allow mouse, touch, keyboard, and multiple selected gamepads to
  keep independent owners, including mixed mouse-plus-keyboard and two-gamepad
  multi-camera scenarios;
- selected gamepad disconnect/reconnect clears stale ownership before any same-frame
  reacquire through normal routing fallback;
- selected-gamepad disconnect is detected from the synchronous current gamepad set in
  `PreInput`, not only from queued connection events;
- focus loss and window close latch recovery consult synchronous current window state
  when available, not only queued platform events;
- a no-position held source whose stale latch is cleared reroutes through explicit
  route, fallback, or hit-test rules in the same `PreInput` phase;
- `CameraInputDisabled`, egui focus blockers, inactive routing, and animation ignore
  clear manual and preset/custom input before animation or controller systems observe it;
- systems in `OrbitCamInputPhase::WriteManual` are visible to `Finalize` in the
  same frame;
- manual writer helpers expose only `OrbitCamManual` cameras, and manual
  writes cannot override preset/custom resolved input;
- debug finalization detects any manual-writer mutation attempted on a non-manual
  camera;
- manual shorthand helpers such as `orbit_pixels` and `pan_pixels` write with
  `ManualInputSource::manual()`;
- manual writer helpers take `ManualInputSource`, always include `MANUAL`, and can
  add observed-device source flags without allowing arbitrary source sets;
- manual zero-delta active helpers emit started/ended lifecycle events correctly;
- manual screen-pixel orbit and pan writes use automatically derived logical surface
  metrics when possible, and missing metrics are detected by the manual writer or
  finalizer instead of silently producing incorrect scaling;
- manual-mode cameras that are not the current routed camera still receive per-camera
  logical metric derivation before finalization;
- logical surface metrics are derived once per frame and cached before finalization;
  same-frame window resize affects the next frame's conversions;
- `orbit_pixels` and `pan_pixels` return `()` and missing logical metrics produce a
  `CameraInputMetricsMissing` event plus one-time error rather than a synchronous
  result;
- surface metrics are documented and tested as logical pixels, including a high-DPI
  case where physical framebuffer size differs from logical window size;
- surface metric derivation covers normal window cameras, render-to-texture explicit
  overrides, multi-window routing, zero-size viewports, missing windows, and image
  targets without explicit metrics;
- held pan/zoom/orbit bindings cannot be built without corresponding engagement
  state;
- reflected or dynamically loaded held bindings go through validation and reject
  motion-without-engagement or source/condition mismatches;
- held bindings reject mismatched route policy, such as cursor-routed mouse motion
  paired with globally routed gamepad engagement;
- held gamepad bindings reject mismatched `CameraInputGamepadSelectionPolicy` between
  motion and engagement halves;
- held-binding compatibility follows the documented matrix for source family, route
  policy, and activation predicates;
- mouse-button release ends the held interaction in the release frame even when motion
  delta is zero, and zero motion while still engaged remains active;
- impulse bindings reject any engagement half because wheel, pinch, and smooth-scroll
  do not have a held phase;
- custom binding specs carry source metadata, and source flags do not need to be
  inferred from enhanced-input internals;
- custom binding specs are action-typed, so orbit/pan and smooth/coarse zoom bindings
  cannot be swapped through the ordinary builder API;
- per-binding source attribution survives enhanced-input action merging, so
  keyboard-plus-gamepad bindings for the same action report only the source that
  actually triggered;
- held sources latch their source-specific owner until release, while impulse wheel/pinch/smooth-scroll
  events route independently per event;
- global Bevy gesture impulses without window metadata route by the documented
  fallback policy and produce no input when ambiguous;
- keyboard/gamepad no-position input defaults to no input without a latch, explicit
  route, or unambiguous cursor-hit camera, and `OnlyEligibleCamera` routes only when
  explicitly configured;
- per-camera logical surface metrics are used for orbit and pan scaling under
  explicit and cursor-hit-test routing;
- internal reconcile, context-gating, and adapter-injection changes are visible to
  `EnhancedInputSystems::Update` in the same frame;
- `enhanced_input_scheduling_invariant` asserts `PreInput` runs before
  `EnhancedInputSystems::Update`, adapter/context setup is visible before action
  update, enhanced-input apply runs before `Finalize`, and manual writers run before
  finalized input is consumed;
- the enhanced-input integration boundary compiles against the pinned API signatures
  for context registration, binding installation, system-set ordering, and adapter/mock
  contribution when mocks are used;
- strict startup diagnostics fail when Lagrange input phases, context registration, or
  enhanced-input ordering integration are missing;
- strict startup diagnostics fail when the expected enhanced-input API marker or
  plugin setup is missing;
- adapter values inserted through command-buffered mock state are visible to enhanced
  input in the same frame because the barrier is structural;
- routing/context-gating/adapter-injection swaps for 10 or more frames produce no
  double injection, stale action state, or phantom lifecycle events;
- disabled, egui-blocked, animation-ignored, inactive, and unrouted preset/custom
  contexts are gated or reset before `EnhancedInputSystems::Update`;
- two cameras can swap routing without the inactive camera retaining stale
  enhanced-input action state;
- two preset cameras can swap routing every frame for multiple frames without phantom
  `OrbitCamInteractionStarted` events from adapter-backed sources;
- despawning a camera during a held interaction clears its source latch before the
  next frame and queues exactly one ended lifecycle transition when the camera still
  exists long enough to observe it;
- one system that removes `OrbitCamPreset` and inserts
  `OrbitCamManual` during an active drag emits exactly one ended lifecycle
  event and leaves no orphaned started event;
- one tick that removes an input mode, inserts a replacement, and despawns the camera
  produces no duplicate lifecycle events and no stale source latch;
- `App::new().add_plugins(LagrangePlugin)` installs the enhanced-input plugin and
  registers `OrbitCamInputContext` without additional app setup;
- spawning `OrbitCam` without `LagrangePlugin` produces a one-time diagnostic error
  that input will not resolve;
- `CameraInteractionSources` has no public raw-bit constructor, ordinary callers can
  only compose named source constants, and `ManualInputSource` cannot be constructed
  without `MANUAL`;
- camera actions do not consume app-owned enhanced-input bindings by default;
- adapter/public-binding conflicts are rejected or reported by `OrbitCamBindings`
  validation;
- binding validation returns structured errors for adapter/public-binding conflicts
  and missing mandatory wheel policy;
- `try_build` returns `OrbitCamBindingsError` with distinct variants and remediation
  text; descriptor validation rejects missing wheel policy instead of defaulting it;
- `OrbitCamInteractionSourcesChanged` and `OrbitCamInteractionState` report source-set
  changes while an interaction remains active;
- `OrbitCamInteractionSourcesChanged::added_sources` and `removed_sources` compute the
  expected source diffs;
- impulse-only interactions such as line wheel emit started and ended in the same
  frame and do not remain active into the next frame;
- lifecycle queue deduplication prevents duplicate started/ended events when control
  replacement, blocker finalization, and despawn cleanup all touch the same camera and
  interaction kind in one frame;
- two cameras with independent source latches can release one source while another
  remains held; only the released camera/source emits the ended or source-change
  event;
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
- pinch suppression uses the per-frame snapshot: held modifier suppresses pinch,
  released modifier allows pinch, and another camera's modifier state is ignored;
- egui click/drag focus tests preserve the current `prev || curr` leak prevention,
  including the frame focus is requested;
- `CameraInputInterruptBehavior::{Ignore, Cancel, Complete}` preserve their exact
  input, animation-event, and controller-consumption behavior on the frame an
  animation starts, completes, is cancelled, or is replaced;
- input-mode replacement during an active `CameraInputInterruptBehavior::Ignore`
  animation emits one ended lifecycle event, clears input, and does not let stale
  bindings interrupt the animation;
- input-mode replacement during an active `Ignore` animation never emits a replacement
  `Started` event before the new bindings produce unblocked observable input;
- touch `n -> 0` for any active touch arity emits one ended transition for the
  previous touch operation without synthesizing an intermediate one-finger frame;
- `Camera::is_active` toggled after routing blocks the originally routed camera for
  the frame rather than re-routing mid-frame;
- global gesture fallback logs an ambiguous-routing debug message when no unique
  camera can be selected;
- dependency validation confirms `bevy_lagrange` uses the workspace-pinned
  `bevy_enhanced_input` and `bitflags` versions without duplicate direct versions,
  and confirms `bevy_kana`'s `input` feature is removed or resolves to the same
  enhanced-input version.
- workspace consumers, especially `crates/bevy_diegetic/examples/*`, compile after
  legacy `OrbitCam` input fields move into input modes and bindings.

## Migration Plan

1. Add workspace-pinned `bevy_enhanced_input` with explicit compatible version bounds as a normal `bevy_lagrange` dependency and have `LagrangePlugin` install the enhanced-input plugin through the internal enhanced-input integration boundary.
2. Add `bitflags = { workspace = true }` as a direct `bevy_lagrange` dependency.
3. Audit `bevy_kana`'s `input` feature. Remove it if unused by `bevy_lagrange`, or validate that it resolves to the same `bevy_enhanced_input` version as the direct dependency.
4. Add the public `input` module with actions, context, input modes, default-on reflected binding descriptors, validated bindings, intent, disabled input, interaction state, manual writing, and interaction events.
5. Add `OrbitCamInput`, typed deltas, active-source fields, and helper methods for manual input.
6. Add `OrbitCamInputContext` as a required component on `OrbitCam` and register it in `LagrangePlugin` after enhanced input is installed.
7. Add mutually exclusive `OrbitCamPreset`, `OrbitCamBindings`, and `OrbitCamManual`.
8. Add `OrbitCamBindings`, `OrbitCamBindingsDescriptor`, private fields, sealed action-typed local builder/spec types with per-binding source metadata, typestate wheel ownership, opaque held-entry builders, engagement invariants, gamepad selection policy, metadata-bearing low-level enhanced-input constructors, descriptor apply status/events, and one shared runtime validation function.
9. Add `ZoomDirection`, `OrbitCamButtonDragZoomBinding`, touch policy, pinch policy, and wheel policy as binding/adapter configuration.
10. Add the private `OrbitCamInputInstallationOf` / `OrbitCamInputInstallation` relationship, the observer-based input-mode exclusivity shim, and input-mode reconciliation.
11. Add the private adapter module for wheel units, smooth scroll, pinch, and touch.
12. Add source-aware interaction tracking, the internal lifecycle queue, `OrbitCamInteractionStarted`, `OrbitCamInteractionEnded`, `OrbitCamInteractionSourcesChanged`, and `OrbitCamInteractionState`.
13. Replace public runtime gating with `CameraInputDisabled` plus internal transient blockers.
14. Rename `CameraInputDetection` to `CameraInputRouting` with `CursorHitTest` and `Explicit`.
15. Implement public routing configuration, internal resolved routing state with an explicit resolver, per-source held latching, deterministic latch recovery, per-event impulse routing, no-position source routing, global gesture fallback routing, logical surface metrics, and inactive-context gating/reset before enhanced-input update.
16. Add the root-level `system_sets` module, internal `LagrangeSystemSetsPlugin`, and public `OrbitCamInputPhase::{PreInput, WriteManual, Finalize}` scheduling surface.
17. Add `animation_input_interrupt` and use finalized `OrbitCamInput` as the user-input interrupt signal for `Cancel` and `Complete`; treat `Ignore` as a finalization and pre-controller blocker.
18. Remove physical binding fields from `OrbitCam` as a breaking change and move their replacement concepts into presets and adapter configuration.
19. Update egui blocking to feed internal UI-focus blockers before finalization.
20. Add the `fairy_dust` camera guidance panel and component-insertion camera setup needed by the input-mode examples.
21. Add the input-mode examples with `fairy_dust` visual feedback.
22. Migrate existing examples according to the example migration notes.
23. Migrate workspace consumers, especially `crates/bevy_diegetic/examples/*`, away from legacy `OrbitCam` input fields.
24. Add missing-plugin diagnostics and first-frame setup validation.
25. Add ECS-only tests for scheduling invariants, reconciliation, routing, blockers, lifecycle events, legacy behavior preservation, interrupt policies, enhanced-input API compatibility, workspace consumers, and dependency versioning.

## Changelog-Style Summary

### Breaking

- Remove legacy raw-input fields from `OrbitCam`; configure user input through
  `OrbitCamPreset`, `OrbitCamBindings`, `OrbitCamManual`,
  and `CameraInputDisabled`.
- Replace `CameraInputDetection::{Automatic, Manual}` with
  `CameraInputRouting::{CursorHitTest, Explicit}`.

### Added

- Add enhanced-input based orbit-camera input modes with mutually exclusive preset,
  bindings, and manual input-mode components.
- Add default-on reflected input-mode descriptors with applied/rejected events and a
  persisted apply-status component for editors, scene files, and keymap tools.
- Add source-aware camera interaction lifecycle events, source-change events, and read-only interaction state.
- Add helper methods on `OrbitCamInteractionSourcesChanged` for added and removed
  source flags.
- Add an internal lifecycle queue that deduplicates started/ended/source-change events
  across routing, blocker, control replacement, and despawn cleanup paths.
- Add `ManualInputSource` so manual camera input always reports `MANUAL` and may include observed device provenance.
- Add logical `CameraInputSurfaceMetrics` for explicit routing, render-to-texture, and custom editor input surfaces.
- Add structured binding validation and missing-plugin diagnostics for common setup mistakes.
- Add an error-reference and binding-invariants docs path for custom binding failures.
- Add input-mode examples with `fairy_dust` guidance that highlights active camera interactions and source flags.

### Changed

- Change the default input model to `OrbitCamPreset::SimpleMouse` and
  make `BlenderLike` an explicit editor-style preset.
- Change camera input routing to use `CameraInputRouting::{CursorHitTest, Explicit}` with internal resolved routing state.
- Change custom bindings to be action-typed and source-aware so lifecycle events can distinguish mouse, wheel, smooth-scroll, pinch, touch, keyboard, gamepad, and manual input.
- Change binding validation so builders, descriptors, reflection, dynamic keymaps, and
  presets share the same validation function.
- Change render-to-texture routing to use explicit routing plus logical surface metrics instead of manually populating `ActiveCameraData`.
- Change examples and workspace consumers to configure input through `OrbitCamPreset`,
  `OrbitCamBindings`, `OrbitCamManual`, and `CameraInputDisabled`.

### Removed

- Remove legacy raw-input fields from `OrbitCam` as a breaking change.
- Remove the old `CameraInputDetection::{Automatic, Manual}` API in favor of `CameraInputRouting::{CursorHitTest, Explicit}`.
- Remove the old keyboard-controls pattern that mutates camera targets directly for user input.
- Do not add a public raw enhanced-input binding escape hatch; advanced enhanced-input
  descriptors must go through typed Lagrange constructors that preserve source
  metadata and held/impulse validation.

## Final Architecture

```text
Preset input mode
  -> OrbitCamPreset::{SimpleMouse, BlenderLike}
      -> preset creates validated OrbitCamBindings
          -> private input installation relationship
              -> public enhanced-input actions + private adapter policy
                  -> OrbitCamInput
                      -> OrbitCamInputPhase::Finalize
                          -> OrbitCam controller

Bindings input mode
  -> OrbitCamBindings supplied by the app
      -> private input installation relationship
          -> public enhanced-input actions + private adapter policy
              -> OrbitCamInput
                  -> OrbitCamInputPhase::Finalize
                      -> OrbitCam controller

Manual input mode
  -> OrbitCamManual
      -> app writes OrbitCamInput through helper methods in OrbitCamInputPhase::WriteManual
      -> OrbitCamInputPhase::Finalize
          -> OrbitCam controller

Programmatic camera operations
  -> OrbitCam state, targets, or animation queues
      -> OrbitCam controller
```

The default path is action-centered. The adapter keeps today's richer wheel,
smooth-scroll, pinch, and touch behavior without making a second public input model.
Manual users can bypass enhanced input for a camera by writing `OrbitCamInput`, but
presets and bindings input modes keep camera input inside the same action/context
architecture used by the rest of the app.

## Future Cleanup

### Roll

Roll is a natural future camera interaction because platform gesture systems can
produce rotation gestures. Bevy already exposes `RotationGesture`, and the current
touch tracker computes two-finger rotation even though the controller does not use it.

Roll should not be added as part of the initial input refactor. It requires extending
the camera behavior model, not just adding another input action.
When roll is added, expect to touch the interaction kind enum, input snapshot,
interaction state, tracker, presets, and manual writer. If that update becomes noisy,
consider a generic interaction tracker keyed by interaction kind and associated action
types, but keep that cleanup out of the initial refactor.
The initial refactor should keep explicit orbit, pan, and zoom fields/tracking rather
than introducing a generic `InteractionTracker<K>`. Add that abstraction only if roll
or another new interaction kind proves the explicit model is too repetitive.

Candidate future additions:

- `OrbitCamRollAction` semantic action.
- `OrbitCamInteractionKind::Roll`.
- `OrbitCamInput::roll`.
- `roll` and `target_roll` camera state.
- `roll_lower_limit` and `roll_upper_limit`.
- `roll_sensitivity` and `roll_smoothness`.

`OrbitCamInteractionKind` should be non-exhaustive so `Roll` can be added later without
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
