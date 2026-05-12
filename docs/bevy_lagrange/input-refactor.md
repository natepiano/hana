# `bevy_lagrange` input refactor

## Goal

Make `bevy_lagrange` opinionated about Bevy's action/context input model while
keeping camera behavior separate from physical input policy.

The target structure is:

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
- Keep reflected editor/keymap apply systems in `bevy_lagrange` behind a default-on
  `reflect-input-modes` feature. Concrete non-generic reflected configuration types
  remain normal API types and rely on `Reflect` derives, not manual type registration.
- Keep three mutually exclusive input-mode marker components:
  `OrbitCamPreset`, `OrbitCamBindings`, and `OrbitCamManual`.
  Use the observer shim for tidy mutations and `PreInput` validation as the
  deterministic authority until native Bevy mutually exclusive components can replace
  the shim. Do not reopen a single `OrbitCamInputMode` enum component for the initial
  refactor; this marker-component pattern is a locked public API decision.
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
  public raw-bit constructors in the initial API. Manual writes use the controlled
  `ManualInputSource` constructor.
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
- Reflected descriptor/editor apply support is a default-on feature, tentatively
  `reflect-input-modes`. Keep it in `bevy_lagrange`, not a separate crate. Disabling it
  removes descriptor apply systems, apply-status components, and related editor/keymap
  integration. It does not remove concrete descriptor value types used by builders and
  dynamic keymaps, preset input modes, custom runtime bindings, manual input, routing,
  lifecycle events, or the enhanced-input adapter.
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

Conceptual structure:

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
Do not add a public strict-startup diagnostics API in the initial refactor. Bevy does
not expose a general runtime schedule proof, so ECS ordering tests remain the
authoritative guard for barrier semantics, context registration, plugin setup, and
the pinned enhanced-input API shape.

References:

- <https://docs.rs/bevy_enhanced_input/latest/bevy_enhanced_input/binding/enum.Binding.html>
- <https://docs.rs/bevy_enhanced_input/latest/bevy_enhanced_input/context/trait.InputContextAppExt.html>
- <https://docs.rs/bevy_enhanced_input/latest/bevy_enhanced_input/modifier/trait.InputModifier.html>
- <https://docs.rs/bevy_enhanced_input/latest/bevy_enhanced_input/condition/trait.InputCondition.html>
- <https://docs.rs/bevy_enhanced_input/latest/bevy_enhanced_input/action/mock/struct.ActionMock.html>
- <https://docs.rs/bevy_enhanced_input/latest/bevy_enhanced_input/context/struct.ExternallyMocked.html>

## Public Module Structure

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
have the Lagrange-owned resources, messages, enhanced-input setup, and systems
required by `OrbitCamInputContext` without panicking. Actual keyboard, mouse,
gamepad, touch, and gesture event production still comes from Bevy input plugins,
normally through `DefaultPlugins`.
Guard plugin setup so workspace-composed apps can add `LagrangePlugin` from multiple
modules without double-installing enhanced input. If Bevy exposes an
`is_plugin_added::<EnhancedInputPlugin>()` equivalent, use it before adding
`EnhancedInputPlugin`; otherwise use an internal setup marker resource and emit a
one-time warning if setup is requested again.

Do not add public startup diagnostics in the initial refactor unless a concrete
in-tree need appears. The shipped diagnostic surface stays limited to private adapter
counts, route/blocker state, lifecycle events, and missing metrics events.

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
`Reflect` when their data can be represented honestly; non-generic reflected types rely
on their derives rather than manual registration. The
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
keymap UIs get systems and status components that apply mutable reflected
representations of camera input modes. The concrete descriptor value types are normal
public API types so builders and dynamic keymaps can use them without a feature split.
That representation should be separate from the validated runtime component:

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
use the descriptor types' `Reflect` derives and Bevy's normal type-registration
behavior. Runtime load/apply validates the
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
    pub last_error: Option<String>,
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
input events must leave exactly one authoritative installation record for that frame;
phase 06 turns the record into enhanced-input entities and adapter state.

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
The reflected runtime structure should be read-only or opaque. A future implementation
may wrap the runtime value in a `ValidatedOrbitCamBindings` newtype internally if that
makes the descriptor-to-runtime authority boundary clearer, but public reflected
mutation must always go through `OrbitCamBindingsDescriptor`.

It contains two kinds of configuration:

- ordinary enhanced-input bindings for public semantic actions;
- adapter policy for sources enhanced input does not currently describe richly enough.

Conceptual structure:

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

Preset and custom input modes own a private input installation record for a camera.
Phase 04 may use placeholder entities; phase 06 replaces them with private
enhanced-input actions, bindings, adapter state, and mock state. Those implementation
entities are not scene hierarchy children. Model their ownership with a private Bevy
relationship or equivalent private ownership record rather than `ChildOf`:

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
Phase 06 implemented the library-owned mutation path as crate-private source-set
helpers. Keep those helpers crate-private so future setters do not accidentally become
an app-facing bypass:

```rust
impl OrbitCamInput {
    pub fn orbit_delta(&self) -> Vec2;
    pub fn pan_delta(&self) -> Vec2;
    pub fn zoom_coarse_delta(&self) -> f32;
    pub fn zoom_smooth_delta(&self) -> f32;

    pub(crate) fn orbit_pixels_with_sources(
        &mut self,
        delta: impl Into<OrbitDelta>,
        sources: CameraInteractionSources,
    );
}
```

External app code can query `OrbitCamInput` for reading, but source-set mutation stays
inside the crate. Public manual writes continue to use `OrbitCamManualInput`.

Manual users should not normally set value, source, and phase fields directly. The
public manual writer API should be method-based:

| API structure | Source attribution | Use when |
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
force the user to provide metrics that Bevy already knows. Expose an explicit
per-camera metrics override component for render-to-texture, offscreen images, or
custom editor surfaces where the input surface is not the camera's window viewport:

```rust
commands.entity(camera).insert(CameraInputSurfaceMetrics {
    camera_view_size: Some(render_target_logical_size),
    input_surface_size: Some(panel_logical_size),
});
```

Metric derivation should use this order:

1. Explicit `CameraInputSurfaceMetrics` component fields on the camera.
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
structure should keep the public scheduling surface small:

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
ECS schedule tests should confirm that the Lagrange input sets are configured and
that the enhanced-input update/apply sets are ordered relative to `PreInput` and
`Finalize` as expected for the supported Bevy/enhanced-input versions.
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
    CameraInputSurfaceMetrics {
        camera_view_size: Some(render_target_logical_size),
        input_surface_size: Some(editor_panel_logical_size),
    },
));

commands.insert_resource(CameraInputRoutingConfig::explicit(render_texture_camera));
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
- ECS tests fail when Lagrange input phases, context registration, or enhanced-input
  ordering integration are missing;
- `LagrangePlugin` initializes the Bevy resources/messages its camera-input systems
  read directly, including `Touches` and `PinchGesture`;
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
- `App::new().add_plugins(LagrangePlugin)` installs the enhanced-input plugin,
  registers `OrbitCamInputContext`, and initializes direct camera-input
  resources/messages without additional app setup;
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

## Implementation Phases

Use integer-prefixed phase names for branches, commits, and review checkpoints. Each
phase should leave the repository usable at the commit boundary. The one exception is
the middle of `08-breaking-cutover-and-callers`: once legacy `OrbitCam` input fields
are removed, the worktree is temporarily unusable until in-repo examples and workspace
callers are migrated. Do not commit that phase until the migration is complete.

Phase contract:

- Treat each numbered phase as one committable implementation unit.
- Keep phases `01` through `07` additive. Existing `OrbitCam` behavior remains the
  runtime source of truth while the new input model is built beside it.
- Treat phase `08` as the only planned breaking window. It is safe to break local
  compilation inside the phase, but not safe to commit until callers are migrated and
  the controller consumes finalized `OrbitCamInput`.
- Keep phases `09` and `10` as follow-through after the new API is usable: examples,
  guidance UI, diagnostics, tests, and cleanup.
