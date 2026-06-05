//! Deterministic proof that bevy's unguarded OIT `oit_heads` indexing writes
//! out of bounds — and that the `oit_guard` bounds check prevents it.
//!
//! Working backwards from the guard condition `screen_index >=
//! arrayLength(&oit_heads)`: instead of waiting for a macOS live-resize to
//! make the drawable outrun the CPU-side buffer-size snapshot (a timing race),
//! this forces the condition directly. A startup system rewrites bevy's
//! `oit_draw.wgsl` at the same anchor `oit_guard` uses, adding
//! `arrayLength(&oit_heads)` to `screen_index` so every OIT fragment writes
//! into `[len, 2*len)` — a bounded, every-frame out-of-bounds write into
//! whatever GPU allocation sits adjacent. Shaders compile with
//! `ShaderRuntimeChecks::unchecked()`, so nothing clamps it.
//!
//! Topology matches the crash: a 3D OIT camera is the out-of-bounds *source*;
//! a second camera renders content on top, and its GPU buffers are the visible
//! *victims* (on the real app these were the screen-space text panels — the
//! 3D OIT view itself looked fine while the panels filled with garbage).
//!
//! # DANGER
//! On Apple Silicon (unified memory) an out-of-bounds GPU write has produced
//! AGX `DATA ABORT` kernel panics and firmware lockups — a forced reboot that
//! can scribble other processes' window surfaces on the way down. Do NOT run
//! this casually. Reboot first, save all work in every app, then run once.
//!
//! Set `SABOTAGE` to `false` to render the same scene with the unmodified
//! shader (a control: the scene should render correctly).

use bevy::asset::AssetId;
use bevy::core_pipeline::oit::OrderIndependentTransparencySettings;
use bevy::prelude::*;
use bevy::shader::Shader;
use bevy::shader::Source;

/// When `true`, force every OIT fragment to write past the heads buffer.
const SABOTAGE: bool = true;

/// The line `oit_guard` anchors on, in bevy's `oit/oit_draw.wgsl`.
const DRAW_ANCHOR: &str =
    "let screen_index = u32(floor(position.x) + floor(position.y) * view.viewport.z);";

/// Replacement that pushes the write index a full buffer length past the end.
const DRAW_BREAK: &str = "var screen_index = u32(floor(position.x) + floor(position.y) * \
     view.viewport.z);\n    screen_index = screen_index + arrayLength(&oit_heads); // FORCED OOB";

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, setup)
        .add_systems(Update, break_oit_draw_shader)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Out-of-bounds source: OIT 3D camera over a stack of transparent quads.
    commands.spawn((
        Camera3d::default(),
        OrderIndependentTransparencySettings::default(),
        Msaa::Off,
        Transform::from_xyz(0.0, 0.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    let quad = meshes.add(Rectangle::new(100.0, 100.0));
    let glass = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.3),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    for layer in 0_u8..12 {
        commands.spawn((
            Mesh3d(quad.clone()),
            MeshMaterial3d(glass.clone()),
            Transform::from_xyz(0.0, 0.0, -0.1 * f32::from(layer)),
        ));
    }

    // Victims: a second camera rendering many sprites on top. Their GPU
    // buffers sit adjacent to the OIT buffer; the out-of-bounds write lands
    // in them and shows as garbage here, with the 3D view above unaffected.
    commands.spawn((
        Camera2d,
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        Msaa::Off,
    ));
    for x in -10_i8..10 {
        for y in -6_i8..6 {
            commands.spawn((
                Sprite::from_color(Color::srgb(0.2, 0.7, 1.0), Vec2::splat(40.0)),
                Transform::from_xyz(f32::from(x) * 48.0, f32::from(y) * 48.0, 0.0),
            ));
        }
    }
}

/// Rewrites bevy's OIT draw shader once it loads, adding a buffer-length
/// offset to the write index. Mirrors `oit_guard`'s anchor-patch machinery,
/// inverted: a guard returns on out-of-bounds; this guarantees it.
fn break_oit_draw_shader(mut shaders: ResMut<Assets<Shader>>, mut done: Local<bool>) {
    if *done || !SABOTAGE {
        return;
    }
    let target = shaders
        .iter()
        .find(|(_, shader)| shader.path.ends_with("oit/oit_draw.wgsl"))
        .map(|(id, _)| id);
    let Some(id) = target else {
        return;
    };
    patch(&mut shaders, id);
    *done = true;
}

fn patch(shaders: &mut Assets<Shader>, id: AssetId<Shader>) {
    let Some(shader) = shaders.get(id) else {
        return;
    };
    let Source::Wgsl(source) = &shader.source else {
        return;
    };
    if !source.contains(DRAW_ANCHOR) {
        error!("oit_oob_forced: anchor not found — bevy shader text changed; SABOTAGE not applied");
        return;
    }
    let patched = source.replace(DRAW_ANCHOR, DRAW_BREAK);
    let path = shader.path.clone();
    warn!(
        "oit_oob_forced: SABOTAGE applied to {path} — every OIT fragment now writes out of bounds"
    );
    let _ = shaders.insert(id, Shader::from_wgsl(patched, path));
}
