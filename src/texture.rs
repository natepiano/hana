use bevy::core_pipeline::core_3d::CORE_3D_DEPTH_FORMAT;
use bevy::prelude::*;
use bevy::render::render_resource::Extent3d;
use bevy::render::render_resource::TextureDimension;
use bevy::render::render_resource::TextureFormat;
use bevy::render::render_resource::TextureUsages;
use bevy_render::camera::ExtractedCamera;
use bevy_render::render_resource::Texture;
use bevy_render::render_resource::TextureDescriptor;
use bevy_render::renderer::RenderDevice;
use bevy_render::texture::CachedTexture;
use bevy_render::texture::TextureCache;

use super::camera::OutlineCamera;
use super::constants::MSAA_DISABLED_SAMPLE_COUNT;
use super::constants::OUTLINE_DEPTH_TEXTURE_LABEL;
use super::extract::ActiveOutlineModes;

#[derive(Clone, Component)]
pub(crate) struct FloodTextures {
    pub(crate) ping_pong:     PingPongState,
    // Textures for storing input-output of flood passes
    pub(crate) input:         CachedTexture,
    pub(crate) output:        CachedTexture,
    /// A dedicated depth texture for mesh outlines to later compare against
    /// global depth
    pub(crate) outline_depth: Texture,
    /// Stores outline color and mesh data
    pub(crate) appearance:    CachedTexture,
    /// Stores per-mesh owner ID in x channel — only allocated when hull outlines are active
    pub(crate) owner:         Option<CachedTexture>,
}

#[derive(Clone, Copy)]
pub(crate) enum PingPongState {
    PrimaryInput,
    SecondaryInput,
}

impl FloodTextures {
    pub(crate) const fn input(&self) -> &CachedTexture {
        match self.ping_pong {
            PingPongState::PrimaryInput => &self.input,
            PingPongState::SecondaryInput => &self.output,
        }
    }

    pub(crate) const fn output(&self) -> &CachedTexture {
        match self.ping_pong {
            PingPongState::PrimaryInput => &self.output,
            PingPongState::SecondaryInput => &self.input,
        }
    }

    pub(crate) const fn swap_ping_pong(&mut self) {
        self.ping_pong = match self.ping_pong {
            PingPongState::PrimaryInput => PingPongState::SecondaryInput,
            PingPongState::SecondaryInput => PingPongState::PrimaryInput,
        };
    }
}

pub(crate) fn prepare_flood_textures(
    mut commands: Commands,
    mut texture_cache: ResMut<TextureCache>,
    render_device: Res<RenderDevice>,
    active: Res<ActiveOutlineModes>,
    cameras: Query<(Entity, &ExtractedCamera), With<OutlineCamera>>,
) {
    for (entity, camera) in cameras.iter() {
        let Some(target_size) = camera.physical_target_size else {
            continue;
        };

        let size = Extent3d {
            width:                 target_size.x,
            height:                target_size.y,
            depth_or_array_layers: 1,
        };

        let texture_descriptor = TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: MSAA_DISABLED_SAMPLE_COUNT,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba32Float,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        };

        // Create the depth texture
        let outline_depth = render_device.create_texture(&TextureDescriptor {
            label: Some(OUTLINE_DEPTH_TEXTURE_LABEL),
            size,
            mip_level_count: 1,
            sample_count: MSAA_DISABLED_SAMPLE_COUNT,
            dimension: TextureDimension::D2,
            format: CORE_3D_DEPTH_FORMAT,
            usage: TextureUsages::RENDER_ATTACHMENT  // For using as depth buffer
        | TextureUsages::TEXTURE_BINDING, // For sampling in composite pass
            view_formats: &[],
        });

        let owner = if active.methods.has_hull() {
            Some(texture_cache.get(&render_device, texture_descriptor.clone()))
        } else {
            None
        };

        commands.entity(entity).insert(FloodTextures {
            ping_pong: PingPongState::PrimaryInput,
            input: texture_cache.get(&render_device, texture_descriptor.clone()),
            output: texture_cache.get(&render_device, texture_descriptor.clone()),
            outline_depth,
            appearance: texture_cache.get(&render_device, texture_descriptor),
            owner,
        });
        texture_cache.update();
    }
}