- If implementation discovers that a public type must change, fold the change back
  into the earliest phase that introduces that type instead of adding a late
  compatibility layer.

Phase index:

| Phase | Commit boundary | Runtime authority at boundary | Primary output |
|-------|-----------------|-------------------------------|----------------|
| `01-dependencies-and-plugin-shell` | Usable | Legacy `OrbitCam` input | Dependencies, plugin shell, system-set module. |
| `02-public-input-surface` | Usable | Legacy `OrbitCam` input | Public input types, manual writer surface, interaction events. |
| `03-actions-bindings-and-presets` | Usable | Legacy `OrbitCam` input | Actions, bindings, presets, validation. |
| `04-input-modes-and-installation` | Usable | Legacy `OrbitCam` input | Exclusive input-mode components and private installation ownership. |
| `05-routing-scheduling-and-blockers` | Usable | Legacy `OrbitCam` input | Routing snapshots, blockers, surface metrics, schedule diagnostics. |
| `06-adapters-and-action-resolution` | Usable | Legacy `OrbitCam` input | Enhanced-input actions and adapters produce `OrbitCamInput`. |
| `07-lifecycle-and-manual-finalization` | Usable | Legacy `OrbitCam` input | Finalized input, lifecycle events, manual path, animation blockers. |
| `08-breaking-cutover-and-callers` | Broken mid-phase; usable only at end | New `OrbitCamInput` pipeline | Controller cutover, legacy field removal, caller migration. |
| `09-examples-guidance-and-doc-cleanup` | Usable | New `OrbitCamInput` pipeline | Teaching examples, fairy-dust guidance, example cleanup. |
| `10-tests-diagnostics-and-cleanup` | Usable | New `OrbitCamInput` pipeline | ECS test coverage, diagnostics, transitional-code removal. |

### 01-dependencies-and-plugin-shell (Complete)

Goal: add dependency and plugin infrastructure without changing camera behavior.

Scope:

- Add workspace-pinned `bevy_enhanced_input` with explicit compatible version bounds
  as a direct `bevy_lagrange` dependency.
- Add `bitflags = { workspace = true }` as a direct `bevy_lagrange` dependency.
- Audit `bevy_kana`'s `input` feature; remove it if unused by `bevy_lagrange`, or
  prove it resolves to the same `bevy_enhanced_input` version.
- Add the private enhanced-input integration boundary for plugin setup, context
  registration, binding installation, and adapter/mock write paths.
- Add the root-level `system_sets` module, internal `LagrangeSystemSetsPlugin`, and
  public `OrbitCamInputPhase::{PreInput, WriteManual, Finalize}` type, but keep new
  input systems inert until later phases.

Repository state: usable. Existing `OrbitCam` input behavior remains authoritative.

Done when:

- `LagrangePlugin` can install the enhanced-input plugin without duplicate setup.
- Dependency validation proves the workspace-pinned enhanced-input and `bitflags`
  versions are used.
- A minimal app with `LagrangePlugin` still compiles and existing camera behavior is
  unchanged.

### Retrospective

**What worked:**

- `bevy_enhanced_input` could be promoted from transitive `bevy_kana/input` usage to a
  direct `bevy_lagrange` dependency without changing the locked crate version.
- `LagrangePlugin` now owns enhanced-input plugin installation and the public
  `OrbitCamInputPhase` schedule shell while the legacy controller remains authoritative.

**What deviated from the plan:**

- `bevy_kana` still remains a direct `bevy_lagrange` dependency for math wrappers, but
  its `input` feature was removed because `bevy_lagrange` does not use the `bevy_kana`
  input helpers.
- Nightly rustfmt touched two existing `bevy_diegetic` benchmark doc comments while
  validating the workspace.

**Surprises:**

- The lockfile already contained `bevy_enhanced_input` `0.24.3`, so phase 01 did not
  require a dependency download or version churn.
- Local `cargo nextest run -p bevy_lagrange` still failed with `sccache: Operation
  not permitted` even through the escalated `/bin/zsh -lc` path, so phase validation
  used formatting, TOML formatting, and `cargo check -q`.

**Implications for remaining phases:**

- Phase 02 can introduce `OrbitCamInputContext` against the direct dependency rather
  than through `bevy_kana`.
- Phase 05 should build on the existing `OrbitCamInputPhase` ordering instead of
  adding a second public scheduling surface.

### Phase 1 Review

- Phase 02 now explicitly stages the existing `src/input.rs` legacy module under the
  new `input` facade instead of creating a second public input surface.
- Phase 02 now introduces the default-on `reflect-input-modes` feature before
  reflected input descriptor APIs land.
- Phase 02 through phase 06 now treat the phase 01 enhanced-input module as a plugin
  guard shell; context registration, installation, and adapter/mock writes are added
  in the phases that introduce the relevant types.
- Phase 03 now reuses or rehomes the existing public `ZoomDirection` type rather than
  introducing a duplicate name.
- Phase 04 now owns structural mode replacement and installation cleanup only; latch
  cleanup is completed in phase 05 and lifecycle cleanup in phase 07.
- Phase 05 now owns routing/blocker diagnostic state, while phase 10 owns broader
  regression coverage and transitional-code cleanup.
- Phase 09 now names the existing workspace `fairy_dust` crate as the source of the
  camera guidance panel.

### 02-public-input-surface (Complete)

Goal: add the public type surface that other phases will fill in.

Scope:

- Move the existing legacy `src/input.rs` implementation to `src/input/legacy.rs` and
  create `src/input/mod.rs` as the public facade. Re-export legacy `InputControl`,
  `TrackpadInput`, `TrackpadBehavior`, `ButtonZoomAxis`, and `ZoomDirection` through
  the facade and crate root until the phase 08 cutover removes the old raw-input
  surface.
- Add the default-on `reflect-input-modes` feature before any reflected descriptor,
  status, or apply APIs land. Keep runtime input, routing, interaction events, and
  enhanced-input integration available without this feature.
- Add the `bevy_lagrange::input` module and root re-exports.
- Add `OrbitCamInputContext` and register it through the private enhanced-input
  integration boundary introduced in phase 01. Do not add action or binding
  installation there yet.
- Add `OrbitCamInput`, typed deltas, read-only accessors, active-source fields, and
  crate-private mutation methods used by manual helpers. Keep direct `OrbitCamInput`
  mutation out of the public API until a library-owned writer needs a private
  mutation-token type.
- Add `CameraInteractionSources`, private source bits, and `ManualInputSource`.
- Add `OrbitCamInteractionStarted`, `OrbitCamInteractionEnded`,
  `OrbitCamInteractionSourcesChanged`, `CameraInputMetricsMissing`,
  `OrbitCamInteractionKind`, and `OrbitCamInteractionState`.
- Add `CameraInputDisabled`, `CameraInputSurfaceMetrics`, and
  `CameraInputMetricKind`.
- Add `OrbitCamManualInput` and `OrbitCamManualInputWriter` signatures, but do not
  require the new manual path to drive the controller yet.

Repository state: usable. Existing `OrbitCam` input behavior remains authoritative.

Done when:

- Public rustdoc describes `OrbitCamInput` as semantic per-frame camera input, not raw
  device input.
- Interaction event types carry `camera`, `kind`, and `sources`.
- `ManualInputSource` cannot be constructed without `MANUAL`.

### Retrospective

**What worked:**

- The existing raw input implementation moved to `input/legacy.rs`, letting
  `bevy_lagrange::input` become the public facade without changing legacy controller
  behavior.
- `OrbitCamInputContext` could be registered through the phase 01 enhanced-input
  shell as soon as the context type existed.
- `reflect-input-modes` was added as a default-on feature and the new reflected input
  surface compiles with the feature disabled.

**What deviated from the plan:**

- The concrete private mutation-token type was deferred because no library-owned
  writer exists before action resolution. Direct `OrbitCamInput` mutation methods are
  crate-private, and public writes go through `OrbitCamManualInput`.
