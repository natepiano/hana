# OrbitCam Preset API

This document captures the planned preset API shape.

The design goal is that casual users spawn a preset camera with one helper, while users who need changes can start from the same preset defaults and override only the pieces they care about.

## Decisions

- Keep `OrbitCamInputMode` as the runtime mode component.
- Keep `OrbitCamPreset` as the compact built-in preset selector.
- Add ergonomic `OrbitCam` helpers so most users do not need to write `OrbitCamInputMode` directly.
- Add concrete configurable preset types as the source of truth for preset defaults.
- Put fluent tuning methods directly on concrete preset config structs.
- Remove `OrbitCamPresetLayers`, `OrbitCamPresetLayer`, and `PresetLayerSet`.
- Remove `OrbitCamBindingsProfile`.
- Treat tuned preset bindings as `OrbitCamInputMode::Bindings`, displayed as custom bindings.
- Keep preset config traits crate-owned or sealed. Public users extend controls through `OrbitCamBindings::builder()`.

## Runtime Mode Model

`OrbitCamInputMode` remains necessary. It is the component that selects the input runtime behavior for an `OrbitCam`.

```rust
pub enum OrbitCamInputMode {
    Preset(OrbitCamPreset),
    Bindings(OrbitCamBindings),
    Manual,
}
```

These variants are the only mode model users should have to understand:

| Mode | Meaning |
| --- | --- |
| `Preset(OrbitCamPreset)` | Built-in default input mapping |
| `Bindings(OrbitCamBindings)` | App-owned or tuned validated bindings |
| `Manual` | App code writes camera intent through `OrbitCamManualInputWriter` |

The new constructors hide this component for common spawn paths; they do not replace it internally.

`OrbitCamInputMode` and `OrbitCamPreset` should remain `#[non_exhaustive]` for future extension. Public docs should avoid exhaustive downstream `match` examples unless they include a wildcard arm.

## User-Facing Tiers

| User goal | API |
| --- | --- |
| Spawn a default preset camera | `OrbitCam::blender_like()` |
| Spawn a camera with tuned preset bindings | `OrbitCam::with_bindings(bindings)` |
| Spawn a manually driven camera | `OrbitCam::manual()` |
| Build tuned preset bindings | `OrbitCamBlenderLikePreset::default().slow_scale(0.25).build()?` |
| Fully custom controls | `OrbitCamBindings::builder()` |

The common path should be this small:

```rust
commands.spawn((
    Transform::from_xyz(0.0, 1.5, 5.0),
    OrbitCam::blender_like(),
));
```

Equivalent helpers:

```rust
commands.spawn((
    Transform::from_xyz(0.0, 1.5, 5.0),
    OrbitCam::simple_mouse(),
));

commands.spawn((
    Transform::from_xyz(0.0, 1.5, 5.0),
    OrbitCam::gamepad(),
));

commands.spawn((
    Transform::from_xyz(0.0, 1.5, 5.0),
    OrbitCam::manual(),
));
```

Helpers should exist for every built-in preset:

| Preset | Helper |
| --- | --- |
| `SimpleMouse` | `OrbitCam::simple_mouse()` |
| `BlenderLike` | `OrbitCam::blender_like()` |
| `Keyboard` | `OrbitCam::keyboard()` |
| `SimpleMouseKeyboard` | `OrbitCam::simple_mouse_keyboard()` |
| `BlenderLikeKeyboard` | `OrbitCam::blender_like_keyboard()` |
| `Gamepad` | `OrbitCam::gamepad()` |

`OrbitCam` already requires `Camera3d`, `OrbitCamInput`, `OrbitCamInputContext`, and `OrbitCamInputMode`, so these helpers do not need to include `Camera3d`.

## Source of Truth

Concrete configurable preset types are the implementation source of truth.

`OrbitCamPreset::BlenderLike` delegates to `OrbitCamBlenderLikePreset::default().build()`.

`OrbitCamPreset::Gamepad` delegates to `OrbitCamGamepadPreset::default().build()`.

`OrbitCamPreset::SimpleMouse` delegates to `OrbitCamSimpleMousePreset::default().build()`.

`OrbitCamPreset::Keyboard` delegates to `OrbitCamKeyboardPreset::default().build()`.

