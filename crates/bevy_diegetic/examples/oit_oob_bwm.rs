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
//! The out-of-bounds is invisible to CPU instrumentation: bevy sizes `oit_heads`
//! from its extracted view snapshot, so on the CPU `heads.capacity()` always
//! equals the view area. The fault opens only when a frame is slow enough that
//! the OS resize lands MID-frame — the GPU rasterizes the new, larger drawable
//! while bevy already extracted and sized `oit_heads` for the old one.
//!
//! This matches `typography`'s render configuration as closely as a standalone
//! example can without the panel system:
//!   - `WinitSettings::continuous()` + `PresentMode::AutoNoVsync`, so the app actively draws
//!     through bwm's hidden -> resize -> visible restore instead of throttling a hidden window
//!     (bevy's default `game()` settings would). The crash needs a transparent draw to execute on
//!     the mismatched frame.
//!   - `fragments_per_pixel_average: 8.0`, the same OIT pool size `StableTransparency` uses.
//!   - A dense grid of `GRID * GRID` distinct-material transparent quads. In a debug build, every
//!     transparent draw call pays objc2 `msg_send` verification, so this volume makes frames slow
//!     enough that an OS resize can land mid-frame — recreating the GPU/CPU size disagreement that
//!     `typography`'s thousands of transparent glyph draws produced.
//!
//! It also logs the OIT heads buffer capacity vs the view size every time they
//! change, so a run that does NOT crash still shows whether the restore
//! reproduced typography's `921600 -> 4.7M -> 921600` oscillation (the
//! condition) or not.
//!
//! Reproduce:
//!   1. cargo run --example `oit_oob_bwm`
//!   2. Drag the window onto the monitor where `typography` crashed.
//!   3. Quit. `bevy_window_manager` saves the window's monitor, size, and scale.
//!   4. cargo run --example `oit_oob_bwm` again. It restores across the DPI boundary on launch —
//!      that is when the panic hits. Watch the terminal: the `oit_heads cap=... view=...` lines
//!      show the buffer oscillating.
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
use bevy::window::PrimaryWindow;
use bevy::window::WindowScaleFactorChanged;
use bevy::winit::WinitSettings;
use bevy_window_manager::WindowManagerPlugin;

/// Grid is `GRID * GRID` distinct-material transparent quads (1296 draw calls).
/// Each transparent draw call pays objc2 `msg_send` verification in debug, so
/// this volume slows frames enough that an OS resize can land mid-frame. Lower
/// it if the window is too sluggish to drag across monitors.
const GRID: i16 = 36;

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
    .add_systems(Startup, setup)
    .add_systems(Update, log_scale_changes);

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

    // A dense grid of distinct-material transparent quads. Distinct materials
    // defeat batching, so each is its own draw call; in debug each transparent
    // draw call pays objc2 `msg_send` verification, making frames slow enough
    // that an OS resize can land mid-frame. The quads overlap heavily, so the
    // OIT draw pass writes `oit_heads` across the whole view with deep per-pixel
    // linked lists — the same draw-call volume `typography`'s glyphs produce.
    let quad = meshes.add(Rectangle::new(0.4, 0.4));
    let span = 5.0_f32;
    let step = span / f32::from(GRID - 1);
    for gx in 0..GRID {
        for gy in 0..GRID {
            let fx = f32::from(gx).mul_add(step, -(span / 2.0));
            let fy = f32::from(gy).mul_add(step, -(span / 2.0));
            let z = f32::from((gx + gy) % 8) * 0.08;
            let r = f32::from(gx) / f32::from(GRID);
            let g = f32::from(gy) / f32::from(GRID);
            commands.spawn((
                Mesh3d(quad.clone()),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgba(r, g, r.mul_add(-0.5, 1.0), 0.4),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    ..default()
                })),
                Transform::from_xyz(fx, fy, z),
            ));
        }
    }
}

/// Main-world probe: logs whether a real cross-DPI scale change fires. The OIT
/// out-of-bounds needs a genuine backing-scale change (window crossing monitors
/// of different scale); a same-monitor resize keeps the scale factor fixed and
/// cannot trigger it.
fn log_scale_changes(
    mut events: MessageReader<WindowScaleFactorChanged>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut last_scale: Local<f32>,
) {
    for msg in events.read() {
        warn!("*** WindowScaleFactorChanged fired: {msg:?} ***");
    }
    if let Ok(window) = windows.single() {
        let scale = window.resolution.scale_factor();
        if (scale - *last_scale).abs() > f32::EPSILON {
            warn!("window scale_factor: {} -> {scale}", *last_scale);
            *last_scale = scale;
        }
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