- `ZoomDirection` stayed in the legacy module and is re-exported through the new
  facade for now. Phase 03 can rehome it when bindings own zoom policy.

**Surprises:**

- `pub mod input` is required to expose the planned `bevy_lagrange::input` namespace;
  the crate otherwise still uses private modules plus explicit root re-exports.

**Implications for remaining phases:**

- Phase 03 should add binding policy types into the existing `input` facade and move
  `ZoomDirection` out of `legacy.rs` only when the new bindings surface owns it.
- Phase 04 can add mode components directly under the `input` facade without another
  public module reshuffle.
- Later library-owned input writers should add the private mutation token when they
  first need to bypass manual-source branding.

### Phase 2 Review

- Phase 02 tightened `OrbitCamInput` so direct mutation is crate-private; public app
  writes go through `OrbitCamManualInput`.
- Phase 02 expanded `OrbitCamInteractionState` to track orbit, pan, and zoom sources
  independently before lifecycle resolution depends on it.
- Phase 02 kept `ManualInputSource` non-reflected to preserve its `MANUAL` branding
  invariant.
- Phase 02 normalized surface metric names to `camera_view_size` and
  `input_surface_size` before routing examples and diagnostics harden them.
- Phase 04 now updates `OrbitCamManualInputWriter` to query only `OrbitCamManual`
  cameras when that marker exists.
- Phase 03/04 now split preset ownership: phase 03 introduces preset values and
  binding conversion, while phase 04 owns the component/exclusivity role.
- Phase 07 keeps the lifecycle implementation work but no longer needs to add
  `OrbitCamInteractionSourcesChanged` source-difference helpers because phase 02
  added them.

### 03-actions-bindings-and-presets (Complete)

Goal: build the action-centered configuration model without installing it into the
controller yet.

Scope:

- Add public semantic enhanced-input actions:
  `OrbitCamOrbitAction`, `OrbitCamPanAction`, `OrbitCamZoomCoarseAction`, and
  `OrbitCamZoomSmoothAction`.
- Add private engagement actions for held interaction phase tracking.
- Add `OrbitCamBindings`, `OrbitCamBindingsDescriptor`, private fields, sealed
  action-typed binding sets, per-binding source metadata, route policy, and the shared
  `validate_bindings` path.
- Add the progressive `OrbitCamBindings` builder with typestate wheel ownership,
  opaque held-entry builders, engagement invariants, gamepad selection policy, and
  metadata-bearing low-level enhanced-input constructors.
- Add `OrbitCamWheelBinding`, `OrbitCamBlenderLikeWheelBinding`,
  `OrbitCamWheelModifier`, `OrbitCamPinchBinding`, `OrbitCamTouchBinding`,
  `OrbitCamButtonDragZoomBinding`, `OrbitCamButtonDragZoomAxis`,
  `CameraInputGamepadSelectionPolicy`, and `ZoomDirection`.
- Reuse or rehome the existing public `ZoomDirection` type from the legacy input
  module; do not introduce a duplicate public `ZoomDirection` name while the old
  controller still consumes it.
- Add `OrbitCamPreset::{SimpleMouse, BlenderLike}` and `OrbitCamPreset::to_bindings`.
  Phase 03 introduces the preset value and binding conversion; phase 04 owns its
  active input-mode component role and exclusivity behavior.

Repository state: usable. Existing `OrbitCam` input behavior remains authoritative.

Done when:

- Presets and custom bindings validate through the same code path.
- Missing wheel policy, held motion without engagement, impulse engagement, adapter
  conflicts, and mismatched held source/route policies return structured
  `OrbitCamBindingsError` values.
- `wheel_from_preset(...)` copies only the preset wheel policy.

### Retrospective

**What worked:**

- Semantic action markers, private held-engagement actions, typed binding sets, presets,
  and validation all fit under the phase 02 `input` facade without disturbing the
  legacy controller.
- The typestate `OrbitCamBindingsBuilder` now prevents ordinary callers from building
  without a wheel policy, while `OrbitCamBindingsDescriptor` still reports
  `MissingWheelPolicy` for reflected or dynamic data.

**What deviated from the plan:**

- Non-generic reflected types rely on their `Reflect` derives only; the explicit
  `register_type` plugin path was removed because modern Bevy handles those
  registrations automatically.
- `OrbitCamWheelBinding::LineZoom` became a tuple variant to keep the reflected enum
  compatible with the workspace clippy profile.
- `ZoomDirection` remains in `input/legacy.rs` and is re-exported through the new
  facade until the phase 08 cutover removes legacy raw-input ownership.

**Surprises:**

- Holding private engagement actions as typestate on held binding sets keeps them real
  phase-03 structure without exposing them or leaving dead code before the resolver
  phase.
- The workspace clippy profile applies `expect_used` and `panic` to tests, so binding
  invariant tests use `Result` returns and direct `Err(...)` comparisons.

**Implications for remaining phases:**

- Phase 04 can make `OrbitCamBindings` an input-mode component directly, but reflected
  editing should continue to flow through `OrbitCamBindingsDescriptor` rather than
  mutable runtime binding fields.
- Phase 06 can install private engagement actions from the held binding set typestate
  instead of asking public binding APIs to name those actions.
- Phase 08 still owns moving or deleting legacy raw-input types, including the final
  home for `ZoomDirection`.

### Phase 3 Review

- Phase 04 now treats `OrbitCamPreset`, `OrbitCamBindings`, and
  `OrbitCamBindingsDescriptor` as already introduced by phase 03, and focuses on
  component promotion, `OrbitCamManual`, exclusivity, descriptor apply, and
  installation records.
- Phase 04 now scopes installation cleanup to an authoritative ownership record; phase
  06 owns actual enhanced-input entities and adapter state.
- Phase 04 now has an internal ordering split between mode/exclusivity/manual-writer
  work and descriptor apply/status/installation replacement.
- The `reflect-input-modes` feature now gates descriptor apply systems and status
  integration, not the concrete descriptor value types or automatic `Reflect` derives.
- Phase 05 now validates the live plugin/context/schedule setup from phases 01-02
  instead of reintroducing setup paths.
- Phase 06 now requires a crate-private binding installer/visitor for held typestate,
  private engagement actions, and adapter policy.
- Phase 06 now names the modifier/condition descriptor gap: expand recipes for the
  supported examples or reject unsupported advanced descriptors without adding a raw
  enhanced-input escape hatch.
- Phase 06 now places preset/bindings resolution after enhanced-input apply and before
  `OrbitCamInputPhase::WriteManual`, with manual cameras bypassing automatic
  resolution.
- Phase 08 now explicitly moves `ZoomDirection` out of `input/legacy.rs` before
  deleting the legacy module.
- Phase 10 now narrows binding coverage to ECS descriptor apply, installation, and
  resolver behavior because phase 03 added pure validation tests.

### 04-input-modes-and-installation (Complete)

Goal: add the runtime input-mode state machine and private installation ownership.

Scope:

- Promote the existing `OrbitCamPreset` and `OrbitCamBindings` runtime types into
  mutually exclusive input-mode components, and add the `OrbitCamManual` marker.
- Activate `OrbitCamPreset` as an input-mode component in the exclusive family; phase
  03 only introduced the preset value and conversion API.
- Add the observer shim for tidy component mutations and the exclusive `PreInput`
  invariant pass as the deterministic authority.
- Add `OrbitCamInputModeDescriptor`, `OrbitCamInputMode`, `OrbitCamInputModeApplied`,
  `OrbitCamInputModeRejected`, `OrbitCamInputModeApplyStatus`, and
  `OrbitCamInputModeApplyState`. Gate the descriptor apply systems and apply-status
  components behind `reflect-input-modes`; keep the concrete descriptor value types
  available as normal public API.
- Add `OrbitCamInputInstallationOf` / `OrbitCamInputInstallation` and private
  installation introspection helpers as an authoritative ownership record. Phase 06
  attaches actual enhanced-input action/context entities and adapter state to this
  ownership boundary.
