//! Deterministic, crash-free isolation of bevy's unguarded OIT `oit_heads`
//! indexing — drives the exact failing condition directly instead of hoping a
//! window resize happens to trip it.
//!
//! Mechanism under test: `oit_draw.wgsl` writes and `oit_resolve.wgsl` reads
//! `oit_heads[floor(x) + floor(y) * view.viewport.z]` with no bounds check, and
//! shaders compile with `ShaderRuntimeChecks::unchecked()`. The buffer is sized
//! from a per-frame snapshot, so the access is only safe while the bound buffer
//! is at least as large as the rasterized view. The organic crash happens when
//! that invariant breaks for a frame during a resize.
//!
//! This forces the invariant to break, deterministically and safely:
//!   - a render-world system replaces `OitBuffers.heads` with a buffer a quarter of the view's
//!     size, every frame, right after `prepare_oit_buffers` (so the bind groups are built from the
//!     small buffer) — this is the exact "buffer smaller than the view" condition a stale resize
//!     binding produces;
//!   - the draw shader is patched with a bounds guard so the unsafe write never happens (no
//!     corruption, no fault — safe to run with no reboot);
//!   - the resolve shader is patched to classify each pixel where the unguarded index WOULD have
//!     gone out of bounds:
//!     - MAGENTA = `arrayLength(&heads) < viewport area` (buffer smaller than the view — the
//!       resize/stale-binding case),
//!     - CYAN = `arrayLength(&heads) >= viewport area` (drawable larger than the buffer — the
//!       backing-size case).
//!
//! With `FORCE_SHRINK = true` the screen's lower ~3/4 turns MAGENTA: proof that
//! bevy's unguarded OIT index addresses past `oit_heads` whenever the buffer is
//! smaller than the view, and that the two-line `arrayLength` guard is what
//! prevents it. Set `FORCE_SHRINK = false` to run the same classifier against a
//! real window resize (drag the window) as a diagnosis.

use bevy::asset::AssetId;
use bevy::core_pipeline::oit::OitBuffers;
use bevy::core_pipeline::oit::OrderIndependentTransparencySettings;
use bevy::core_pipeline::oit::prepare_oit_buffers;
use bevy::prelude::*;
use bevy::render::Render;
use bevy::render::RenderApp;
use bevy::render::RenderSystems;
use bevy::render::camera::ExtractedCamera;
use bevy::render::render_resource::BufferUsages;
use bevy::render::render_resource::UninitBufferVec;
use bevy::render::renderer::RenderDevice;
use bevy::shader::Shader;
use bevy::shader::Source;

/// When true, force `oit_heads` smaller than the view every frame (the
/// deterministic isolation). When false, leave it alone and run the classifier
/// against a real window resize.
const FORCE_SHRINK: bool = true;

const RESOLVE_SHADER_PATH: &str = "embedded://bevy_core_pipeline/oit/resolve/oit_resolve.wgsl";

const DRAW_ANCHOR: &str =
    "let screen_index = u32(floor(position.x) + floor(position.y) * view.viewport.z);";
const DRAW_GUARD: &str = "\n    if screen_index >= arrayLength(&oit_heads) { return; } // isolate: prevent unsafe OOB write";

const RESOLVE_ANCHOR: &str =
    "let screen_index = u32(floor(in.position.x) + floor(in.position.y) * view.viewport.z);";
const RESOLVE_CLASSIFY: &str = "
    if screen_index >= arrayLength(&heads) { // isolate: bevy would index OOB here
        let view_area = u32(view.viewport.z * view.viewport.w);
        if arrayLength(&heads) < view_area { return vec4(1.0, 0.0, 1.0, 1.0); } // magenta: buffer < view
        return vec4(0.0, 1.0, 1.0, 1.0); // cyan: drawable > buffer
    }";

