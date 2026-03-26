[![Crates.io](https://img.shields.io/crates/v/bevy_lagrange)](https://crates.io/crates/bevy_lagrange)
[![docs.rs](https://docs.rs/bevy_lagrange/badge.svg)](https://docs.rs/bevy_lagrange)
[![Bevy tracking](https://img.shields.io/badge/Bevy%20tracking-released%20version-lightblue)](https://github.com/bevyengine/bevy/blob/main/docs/plugins_guidelines.md#main-branch-tracking)

<div style="text-align: center">
  <h1>Bevy Pan/Orbit Camera</h1>
</div>

![A screen recording showing camera movement](https://user-images.githubusercontent.com/7709415/230715348-eb19d9a8-4826-4a73-a039-02cacdcb3dc9.gif "Demo of bevy_lagrange")

## Summary

Bevy Pan/Orbit Camera provides orbit camera controls for Bevy Engine, designed with simplicity and flexibility in mind.
Use it to quickly prototype, experiment, for model viewers, and more!

## Features:

- Smoothed orbiting, panning, and zooming
- Works with orthographic camera projection in addition to perspective
- Customisable controls, sensitivity, and more
- Touch support
- Works with multiple viewports and/or windows
- Easy to control manually, e.g. for keyboard control or animation
- Can control cameras that render to a texture
- Zoom-to-fit, camera animations, and debug visualization (optional `extras_debug` feature)

## Controls

Default mouse controls:

- Left Mouse - Orbit
- Right Mouse - Pan
- Scroll Wheel - Zoom

Default touch controls:

- One finger - Orbit
- Two fingers - Pan
- Pinch - Zoom

## Quick Start

Add the plugin:

```rust ignore
.add_plugins(PanOrbitCameraPlugin)
```

Add `PanOrbitCamera` (this will automatically add a `Camera3d` but you can add it manually if necessary):

```rust ignore
commands.spawn((
    Transform::from_translation(Vec3::new(0.0, 1.5, 5.0)),
    PanOrbitCamera::default(),
));
```

This will set up a camera with good defaults.

Check out the [advanced example](https://github.com/natepiano/bevy_lagrange/tree/master/examples/advanced.rs) to see
all the possible configuration options.

## Cargo Features

- `bevy_egui` (optional): Makes `PanOrbitCamera` ignore any input that `egui` uses, thus preventing moving the camera
  when interacting with egui windows
- `extras_debug` (optional): Zoom-to-fit, queued camera animations, event-driven camera control, and debug visualization of fit targets with gizmos and screen-space labels

## Extras

Enable the `extras_debug` feature for zoom-to-fit, queued camera animations, event-driven camera control, and debug visualization:

```toml
bevy_lagrange = { version = "0.0.1", features = ["extras_debug"] }
```

Trigger a zoom-to-fit:

```rust ignore
commands.entity(camera).trigger(ZoomToFit {
    target,
    margin: 0.15,
    ..default()
});
```

Queue camera animations:

```rust ignore
commands.entity(camera).trigger(PlayAnimation {
    moves: vec![CameraMove::ToOrbit { .. }],
    ..default()
});
```

Look at a target:

```rust ignore
commands.entity(camera).trigger(LookAt { target, duration, easing });
```

See the `extras_*` examples for more.

## Version Compatibility

| bevy | `bevy_lagrange` |
|------|-----------------|
| 0.18 | 0.0.1        |

## Alternatives

If you're looking for a lighter-weight orbit camera, check out [bevy_panorbit_camera](https://github.com/Plonq/bevy_panorbit_camera) by [Plonq](https://github.com/Plonq), which this crate is based on.

## Credits

- [Plonq](https://github.com/Plonq): For graciously allowing this project to build on [bevy_panorbit_camera](https://github.com/Plonq/bevy_panorbit_camera)
- [Bevy Cheat Book](https://bevy-cheatbook.github.io): For providing an example that I started from
- [babylon.js](https://www.babylonjs.com): I referenced their arc rotate camera for some of this
- [bevy_pancam](https://github.com/johanhelsing/bevy_pancam): For the egui feature idea

## License

All code in this repository is dual-licensed under either:

* MIT License ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))
* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)
  or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))

at your option.
This means you can select the license you prefer!
This dual-licensing approach is the de-facto standard in the Rust ecosystem and there
are [very good reasons](https://github.com/bevyengine/bevy/issues/2373) to include both.