- Add descriptor apply, validation, mode exclusivity, old-installation cleanup, and
  reconciliation inside the same exclusive `PreInput` structural boundary.
- Update `OrbitCamManualInputWriter` so it only yields writers for cameras that have
  `OrbitCamManual`.
- Keep mode-replacement cleanup structural in this phase: remove stale private
  installations and clear same-frame `OrbitCamInput`. Source-latch cleanup is
  completed in phase 05 after latches exist, and lifecycle queue cleanup is completed
  in phase 07 after lifecycle state exists.
- Implement phase 04 in two internal passes: first component promotion, exclusivity,
  defaults, and manual-writer filtering; then descriptor apply, apply-status events,
  and installation-record replacement.

Repository state: usable. Existing `OrbitCam` input behavior remains authoritative.
The new input-mode components may exist, but the old controller path is still the
behavioral source of truth.

Done when:

- Every `OrbitCam` has exactly one input-mode component by `PreInput` completion.
- Descriptor apply is atomic: a changed descriptor produces exactly one authoritative
  installation record in the same frame. Phase 06 is responsible for turning that
  record into enhanced-input entities and adapter state.
- Switching modes clears stale `OrbitCamInput` and stale private installations.
  Later latch and lifecycle cleanup phases must hook into the same mode-replacement
  signal rather than adding a second replacement path.

### Retrospective

**What worked:**

- `OrbitCamPreset`, opaque-reflected `OrbitCamBindings`, and `OrbitCamManual` now form
  the runtime input-mode component family without changing legacy controller behavior.
- The exclusive `PreInput` reconciler is the deterministic authority: it restores the
  default preset, resolves conflicts with `Manual > Bindings > Preset`, clears stale
  `OrbitCamInput`, and replaces the private installation record.
- Descriptor apply runs before reconciliation and keeps invalid drafts from replacing
  the previous valid runtime mode.

**What deviated from the plan:**

- `OrbitCamInputInstallation` is an authoritative private record with placeholder
  entities in phase 04. Phase 06 still owns real enhanced-input action/context and
  adapter entities.
- `OrbitCamInputModeRejected` keeps the structured `OrbitCamBindingsError`, but
  reflected `OrbitCamInputModeApplyStatus` stores the display string so the status
  component remains reflectable without forcing reflection onto the error enum.
- The observer shim skips same-tick conflicting mode inserts so the exclusive
  `PreInput` pass, not observer ordering, chooses the authoritative mode.

**Surprises:**

- Opaque reflection is available for `OrbitCamBindings`, so the runtime bindings
  component can derive `Reflect` without exposing unchecked private fields.
- Simultaneous bundle insertion of multiple mode components can fire tidy observers in
  an order that disagrees with mode precedence; tests now cover that edge.

**Implications for remaining phases:**

- Phase 05 should consume the phase 04 `OrbitCamInputModeReplaced` replacement hook to
  clear owner latches instead of adding a separate replacement detector.
- Phase 06 should replace placeholder installation entities with real
  enhanced-input/action/adapter entities through the existing installation record.
- Phase 07 should consume the same replacement point for lifecycle cleanup and keep
  manual finalization scoped to `OrbitCamManual`.

### Phase 4 Review

- Phase 04 now provides a named crate-private `OrbitCamInputModeReplaced` hook emitted
  exactly when input installation replacement runs; phases 05 and 07 should consume
  that hook for latch and lifecycle cleanup.
- Phase 06 now explicitly mutates the phase 04 installation record and replaces its
  placeholder entities instead of creating a competing installer path.
- Phase 05 diagnostics now focus on routing, blockers, and schedule placement; private
  enhanced-input context/entity diagnostics move to phase 06 when those entities exist.
- Phase 07 now owns manual zero-delta active helpers such as orbit-active, pan-active,
  and zoom-active before lifecycle behavior is considered complete.
- Phase 07 treats manual-writer filtering as an existing phase 04 precondition rather
  than reimplementing it.
- Phase 09 and phase 10 now separate always-on runtime modes from feature-gated
  reflected descriptor tooling.
- Phase 10 now avoids repeating phase 04 unit tests for descriptor apply and mode
  exclusivity unless routing, lifecycle, or resolver integration changes the behavior.
- Phase 06 still owns the binding-recipe modifier/condition decision before phase 09
  examples depend on keyboard/gamepad expressiveness.

### 05-routing-scheduling-and-blockers (Complete)

Goal: add deterministic frame routing and blocker computation.

Scope:

- Replace the planned public routing API with `CameraInputRouting::{CursorHitTest,
  Explicit}` and `CameraInputRoutingConfig`, while leaving old
  `CameraInputDetection` call sites intact until the cutover phase.
- Add internal resolved routing state, explicit resolver function, source-specific
  owner latches, `OrbitCamInputOwnerLatch`, deterministic latch recovery, no-position
  fallback routing, global gesture fallback, and per-camera logical surface metrics.
- Add `OrbitCamInputBlockers` as the single computed blocker source of truth.
- Build on the existing phase 01 `OrbitCamInputPhase` shell. Do not add a second
  public scheduling surface; add private internal sets only where ordering tests prove
  they are needed.
- Gate inactive `OrbitCamInputContext` state before `EnhancedInputSystems::Update`.
- Add tests and private diagnostic state for routing config, schedule setup, blocker
  computation, and input-mode replacement cleanup. Treat phase 01's plugin guard,
  `OrbitCamInputContext` registration, and `OrbitCamInputPhase` schedule shell as
  already installed; this phase validates routing/blocker setup instead of
  reintroducing setup paths.

Repository state: usable. Existing `OrbitCam` input behavior remains authoritative,
but the new route/blocker snapshots can be tested in isolation.

Done when:

- `CameraInputRoutingConfig` mutations take effect at the next `PreInput` route phase.
- Stale latches are cleared and rerouted in the same route phase.
- `OrbitCamInputModeReplaced` signals from phase 04 clear source latches in this
  phase.
- Surface metrics are derived per camera, including non-routed manual cameras.
- Egui, disabled, inactive-camera, animation-ignore, and unavailable-owner blockers
  all feed `OrbitCamInputBlockers`.

### Retrospective

**What worked:**

- `CameraInputRoutingConfig`, `CameraInputRouting`, and `NoPositionFallback` now give
  apps an always-on public routing preference without touching legacy
  `ActiveCameraData` behavior.
- The new internal routing set runs after input-mode reconciliation and publishes one
  per-frame route snapshot with per-camera metrics, blockers, and context-gating state.
- `OrbitCamInputModeReplaced` now clears owner latches through a single hook, and stale
  latch recovery runs inside the routing phase.

**What deviated from the plan:**

- Source latches are present as internal state, but public acquire/release operations
  wait until action/lifecycle phases have real held transitions to call them.
- Routing tests cover explicit routing, latch cleanup/recovery, disabled blockers,
  context gating, and non-routed manual metrics. Cursor/window hit-test behavior remains
  isolated in the resolver until later adapter/input paths need event-backed routing.
- `OrbitCamInputBlockers` and routing snapshots use internal bitflags instead of bool
  field structs to satisfy the workspace clippy profile and keep blocker composition
  compact.

**Surprises:**

- The existing legacy active-camera detector can stay completely separate while the
  new routing snapshot is introduced, preserving current controller behavior.
- Manual cameras need surface metrics even when they are deliberately not the routed
  preset/custom camera.

**Implications for remaining phases:**

- Phase 06 should read `ResolvedOrbitCamInputRoute`, `OrbitCamInputBlockers`, and
  `OrbitCamInputContextGated` instead of recomputing route or blocker state.
- Phase 06 should emit held-source transition intent from action engagement state, but
  phase 07 should apply latch acquire/release through the lifecycle authority so route,
  blocker, mode-replacement, despawn, and lifecycle cleanup cannot diverge.
- Phase 07 should use the same blocker snapshot during finalization and late
  pre-controller guarding.