Composed enum presets should also delegate to concrete source-of-truth code, not to a public layer builder:

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
        }
    }
}
```

The enum remains useful as the compact default selector, but it should not duplicate binding construction logic.

Composed preset config structs should compose child configs directly:

```rust
pub struct OrbitCamBlenderLikeKeyboardPreset {
    pointer:  OrbitCamBlenderLikePreset,
    keyboard: OrbitCamKeyboardPreset,
}
```

Each concrete preset family should expose private `build_into(builder)` helpers so composed presets reuse the child preset logic without reintroducing public layers or duplicating binding construction.

Composed presets should still be configurable without exposing layers. They should provide child-config setters and common pass-through setters:

```rust
let bindings = OrbitCamBlenderLikeKeyboardPreset::default()
    .blender_like(OrbitCamBlenderLikePreset::default().slow_scale(0.25))
    .build()?;
```

`OrbitCamBlenderLikeKeyboardPreset` inherits BlenderLike slow mode by default. Users can tune or disable it through the composed config's BlenderLike child.

## Configurable Preset Build Contract

Each concrete preset config exposes an inherent `build()` method:

```rust
impl OrbitCamBlenderLikePreset {
    pub fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        <Self as OrbitCamPresetConfig>::build(self)
    }
}
```

This keeps tuned preset examples small. Users should not need to import a trait just to call `.build()?`.

Inside the crate, each concrete preset config implements a shared crate-owned or sealed trait:

```rust
trait OrbitCamPresetConfig: Sized {
    fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError>;
}
```

`Sized` is enough because preset configs are concrete values consumed by `build(self)`. The trait is not intended for `dyn OrbitCamPresetConfig`; each concrete preset type stores its own configuration.

Example:

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

impl OrbitCamPresetConfig for OrbitCamBlenderLikePreset {
    fn build(self) -> Result<OrbitCamBindings, OrbitCamBindingsError> {
        // Builds validated Blender-like bindings.
    }
}
```

The trait defines internal common behavior. The concrete preset type owns whatever storage it needs.

This trait is not the public extension model. External users who need new control schemes should build `OrbitCamBindings` directly.

## Customization Surface

Users should not need configurable preset types for defaults. They use them only when overriding defaults:

```rust
let bindings = OrbitCamBlenderLikePreset::default()
    .slow_scale(0.25)
    .build()?;

commands.spawn((
    Transform::from_xyz(0.0, 1.5, 5.0),
    OrbitCam::with_bindings(bindings),
));
```

Disable a default:

```rust
let bindings = OrbitCamBlenderLikePreset::default()
    .slow_toggle_key(None)
    .build()?;
```

Tune gamepad:

```rust
let bindings = OrbitCamGamepadPreset::default()
    .slow_orbit_scale(160.0)
    .slow_pan_scale(120.0)
    .build()?;
```

Do not keep a separate public `OrbitCamGamepadPresetBuilder`. With `OrbitCamBindingsProfile` removed, the `customized` flag and `.customize()` indirection are no longer needed. Each concrete preset config should provide fluent setters directly.

Preset config `build()` methods should validate preset-specific numeric invariants before producing bindings. For slow mode, `slow_scale` must be finite and should be greater than `0.0` and less than or equal to `1.0`, so slow mode cannot invert or amplify input.

Validation should live in each concrete config's `build()` method before descriptor construction:

| Config | Invariants |
| --- | --- |
| `OrbitCamBlenderLikePreset` | `slow_scale` is finite, greater than `0.0`, and less than or equal to `1.0` when slow mode is enabled |
| `OrbitCamGamepadPreset` | fast and slow orbit, pan, and zoom scales are finite and non-negative |
| `OrbitCamGamepadPreset` | slow scale values are no greater than their corresponding fast scale values |
| `OrbitCamGamepadPreset` | stick dead-zone lower and upper bounds are finite, ordered, and inside the accepted input range |
| Composed presets | child config validation runs through the child `build_into` path |

Later preset-specific tunables should add their own invariants at the config layer, not rely only on generic binding validation.

## Built-In BlenderLike Slow Mode

`OrbitCamPreset::BlenderLike` should include Lagrange's slow mode by default.

Default:

| Setting | Value |
| --- | --- |
| Slow toggle | `KeyCode::CapsLock` |
| Slow behavior | Reduces orbit, pan, and zoom scale |
| Default slow scale | `0.15` |

CapsLock slow mode is a Lagrange-maintained toggle state, not a held key gate and not the operating system CapsLock text state. Pressing CapsLock toggles Lagrange slow mode on or off for the camera input context.

