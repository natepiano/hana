# `bevy_lagrange` Future Directions

The input refactor is complete. The remaining ideas below are future product and
architecture work, not prerequisites for the current camera-input model.

## Gamepad And Touch Ownership

The current input model supports gamepad and touch source attribution, but stable
per-device gamepad ownership and per-touch ownership are future policy work.

Source latches currently stabilize mouse-like and keyboard ownership only. Add
gamepad owner latches after a selected-gamepad API exists. Add touch owner latches
only with a concrete touch-owner policy that defines what happens when fingers
begin, end, or transfer between cameras.

## Roll

Roll is a natural future camera interaction because platform gesture systems can
produce rotation gestures. Bevy already exposes `RotationGesture`, and the current
touch tracker computes two-finger rotation even though the controller does not use
it.

Roll should be added as camera behavior work, not as a binding-only change. Expect
to touch the interaction kind enum, input snapshot, interaction state, tracker,
presets, and manual writer.

Candidate additions:

- `OrbitCamRollAction` semantic action.
- `OrbitCamInteractionKind::Roll`.
- `OrbitCamInput::roll`.
- `roll` and `target_roll` camera state.
- `roll_lower_limit` and `roll_upper_limit`.
- `roll_sensitivity` and `roll_smoothness`.

If that update becomes noisy, consider a generic interaction tracker keyed by
interaction kind and associated action types. Keep explicit orbit, pan, and zoom
fields until roll or another new interaction kind proves that the explicit model is
too repetitive.

`OrbitCamInteractionKind` should be non-exhaustive so `Roll` can be added later
without forcing downstream exhaustive matches to break.

## Angle State

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
update inconsistently.