### Phase 5 Review

- Phase 06 now consumes phase 05 routing, metrics, blockers, and context-gating
  snapshots instead of designing or recomputing them.
- Phase 06 now adds explicit internal scheduling sets for adapter injection before
  `EnhancedInputSystems::Update` and action resolution after
  `EnhancedInputSystems::Apply` but before `OrbitCamInputPhase::WriteManual`.
- Phase 06 treats manual input as already writing `OrbitCamInput`; remaining manual
  work moves to phase 07 finalization, active helpers, blockers, and lifecycle.
- Phase 07 now reshapes `OrbitCamInput` to store per-kind source sets before lifecycle
  events are implemented.
- Phase 07 now owns held-source transition authority: phase 06 reports engagement
  transitions, and phase 07 applies latch acquire/release alongside lifecycle state.
- Phase 07 now freezes late-blocker semantics before phase 08 cutover: late blockers
  may clear intent and end interactions, but cannot reroute or re-enable a camera gated
  off in `PreInput`.
- Phase 09 now includes a post-cutover docs pass over `input/mod.rs` and crate-root
  re-exports before examples and guidance are finalized.
- Phase 10 no longer assumes phase 05 shipped public startup diagnostics; diagnostics
  coverage tracks the concrete private diagnostics and events that actually exist.

### 06-adapters-and-action-resolution (Complete)

Goal: make enhanced-input actions and Lagrange adapters produce `OrbitCamInput`.

Scope:

- Add private adapter modules for wheel units, smooth scroll, pinch, touch, and future
  roll gesture input.
- Add private adapter diagnostics for tests and debug logs.
- Add private internal schedule sets for adapter injection before
  `EnhancedInputSystems::Update` and action resolution after
  `EnhancedInputSystems::Apply` but before `OrbitCamInputPhase::WriteManual`.
- Consume the phase 05 `ResolvedOrbitCamInputRoute`, `OrbitCamInputBlockers`,
  `OrbitCamInputContextGated`, and per-camera metrics snapshots. Do not recompute route
  or blocker state inside adapters/resolvers.
- Install action/context entities and private adapter state for preset and bindings
  modes by replacing the placeholder entities inside the existing
  `OrbitCamInputInstallation` record. Do not add a second installer ownership path,
  and do not tear down unchanged modes every frame.
- Add a crate-private binding installer/visitor that reads `OrbitCamBindings` held
  entries, motion recipes, engagement recipes, source metadata, and adapter policy
  without exposing private engagement actions or duplicating binding structure.
- Inject adapter-backed values before `EnhancedInputSystems::Update` and resolve
  public semantic actions plus adapter contributions into `OrbitCamInput`.
- Add the adapter/mock write portions of the private enhanced-input integration
  boundary introduced in phase 01. Earlier phases should not assume these paths exist.
- Preserve per-binding source attribution so lifecycle events can distinguish mouse,
  wheel, smooth-scroll, pinch, touch, keyboard, gamepad, and manual input.
- Keep camera action consumption non-consuming by default so app contexts can still
  observe shared bindings.
- Emit held-source transition intent from engagement actions, but do not mutate owner
  latches directly from the resolver. Phase 07 applies latch acquire/release through
  the serialized lifecycle authority.
- Add a private post-enhanced-input resolution set after `EnhancedInputSystems::Apply`
  and before `OrbitCamInputPhase::WriteManual`; preset and bindings modes write
  `OrbitCamInput` there, while manual cameras bypass automatic resolution.
- Before installing custom recipes, either extend `BindingRecipe` with the modifier
  and condition descriptors needed by in-tree keyboard/gamepad examples or reject
  unsupported advanced descriptors with structured validation errors. Do not add a raw
  public enhanced-input escape hatch that bypasses source metadata and held/impulse
  validation.
- Move private enhanced-input installation diagnostics here: context entity
  installation counts, missing context activation, expected enhanced-input action API
  shape, and adapter entity visibility are meaningful only after real action/context
  entities exist.

Repository state: usable. Existing `OrbitCam` input behavior remains authoritative.
The new pipeline can be tested by reading `OrbitCamInput`, but the controller has not
cut over yet.

Done when:

- Wheel line/pixel, pinch, touch, keyboard, mouse, and gamepad writes can each produce
  expected `OrbitCamInput` values in ECS tests. Manual write smoke coverage may remain,
  but manual finalization, active helpers, blockers, and lifecycle are phase 07 work.
- Adapter injection is visible to enhanced input in the same frame.
- Inactive/gated cameras do not retain stale enhanced-input action state.
- The installer uses the phase 03 binding typestate/accessors to attach private
  engagement actions and adapter policy; it does not re-derive held/impulse structure
  from raw recipes.

### Retrospective

**What worked:**

- The phase 04 installation record became the single ownership path for real
  enhanced-input action entities, binding entities, and private adapter actions.
- Private adapter actions plus `ActionMock` let wheel, pinch, touch, and button-drag
  values enter enhanced-input timing without overriding public semantic actions.
- Phase 05 routing, blockers, context gating, and surface metrics were consumed as
  snapshots; the adapter/resolver does not recompute route or blocker state.

**What deviated from the plan:**

- `OrbitCamInput` gained crate-private source-set mutation helpers instead of a
  separate private write-token type. Public mutation still only goes through
  `OrbitCamManualInput`.
- Camera actions use non-consuming, non-resetting `ActionSettings` so newly installed
  contexts can respond in the same frame. Context gating clears blocked action state
  instead of relying on first-activation reset.
- Touch production code reads `TouchTracker`, while adapter ECS tests use a test-only
  touch-gesture override because Bevy's concrete `Touch` fields are private.

**Surprises:**

- Enhanced-input gamepad button bindings read the analog gamepad button value; tests
  need to set both the analog value and digital pressed state.
- `ActionSettings::require_reset` suppresses inputs that are already active when a
  binding is installed, which conflicts with the same-frame installation guarantee.

**Implications for remaining phases:**

- Phase 07 can use `OrbitCamHeldSourceTransitionIntents` as the resolver-owned handoff
  for latch acquire/release and lifecycle serialization after it refines the handoff to
  identify the specific active source contribution, not only the action-set union.
- Phase 07 should preserve the current late-blocker behavior: context gating resets
  action state, and finalization clears `OrbitCamInput` for blocked cameras.
- Phase 09 gamepad examples should show analog button values for tests and explain
  that selected-gamepad policy remains future work beyond the current `Active`/`Disabled`
  enum.

### Phase 6 Review

- Phase 07 now says the public lifecycle event/state shells already exist; remaining
  work is lifecycle queue emission and `OrbitCamInteractionState` mutation.
- Phase 07 now keeps per-kind `OrbitCamInput` source sets as the first lifecycle
  prerequisite, because the current frame input still has one merged public source set.
- Phase 07 now refines `OrbitCamHeldSourceTransitionIntents` before latch/lifecycle
  use so transitions name the specific active source contribution, not only a unioned
  binding-set source.
- Phase 07 now explicitly makes latches and `BindingRoutePolicy` influence no-position
  routing before phase 08 cutover.
- Phase 07 now owns screen-pixel manual finalization and `CameraInputMetricsMissing`
  emission for cameras without required logical metrics.
- Phase 07 now owns pinch suppression for active non-pinch modifiers or held camera
  actions on the routed camera.
- Phase 10 diagnostics scope now reflects what phase 06 actually shipped: basic
  adapter count diagnostics exist, richer live diagnostics still need implementation
  or narrower tests.
- Phase 09 now requires the gamepad example to document analog button values for
  tests and the current `Active`/`Disabled` policy limit.
- Phase 08 keeps the existing `ZoomDirection` migration note because the new bindings
  still import it from `input/legacy.rs`.

### 07-lifecycle-and-manual-finalization (Complete)

Goal: make source-aware interaction events and finalization deterministic.

Scope:

- Add the serialized lifecycle queue and update the existing
  `OrbitCamInteractionState`. The public event/state types already exist; this phase
  wires emission and mutation rather than creating a new public event surface.