The existing held-binding gate model is not sufficient for this by itself because those gates are active only while the gate input is pressed.

The implementation should add a small typed runtime contract:

- Preset configs emit a validated slow-mode descriptor, for example `OrbitCamSlowMode { toggle_key, scale }` or an equivalent `OrbitCamRuntimeGate::SlowMode`.
- The adapter owns a private per-camera slow-mode latch, tied to the routed `OrbitCamInputContext`.
- The latch toggles on the key edge, not on key-repeat and not from the operating system CapsLock text state.
- Slow mode applies only to the camera whose input context receives the routed toggle.
- When a camera's mode or bindings no longer reference slow mode, the adapter ignores or resets that camera's slow latch.

Slow scaling must cover every BlenderLike movement source:

| Source | Slow coverage |
| --- | --- |
| Mouse drag orbit | scaled |
| Trackpad orbit | scaled |
| Mouse drag pan | scaled |
| Trackpad pan | scaled |
| Wheel zoom | scaled |
| Trackpad zoom | scaled |
| Pinch zoom | scaled |

Preset modules may use private descriptor-construction helpers or extend the typed binding wrappers so scale/gate metadata can apply consistently to held bindings, trackpad bindings, wheel bindings, and pinch bindings. The adapter should execute validated descriptors; preset config code owns the meaning of `slow_scale`.

The implementation should choose one descriptor model before coding. Either add a shared scale policy that each relevant descriptor can carry, or add per-source typed descriptors with base and slow scale fields for trackpad, wheel, and pinch inputs. Tests should cover every source listed in the slow coverage table.

This is an intentional Lagrange extension to Blender-like controls. It avoids using Blender-reserved navigation chords:

| Chord | Reserved for |
| --- | --- |
| `Shift` | Pan |
| `Ctrl` | Zoom |
| `Alt` | Orbit center, axis align, orbit snap |
| `Ctrl + Shift` | Dolly view |
| Backquote | View/navigation pie menu |

Users who dislike CapsLock slow mode can override or disable it through `OrbitCamBlenderLikePreset`.

## OrbitCam Preset Bundle

Add a small public bundle for ergonomic camera spawning:

```rust
#[derive(Bundle)]
pub struct OrbitCamPresetBundle {
    pub orbit_cam: OrbitCam,
    pub input_mode: OrbitCamInputMode,
}
```

Suggested constructors:

```rust
impl OrbitCamPresetBundle {
    pub fn new(preset: OrbitCamPreset) -> Self {
        Self {
            orbit_cam: OrbitCam::default(),
            input_mode: OrbitCamInputMode::Preset(preset),
        }
    }

    pub fn bindings(bindings: OrbitCamBindings) -> Self {
        Self {
            orbit_cam: OrbitCam::default(),
            input_mode: OrbitCamInputMode::Bindings(bindings),
        }
    }

    pub fn manual() -> Self {
        Self {
            orbit_cam: OrbitCam::default(),
            input_mode: OrbitCamInputMode::Manual,
        }
    }

    pub fn with_orbit_cam_preset(orbit_cam: OrbitCam, preset: OrbitCamPreset) -> Self {
        Self {
            orbit_cam,
            input_mode: OrbitCamInputMode::Preset(preset),
        }
    }

    pub fn with_orbit_cam_bindings(orbit_cam: OrbitCam, bindings: OrbitCamBindings) -> Self {
        Self {
            orbit_cam,
            input_mode: OrbitCamInputMode::Bindings(bindings),
        }
    }

    pub fn with_manual_orbit_cam(orbit_cam: OrbitCam) -> Self {
        Self {
            orbit_cam,
            input_mode: OrbitCamInputMode::Manual,
        }
    }

    pub fn with_orbit_cam(orbit_cam: OrbitCam, mode: OrbitCamInputMode) -> Self {
        Self {
            orbit_cam,
            input_mode: mode,
        }
    }
}
```

Suggested `OrbitCam` helpers:

