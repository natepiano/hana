# fairy_dust

Workspace example helper — quiet logs and opt-in dev capabilities for bevy_hana examples.

Use `sprinkle_example()` to construct a `SprinkleBuilder` before Fairy Dust installs
its baseline plugins. The first builder operation installs `DefaultPlugins` with a
quiet log filter, then the chain opts into specific dev conveniences:

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

The builder has independent baseline and orbit-camera typestates. The baseline
starts as `AssetRootPending`, where `SprinkleBuilder::with_asset_root` can configure
`AssetPlugin` before `DefaultPlugins` is installed. That method and every ordinary
builder operation return `BaselineInstalled`, where `with_asset_root` is no longer
available.

Code that needs direct access to the Bevy app before adding a Fairy Dust capability
must complete the first-step transition explicitly:

```rust,ignore
let mut builder = fairy_dust::sprinkle_example().with_default_asset_root();
let app = builder.app_mut();
```

The pending builder intentionally has no `app_mut` method, which keeps asset-root
selection ahead of any operation that can initialize Bevy's asset plugin.

The orbit-camera marker starts as `NoOrbitCam` and transitions to `WithOrbitCam`.
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