- Reshape `OrbitCamInput` so it stores per-kind source sets for orbit, pan, and zoom
  before lifecycle events are derived. A single merged source set cannot represent
  simultaneous interactions such as mouse orbit plus wheel zoom.
- Emit `OrbitCamInteractionStarted`, `OrbitCamInteractionEnded`, and
  `OrbitCamInteractionSourcesChanged` from finalized `OrbitCamInput`.
- Finalize manual, preset, and bindings input after `OrbitCamInputPhase::WriteManual`.
- Add manual active-state helper methods for held zero-delta input, such as
  orbit-active, pan-active, and zoom-active writes. Lifecycle events must not require
  a nonzero delta when an app wants to report a held manual interaction.
- Apply `CameraInputDisabled`, egui focus, inactive routing, unavailable-owner, and
  animation-ignore blockers before events are flushed.
- Add the pre-controller guard that cancels or replaces queued events if a late
  blocker suppresses input.
- Add `animation_input_interrupt` wiring for finalized `OrbitCamInput` while the old
  controller path is still present.
- Apply held-source latch acquire/release through the lifecycle queue from phase 06
  transition intent. Resolver systems should not mutate latches independently.
- Refine phase 06 held transition intents before consuming them: lifecycle needs the
  specific source contribution that became active or inactive, not only the unioned
  source set for an action binding group.
- Make source latches affect routing for no-position held input. `BindingRoutePolicy`
  values should decide whether an input can acquire from cursor position, an existing
  latch, explicit routing, or no-position fallback before phase 08 cutover.
- Finalize logical metrics handling for screen-pixel manual input. If a camera lacks
  the metrics needed to translate that input, drop the input for that frame and emit
  `CameraInputMetricsMissing` through the lifecycle/finalization path.
- Implement the pinch-suppression behavior documented in the adapter design: pinch
  zoom is ignored while non-pinch camera modifiers or held camera actions are active
  for the routed camera.
- Settle late-blocker semantics before phase 08: late blockers may clear intent and
  end active interactions, but they must not reroute input or re-enable a camera that
  `PreInput` already gated off.

Repository state: usable. Existing `OrbitCam` input behavior remains authoritative,
but the new lifecycle events may be tested against the new pipeline.

Done when:

- Held interactions emit one started event, source-change events for joins/leaves, and
  one ended event.
- `OrbitCamInteractionState` maintains independent source sets for orbit, pan, and
  zoom so simultaneous interactions can be observed correctly.
- Impulse interactions emit started and ended in the same frame.
- Input-mode replacement, despawn cleanup, and blockers cannot duplicate lifecycle
  events.
- `OrbitCamInputModeReplaced` signals from phase 04 clear active interaction state
  through the lifecycle queue in this phase.
- Manual writers already work only for `OrbitCamManual` cameras; this phase verifies
  finalization and lifecycle behavior for manual input.
- Pinch suppression is covered for a held camera action on the routed camera and for a
  held modifier/action on a non-routed camera that must not suppress routed pinch.
- Latches influence no-position routing before lifecycle events acquire or release
  ownership.

### Retrospective

**What worked:**

- `input/lifecycle.rs` became the single finalization point for interaction events,
  `OrbitCamInteractionState`, blockers, metrics checks, and latch acquire/release.
- Per-kind source sets in `OrbitCamInput` were enough to serialize simultaneous
  orbit, pan, and zoom interactions without adding a separate lifecycle queue
  resource.
- Source latches now influence cursor-hit routing before no-position fallback, so
  held keyboard/mouse ownership can keep routing stable after the cursor leaves a
  camera surface.

**What deviated from the plan:**

- Lifecycle does not consume `OrbitCamHeldSourceTransitionIntents`. The resolver now
  writes refined per-kind source sets directly into `OrbitCamInput`, and finalization
  derives started/ended/source-change events from those finalized source deltas.
- Late blockers clear finalized input and end active interactions in the finalizer
  rather than through a separate pre-controller event guard.
- Pinch suppression covers active keyboard and mouse-button engagement recipes. Gamepad
  suppression remains coupled to future selected-device policy work.
- `animation_input_interrupt` remains controller-cutover work because the legacy
  controller still owns behavior until phase 08 consumes finalized `OrbitCamInput`.

**Surprises:**

- Manual screen-pixel metric coverage needed a lifecycle-only test app because the
  routing plugin intentionally supplies default camera metrics.
- Wheel zoom needed separate impulse-source tracking so it can emit same-frame
  started/ended events without leaving zoom marked active in
  `OrbitCamInteractionState`.

**Implications for remaining phases:**

- Phase 08 must wire `animation_input_interrupt` at the same time the controller moves
  to finalized `OrbitCamInput`.
- Phase 08 should remove `OrbitCamHeldSourceTransitionIntents` and its
  `push_held_intent` write path because finalized per-kind source deltas are now the
  lifecycle authority.
- Phase 10 lifecycle tests should focus on finalizer ordering, blockers, metrics,
  latch recovery, and impulse-vs-held state rather than a separate lifecycle queue
  resource.

### Phase 7 Review

- Phase 08 now explicitly preserves finalization-before-controller ordering and places
  `animation_input_interrupt` after finalization but before controller movement.
- Phase 08 now replaces `ActiveCameraData` metric consumption with finalized
  routing/surface metrics instead of switching only the intent payload.
- Phase 08 now removes the obsolete `OrbitCamHeldSourceTransitionIntents` resource and
  `push_held_intent` path during cutover.
- Phase 09 now documents current latch scope: mouse-like and keyboard ownership are in
  scope, while gamepad/touch owner latches wait for selected-device/touch-owner policy.
- Phase 10 now focuses lifecycle coverage on cross-system cutover, scheduling,
  interrupt policy, workspace consumers, and diagnostics rather than duplicating phase
  07 unit coverage.
- Phase 08 now names legacy systems and public facades that must be removed or
  replaced: `mouse_key_tracker`, `active_viewport_data`, `ActiveCameraData`, and
  legacy input re-exports.

### 08-breaking-cutover-and-callers (Complete)

Goal: switch the actual camera controller to the new input model and remove the old
API.

Scope:

- Make `OrbitCam` require `OrbitCamInput`, `OrbitCamInputContext`, and the default
  `OrbitCamPreset::SimpleMouse`.
- Switch `orbit_cam` controller movement to consume finalized `OrbitCamInput`.
- Preserve ordering during the cutover: `OrbitCamInputPhase::Finalize` must complete
  before animation interruption and controller movement, and
  `animation_input_interrupt` must read finalized input after finalization but before
  the controller consumes movement.
- Replace the legacy `ActiveCameraData` metric path with finalized
  `CameraInputSurfaceMetrics` and resolved routing metrics. The controller should not
  keep reading stale `window_size` or `viewport_size` values from the old active-camera
  pipeline after it consumes semantic input.
- Remove `OrbitCamHeldSourceTransitionIntents` and its `push_held_intent` write path;
  lifecycle and latch transitions are derived from finalized per-kind source sets on
  `OrbitCamInput`.
- Remove old physical input fields from `OrbitCam`, including old mouse/key/touch,
  trackpad, wheel, button-zoom, and zoom-direction input fields.
- Remove or replace the old raw-input pipeline systems and facade exports, including
  `mouse_key_tracker`, `active_viewport_data`, `ActiveCameraData`, and public legacy
  input re-exports from `input/mod.rs` and `lib.rs`.
- Move `ZoomDirection` out of `input/legacy.rs` before deleting legacy raw-input code,
  or otherwise give the public facade a non-legacy home for the binding zoom policy.
- Remove `CameraInputDetection::{Automatic, Manual}` and migrate to
  `CameraInputRouting::{CursorHitTest, Explicit}`.
- Migrate every in-repo example and workspace consumer that still references legacy
  `OrbitCam` input fields, especially `crates/bevy_diegetic/examples/*`.
