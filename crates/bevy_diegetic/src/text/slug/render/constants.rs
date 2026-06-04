use bevy::asset::uuid_handle;
use bevy::prelude::Handle;
use bevy::shader::Shader;

pub(super) const SLUG_TEXT_SHADER_PATH: &str =
    "embedded://bevy_diegetic/text/slug/shaders/slug_text.wgsl";

/// Vertex-pulling stage swapped in by `TextExtension::specialize` for
/// materials whose `vertex_pull` flag is set. Loaded behind a stable handle so
/// specialization can name it without an asset server.
pub const SLUG_TEXT_VERTEX_PULL_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("3f8a5c21-9d4b-4e6f-8a07-5b2c9e1d4a73");
