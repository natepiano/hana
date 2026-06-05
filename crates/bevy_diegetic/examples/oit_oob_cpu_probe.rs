//! CPU-side probe for the OIT heads-buffer / view-size invariant during resize.
//!
//! `prepare_oit_buffers` sizes `oit_heads` from `physical_target_size`. The OIT
//! shaders index it by `floor(x) + floor(y) * viewport.z`, so the buffer must
//! hold at least `width * height` u32s for the rendered view, every frame. This
//! probe runs in the render world after `prepare_oit_buffers` and logs whenever
//! the buffer is recreated or fails to cover the view — no shaders, no risk.
//!
//! Run, then resize the window (drag, or SPACE to toggle maximized). Watch for
//! `UNDERSIZED` lines = a frame where the buffer is smaller than the view.

use bevy::core_pipeline::oit::OitBuffers;
use bevy::core_pipeline::oit::OrderIndependentTransparencySettings;
use bevy::prelude::*;
use bevy::render::RenderApp;
use bevy::render::Render;
use bevy::render::RenderSystems;
use bevy::render::camera::ExtractedCamera;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, ProbePlugin))
        .add_systems(Startup, setup)
        .add_systems(Update, toggle_maximized)
        .run();
}

struct ProbePlugin;

impl Plugin for ProbePlugin {
    fn build(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.add_systems(
            Render,
            probe_oit_buffer.in_set(RenderSystems::PrepareResources).after(
                bevy::core_pipeline::oit::prepare_oit_buffers,
            ),
        );
    }
}

fn probe_oit_buffer(
    buffers: Res<OitBuffers>,
    cameras: Query<&ExtractedCamera, With<OrderIndependentTransparencySettings>>,
    mut last_capacity: Local<usize>,
) {
    let capacity = buffers.heads.capacity();
    if capacity != *last_capacity {
        warn!("oit_heads recreated: {} -> {} u32", *last_capacity, capacity);
        *last_capacity = capacity;
    }
    for camera in &cameras {
        let Some(size) = camera.physical_target_size else {
            continue;
        };
        let needed = (size.x * size.y) as usize;
        if capacity < needed {
            warn!(
                "UNDERSIZED: view {}x{} needs {needed} u32 but oit_heads holds {capacity}",
                size.x, size.y
            );
        }
    }
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
    commands.spawn((
        Mesh3d(meshes.add(Rectangle::new(1000.0, 1000.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.3, 0.6, 1.0, 0.5),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        })),
    ));
}

fn toggle_maximized(
    keys: Res<ButtonInput<KeyCode>>,
    mut windows: Query<&mut Window>,
    mut maximized: Local<bool>,
) {
    if !keys.just_pressed(KeyCode::Space) {
        return;
    }
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    *maximized = !*maximized;
    window.set_maximized(*maximized);
}