- Update egui blocking to feed internal UI-focus blockers instead of old controller
  fields.

Repository state: temporarily unusable inside the phase after old fields are removed.
It becomes usable again only after all in-repo callers are migrated and the controller
consumes `OrbitCamInput`. This phase should be one commit or PR; do not commit the
half-cutover state.

Done when:

- Existing examples and workspace consumers compile against `OrbitCamPreset`,
  `OrbitCamBindings`, `OrbitCamManual`, `CameraInputDisabled`, and
  `CameraInputRouting`.
- No call site references removed raw `OrbitCam` input fields or old
  `CameraInputDetection`.
- The default camera still works with mouse-oriented `SimpleMouse` behavior.
- `CameraInputInterruptBehavior::{Ignore, Cancel, Complete}` preserve their old
  externally visible behavior through finalized `OrbitCamInput`.
- Orbit and pan scaling use finalized routing/surface metrics, including explicit
  render-to-texture metrics, rather than `ActiveCameraData`.

### Retrospective

**What worked:**

- `OrbitCam` now requires `OrbitCamInput`, `OrbitCamInputContext`, and
  `OrbitCamPreset`; the controller consumes finalized `OrbitCamInput`.
- Removing `ActiveCameraData`, `CameraInputDetection`, `mouse_key_tracker`, and the
  legacy input facade forced all in-repo callers onto the new input modes or default
  `SimpleMouse` preset.
- `render_to_texture.rs` now uses `CameraInputRoutingConfig::explicit` plus
  `CameraInputSurfaceMetrics`, which validates the intended replacement for manual
  active-camera setup.

**What deviated from the plan:**

- Existing examples were migrated to compile against the new API, but most were not
  redesigned as teaching examples. Phase 09 still owns polished preset, bindings,
  gamepad, manual, and guidance examples.
- Animation input interruption now reads finalized `OrbitCamInput` in the existing
  `process_camera_move_list` system rather than adding a separately named
  `animation_input_interrupt` system.
- `CameraInputSurfaceMetrics` remains the explicit per-camera override component, but
  routing no longer overwrites that component with derived values every frame. The
  resolved route resource carries derived metrics, and explicit component values
  override derived fields when present.

**Surprises:**

- The old `TouchInput` API had no remaining production role after the adapter cutover;
  `OrbitCamTouchBinding` fully replaced it.
- Several examples only used old raw-input fields to request Blender-like trackpad
  behavior. After the cutover those examples can use the default `SimpleMouse` path
  until phase 09 adds explicit input-mode examples.

**Implications for remaining phases:**

- Phase 09 should decide which existing examples should opt into `OrbitCamPreset` or
  `OrbitCamBindings` explicitly instead of silently relying on the default preset.
- Phase 09 should update user-facing prose and example comments that still describe
  old middle-click or trackpad behavior after the compile-only migration.
- Phase 10 should add cross-system tests for animation interrupt policies and
  controller movement from finalized input; phase 08 only added focused controller
  unit coverage for scaling and metric precedence.

### Phase 8 Review

- Phase 09 is now scoped as teaching polish, explicit example selection, guidance UI,
  and stale prose cleanup. Broad caller migration is already complete.
- Phase 09 now documents the phase 08 metrics contract: derived metrics live in the
  resolved route resource, while explicit `CameraInputSurfaceMetrics` component fields
  override derived values when present.
- Phase 10 now tests the route-resource plus explicit-metrics override path end to end.
- Phase 10 now targets the implemented animation-interrupt shape:
  `process_camera_move_list` reads finalized input in `Update` after `Finalize` and
  before controller movement.
- Phase 10 now includes an ECS controller integration test where finalized input
  changes yaw, pitch, focus, or radius through the public plugin schedule.
- Phase 09 keeps gamepad/touch ownership wording future-scoped; Phase 10 either tests
  current non-latching behavior or removes speculative gamepad/touch latch fields.
- Phase 09 now audits existing comments and README-style prose before adding new
  examples so stale middle-click/trackpad wording does not leak into the teaching
  surface.
- Phase 10 now decides whether diagnostics expose concrete blocker causes such as
  disabled camera, inactive camera, egui focus, and animation-ignore blockers.

### 09-examples-guidance-and-doc-cleanup (Complete)

Goal: add the teaching examples and visual feedback requested by the new API.

Scope:

- Use the existing workspace `fairy_dust` crate to add the camera guidance panel and
  component-insertion camera setup needed by input-mode examples.
- Treat existing example caller migration as complete from phase 08. This phase owns
  teaching polish, explicit example selection, guidance UI, and stale prose cleanup.
- Audit existing example comments and README-style prose before adding new examples so
  old raw-input, middle-click, trackpad, and active-camera wording is removed or
  updated.
- Add separate examples:
  `orbit_cam_preset_blender_like.rs`, `orbit_cam_preset_simple_mouse.rs`,
  `orbit_cam_bindings_keyboard.rs`, `orbit_cam_bindings_gamepad.rs`, and
  `orbit_cam_manual.rs`.
- The gamepad binding example should explain that tests and synthetic input must set
  analog gamepad button values as well as digital pressed state. It should also note
  that the current custom gamepad policy is `Active`/`Disabled`; selected-gamepad
  routing remains future work until a selected-device API lands.
- Document that current source latches stabilize mouse-like and keyboard held
  ownership. Gamepad and touch source attribution is supported, but owner latching for
  those sources remains future selected-device or touch-owner policy work.
- Document the current surface-metrics model consistently: routing derives per-camera
  metrics into `ResolvedOrbitCamInputRoute`, and explicit
  `CameraInputSurfaceMetrics` component fields override derived values only where
  present. Examples should not imply that routing overwrites the explicit component
  every frame.
- Consume `OrbitCamInteractionStarted`, `OrbitCamInteractionEnded`,
  `OrbitCamInteractionSourcesChanged`, and `OrbitCamInteractionState` in examples so
  guidance text highlights active orbit, pan, and zoom rows with source attribution.
- Update existing examples according to the example migration notes.
- After phase 08 cutover, update the public `input/mod.rs` docs and crate-root
  re-export docs so they no longer describe the new input module as merely additive
  while legacy raw-input fields remain authoritative.
- Collapse the old `zoom_to_fit/main.rs` plus `constants.rs` directory example back
  into one `zoom_to_fit.rs` file.
- Keep examples that use `OrbitCamPreset`, `OrbitCamBindings`, and `OrbitCamManual`
  available without `reflect-input-modes`. Descriptor/editor examples that use
  `OrbitCamInputModeDescriptor` or apply-status components must declare the
  feature-gated requirement explicitly.

Repository state: usable.

Done when:

- Examples show how to use every supported input mode.
- Fairy-dust guidance visibly distinguishes mouse, wheel, smooth-scroll, pinch, touch,
  keyboard, gamepad, and manual sources where the example supports them.
- Render-to-texture examples demonstrate explicit routing plus logical surface
  metrics.
- Guidance and docs do not imply stable gamepad or touch owner latching; they describe
  those as future selected-device or touch-owner policy work.

### Retrospective

**What worked:**

- `fairy_dust::CameraGuidance` now drives live guidance panels from
  `OrbitCamInteractionState` and interaction lifecycle observers.
- Separate examples now cover `OrbitCamPreset`, `OrbitCamBindings`,
  `OrbitCamManual`, custom bindings, render-to-texture routing, and zoom-to-fit.

**What deviated from the plan:**

- Keyboard and gamepad examples needed additive `BindingRecipe` variants for cardinal
  keys, bidirectional keys, 2D gamepad axes, and bidirectional analog buttons.
- `advanced.rs` became `custom_bindings.rs`, `keyboard_controls.rs` was removed, and
  `zoom_to_fit` was collapsed into `zoom_to_fit.rs`.

**Surprises:**

- `bevy_lagrange` examples can dev-depend on `fairy_dust` without a Cargo cycle even
  though `fairy_dust` depends on `bevy_lagrange`.
