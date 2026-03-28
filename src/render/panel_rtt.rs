//! Render-to-texture compositing for diegetic panels.
//!
//! Each panel gets its own offscreen render texture, orthographic camera, and
//! display quad. All panel content (backgrounds, borders, text, images) is
//! tagged with a per-panel render layer so only the panel's camera sees it.
//! The result is a single textured quad in 3D space — zero depth issues, correct
//! alpha compositing, pixel-perfect output.

use std::collections::HashMap;

use bevy::camera::RenderTarget;
use bevy::camera::ScalingMode;
use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::picking::mesh_picking::ray_cast::RayCastBackfaces;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;

use crate::plugin::ComputedDiegeticPanel;
use crate::plugin::DiegeticPanel;
use crate::plugin::RenderMode;
use crate::plugin::UnitConfig;

/// Render layer offset — panel layers start here to avoid conflicts with
/// user-defined layers.
const PANEL_LAYER_OFFSET: usize = 16;

/// Default texels per world-space meter for RTT resolution.
/// ~200 DPI at arm's length.
const DEFAULT_TEXELS_PER_METER: f32 = 10000.0;

/// Minimum texture dimension in pixels.
const MIN_TEXTURE_SIZE: u32 = 64;

/// Maximum texture dimension in pixels.
const MAX_TEXTURE_SIZE: u32 = 4096;

// ── Components ──────────────────────────────────────────────────────────────

/// Marker for the per-panel orthographic RTT camera.
#[derive(Component)]
pub(super) struct PanelRttCamera;

/// Marker for the display quad that shows the rendered texture.
#[derive(Component)]
pub(super) struct PanelDisplayQuad;

// ── Resources ───────────────────────────────────────────────────────────────

/// Tracks render layer assignments and RTT state for each panel.
///
/// Updated synchronously (not via commands) so that content spawners
/// running in the same frame can read the assigned layer.
#[derive(Resource)]
pub(super) struct PanelRttRegistry {
    next_layer:  usize,
    assignments: HashMap<Entity, PanelRttState>,
}

impl Default for PanelRttRegistry {
    fn default() -> Self {
        Self {
            next_layer:  PANEL_LAYER_OFFSET,
            assignments: HashMap::new(),
        }
    }
}

impl PanelRttRegistry {
    /// Returns the render layer for a panel, assigning one if needed.
    pub(super) fn layer_for(&mut self, entity: Entity) -> usize {
        self.assignments
            .entry(entity)
            .or_insert_with(|| {
                let layer = self.next_layer;
                self.next_layer += 1;
                PanelRttState { layer }
            })
            .layer
    }

    /// Returns the render layer for a panel if already assigned.
    #[must_use]
    pub(super) fn get_layer(&self, entity: Entity) -> Option<usize> {
        self.assignments.get(&entity).map(|s| s.layer)
    }
}

struct PanelRttState {
    layer: usize,
}

// ── Plugin ──────────────────────────────────────────────────────────────────

/// Plugin that adds render-to-texture compositing for diegetic panels.
///
/// Registers resources. The `setup_panel_rtt` system is added by
/// [`super::text_renderer::TextRenderPlugin`] with correct ordering
/// relative to content spawners.
pub struct PanelRttPlugin;

impl Plugin for PanelRttPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PanelRttRegistry>();
        app.add_systems(PostUpdate, cleanup_removed_panels);
    }
}

// ── Systems ─────────────────────────────────────────────────────────────────

