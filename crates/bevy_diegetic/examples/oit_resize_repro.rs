//! Minimal repro: OIT heads-buffer out-of-bounds access during window resize.
//!
//! An OIT camera renders a transparent quad covering every pixel while the
//! window size changes every frame. `oit_draw.wgsl` and `oit_resolve.wgsl`
//! index `oit_heads[x + y * view.viewport.z]` without a bounds check, and
//! bevy compiles shaders with `ShaderRuntimeChecks::unchecked()`, so any
//! frame where the rasterized size and the extracted snapshot disagree
//! writes outside the buffer. On Apple Silicon (unified memory) this has
//! escalated to a GPU fault / macOS kernel panic.
//!
//! The window resizes itself; drag-resizing by hand also triggers it.

use bevy::core_pipeline::oit::OrderIndependentTransparencySettings;
use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, setup)
        .add_systems(Update, oscillate_window_size)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        OrderIndependentTransparencySettings::default(),
        Msaa::Off,
        Transform::from_xyz(0.0, 0.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // Transparent quad far larger than the frustum: every pixel writes an OIT fragment.
    commands.spawn((
        Mesh3d(meshes.add(Rectangle::new(100.0, 100.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 1.0, 1.0, 0.5),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        })),
    ));
}

fn oscillate_window_size(mut windows: Query<&mut Window>, mut grow: Local<bool>) {
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    let (w, h) = (window.resolution.width(), window.resolution.height());
    if w <= 500.0 {
        *grow = true;
    } else if w >= 1500.0 {
        *grow = false;
    }
    let step = if *grow { 7.0 } else { -7.0 };
    window.resolution.set(w + step, h + step);
}