- Named-field `BindingRecipe` variants interacted poorly with `Reflect` under clippy,
  so the new recipe variants are tuple-style.

**Implications for remaining phases:**

- Phase 10 should test the new multi-binding `BindingRecipe` variants, not only the
  older single-key and single-axis paths.
- Phase 10 should include a quick stale-example audit for removed example names and
  renamed `custom_bindings.rs` references.

### Phase 9 Review

- Phase 10 now explicitly tests the new tuple-style multi-binding `BindingRecipe`
  variants through installation and ECS resolver behavior.
- Phase 10 adapter coverage is narrowed away from already-covered mouse, wheel,
  scroll, pinch, touch, single keyboard, single gamepad, manual bypass, and gating
  paths and toward new variant coverage plus cross-system integration.
- Phase 10 now treats gamepad/touch latch cleanup as a required gate before closing
  lifecycle coverage: either remove speculative internal fields or test the current
  non-latching behavior.
- Phase 10 diagnostics are narrowed to concrete internal diagnostics and implemented
  log/event behavior unless a separate diagnostics implementation is intentionally
  added.
- Phase 10 stale-example cleanup is a targeted manifest/doc/reference audit for
  removed and renamed examples, not another broad migration pass.
- Phase 10 workspace consumer validation now explicitly includes `fairy_dust`
  guidance and bundle camera setup paths.

### 10-tests-diagnostics-and-cleanup

Goal: harden the cutover and remove leftover transitional code.

Scope:

- Add ECS-only tests for scheduling invariants, descriptor apply, reconciliation,
  input-mode exclusivity, routing, blockers, lifecycle events, latch recovery,
  focused adapter behavior, manual writes, interrupt policies, workspace consumers,
  and dependency versioning.
- Add ECS installation and resolver coverage for the phase 09 tuple-style
  `BindingRecipe` variants: `CardinalKeys`, `BidirectionalKeys`, `GamepadAxes2d`,
  and `BidirectionalGamepadButtons`.
- Do not repeat the phase 07 unit coverage for held transitions, same-frame impulses,
  manual zero-delta activity, blocker clearing, metric drops, latch routing, stale
  latch recovery, or pinch suppression. Phase 10 lifecycle coverage should focus on
  cross-system cutover tests, finalizer/controller scheduling, interrupt-policy
  integration, workspace consumers, and diagnostics.
- Add an ECS controller integration test where finalized input changes yaw, pitch,
  focus, or radius through the public plugin schedule.
- Add end-to-end coverage for the route-resource metrics path plus explicit
  `CameraInputSurfaceMetrics` component overrides.
- Test animation interruption through the implemented system shape:
  `OrbitCamInputPhase::Finalize` in `PreUpdate`, `process_camera_move_list` in
  `Update`, and controller movement in `PostUpdate`. Do not look for a separate
  `animation_input_interrupt` system.
- Either test the current non-latching behavior for gamepad/touch ownership or remove
  speculative gamepad/touch fields from the internal latch resource.
- Do not duplicate the phase 03 pure binding-validator unit tests. Phase 10 binding
  coverage should focus on descriptor apply, installation replacement, ECS resolver
  behavior, new multi-binding recipe variants, and integration with
  routing/lifecycle/blockers.
- Do not duplicate the phase 04 unit tests for default preset restoration, manual
  precedence, descriptor success/rejection, and manual-writer filtering unless later
  routing, lifecycle, or resolver integration changes those behaviors.
- Add the `enhanced_input_scheduling_invariant` test.
- Add regression coverage only for diagnostics that concretely exist by phase 10.
  Phase 05 added routing/blocker resources and tests, and phase 06 added private
  adapter count/gated-camera diagnostics. Do not create tests that imply a public
  startup-diagnostics or blocker-cause API unless phase 10 intentionally implements
  that API first.
- Decide whether diagnostics expose concrete phase 08 blocker causes, including
  disabled cameras, inactive cameras, egui focus, and animation-ignore blockers. If
  those causes remain internal-only, diagnostics tests should not imply a public
  blocker-cause API exists.
- Do not add strict startup diagnostic tests for schedule/plugin/context/enhanced-input
  API assumptions unless those diagnostics are intentionally implemented first; keep
  the phase 10 diagnostics pass scoped to private diagnostics and documented log/event
  behavior.
- Audit manifests and docs for stale references to removed or renamed examples:
  `advanced.rs`, `keyboard_controls.rs`, the old `zoom_to_fit/` directory example,
  and the new `custom_bindings.rs` name.
- Treat `fairy_dust` as a workspace consumer of the public input API. Validation
  should compile the guidance panel, lifecycle/source-flag consumption, and
  bundle-based orbit-camera setup paths.
- Remove any internal compatibility scaffolding used only to keep phases 01-07
  side-by-side with legacy input.

Repository state: usable.

Done when:

- The full workspace validation target passes.
- There are no references to old input fields, old routing names, or temporary
  compatibility modules.
- The test suite covers the event, routing, schedule, adapter, descriptor, and manual
  input invariants described in this plan.
- The new multi-binding `BindingRecipe` variants are covered by ECS resolver tests.
- Stale example names and old `zoom_to_fit` paths are absent from manifests and
  user-facing docs.
- Feature-gated descriptor tooling tests run separately from always-on runtime mode
  tests so `--no-default-features` remains meaningful.

### Retrospective

**What worked:**

- Adapter ECS tests now cover `CardinalKeys`, `BidirectionalKeys`,
  `GamepadAxes2d`, and `BidirectionalGamepadButtons` through installed bindings.
- The public `LagrangePlugin` schedule is covered by
  `enhanced_input_scheduling_invariant`, which exercises manual writes, lifecycle
  finalization, animation cancellation, and controller movement in one frame.

**What deviated from the plan:**

- Diagnostics coverage stayed internal and concrete: route blockers, route metrics,
  adapter counts, and existing lifecycle events. Phase 10 did not add public startup
  diagnostics or blocker-cause APIs.
- Speculative gamepad and touch latch maps were removed instead of preserved behind
  tests, matching the phase 09 documentation that selected-device/touch-owner
  policies are future work.

**Surprises:**

- Minimal `LagrangePlugin` schedule tests exposed that the plugin should initialize
  Bevy's `PinchGesture` message and `Touches` resource because its own systems read
  them directly.

### Phase 10 Review

- The plugin setup contract now says `LagrangePlugin` owns its direct camera-input
  resources/messages (`Touches` and `PinchGesture`) while event production still
  comes from Bevy input plugins.
- Strict startup-diagnostics claims were removed or deferred; the shipped diagnostic
  surface remains private adapter diagnostics, route/blocker state, lifecycle events,
  and missing-metrics events.
- Surface-metrics snippets now show `CameraInputSurfaceMetrics` as a camera component
  override rather than a `CameraInputRoutingConfig` builder method.
- Future cleanup now names gamepad/touch owner latching as selected-device and
  touch-owner policy work.
- The schedule-test scope is recorded as a public-plugin manual input, finalization,
  animation, and controller invariant, while multi-binding resolver coverage remains
  adapter ECS coverage.

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
- Add an internal finalization path that derives started/ended/source-change events
  from finalized per-kind source deltas and applies blocker, metric, and latch
  cleanup before the controller consumes input.
- Add `ManualInputSource` so manual camera input always reports `MANUAL` and may include observed device provenance.
- Add logical `CameraInputSurfaceMetrics` for explicit routing, render-to-texture, and custom editor input surfaces.
- Add structured binding validation, private adapter diagnostics, and missing-metrics
  events for common setup mistakes.
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

### Gamepad And Touch Ownership

The initial refactor supports gamepad and touch source attribution, but stable
per-device gamepad ownership and per-touch ownership are future policy work. Source
latches currently stabilize mouse-like and keyboard ownership only. Add gamepad
owner latches after a selected-gamepad API exists, and add touch owner latches only
with a concrete touch-owner policy that defines what happens when fingers begin,
end, or transfer between cameras.

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
