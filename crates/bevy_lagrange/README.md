[![Crates.io](https://img.shields.io/crates/v/bevy_lagrange)](https://crates.io/crates/bevy_lagrange)
[![docs.rs](https://docs.rs/bevy_lagrange/badge.svg)](https://docs.rs/bevy_lagrange)
[![Bevy tracking](https://img.shields.io/badge/Bevy%20tracking-released%20version-lightblue)](https://github.com/bevyengine/bevy/blob/main/docs/plugins_guidelines.md#main-branch-tracking)

# `bevy_lagrange`

> **Work in progress.** This crate is in active development and not subject to
> semver stability guarantees. APIs will change without notice between commits.
> Do not depend on this in production code yet.

A camera controller for [Bevy](https://bevyengine.org) that combines smooth orbit controls with event-driven camera operations — zoom-to-fit, queued animations, and a debug overlay for fit targets.

![A screen recording showing camera movement](https://user-images.githubusercontent.com/7709415/230715348-eb19d9a8-4826-4a73-a039-02cacdcb3dc9.gif "Demo of bevy_lagrange")

## Features

- Smooth orbit, pan, and zoom with configurable limits
- Zoom-to-fit, look-at, and queued camera animations with easing
- Event-driven control with full lifecycle events for sequencing
- Orthographic and perspective projection, multi-viewport, render-to-texture
- Touch, trackpad, and `bevy_egui` support
- Debug overlay for fit targets (optional `fit_overlay` feature)

## Quick Start

Add the plugin and spawn a camera:

```rust ignore
use bevy::prelude::*;
use bevy_lagrange::LagrangePlugin;
use bevy_lagrange::OrbitCam;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(LagrangePlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn((
        Transform::from_translation(Vec3::new(0.0, 1.5, 5.0)),
        OrbitCam::default(),
    ));
}
```

`OrbitCam` automatically requires `Camera3d`. Out of the box you get orbit, pan, and zoom with smoothing. For perspective cameras, the default near clip plane scales with orbit radius so close-up zooming does not clip away the target.

## Controls

Default mouse controls:

| Input | Action |
|-------|--------|
| Left Mouse | Orbit |
| Right Mouse | Pan |
| Scroll Wheel | Zoom |

Default touch controls:

| Input | Action |
|-------|--------|
| One finger | Orbit |
| Two fingers | Pan |
| Pinch | Zoom |

All controls are configurable via `OrbitCam` fields — buttons, modifiers, sensitivity, smoothness, and limits.

## Event-Driven Camera Control

Enable the `fit_overlay` feature:

```toml
bevy_lagrange = { version = "...", features = ["fit_overlay"] }
```

### Zoom-to-fit

Frame a target entity in the camera view:

```rust ignore
commands.trigger(
    ZoomToFit::new(camera, target)
        .margin(0.15)
        .duration(Duration::from_millis(800))
        .easing(EaseFunction::CubicOut),
);
```

### Look at

Rotate the camera in place to face a target:

```rust ignore
commands.trigger(
    LookAt::new(camera, target)
        .duration(Duration::from_millis(600)),
);
```

### Animate to a specific orientation

Animate to a chosen yaw/pitch while framing the target:

```rust ignore
commands.trigger(
    AnimateToFit::new(camera, target)
        .yaw(PI / 4.0)
        .pitch(PI / 6.0)
        .duration(Duration::from_millis(1200)),
);
```

### Queued animations

Chain multiple movements into a sequence:

```rust ignore
commands.trigger(PlayAnimation::new(camera, [
    CameraMove::ToOrbit {
        focus: Vec3::ZERO,
        yaw: 0.0,
        pitch: 0.5,
        radius: 5.0,
        duration: Duration::from_millis(800),
        easing: EaseFunction::CubicOut,
    },
]));
```

All operations support instant (`Duration::ZERO`) and animated paths with full lifecycle events for sequencing.

## Cargo Features

| Feature | Default | Description |
|---------|---------|-------------|
| `fit_overlay` | no | Zoom-to-fit, camera animations, event-driven control, and debug overlay |
| `bevy_egui` | no | Prevents camera movement when interacting with egui windows |

## Version Compatibility

| `bevy_lagrange` | Bevy |
|---------------|------|
| 0.0.3         | 0.18 |

## Credits

- [Plonq](https://github.com/Plonq) — `bevy_lagrange` builds on [bevy_panorbit_camera](https://github.com/Plonq/bevy_panorbit_camera), with permission

## License

All code in this repository is dual-licensed under either:

- MIT License ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)

at your option.
