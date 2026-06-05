//! Crash-free detector for bevy's unguarded OIT `oit_heads` indexing.
//!
//! `oit_draw.wgsl` writes and `oit_resolve.wgsl` reads
//! `oit_heads[floor(x) + floor(y) * view.viewport.z]` with no bounds check.
//! The buffer is sized from a per-frame `physical_target_size` snapshot
//! (`prepare_oit_buffers`). During a window live-resize the rasterized surface
//! can momentarily exceed that snapshot, so `screen_index` runs past the
//! buffer end — an out-of-bounds access (shaders compile with
//! `ShaderRuntimeChecks::unchecked()`, so nothing clamps it). On Apple Silicon
//! this has faulted the GPU / panicked the kernel.
//!
//! Rather than let that write land (destructive), this example patches both
//! shaders before OIT runs:
//!   - draw pass: a bounds guard, so the out-of-bounds write never happens;
//!   - resolve pass: output MAGENTA wherever `screen_index >= arrayLength`.
//! OIT activation is GATED behind the patch (following `oit_guard.rs`): the
//! camera gets `OrderIndependentTransparencySettings` only after both shaders
//! are confirmed patched, so the resolve pass can never run unpatched. The run
//! is safe — no out-of-bounds access occurs — and the screen flashes magenta
//! on exactly the pixels where the unguarded shader WOULD have gone OOB.
//!
//! Watch the log for `DETECTOR ARMED` (both patches applied, OIT active). Then:
//!   - drag a window edge/corner, especially enlarging quickly, and/or
//!   - press SPACE to toggle maximized (an OS-animated zoom).
//! Magenta along the bottom/right edges = a reproduction of the race.

use bevy::asset::AssetId;
use bevy::core_pipeline::oit::OrderIndependentTransparencySettings;
use bevy::prelude::*;
use bevy::shader::Shader;
use bevy::shader::Source;

/// Bevy loads the resolve shader lazily (only when an OIT view specializes),
/// and activation is gated behind the patch — so request it at startup, or the
/// gate would wait forever for a shader that never loads.
const RESOLVE_SHADER_PATH: &str = "embedded://bevy_core_pipeline/oit/resolve/oit_resolve.wgsl";

const DRAW_ANCHOR: &str =
    "let screen_index = u32(floor(position.x) + floor(position.y) * view.viewport.z);";
const DRAW_GUARD: &str =
    "\n    if screen_index >= arrayLength(&oit_heads) { return; } // detector guard";

const RESOLVE_ANCHOR: &str =
    "let screen_index = u32(floor(in.position.x) + floor(in.position.y) * view.viewport.z);";
const RESOLVE_DETECT: &str =
    "\n    if screen_index >= arrayLength(&heads) { return vec4(1.0, 0.0, 1.0, 1.0); } // detector";

#[derive(Resource, Default)]
struct Detector {
    draw:           bool,
    resolve:        bool,
    activated:      bool,
    resolve_handle: Option<Handle<Shader>>,
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .init_resource::<Detector>()
        .add_systems(Startup, (setup, request_resolve_shader))
        .add_systems(Update, (patch_shaders, activate_oit, toggle_maximized))
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // No OIT settings yet — activate_oit adds them once the shaders are patched.
    commands.spawn((
        Camera3d::default(),
        Msaa::Off,
        Transform::from_xyz(0.0, 0.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // One transparent quad is enough to activate OIT; the resolve pass then
    // runs over every pixel, so the detector reports across the whole screen.
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

fn request_resolve_shader(asset_server: Res<AssetServer>, mut detector: ResMut<Detector>) {
    detector.resolve_handle = Some(asset_server.load(RESOLVE_SHADER_PATH));
}

fn patch_shaders(mut shaders: ResMut<Assets<Shader>>, mut detector: ResMut<Detector>) {
    if detector.draw && detector.resolve {
        return;
    }
    let mut draw_id: Option<AssetId<Shader>> = None;
    let mut resolve_id: Option<AssetId<Shader>> = None;
    for (id, shader) in shaders.iter() {
        if !detector.draw && shader.path.ends_with("oit/oit_draw.wgsl") {
            draw_id = Some(id);
        }
        if !detector.resolve && shader.path.ends_with("resolve/oit_resolve.wgsl") {
            resolve_id = Some(id);
        }
    }
    if let Some(id) = draw_id {
        detector.draw = insert_after(&mut shaders, id, DRAW_ANCHOR, DRAW_GUARD);
    }
    if let Some(id) = resolve_id {
        detector.resolve = insert_after(&mut shaders, id, RESOLVE_ANCHOR, RESOLVE_DETECT);
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
        error!("oit_oob_detector: anchor not found in {} — bevy shader text changed", shader.path);
        return true; // give up on this shader rather than spin forever
    };
    let mut patched = source.to_string();
    patched.insert_str(at, addition);
    let path = shader.path.clone();
    warn!("oit_oob_detector: patched {path}");
    let _ = shaders.insert(id, Shader::from_wgsl(patched, path));
    true
}

fn activate_oit(
    mut commands: Commands,
    mut detector: ResMut<Detector>,
    cameras: Query<Entity, (With<Camera3d>, Without<OrderIndependentTransparencySettings>)>,
) {
    if detector.activated || !(detector.draw && detector.resolve) {
        return;
    }
    for cam in &cameras {
        commands
            .entity(cam)
            .insert(OrderIndependentTransparencySettings::default());
    }
    detector.activated = true;
    warn!("oit_oob_detector: DETECTOR ARMED — OIT active with guarded+instrumented shaders; resize now");
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
