//! wgpu compute pipeline + bind-group layout for GPU glyph rasterization.

use std::borrow::Cow;

use bevy::asset::AssetServer;
use bevy::asset::Handle;
use bevy::prelude::Resource;
use bevy::render::render_resource::BindGroupLayoutDescriptor;
use bevy::render::render_resource::BindGroupLayoutEntries;
use bevy::render::render_resource::CachedComputePipelineId;
use bevy::render::render_resource::ComputePipelineDescriptor;
use bevy::render::render_resource::PipelineCache;
use bevy::render::render_resource::ShaderStages;
use bevy::render::render_resource::StorageTextureAccess;
use bevy::render::render_resource::TextureFormat;
use bevy::render::render_resource::TextureSampleType;
use bevy::render::render_resource::binding_types::storage_buffer_read_only_sized;
use bevy::render::render_resource::binding_types::texture_2d;
use bevy::render::render_resource::binding_types::texture_storage_2d;
use bevy::render::render_resource::binding_types::uniform_buffer_sized;
use bevy::shader::Shader;
use bevy::shader::ShaderDefVal;

/// Asset path for the embedded SDF generation kernel.
///
/// Loaded via [`bevy::asset::embedded_asset!`] in the plugin so the
/// WGSL ships in the crate binary.
pub(super) const SDF_SHADER_ASSET_PATH: &str =
    "embedded://bevy_diegetic/text/gpu_rasterizer/shaders/sdf_gen.wgsl";

/// Asset path for the embedded MSDF generation kernel.
pub(super) const MSDF_SHADER_ASSET_PATH: &str =
    "embedded://bevy_diegetic/text/gpu_rasterizer/shaders/msdf_gen.wgsl";

/// Asset path for the embedded MSDF error-correction kernel.
pub(super) const MSDF_CORRECT_SHADER_ASSET_PATH: &str =
    "embedded://bevy_diegetic/text/gpu_rasterizer/shaders/msdf_correct.wgsl";

/// Workgroup edge length — 8 × 8 = 64 threads per workgroup.
pub(super) const WORKGROUP_SIZE: u32 = 8;

/// std140 uniform layout. Matches `RasterParams` in `sdf_gen.wgsl`.
///
/// All four fields are 4-byte aligned; the total is 16 bytes which is
/// already a multiple of 16 so no trailing pad is needed.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct RasterParams {
    pub sdf_range:      f32,
    pub padding_texels: u32,
    pub distance_field: u32,
    pub glyph_count:    u32,
}

/// std140 per-glyph header. Matches `GlyphHeader` in `sdf_gen.wgsl`,
/// `msdf_gen.wgsl`, and `msdf_correct.wgsl`.
///
/// 32 bytes per record so the storage-buffer array stride stays a
/// multiple of 16. The generator kernels ignore the corner fields; the
/// MSDF correction kernel uses them to walk the per-glyph slice of the
/// global corners buffer.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct GlyphHeader {
    pub edge_offset:   u32,
    pub edge_count:    u32,
    pub atlas_origin:  [u32; 2],
    pub bitmap_size:   [u32; 2],
    pub corner_offset: u32,
    pub corner_count:  u32,
}

/// Render-world resource holding the compute pipelines + bind layouts.
#[derive(Resource)]
pub(super) struct GpuRasterizerPipeline {
    pub layout:                 BindGroupLayoutDescriptor,
    pub correction_layout:      BindGroupLayoutDescriptor,
    pub sdf_pipeline:           CachedComputePipelineId,
    pub msdf_pipeline:          CachedComputePipelineId,
    pub msdf_correct_pipeline:  CachedComputePipelineId,
    /// MTSDF correction. Reuses the MSDF correction kernel built with
    /// the `MTSDF=1` `shader_def` — the only delta is that the alpha
    /// channel carries an encoded signed true distance rather than the
    /// MSDF path's hardcoded `1.0`.
    pub mtsdf_correct_pipeline: CachedComputePipelineId,
    #[allow(dead_code, reason = "kept for shader hot-reload diagnostics")]
    pub sdf_shader:             Handle<Shader>,
    #[allow(dead_code, reason = "kept for shader hot-reload diagnostics")]
    pub msdf_shader:            Handle<Shader>,
    #[allow(dead_code, reason = "kept for shader hot-reload diagnostics")]
    pub msdf_correct_shader:    Handle<Shader>,
}