/// Sets up RTT infrastructure for panels that have a computed layout.
///
/// For each panel with a `ComputedDiegeticPanel`:
/// 1. Assigns a render layer (synchronous via resource, not commands)
/// 2. Spawns an orthographic camera targeting an offscreen texture
/// 3. Spawns a display quad showing the texture to the main camera
///
/// Only runs for panels that don't already have RTT children.
pub(super) fn setup_panel_rtt(
    panels: Query<(Entity, &DiegeticPanel, &ComputedDiegeticPanel), Changed<ComputedDiegeticPanel>>,
    existing_cameras: Query<&ChildOf, With<PanelRttCamera>>,
    mut registry: ResMut<PanelRttRegistry>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    unit_config: Res<UnitConfig>,
    mut commands: Commands,
) {
    for (panel_entity, panel, computed) in &panels {
        if computed.result().is_none() {
            continue;
        }

        // Geometry mode renders directly — no RTT infrastructure needed.
        if panel.render_mode == RenderMode::Geometry {
            continue;
        }

        // The RTT camera renders live every frame — content changes are
        // automatically reflected. Only create infrastructure on first setup.
        let has_camera = existing_cameras
            .iter()
            .any(|child_of| child_of.parent() == panel_entity);
        if has_camera {
            continue;
        }

        // Assign a render layer (immediate, no commands).
        let layer = registry.layer_for(panel_entity);
        let render_layers = RenderLayers::layer(layer);

        let world_w = panel.world_width(&unit_config);
        let world_h = panel.world_height(&unit_config);
        let (anchor_x, anchor_y) = panel.anchor_offsets(&unit_config);

        // Compute texture dimensions.
        let tex_w = (world_w * DEFAULT_TEXELS_PER_METER)
            .round()
            .clamp(MIN_TEXTURE_SIZE as f32, MAX_TEXTURE_SIZE as f32) as u32;
        let tex_h = (world_h * DEFAULT_TEXELS_PER_METER)
            .round()
            .clamp(MIN_TEXTURE_SIZE as f32, MAX_TEXTURE_SIZE as f32) as u32;

        // Create render target texture.
        let image = Image::new_target_texture(
            tex_w,
            tex_h,
            TextureFormat::Rgba8Unorm,
            Some(TextureFormat::Rgba8UnormSrgb),
        );
        let image_handle = images.add(image);

        // Camera center: the midpoint of the panel content area.
        // Content spans from (-anchor_x, anchor_y) [TL] to
        // (world_w - anchor_x, anchor_y - world_h) [BR].
        let cam_x = world_w * 0.5 - anchor_x;
        let cam_y = anchor_y - world_h * 0.5;

        // Spawn orthographic camera targeting the render texture.
        commands.entity(panel_entity).with_child((
            PanelRttCamera,
            Camera3d::default(),
            Camera {
                order: -1, // Render before the main camera.
                clear_color: ClearColorConfig::Custom(Color::NONE),
                ..default()
            },
            RenderTarget::Image(image_handle.clone().into()),
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::Fixed {
                    width:  world_w,
                    height: world_h,
                },
                near: -1.0,
                far: 10.0,
                ..OrthographicProjection::default_3d()
            }),
            render_layers.clone(),
            Transform::from_xyz(cam_x, cam_y, 5.0)
                .looking_at(Vec3::new(cam_x, cam_y, 0.0), Vec3::Y),
        ));

        // The MSDF text material uses PBR lighting. A directional light
        // on the panel's render layer ensures text is visible in the RTT pass.
        // Panel geometry (backgrounds, borders) uses unlit materials and is
        // unaffected.
        commands.entity(panel_entity).with_child((
            DirectionalLight {
                shadows_enabled: false,
                illuminance: 10_000.0,
                ..default()
            },
            render_layers.clone(),
            Transform::from_xyz(cam_x, cam_y, 5.0)
                .looking_at(Vec3::new(cam_x, cam_y, 0.0), Vec3::Y),
        ));

        // Spawn display quad — visible to main camera only (layer 0).
        let quad_mesh = meshes.add(Rectangle::new(world_w, world_h));
        let quad_material = materials.add(StandardMaterial {
            base_color_texture: Some(image_handle),
            alpha_mode: AlphaMode::Premultiplied,
            double_sided: true,
            cull_mode: None,
            // Purely diffuse surface — no specular highlights or Fresnel
            // reflections. Without this, PBR adds a gray specular tint to
            // transparent regions of the RTT texture.
            reflectance: 0.0,
            perceptual_roughness: 1.0,
            ..default()
        });

        // Position the quad at the center of the panel content area.
        commands.entity(panel_entity).with_child((
            PanelDisplayQuad,
            RayCastBackfaces,
            NotShadowCaster,
            Mesh3d(quad_mesh),
            MeshMaterial3d(quad_material),
            RenderLayers::layer(0),
            Transform::from_xyz(cam_x, cam_y, 0.0),
        ));
    }
}

/// Cleans up RTT resources when panel entities are removed.
fn cleanup_removed_panels(
    mut registry: ResMut<PanelRttRegistry>,
    panels: Query<Entity, With<DiegeticPanel>>,
) {
    let active: Vec<Entity> = panels.iter().collect();
    registry
        .assignments
        .retain(|entity, _| active.contains(entity));
}
