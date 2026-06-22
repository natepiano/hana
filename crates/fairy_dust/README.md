# fairy_dust

Workspace example helper — quiet logs and opt-in dev capabilities for bevy_hana examples.

Use `sprinkle_example()` to construct a `SprinkleBuilder` preloaded with `DefaultPlugins`
configured for a quiet log filter, then chain capability methods to opt into specific
dev conveniences:

```rust,ignore
fairy_dust::sprinkle_example()
    .with_orbit_cam_preset(
        |orbit_cam| { orbit_cam.radius = Some(5.0); },
        OrbitCamPreset::blender_like(),
    )
    .with_stable_transparency()       // only callable after with_orbit_cam_*
    .with_save_window_position()
    .with_brp_extras()
    .with_camera_control_panel()
    .add_systems(Startup, setup)
    .run();
```

When `.with_camera_home()` is installed, the home fit can be positioned in the
viewport with `.anchor(Anchor::TopLeft)` and adjusted with
`.offset_px(Vec2::new(x, y))`. The default remains centered with no pixel
offset.

## Typestate

The builder is parameterized by a state marker (`NoOrbitCam` / `WithOrbitCam`).
Methods that act on the spawned `OrbitCam` entity (such as
`SprinkleBuilder::with_stable_transparency`) are only defined on
`SprinkleBuilder<WithOrbitCam>`, so calling them before
`SprinkleBuilder::with_orbit_cam_preset` is a compile error.

## Plugin deduplication

Capabilities that share infrastructure (for example a `DiegeticUiPlugin` for HUD panels)
ensure the required plugin is registered exactly once via `ensure_plugin`, regardless of
how many capabilities pull it in.

## License

Dual-licensed under either of [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0)
or [MIT license](https://opensource.org/licenses/MIT) at your option.