```rust
impl OrbitCam {
    pub fn simple_mouse() -> OrbitCamPresetBundle {
        OrbitCamPresetBundle::new(OrbitCamPreset::SimpleMouse)
    }

    pub fn blender_like() -> OrbitCamPresetBundle {
        OrbitCamPresetBundle::new(OrbitCamPreset::BlenderLike)
    }

    pub fn gamepad() -> OrbitCamPresetBundle {
        OrbitCamPresetBundle::new(OrbitCamPreset::Gamepad)
    }

    pub fn keyboard() -> OrbitCamPresetBundle {
        OrbitCamPresetBundle::new(OrbitCamPreset::Keyboard)
    }

    pub fn simple_mouse_keyboard() -> OrbitCamPresetBundle {
        OrbitCamPresetBundle::new(OrbitCamPreset::SimpleMouseKeyboard)
    }

    pub fn blender_like_keyboard() -> OrbitCamPresetBundle {
        OrbitCamPresetBundle::new(OrbitCamPreset::BlenderLikeKeyboard)
    }

    pub fn with_bindings(bindings: OrbitCamBindings) -> OrbitCamPresetBundle {
        OrbitCamPresetBundle::bindings(bindings)
    }

    pub fn manual() -> OrbitCamPresetBundle {
        OrbitCamPresetBundle::manual()
    }
}
```

The constructor names should be snake_case. `simple_mouse` is clearer than `simple` because the enum preset is `SimpleMouse`.

When users need to configure `OrbitCam` fields and still avoid writing `OrbitCamInputMode`, they can use the bundle constructor directly:

```rust
let orbit_cam = OrbitCam {
    target_focus: Vec3::Y,
    target_radius: 8.0,
    ..default()
};

commands.spawn((
    Transform::from_xyz(0.0, 1.5, 5.0),
    OrbitCamPresetBundle::with_orbit_cam_preset(orbit_cam, OrbitCamPreset::BlenderLike),
));
```

`with_orbit_cam(orbit_cam, mode)` is the low-level escape hatch for code that intentionally wants to pass `OrbitCamInputMode` directly. Examples should prefer the mode-specific constructors above.

`OrbitCam::*` helpers are bundle factories. They return `OrbitCamPresetBundle`, not a bare `OrbitCam` component.

## Cleanup

Remove these existing public API concepts:

| Existing item | Action | Reason |
| --- | --- | --- |
| `OrbitCamPresetLayers` | Remove | Layer composition is another model users have to learn |
| `OrbitCamPresetLayer` | Remove | Only exists to support public layer composition |
| `PresetLayerSet` | Remove | Only exists to store layer metadata |
| `OrbitCamBindingsProfile` | Remove | Duplicates concepts already covered by `OrbitCamInputMode`; only useful for labels |

After this cleanup:

- `OrbitCamInputMode::Preset(preset)` displays as preset input.
- `OrbitCamInputMode::Bindings(bindings)` displays as custom bindings.
- `OrbitCamInputMode::Manual` displays as manual input.
- Tuned preset bindings are treated as custom bindings once built.

This keeps the public mental model small: preset, bindings, manual.

The cleanup must remove profile/layer plumbing completely:

- Remove `profile` from `OrbitCamBindings`.
- Remove `profile` from `OrbitCamBindingsDescriptor`.
- Remove `OrbitCamBindingsBuilder::profile`.
- Remove `OrbitCamBindings::profile()`.
- Remove profile copy-through in validation.
- Remove `OrbitCamBindingsProfile` exports from `input/bindings/mod.rs`, `input/mod.rs`, and `lib.rs`.
- Remove profile and layer assertions from tests.
- Update `describe_orbit_cam_controls` so labels derive only from `OrbitCamInputMode`.
- Update reflected input-mode descriptors and tests so `OrbitCamInputModeDraft::Bindings(OrbitCamBindingsDescriptor)` no longer exposes profile or layer metadata.
- Reflected mode drafts should remain the same three concepts: preset, bindings, manual.

## Public Exports

Export these from the crate root for ergonomic examples:

| Type | Export path |
| --- | --- |
| `OrbitCamPresetBundle` | `bevy_lagrange::OrbitCamPresetBundle` |
| `OrbitCamSimpleMousePreset` | `bevy_lagrange::OrbitCamSimpleMousePreset` |
| `OrbitCamBlenderLikePreset` | `bevy_lagrange::OrbitCamBlenderLikePreset` |
| `OrbitCamKeyboardPreset` | `bevy_lagrange::OrbitCamKeyboardPreset` |
| `OrbitCamSimpleMouseKeyboardPreset` | `bevy_lagrange::OrbitCamSimpleMouseKeyboardPreset` |
| `OrbitCamBlenderLikeKeyboardPreset` | `bevy_lagrange::OrbitCamBlenderLikeKeyboardPreset` |
| `OrbitCamGamepadPreset` | `bevy_lagrange::OrbitCamGamepadPreset` |

