//! Minimal repro: OIT heads-buffer out-of-bounds access during window resize.
//!
//! `oit_draw.wgsl` and `oit_resolve.wgsl` index
//! `oit_heads[x + y * view.viewport.z]` without a bounds check, and bevy
//! compiles shaders with `ShaderRuntimeChecks::unchecked()`, so any frame
//! where the rasterized size and the extracted snapshot disagree accesses
//! memory outside the buffer. On Apple Silicon (unified memory) this has
//! escalated to a GPU fault / macOS kernel panic.
//!
//! The crash was observed during a window restore at launch: the window is
//! created small and hidden, then sized up, shown, and maximized while the
//! first pipelines are still compiling, so the window-manager animation runs
//! many compositor frames between slow app frames. This example replays that
//! sequence (with artificial frame stalls standing in for heavier pipeline
//! compilation), then keeps toggling maximized every 2 seconds.
//!
//! Run with `--release`, relaunch several times if needed. Drag-resizing the
//! window by hand exercises the same path.

use std::thread;
use std::time::Duration;

use bevy::camera::Hdr;
use bevy::core_pipeline::oit::OrderIndependentTransparencySettings;
use bevy::prelude::*;
use bevy::window::WindowResolution;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                resolution: WindowResolution::new(640, 360),
                visible: false,
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .add_systems(Update, launch_restore)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        Hdr,
        OrderIndependentTransparencySettings {
            fragments_per_pixel_average: 8.0,
            ..default()
        },
        Msaa::Off,
        Transform::from_xyz(0.0, 0.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // Overlay camera sharing the window, as a screen-space UI camera would.
    commands.spawn((
        Camera2d,
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        Msaa::Off,
    ));
    // Stack of transparent quads far larger than the frustum: every pixel
    // writes several OIT fragments.
    let mesh = meshes.add(Rectangle::new(100.0, 100.0));
    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.3),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    for layer in 0_u8..12 {
        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(0.0, 0.0, -0.1 * f32::from(layer)),
        ));
    }
}

/// Replays the launch-restore sequence, then keeps toggling maximized.
fn launch_restore(
    time: Res<Time>,
    mut windows: Query<&mut Window>,
    mut frame: Local<u32>,
    mut elapsed: Local<f32>,
    mut maximized: Local<bool>,
) {
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    *frame += 1;
    match *frame {
        // Size up and show while pipelines are still compiling, as a window
        // manager restoring the saved frame at launch does.
        2 => {
            window.resolution = WindowResolution::new(2560, 1440);
            window.visible = true;
        },
        // Window-manager-animated zoom during the stall window.
        12 | 32 | 52 => {
            *maximized = !*maximized;
            window.set_maximized(*maximized);
        },
        _ => {},
    }
    // Stall the early frames so the zoom animation runs many compositor
    // frames between app frames, standing in for pipeline compilation.
    if *frame < 70 {
        thread::sleep(Duration::from_millis(100));
        return;
    }
    // Long-run stress: keep toggling maximized every 2 seconds.
    *elapsed += time.delta_secs();
    if *elapsed < 2.0 {
        return;
    }
    *elapsed = 0.0;
    *maximized = !*maximized;
    window.set_maximized(*maximized);
}