impl GpuRasterizerPipeline {
    /// Constructs both bind-group layouts + queues the SDF, MSDF, and
    /// MSDF-correction compute pipelines through the `PipelineCache`.
    /// Each pipeline becomes usable once `PipelineCache` reports
    /// `CachedPipelineState::Ok` for it.
    #[must_use]
    pub fn create(pipeline_cache: &PipelineCache, asset_server: &AssetServer) -> Self {
        let layout = BindGroupLayoutDescriptor::new(
            "gpu_rasterizer_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    // 0: edges storage buffer (read-only)
                    storage_buffer_read_only_sized(false, None),
                    // 1: glyph headers storage buffer (read-only)
                    storage_buffer_read_only_sized(false, None),
                    // 2: output storage texture (write-only)
                    texture_storage_2d(TextureFormat::Rgba8Unorm, StorageTextureAccess::WriteOnly),
                    // 3: raster params uniform
                    uniform_buffer_sized(false, None),
                ),
            ),
        );

        let correction_layout = BindGroupLayoutDescriptor::new(
            "gpu_rasterizer_correction_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    // 0: edges storage buffer (read-only)
                    storage_buffer_read_only_sized(false, None),
                    // 1: glyph headers storage buffer (read-only)
                    storage_buffer_read_only_sized(false, None),
                    // 2: scratch MSDF input — sampled (textureLoad, no sampler).
                    texture_2d(TextureSampleType::Float { filterable: false }),
                    // 3: page output storage texture (write-only)
                    texture_storage_2d(TextureFormat::Rgba8Unorm, StorageTextureAccess::WriteOnly),
                    // 4: raster params uniform
                    uniform_buffer_sized(false, None),
                    // 5: corner points storage buffer (read-only)
                    storage_buffer_read_only_sized(false, None),
                ),
            ),
        );
        let sdf_shader: Handle<Shader> = asset_server.load(SDF_SHADER_ASSET_PATH);
        let msdf_shader: Handle<Shader> = asset_server.load(MSDF_SHADER_ASSET_PATH);
        let msdf_correct_shader: Handle<Shader> = asset_server.load(MSDF_CORRECT_SHADER_ASSET_PATH);

        let sdf_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label:                            Some("gpu_rasterizer_sdf_pipeline".into()),
            layout:                           vec![layout.clone()],
            push_constant_ranges:             Vec::new(),
            shader:                           sdf_shader.clone(),
            shader_defs:                      Vec::new(),
            entry_point:                      Some(Cow::Borrowed("sdf_main")),
            zero_initialize_workgroup_memory: false,
        });
        let msdf_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label:                            Some("gpu_rasterizer_msdf_pipeline".into()),
            layout:                           vec![layout.clone()],
            push_constant_ranges:             Vec::new(),
            shader:                           msdf_shader.clone(),
            shader_defs:                      Vec::new(),
            entry_point:                      Some(Cow::Borrowed("msdf_main")),
            zero_initialize_workgroup_memory: false,
        });
        Self {
            layout,
            correction_layout: correction_layout.clone(),
            sdf_pipeline,
            msdf_pipeline,
            msdf_correct_pipeline: pipeline_cache.queue_compute_pipeline(
                ComputePipelineDescriptor {
                    label:                            Some(
                        "gpu_rasterizer_msdf_correct_pipeline".into(),
                    ),
                    layout:                           vec![correction_layout.clone()],
                    push_constant_ranges:             Vec::new(),
                    shader:                           msdf_correct_shader.clone(),
                    shader_defs:                      Vec::new(),
                    entry_point:                      Some(Cow::Borrowed("msdf_correct_main")),
                    zero_initialize_workgroup_memory: false,
                },
            ),
            mtsdf_correct_pipeline: pipeline_cache.queue_compute_pipeline(
                ComputePipelineDescriptor {
                    label:                            Some(
                        "gpu_rasterizer_mtsdf_correct_pipeline".into(),
                    ),
                    layout:                           vec![correction_layout],
                    push_constant_ranges:             Vec::new(),
                    shader:                           msdf_correct_shader.clone(),
                    shader_defs:                      vec![ShaderDefVal::Bool(
                        "MTSDF".into(),
                        true,
                    )],
                    entry_point:                      Some(Cow::Borrowed("msdf_correct_main")),
                    zero_initialize_workgroup_memory: false,
                },
            ),
            sdf_shader,
            msdf_shader,
            msdf_correct_shader,
        }
    }
}
