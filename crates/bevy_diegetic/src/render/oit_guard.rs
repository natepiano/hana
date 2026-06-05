//! Bounds guards patched into bevy's OIT shaders at runtime.
//!
//! Bevy compiles every shader with `ShaderRuntimeChecks::unchecked()`, and its
//! OIT shaders index the `oit_heads` / `heads` storage buffers by
//! `frag_coord + frag_coord.y × viewport_width` with no bounds check. The
//! buffers are sized from a CPU-side snapshot of the camera target size;
//! during a macOS live-resize the drawable can be larger than that snapshot
//! for a frame, so fragments land past the buffer end and the unchecked
//! writes fault the GPU (observed: AGX `DATA ABORT` kernel panic and a
//! firmware-detected lockup, both during window-restore resizes).
//!
//! This module rewrites the two shader assets in place once they load (the
//! resolve shader is requested explicitly at startup — bevy would otherwise
//! only load it after OIT activates, which the gate below prevents):
//! it finds the `screen_index` computation by exact anchor text and inserts
//! an `arrayLength` guard after it (plus an in-bounds condition on the
//! resolve pass's node-chain walk). Replacing the asset emits
//! `AssetEvent::Modified`, which the pipeline cache already handles by
//! rebuilding dependent pipelines, so a pipeline compiled before the patch
//! is replaced on the next frame.
//!
//! If bevy's shader text changes, the anchors stop matching: the system
//! logs an error and leaves the shader untouched rather than corrupt it.
//! Remove this module when upstream bevy guards these accesses itself.

use bevy::asset::AssetId;
use bevy::prelude::*;
use bevy::shader::Shader;
use bevy::shader::Source;

/// Marker comment inserted with every guard; its presence means the shader is
/// already patched.
const GUARD_MARKER: &str = "// bevy_diegetic oit_guard";

/// Embedded asset path of bevy's OIT resolve shader. The draw shader is a
/// shader library loaded at plugin build, but bevy only loads the resolve
/// shader when an OIT view's resolve pipeline specializes — and the
/// activation gate keeps OIT off until the shader is patched, so without an
/// explicit load request the gate would wait forever.
const RESOLVE_SHADER_PATH: &str = "embedded://bevy_core_pipeline/oit/resolve/oit_resolve.wgsl";

/// Anchor in `bevy_core_pipeline/src/oit/oit_draw.wgsl` (the `oit_draw`
/// shader-library function every OIT material fragment calls).
const DRAW_ANCHOR: &str =
    "let screen_index = u32(floor(position.x) + floor(position.y) * view.viewport.z);";

/// Guard for the draw shader: `oit_heads[screen_index]` is the unguarded
/// write (`oit_nodes` is already capacity-checked).
const DRAW_GUARD: &str = "
    // bevy_diegetic oit_guard: heads is sized from a CPU-side viewport
    // snapshot; clamp so a drawable larger than the snapshot cannot write
    // past the buffer end (shaders compile without bounds checks).
    if screen_index >= arrayLength(&oit_heads) {
        return;
    }";

/// Anchor in `bevy_core_pipeline/src/oit/resolve/oit_resolve.wgsl`.
const RESOLVE_ANCHOR: &str =
    "let screen_index = u32(floor(in.position.x) + floor(in.position.y) * view.viewport.z);";

/// Guard for the resolve pass: it both reads and clears `heads[screen_index]`.
/// The `if true { discard; }` form mirrors the workaround already used in the
/// same file (gfx-rs/wgpu#4416).
const RESOLVE_GUARD: &str = "
    // bevy_diegetic oit_guard: see DRAW_GUARD in bevy_diegetic.
    if screen_index >= arrayLength(&heads) {
        if true {
            discard;
        }
        return vec4(0.0);
    }";

/// The resolve pass walks the per-pixel fragment chain; a corrupted head or
/// node could point past the nodes buffer, so the walk gains an in-bounds
/// condition.
const RESOLVE_WALK: &str = "while current_node != LINKED_LIST_END_SENTINEL {";
const RESOLVE_WALK_GUARDED: &str =
    "while current_node != LINKED_LIST_END_SENTINEL && current_node < arrayLength(&nodes) {";

/// One shader's patch progress. `Failed` is terminal: the unguarded source is
/// still loaded, so OIT must never activate.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum GuardStatus {
    #[default]
    Pending,
    Applied,
    Failed,
}