#[derive(Resource, Default)]
struct Patches {
    draw:           bool,
    resolve:        bool,
    activated:      bool,
    resolve_handle: Option<Handle<Shader>>,
}

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins)
        .init_resource::<Patches>()
        .add_systems(Startup, (setup, request_resolve_shader))
        .add_systems(Update, (patch_shaders, activate_oit));

    if FORCE_SHRINK && let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app.add_systems(
            Render,
            force_shrink_heads
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
    // OIT added later by activate_oit, once both shaders are patched.
    commands.spawn((
        Camera3d::default(),
        Msaa::Off,
        Transform::from_xyz(0.0, 0.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // One transparent quad activates OIT; the resolve pass then runs over every
    // pixel, so the classifier reports across the whole screen.
    commands.spawn((
        Mesh3d(meshes.add(Rectangle::new(2.0, 2.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.3, 0.6, 1.0, 0.5),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        })),
    ));
}

fn request_resolve_shader(asset_server: Res<AssetServer>, mut patches: ResMut<Patches>) {
    patches.resolve_handle = Some(asset_server.load(RESOLVE_SHADER_PATH));
}

fn patch_shaders(mut shaders: ResMut<Assets<Shader>>, mut patches: ResMut<Patches>) {
    if patches.draw && patches.resolve {
        return;
    }
    let mut draw_id: Option<AssetId<Shader>> = None;
    let mut resolve_id: Option<AssetId<Shader>> = None;
    for (id, shader) in shaders.iter() {
        if !patches.draw && shader.path.ends_with("oit/oit_draw.wgsl") {
            draw_id = Some(id);
        }
        if !patches.resolve && shader.path.ends_with("resolve/oit_resolve.wgsl") {
            resolve_id = Some(id);
        }
    }
    if let Some(id) = draw_id {
        patches.draw = insert_after(&mut shaders, id, DRAW_ANCHOR, DRAW_GUARD);
    }
    if let Some(id) = resolve_id {
        patches.resolve = insert_after(&mut shaders, id, RESOLVE_ANCHOR, RESOLVE_CLASSIFY);
    }
}

fn insert_after(
    shaders: &mut Assets<Shader>,
    id: AssetId<Shader>,
    anchor: &str,
    addition: &str,
) -> bool {
    let Some(shader) = shaders.get(id) else {
        return false;
    };
    let Source::Wgsl(source) = &shader.source else {
        return false;
    };
    let Some(at) = source.find(anchor).map(|i| i + anchor.len()) else {
        error!(
            "oit_oob_isolate: anchor not found in {} — bevy shader text changed",
            shader.path
        );
        return true;
    };
    let mut patched = source.to_string();
    patched.insert_str(at, addition);
    let path = shader.path.clone();
    warn!("oit_oob_isolate: patched {path}");
    let _ = shaders.insert(id, Shader::from_wgsl(patched, path));
    true
}

fn activate_oit(
    mut commands: Commands,
    mut patches: ResMut<Patches>,
    cameras: Query<
        Entity,
        (
            With<Camera3d>,
            Without<OrderIndependentTransparencySettings>,
        ),
    >,
) {
    if patches.activated || !(patches.draw && patches.resolve) {
        return;
    }
    for cam in &cameras {
        commands
            .entity(cam)
            .insert(OrderIndependentTransparencySettings::default());
    }
    patches.activated = true;
    warn!(
        "oit_oob_isolate: ARMED (force_shrink={FORCE_SHRINK}) — lower ~3/4 of the screen should be \
         MAGENTA, marking pixels where bevy's unguarded OIT index exceeds oit_heads"
    );
}

/// Replace `oit_heads` with a buffer a quarter of the view's size, every frame,
/// after `prepare_oit_buffers` sizes it correctly. The bind groups (built later
/// in `PrepareBindGroups`) then bind this undersized buffer, reproducing the
/// "buffer smaller than the rasterized view" condition without any resize.
fn force_shrink_heads(
    mut buffers: ResMut<OitBuffers>,
    device: Res<RenderDevice>,
    cameras: Query<&ExtractedCamera, With<OrderIndependentTransparencySettings>>,
) {
    let Some(size) = cameras.iter().find_map(|c| c.physical_target_size) else {
        return;
    };
    let small = ((size.x * size.y) as usize / 4).max(1);
    let mut heads = UninitBufferVec::new(BufferUsages::COPY_DST | BufferUsages::STORAGE);
    heads.reserve(small, &device);
    buffers.heads = heads;
}
