//! Real reproduction of bevy 0.19's OIT out-of-bounds crash, driven by a
//! genuine cross-DPI window restore (no forcing, no shader patching).
//!
//! Bevy's Order-Independent Transparency draws into a per-pixel buffer
//! (`oit_heads`) sized to the window. `oit_draw.wgsl` writes
//! `oit_heads[floor(x) + floor(y) * view.viewport.z]` with NO bounds check, and
//! shaders compile with `ShaderRuntimeChecks::unchecked()`, so the GPU does not
//! clamp the index. During a cross-DPI restore — the window lands on a monitor
//! whose scale factor differs from where it opened — winit fires
//! `WindowScaleFactorChanged`, the real Metal drawable resizes, and bevy
//! reallocates `oit_heads`. For the frame where the rasterized area exceeds the
//! just-reallocated buffer, the unguarded draw shader writes past the end. On
//! Apple Silicon that out-of-bounds write can reach protected memory and
//! kernel-panic the machine.
//!
//! `bevy_window_manager` performs exactly that cross-DPI restore at launch,
//! which is what crashed the `typography` example.
//!
//! This matches `typography`'s render configuration as closely as a standalone
//! example can without the panel system:
//!   - `WinitSettings::continuous()` + `PresentMode::AutoNoVsync`, so the app
//!     actively draws through bwm's hidden -> resize -> visible restore instead
//!     of throttling a hidden window (bevy's default `game()` settings would).
//!     The crash needs a transparent draw to execute on the mismatched frame.
//!   - `fragments_per_pixel_average: 8.0`, the same OIT pool size `StableTransparency` uses.
//!
//! It also logs the OIT heads buffer capacity vs the view size every time they
//! change, so a run that does NOT crash still shows whether the restore
//! reproduced typography's `921600 -> 4.7M -> 921600` oscillation (the
//! condition) or not.
//!
//! Reproduce:
//!   1. cargo run --example oit_oob_bwm
//!   2. Drag the window onto the monitor where `typography` crashed.
//!   3. Quit. `bevy_window_manager` saves the window's monitor, size, and scale.
//!   4. cargo run --example oit_oob_bwm again. It restores across the DPI
//!      boundary on launch — that is when the panic hits. Watch the terminal:
//!      the `oit_heads cap=... view=...` lines show the buffer oscillating.
//!
//! State file: `~/Library/Application Support/oit_oob_bwm/windows.ron`
//! (isolated from `typography`'s state; delete it to reset to a fresh window.)
//!
//! The two-line fix that removes the crash:
//!   - `oit_draw.wgsl`:    `if screen_index >= arrayLength(&oit_heads) { return; }`
//!   - `oit_resolve.wgsl`: same guard before `heads[screen_index]`.

use bevy::core_pipeline::oit::OitBuffers;
use bevy::core_pipeline::oit::OrderIndependentTransparencySettings;
use bevy::core_pipeline::oit::prepare_oit_buffers;
use bevy::prelude::*;
use bevy::render::Render;
use bevy::render::RenderApp;
use bevy::render::RenderSystems;
use bevy::render::camera::ExtractedCamera;
use bevy::window::PresentMode;
use bevy::winit::WinitSettings;
use bevy_window_manager::WindowManagerPlugin;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            // Render as fast as the GPU allows; do not pin to vsync steps.
            present_mode: PresentMode::AutoNoVsync,
            ..default()
        }),
        ..default()
    }))
    // Render every frame regardless of focus/visibility, so a transparent draw
    // executes during bwm's hidden -> resize -> visible restore transition.
    .insert_resource(WinitSettings::continuous())
    // Cross-DPI restore on launch — the real trigger.
    .add_plugins(WindowManagerPlugin)
    .add_systems(Startup, setup);

    // Safe size probe in the render world: log oit_heads capacity vs view size
    // whenever they change. Shows whether the restore reproduces the buffer
    // oscillation even on a run that does not crash.
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app.add_systems(
            Render,
            log_oit_sizes
                .in_set(RenderSystems::PrepareResources)
                .after(prepare_oit_buffers),
        );
    }

    app.run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        // OIT requires MSAA off.
        Msaa::Off,
        // Raw bevy OIT — unguarded oit_heads indexing, with the same pool size
        // typography uses.
        OrderIndependentTransparencySettings {
            fragments_per_pixel_average: 8.0,
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Several overlapping transparent quads, each larger than the view, so the
    // OIT draw pass builds a real per-pixel linked list across the whole screen
    // — that is what writes into `oit_heads` for every pixel.
    let quad = meshes.add(Rectangle::new(12.0, 12.0));
    let layers = [
        (Color::srgba(0.9, 0.2, 0.2, 0.4), 0.0),
        (Color::srgba(0.2, 0.9, 0.2, 0.4), 0.4),
        (Color::srgba(0.2, 0.4, 0.9, 0.4), 0.8),
    ];
    for (color, z) in layers {
        commands.spawn((
            Mesh3d(quad.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_xyz(0.0, 0.0, z),
        ));
    }
}

/// Render-world probe: logs `oit_heads` capacity against the OIT view's pixel
/// size each time either changes. A `HEADS < VIEW` line marks a frame where the
/// unguarded draw index can exceed the buffer — the out-of-bounds condition.
fn log_oit_sizes(
    buffers: Res<OitBuffers>,
    cameras: Query<&ExtractedCamera, With<OrderIndependentTransparencySettings>>,
    mut last: Local<Option<(usize, UVec2)>>,
) {
    let Some(size) = cameras.iter().find_map(|c| c.physical_target_size) else {
        return;
    };
    let cap = buffers.heads.capacity();
    if *last == Some((cap, size)) {
        return;
    }
    *last = Some((cap, size));
    let view_area = (size.x * size.y) as usize;
    let flag = if cap < view_area {
        "  <-- HEADS SMALLER THAN VIEW (out-of-bounds condition)"
    } else {
        ""
    };
    info!(
        "oit_heads cap={cap} view={}x{} (area={view_area}){flag}",
        size.x, size.y
    );
}