/// Tracks both shaders' patch progress. [`Self::ready`] gates OIT activation
/// in `transparency.rs`: the unguarded shaders can fault the GPU during a
/// window resize, so no camera receives
/// `OrderIndependentTransparencySettings` until both patches are confirmed.
#[derive(Resource, Default)]
pub(super) struct OitGuardState {
    draw:           GuardStatus,
    resolve:        GuardStatus,
    /// Keeps the requested resolve shader asset alive; bevy's resolve
    /// pipeline only takes its own handle after OIT activates.
    resolve_handle: Option<Handle<Shader>>,
}

impl OitGuardState {
    /// Both OIT shaders carry the bounds guards — OIT is safe to turn on.
    pub(super) fn ready(&self) -> bool {
        self.draw == GuardStatus::Applied && self.resolve == GuardStatus::Applied
    }

    fn finished(&self) -> bool {
        self.draw != GuardStatus::Pending && self.resolve != GuardStatus::Pending
    }
}

/// Requests bevy's OIT resolve shader so the patch → gate → activate
/// sequence can make progress (see [`RESOLVE_SHADER_PATH`]). The handle is
/// held in [`OitGuardState`] so the asset stays loaded until bevy's resolve
/// pipeline takes its own.
pub(super) fn request_oit_resolve_shader(
    asset_server: Res<AssetServer>,
    mut state: ResMut<OitGuardState>,
) {
    state.resolve_handle = Some(asset_server.load(RESOLVE_SHADER_PATH));
}

/// Scans `Assets<Shader>` until both OIT shaders have been found and patched.
/// Embedded assets load asynchronously, so the targets appear a few frames
/// after startup; matching is by asset path suffix.
pub(super) fn guard_oit_shaders(
    mut shaders: ResMut<Assets<Shader>>,
    mut state: ResMut<OitGuardState>,
) {
    if state.finished() {
        return;
    }

    let mut draw_target: Option<AssetId<Shader>> = None;
    let mut resolve_target: Option<AssetId<Shader>> = None;
    for (id, shader) in shaders.iter() {
        if state.draw == GuardStatus::Pending && shader.path.ends_with("oit/oit_draw.wgsl") {
            draw_target = Some(id);
        }
        if state.resolve == GuardStatus::Pending
            && shader.path.ends_with("resolve/oit_resolve.wgsl")
        {
            resolve_target = Some(id);
        }
    }

    if let Some(id) = draw_target {
        state.draw = patch_shader(&mut shaders, id, &[(DRAW_ANCHOR, DRAW_GUARD)], &[]);
    }
    if let Some(id) = resolve_target {
        state.resolve = patch_shader(
            &mut shaders,
            id,
            &[(RESOLVE_ANCHOR, RESOLVE_GUARD)],
            &[(RESOLVE_WALK, RESOLVE_WALK_GUARDED)],
        );
    }
}

/// Applies anchor insertions and exact-text replacements to one shader asset,
/// rewriting it in place via [`Shader::from_wgsl`] (which re-derives the
/// import path from the patched source). `Failed` means the shader text did
/// not match (logged as an error) — the caller keeps OIT off in that case.
fn patch_shader(
    shaders: &mut Assets<Shader>,
    id: AssetId<Shader>,
    insertions: &[(&str, &str)],
    replacements: &[(&str, &str)],
) -> GuardStatus {
    let Some(shader) = shaders.get(id) else {
        return GuardStatus::Pending;
    };
    let Source::Wgsl(source) = &shader.source else {
        error!("oit_guard: {} is not WGSL; guard not applied", shader.path);
        return GuardStatus::Failed;
    };
    if source.contains(GUARD_MARKER) {
        return GuardStatus::Applied;
    }

    let mut patched = source.to_string();
    for (anchor, guard) in insertions {
        let Some(position) = patched.find(anchor).map(|at| at + anchor.len()) else {
            error!(
                "oit_guard: anchor not found in {} (bevy shader text changed?); \
                 guard NOT applied — OIT stays disabled to avoid GPU faults on \
                 resize",
                shader.path
            );
            return GuardStatus::Failed;
        };
        patched.insert_str(position, guard);
    }
    for (from, to) in replacements {
        if !patched.contains(from) {
            error!(
                "oit_guard: replacement target not found in {} (bevy shader \
                 text changed?); guard NOT applied — OIT stays disabled",
                shader.path
            );
            return GuardStatus::Failed;
        }
        patched = patched.replace(from, to);
    }

    let path = shader.path.clone();
    info!("oit_guard: bounds guards applied to {path}");
    if let Err(error) = shaders.insert(id, Shader::from_wgsl(patched, path)) {
        error!("oit_guard: failed to replace shader asset: {error}");
        return GuardStatus::Failed;
    }
    GuardStatus::Applied
}