## Proposed Module Structure

Keep the public preset API under `input::bindings`, but split preset implementation by preset family:

| Path | Responsibility |
| --- | --- |
| `input/bindings/preset/mod.rs` | Re-export public preset API and shared private helpers |
| `input/bindings/preset/config.rs` | crate-owned or sealed preset build trait and shared validation helpers |
| `input/bindings/preset/enum_preset.rs` | `OrbitCamPreset` enum and `to_bindings` delegation |
| `input/bindings/preset/simple_mouse.rs` | `OrbitCamSimpleMousePreset` defaults and direct fluent tuning methods |
| `input/bindings/preset/blender_like.rs` | `OrbitCamBlenderLikePreset`, CapsLock slow-mode defaults, Blender-like binding construction |
| `input/bindings/preset/keyboard.rs` | `OrbitCamKeyboardPreset` defaults and direct fluent tuning methods |
| `input/bindings/preset/simple_mouse_keyboard.rs` | `OrbitCamSimpleMouseKeyboardPreset` composition |
| `input/bindings/preset/blender_like_keyboard.rs` | `OrbitCamBlenderLikeKeyboardPreset` composition |
| `input/bindings/preset/gamepad.rs` | `OrbitCamGamepadPreset` and direct fluent tuning methods |

Add spawn helpers near `OrbitCam`:

| Path | Responsibility |
| --- | --- |
| `orbit_cam/preset_bundle.rs` | `OrbitCamPresetBundle` and `OrbitCam::simple_mouse`, `OrbitCam::blender_like`, `OrbitCam::gamepad`, `OrbitCam::with_bindings`, `OrbitCam::manual` |
| `orbit_cam/mod.rs` | Core `OrbitCam` component and re-export of preset bundle |

This keeps preset construction out of `orbit_cam/mod.rs`, while still making the spawn helpers discoverable from `OrbitCam`.

Replace the existing flat `input/bindings/preset.rs` file with the `input/bindings/preset/` module directory in the same change that updates `input/bindings/mod.rs` exports.

## Teaching Sequence

Docs and examples should teach the API in this order:

1. Default preset camera

```rust
commands.spawn((
    Transform::from_xyz(0.0, 1.5, 5.0),
    OrbitCam::blender_like(),
));
```

2. Tuned preset bindings

```rust
let bindings = OrbitCamBlenderLikePreset::default()
    .slow_scale(0.25)
    .build()?;

commands.spawn((
    Transform::from_xyz(0.0, 1.5, 5.0),
    OrbitCam::with_bindings(bindings),
));
```

3. Fully custom bindings

```rust
let bindings = OrbitCamBindings::builder()
    // Custom app bindings.
    .build()?;

commands.spawn((
    Transform::from_xyz(0.0, 1.5, 5.0),
    OrbitCam::with_bindings(bindings),
));
```

Tuned presets and fully custom controls both become `OrbitCamInputMode::Bindings` at runtime and display as custom bindings. The difference is where the bindings come from: a preset config for tuned presets, or `OrbitCamBindings::builder()` for fully custom controls.

## Example Updates

Implementation should update examples so they teach the new tiers:

| Example | Update |
| --- | --- |
| `input_preset_simple.rs` | Use `OrbitCam::simple_mouse()` |
| `input_preset_blender_like.rs` | Use `OrbitCam::blender_like()` and document that CapsLock toggles slow orbit, pan, and zoom |
| `input_gamepad.rs` | Use `OrbitCam::gamepad()` |
| `input_keyboard.rs` | Use `OrbitCam::keyboard()` and fix stale text that says custom bindings |
| `input_manual.rs` | Use `OrbitCam::manual()` |
| `input_custom.rs` | Keep showing `OrbitCamBindings::builder()` and `OrbitCam::with_bindings(bindings)` |

Add or extend one example section for tuned presets. Include both a slow-scale tuning snippet and a disable-default snippet:

```rust
let bindings = OrbitCamBlenderLikePreset::default()
    .slow_scale(0.25)
    .build()?;

commands.spawn((
    Transform::from_xyz(0.0, 1.5, 5.0),
    OrbitCam::with_bindings(bindings),
));
```

```rust
let bindings = OrbitCamBlenderLikePreset::default()
    .slow_toggle_key(None)
    .build()?;
```
