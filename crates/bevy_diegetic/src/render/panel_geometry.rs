//! Panel geometry rendering — SDF rounded rectangles from layout render commands.
//!
//! Each element with a background or border produces one SDF quad entity.
//! The SDF fragment shader renders both fill and border in a single pass
//! with pixel-perfect anti-aliased edges and per-corner radii.

use std::collections::HashMap;

use bevy::asset::load_internal_asset;
use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::picking::mesh_picking::ray_cast::RayCastBackfaces;
use bevy::prelude::*;
use bevy_kana::ToUsize;

use super::PanelChildSystems;
use super::clip;
use super::constants;
use super::constants::DRAW_LEVEL_GEOMETRY_LANES;
use super::constants::OIT_DEPTH_STEP;
use super::constants::OIT_FOCUS_DEPTH;
use super::constants::SDF_STROKE_SHADER_HANDLE;
use super::draw_order::DrawCommandDepth;
use super::draw_order::DrawOrderProjection;
use super::sdf_material;
use super::sdf_material::SdfPanelMaterial;
use super::sdf_material::SdfPanelMaterialInput;
use crate::layout::BoundingBox;
use crate::layout::RectangleSource;
use crate::layout::RenderCommand;
use crate::layout::RenderCommandKind;
use crate::panel::ComputedDiegeticPanel;
use crate::panel::DiegeticPanel;
use crate::panel::SurfaceShadow;

/// Marker for SDF panel mesh entities.
#[derive(Component)]
struct PanelSdfMesh;

/// What a [`PanelSdfMesh`] quad was built from. A panel rebuild compares this
/// against the freshly gathered surface to decide whether the quad can stay
/// untouched, have its material rewritten in place, or must be respawned.
/// Geometry and color `f32` values are stored as raw bits for exact comparison.
#[derive(Component, Clone, Copy, PartialEq)]
struct PanelSdfSurface {
    /// Render-command index — the quad's stable key across rebuilds.
    command_index: usize,
    /// Projected ordering values used by the quad's material.
    draw_depth:    DrawCommandDepth,
    /// World-space transform center (x, y).
    center:        [u32; 2],
    /// World-space mesh size (full width, height).
    mesh_size:     [u32; 2],
    /// World-space per-corner radii [TL, TR, BR, BL].
    corner_radii:  [u32; 4],
    /// World-space border widths [top, right, bottom, left].
    border_widths: [u32; 4],
    /// Clip rect in local quad space [left, bottom, right, top].
    clip_rect:     [u32; 4],
    /// Fill color, linear RGBA.
    fill_color:    [u32; 4],
    /// Border color, linear RGBA.
    border_color:  [u32; 4],
}

impl PanelSdfSurface {
    /// True when two surfaces have identical geometry, ignoring color. Callers
    /// match by `command_index` first, so that field is already known equal.
    fn geometry_eq(&self, other: &Self) -> bool {
        self.center == other.center
            && self.mesh_size == other.mesh_size
            && self.corner_radii == other.corner_radii
            && self.border_widths == other.border_widths
            && self.clip_rect == other.clip_rect
    }
}

/// The invisible full-panel interaction quad (Geometry mode only), tagged with
/// its world size and center so a rebuild can leave it untouched when the panel
/// has not resized. Both pairs are `f32` bits, as in [`PanelSdfSurface`].
#[derive(Component, Clone, Copy, PartialEq, Eq)]
struct PanelInteractionMesh {
    /// World size (width, height).
    size:   [u32; 2],
    /// World-space transform center (x, y).
    center: [u32; 2],
}

/// Whether shadow casting is enabled or suppressed for panel meshes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShadowMode {
    Enabled,
    Suppressed,
}

/// Plugin that adds panel geometry rendering (backgrounds and borders).
pub(super) struct PanelGeometryPlugin;

impl Plugin for PanelGeometryPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            SDF_STROKE_SHADER_HANDLE,
            "sdf_stroke.wgsl",
            Shader::from_wgsl
        );
        app.add_plugins(MaterialPlugin::<SdfPanelMaterial>::default());
        app.add_systems(
            PostUpdate,
            build_panel_geometry.in_set(PanelChildSystems::Build),
        );
    }
}

/// Gathered fill + border data for a single element.
struct ElementSurface {
    /// `Element` index in the layout tree.
    index:         usize,
    /// Bounding box from the render command.
    bounds:        BoundingBox,
    /// Fill color (from Rectangle command), if any.
    fill_color:    Option<Color>,
    /// Border widths [top, right, bottom, left] in layout points.
    border_widths: [f32; 4],
    /// Border color.
    border_color:  Option<Color>,
    /// Index of the first render command for this element (the reconcile
    /// reuse key).
    command_index: usize,
    /// Projected ordering values of the first render command for this element.
    draw_depth:    DrawCommandDepth,
    /// Active clip rect in layout coordinates when this surface was
    /// gathered. `None` means unclipped.
    clip_rect:     Option<BoundingBox>,
}

/// Reconciles each panel's SDF quads against its render commands.
///
/// `ComputedDiegeticPanel` is marked changed even for a color-only update (the
/// layout fast path regenerates render commands in place), so this system runs
/// on more than geometry moves. Rather than tear down and respawn every quad,
/// it diffs each surface against the existing child by [`PanelSdfSurface`]:
/// identical surfaces stay untouched, a material-only difference rewrites the
/// material in place, and a geometry difference (or a new/removed surface)
/// respawns just that quad. A surface that changes only its text color thus
/// leaves the background, border, and divider quads alone.
fn build_panel_geometry(
    changed_panels: Query<
        (
            Entity,
            &DiegeticPanel,
            &ComputedDiegeticPanel,
            Option<&RenderLayers>,
        ),
        Changed<ComputedDiegeticPanel>,
    >,
    old_sdf: Query<(
        Entity,
        &ChildOf,
        &PanelSdfSurface,
        &MeshMaterial3d<SdfPanelMaterial>,
    )>,
    old_interaction: Query<(Entity, &ChildOf, &PanelInteractionMesh)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
    mut sdf_materials: ResMut<Assets<SdfPanelMaterial>>,
    mut commands: Commands,
) {
    for (panel_entity, panel, computed, panel_layers) in &changed_panels {
        let Some(result) = computed.result() else {
            continue;
        };

        let (anchor_x, anchor_y) = panel.anchor_offsets();
        let context = PanelReconcileContext {
            panel_entity,
            panel,
            points_to_world: panel.points_to_world(),
            anchor_x,
            anchor_y,
            shadow_mode: if panel.surface_shadow() == SurfaceShadow::Off {
                ShadowMode::Suppressed
            } else {
                ShadowMode::Enabled
            },
            layer: panel_layers.cloned().unwrap_or(RenderLayers::layer(0)),
        };

        reconcile_sdf_quads(
            &context,
            &result.commands,
            computed.draw_order(),
            &old_sdf,
            &mut meshes,
            &mut sdf_materials,
            &mut commands,
        );
        reconcile_interaction_mesh(
            &context,
            &old_interaction,
            &mut meshes,
            &mut standard_materials,
            &mut commands,
        );
    }
}

/// Shared per-panel inputs for one reconcile pass, derived once from the panel.
struct PanelReconcileContext<'a> {
    panel_entity:    Entity,
    panel:           &'a DiegeticPanel,
    points_to_world: f32,
    anchor_x:        f32,
    anchor_y:        f32,
    shadow_mode:     ShadowMode,
    layer:           RenderLayers,
}

/// Reconciles a panel's SDF quads against its render commands: identical
/// surfaces stay untouched, a color-only difference recolors the material in
/// place, and a geometry difference (or a new/removed surface) respawns just
/// that quad.
fn reconcile_sdf_quads(
    context: &PanelReconcileContext<'_>,
    render_commands: &[RenderCommand],
    draw_order: &DrawOrderProjection,
    old_sdf: &Query<(
        Entity,
        &ChildOf,
        &PanelSdfSurface,
        &MeshMaterial3d<SdfPanelMaterial>,
    )>,
    meshes: &mut Assets<Mesh>,
    sdf_materials: &mut Assets<SdfPanelMaterial>,
    commands: &mut Commands,
) {
    let occupancy = draw_order.level_occupancy();
    if let Some((z_level, level_count)) = occupancy.iter().copied().max_by_key(|(_, count)| *count)
        && per_level_band_overflows(level_count)
    {
        warn_once!(
            "panel {:?} has {} draw commands at z-level {}, reaching the per-level screen band \
             cap ({}); coplanar geometry at that level reaches the shared line/text sub-lanes",
            context.panel_entity,
            level_count,
            z_level,
            per_level_band_capacity(),
        );
    }

    let panel_total = occupancy.iter().map(|(_, count)| *count).sum();
    if oit_total_overflows(panel_total) {
        warn_once!(
            "panel {:?} has {} total draw commands, reaching the OIT depth budget ({}); the \
             panel-global ordinal span exhausts 24-bit OIT depth headroom and coplanar ordering \
             degrades to OIT-list insertion order",
            context.panel_entity,
            panel_total,
            oit_depth_budget(),
        );
    }

    let gathered = gather_surfaces(context.panel, render_commands, draw_order);
    let desired = desired_surfaces(gathered);

    let mut existing: HashMap<usize, (Entity, PanelSdfSurface, Handle<SdfPanelMaterial>)> = old_sdf
        .iter()
        .filter(|(_, child_of, ..)| child_of.parent() == context.panel_entity)
        .map(|(entity, _, signature, material)| {
            (
                signature.command_index,
                (entity, *signature, material.0.clone()),
            )
        })
        .collect();

    for surface in &desired {
        let quad = build_sdf_quad(
            surface,
            context.panel,
            context.points_to_world,
            context.anchor_x,
            context.anchor_y,
        );
        match existing.remove(&quad.signature.command_index) {
            // Identical — leave the quad untouched.
            Some((_, signature, _)) if signature == quad.signature => {},
            // Geometry unchanged, material differs — rewrite the material.
            Some((entity, signature, material)) if signature.geometry_eq(&quad.signature) => {
                if let Some(mut existing_material) = sdf_materials.get_mut(&material) {
                    *existing_material = quad.material;
                }
                commands.entity(entity).insert(quad.signature);
            },
            // Geometry moved, or a brand-new surface — respawn this quad.
            Some((entity, ..)) => {
                commands.entity(entity).despawn();
                spawn_sdf_quad(quad, context, meshes, sdf_materials, commands);
            },
            None => {
                spawn_sdf_quad(quad, context, meshes, sdf_materials, commands);
            },
        }
    }

    // Any quad whose surface disappeared between builds is now stale.
    for (_, (entity, ..)) in existing {
        commands.entity(entity).despawn();
    }
}

fn per_level_band_capacity() -> usize {
    usize::try_from(DRAW_LEVEL_GEOMETRY_LANES).unwrap_or(usize::MAX)
}

fn per_level_band_overflows(busiest: usize) -> bool { busiest >= per_level_band_capacity() }

fn oit_depth_budget() -> usize {
    if OIT_DEPTH_STEP <= 0.0 {
        return usize::MAX;
    }
    (OIT_FOCUS_DEPTH / OIT_DEPTH_STEP).floor().to_usize()
}

fn oit_total_overflows(panel_total: usize) -> bool { panel_total >= oit_depth_budget() }

/// Reconciles the invisible full-panel interaction quad: it is respawned only
/// when the panel's world size or center changed, and left untouched otherwise.
fn reconcile_interaction_mesh(
    context: &PanelReconcileContext<'_>,
    old_interaction: &Query<(Entity, &ChildOf, &PanelInteractionMesh)>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    commands: &mut Commands,
) {
    let world_size = Vec2::new(context.panel.world_width(), context.panel.world_height());
    let center = Vec2::new(
        world_size.x.mul_add(0.5, -context.anchor_x),
        world_size.y.mul_add(-0.5, context.anchor_y),
    );
    let interaction = PanelInteractionMesh {
        size:   vec2_bits(world_size),
        center: vec2_bits(center),
    };
    let existing = old_interaction
        .iter()
        .find(|(_, child_of, _)| child_of.parent() == context.panel_entity);
    match existing {
        Some((_, _, current)) if *current == interaction => {},
        Some((entity, ..)) => {
            commands.entity(entity).despawn();
            spawn_interaction_mesh(
                interaction,
                world_size,
                center,
                context,
                meshes,
                materials,
                commands,
            );
        },
        None => {
            spawn_interaction_mesh(
                interaction,
                world_size,
                center,
                context,
                meshes,
                materials,
                commands,
            );
        },
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

struct GatheredCommands {
    surfaces: HashMap<usize, ElementSurface>,
    dividers: Vec<ElementSurface>,
}

/// Gathers fill + border data per element from render commands.
///
/// Computes the active clip rect for each command via
/// [`clip::compute_clip_rects`] and stores it on each surface and divider.
fn gather_surfaces(
    panel: &DiegeticPanel,
    commands: &[RenderCommand],
    draw_order: &DrawOrderProjection,
) -> GatheredCommands {
    let clip_rects = clip::compute_clip_rects(commands);
    let viewport = clip::panel_viewport(panel);
    let mut surfaces: HashMap<usize, ElementSurface> = HashMap::new();
    let mut dividers: Vec<ElementSurface> = Vec::new();

    for (cmd_index, cmd) in commands.iter().enumerate() {
        let active_clip = clip::effective_clip(cmd.bounds, clip_rects[cmd_index], viewport);
        match &cmd.kind {
            RenderCommandKind::Rectangle { color, source } => {
                let Some(active_clip) = active_clip else {
                    continue;
                };
                let Some(draw_depth) = draw_order.depth_for(cmd_index) else {
                    continue;
                };
                if *source == RectangleSource::ChildDivider {
                    dividers.push(ElementSurface {
                        index: usize::MAX,
                        bounds: cmd.bounds,
                        fill_color: Some(*color),
                        border_widths: [0.0; 4],
                        border_color: None,
                        command_index: cmd_index,
                        draw_depth,
                        clip_rect: Some(active_clip),
                    });
                } else {
                    surfaces
                        .entry(cmd.element_idx)
                        .or_insert_with(|| ElementSurface {
                            index: cmd.element_idx,
                            bounds: cmd.bounds,
                            fill_color: None,
                            border_widths: [0.0; 4],
                            border_color: None,
                            command_index: cmd_index,
                            draw_depth,
                            clip_rect: Some(active_clip),
                        })
                        .fill_color = Some(*color);
                }
            },
            RenderCommandKind::Border { border } => {
                let Some(active_clip) = active_clip else {
                    continue;
                };
                let Some(draw_depth) = draw_order.depth_for(cmd_index) else {
                    continue;
                };
                let surface = surfaces
                    .entry(cmd.element_idx)
                    .or_insert_with(|| ElementSurface {
                        index: cmd.element_idx,
                        bounds: cmd.bounds,
                        fill_color: None,
                        border_widths: [0.0; 4],
                        border_color: None,
                        command_index: cmd_index,
                        draw_depth,
                        clip_rect: Some(active_clip),
                    });
                surface.border_widths = [
                    border.top.value,
                    border.right.value,
                    border.bottom.value,
                    border.left.value,
                ];
                surface.border_color = Some(border.color);
            },
            _ => {},
        }
    }

    GatheredCommands { surfaces, dividers }
}

/// Flattens the gathered surfaces and dividers into one list of surfaces to
/// render, each keyed by its `command_index`.
fn desired_surfaces(gathered: GatheredCommands) -> Vec<ElementSurface> {
    let mut desired: Vec<ElementSurface> = gathered.surfaces.into_values().collect();
    desired.extend(gathered.dividers);
    desired
}

/// An SDF quad ready to spawn or recolor: its material, mesh size, world
/// center, and the [`PanelSdfSurface`] signature describing what it was built
/// from.
struct BuiltSdfQuad {
    material:  SdfPanelMaterial,
    mesh_size: Vec2,
    center:    Vec2,
    signature: PanelSdfSurface,
}

/// Resolves a surface into a [`BuiltSdfQuad`] — pure computation, no asset or
/// entity mutation. The caller decides whether to spawn, recolor, or skip.
fn build_sdf_quad(
    surface: &ElementSurface,
    panel: &DiegeticPanel,
    points_to_world: f32,
    anchor_x: f32,
    anchor_y: f32,
) -> BuiltSdfQuad {
    let element_mat = panel.tree().element_material(surface.index);
    let corner_radius = panel.tree().element_corner_radius(surface.index);

    // Fill color from .background() or element .material() — never panel material.
    let effective_color = surface.fill_color.or_else(|| {
        if element_mat.is_some() {
            None
        } else {
            Some(Color::NONE)
        }
    });
    let mut base = super::resolve_material(element_mat, panel.material(), effective_color);
    base.depth_bias = surface.draw_depth.depth_bias().get();
    let fill_color = base.base_color;

    let world_width = surface.bounds.width * points_to_world;
    let world_height = surface.bounds.height * points_to_world;
    let world_radii = corner_radius
        .to_array()
        .map(|radius| radius * points_to_world);
    let world_borders = surface.border_widths.map(|width| width * points_to_world);

    let half_width = world_width * 0.5;
    let half_height = world_height * 0.5;

    // Mesh is slightly larger than the SDF form so the exterior
    // anti-aliasing ramp has fragments to render on.
    let pad = constants::SDF_AA_PADDING;
    let mesh_half_width = half_width + pad;
    let mesh_half_height = half_height + pad;

    // Convert layout-space clip rect to local quad coords (centered, Y-up).
    //
    // ── DO NOT REMOVE THE `± pad` EXPANSION BELOW ─────────────────────────
    //
    // Every element's mesh extends `SDF_AA_PADDING` beyond its SDF form so
    // the shader's `smoothstep` exterior AA ramp has fragments to render on.
    // The clip rect coming in from `clip::effective_clip` is bound to the
    // panel viewport (or a tighter scissor), which sits *at* the SDF form
    // boundary — i.e. exactly where the AA ramp needs to fade. Without the
    // padding here, the shader's `discard` (in `sdf_panel.wgsl`) throws away
    // every fragment in the AA region at panel/viewport edges, and the
    // visible boundary collapses to the underlying polygon edge — a hard
    // staircase with no MSAA to smooth it.
    //
    // Expanding the clip outward by `pad` lets the AA ramp survive while
    // still clipping anything that's actually past the panel by more than
    // 1mm of world space (the case the clip was added for: retained
    // children that overflow after a panel shrink). The two regions don't
    // conflict — they're separated by exactly `SDF_AA_PADDING`.
    //
    // If you find yourself "cleaning this up" because the `- pad` / `+ pad`
    // look redundant or suspicious: run the `units` example, look at the
    // ruler ticks, and you'll see the staircase return immediately.
    // ──────────────────────────────────────────────────────────────────────
    let clip_rect = surface.clip_rect.map_or_else(
        || {
            Vec4::new(
                -mesh_half_width,
                -mesh_half_height,
                mesh_half_width,
                mesh_half_height,
            )
        },
        |clip_rect| {
            let (cx, cy) = surface.bounds.center();
            let left = (clip_rect.x - cx) * points_to_world;
            let right = (clip_rect.x + clip_rect.width - cx) * points_to_world;
            // Layout Y-down → local Y-up.
            let top = -(clip_rect.y - cy) * points_to_world;
            let bottom = -(clip_rect.y + clip_rect.height - cy) * points_to_world;
            Vec4::new(
                left - pad,
                bottom.min(top) - pad,
                right + pad,
                bottom.max(top) + pad,
            )
        },
    );

    let material = sdf_material::sdf_panel_material(
        base,
        SdfPanelMaterialInput {
            half_size: Vec2::new(half_width, half_height),
            mesh_half_size: Vec2::new(mesh_half_width, mesh_half_height),
            corner_radii: world_radii,
            border_widths: world_borders,
            border_color: surface.border_color,
            clip_rect,
            oit_depth_offset: surface.draw_depth.oit_depth_offset().get(),
        },
    );

    let world_rect = bounds_to_world_rect(&surface.bounds, points_to_world, anchor_x, anchor_y);
    let mesh_size = Vec2::new(mesh_half_width * 2.0, mesh_half_height * 2.0);

    let signature = PanelSdfSurface {
        command_index: surface.command_index,
        draw_depth:    surface.draw_depth,
        center:        vec2_bits(world_rect.center),
        mesh_size:     vec2_bits(mesh_size),
        corner_radii:  array4_bits(world_radii),
        border_widths: array4_bits(world_borders),
        clip_rect:     vec4_bits(clip_rect),
        fill_color:    color_bits(fill_color),
        border_color:  surface.border_color.map_or([0; 4], color_bits),
    };

    BuiltSdfQuad {
        material,
        mesh_size,
        center: world_rect.center,
        signature,
    }
}

/// Spawns one SDF quad as a child of `panel_entity`, tagged with its signature.
fn spawn_sdf_quad(
    quad: BuiltSdfQuad,
    context: &PanelReconcileContext<'_>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<SdfPanelMaterial>,
    commands: &mut Commands,
) {
    let mesh = meshes.add(Rectangle::new(quad.mesh_size.x, quad.mesh_size.y));
    let material = materials.add(quad.material);
    let base_components = (
        PanelSdfMesh,
        quad.signature,
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_xyz(quad.center.x, quad.center.y, 0.0),
        context.layer.clone(),
    );
    match context.shadow_mode {
        ShadowMode::Suppressed => {
            commands
                .entity(context.panel_entity)
                .with_child((base_components, NotShadowCaster));
        },
        ShadowMode::Enabled => {
            commands
                .entity(context.panel_entity)
                .with_child(base_components);
        },
    }
}

/// Spawns the invisible full-panel interaction quad as a child of the panel.
fn spawn_interaction_mesh(
    interaction: PanelInteractionMesh,
    size: Vec2,
    center: Vec2,
    context: &PanelReconcileContext<'_>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    commands: &mut Commands,
) {
    let material = materials.add(StandardMaterial {
        base_color: Color::NONE,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        double_sided: true,
        cull_mode: None,
        depth_bias: -constants::LAYER_DEPTH_BIAS,
        ..default()
    });
    commands.entity(context.panel_entity).with_child((
        interaction,
        RayCastBackfaces,
        NotShadowCaster,
        Mesh3d(meshes.add(Rectangle::new(size.x, size.y))),
        MeshMaterial3d(material),
        Transform::from_xyz(center.x, center.y, 0.0),
        context.layer.clone(),
    ));
}

const fn vec2_bits(value: Vec2) -> [u32; 2] { [value.x.to_bits(), value.y.to_bits()] }

fn vec4_bits(value: Vec4) -> [u32; 4] {
    [
        value.x.to_bits(),
        value.y.to_bits(),
        value.z.to_bits(),
        value.w.to_bits(),
    ]
}

fn array4_bits(value: [f32; 4]) -> [u32; 4] { value.map(f32::to_bits) }

fn color_bits(color: Color) -> [u32; 4] {
    let linear = color.to_linear();
    [
        linear.red.to_bits(),
        linear.green.to_bits(),
        linear.blue.to_bits(),
        linear.alpha.to_bits(),
    ]
}

struct WorldRect {
    center: Vec2,
}

fn bounds_to_world_rect(
    bounds: &BoundingBox,
    points_to_world: f32,
    anchor_x: f32,
    anchor_y: f32,
) -> WorldRect {
    let width = bounds.width * points_to_world;
    let height = bounds.height * points_to_world;
    let left = bounds.x.mul_add(points_to_world, -anchor_x);
    let top = -(bounds.y.mul_add(points_to_world, -anchor_y));

    WorldRect {
        center: Vec2::new(width.mul_add(0.5, left), height.mul_add(-0.5, top)),
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use std::sync::Arc;

    use bevy::asset::AssetPlugin;

    use super::*;
    use crate::Mm;
    use crate::layout::El;
    use crate::layout::LayoutBuilder;
    use crate::layout::LayoutTree;
    use crate::layout::Sizing;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;
    use crate::layout::TextStyle;
    use crate::panel::DiegeticPanelCommands;
    use crate::panel::HeadlessLayoutPlugin;
    use crate::text::DiegeticTextMeasurer;

    #[test]
    fn per_level_band_overflows_at_screen_band_capacity() {
        let capacity = per_level_band_capacity();

        assert!(!per_level_band_overflows(capacity.saturating_sub(1)));
        assert!(per_level_band_overflows(capacity));
    }

    #[test]
    fn oit_total_overflows_at_depth_budget() {
        let budget = oit_depth_budget();

        assert!(!oit_total_overflows(budget.saturating_sub(1)));
        assert!(oit_total_overflows(budget));
    }

    /// Minimal measurer for geometry tests that include text commands.
    fn zero_measurer() -> DiegeticTextMeasurer {
        DiegeticTextMeasurer {
            measure_fn: Arc::new(|_: &str, measure: &TextMeasure| TextDimensions {
                width:       0.0,
                height:      measure.size,
                line_height: measure.size,
            }),
        }
    }

    fn geometry_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(AssetPlugin::default());
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();
        app.init_asset::<SdfPanelMaterial>();
        app.insert_resource(zero_measurer());
        app.add_plugins(HeadlessLayoutPlugin);
        app.add_systems(PostUpdate, build_panel_geometry);
        app
    }

    /// A full-size background with a full-size backed child on top: two
    /// overlapping backing surfaces at consecutive command indices, identical
    /// in geometry and color.
    fn stacked_backgrounds_tree() -> LayoutTree {
        let mut builder = LayoutBuilder::new(Mm(100.0), Mm(50.0));
        builder.with(
            El::new()
                .width(Sizing::GROW)
                .height(Sizing::GROW)
                .background(Color::WHITE),
            |builder| {
                builder.with(
                    El::new()
                        .width(Sizing::GROW)
                        .height(Sizing::GROW)
                        .background(Color::WHITE),
                    |_| {},
                );
            },
        );
        builder.build()
    }

    fn text_toggle_tree(include_text: bool) -> LayoutTree {
        let mut builder = LayoutBuilder::new(Mm(100.0), Mm(50.0));
        builder.with(
            El::new()
                .width(Sizing::GROW)
                .height(Sizing::GROW)
                .background(Color::WHITE),
            |builder| {
                if include_text {
                    builder.text("Alpha", TextStyle::new(10.0));
                }
            },
        );
        builder.build()
    }

    fn single_sdf_quad(app: &mut App) -> (Entity, PanelSdfSurface, f32) {
        let entries: Vec<(Entity, PanelSdfSurface, Handle<SdfPanelMaterial>)> = {
            let world = app.world_mut();
            let mut query =
                world.query::<(Entity, &PanelSdfSurface, &MeshMaterial3d<SdfPanelMaterial>)>();
            query
                .iter(world)
                .map(|(entity, surface, material)| (entity, *surface, material.0.clone()))
                .collect()
        };
        assert_eq!(entries.len(), 1, "expected exactly one SDF quad");
        let (entity, surface, material) = entries.into_iter().next().expect("one SDF quad exists");
        let oit_depth_offset = app
            .world()
            .resource::<Assets<SdfPanelMaterial>>()
            .get(&material)
            .expect("quad material exists")
            .extension
            .uniforms
            .oit_depth_offset;
        (entity, surface, oit_depth_offset)
    }

    #[test]
    fn text_toggle_updates_sdf_signature_oit_depth_offset() {
        let mut app = geometry_app();
        let panel = app
            .world_mut()
            .spawn(
                DiegeticPanel::world()
                    .size(Mm(100.0), Mm(50.0))
                    .with_tree(text_toggle_tree(true))
                    .build()
                    .expect("panel should build"),
            )
            .id();
        app.update();
        app.update();

        let (entity_before, surface_before, material_offset_before) = single_sdf_quad(&mut app);
        assert_eq!(
            surface_before.draw_depth.oit_depth_offset().get().to_bits(),
            (-constants::OIT_DEPTH_STEP).to_bits(),
        );
        assert_eq!(
            material_offset_before.to_bits(),
            surface_before.draw_depth.oit_depth_offset().get().to_bits(),
        );

        app.world_mut()
            .commands()
            .set_tree(panel, text_toggle_tree(false));
        app.update();
        app.update();

        let (entity_after, surface_after, material_offset_after) = single_sdf_quad(&mut app);
        assert_eq!(entity_after, entity_before);
        assert_ne!(
            surface_before.draw_depth.oit_depth_offset().get().to_bits(),
            surface_after.draw_depth.oit_depth_offset().get().to_bits(),
        );
        assert_eq!(
            surface_after.draw_depth.oit_depth_offset().get().to_bits(),
            0.0_f32.to_bits(),
        );
        assert_eq!(
            material_offset_after.to_bits(),
            surface_after.draw_depth.oit_depth_offset().get().to_bits(),
        );
    }

    #[test]
    fn overlapping_backings_order_the_same_on_sorted_and_oit_paths() {
        let mut app = geometry_app();
        app.world_mut().spawn(
            DiegeticPanel::world()
                .size(Mm(100.0), Mm(50.0))
                .with_tree(stacked_backgrounds_tree())
                .build()
                .expect("panel should build"),
        );
        app.update();
        app.update();

        let mut quads: Vec<(usize, SdfPanelMaterial)> = {
            let world = app.world_mut();
            let mut query = world.query::<(&PanelSdfSurface, &MeshMaterial3d<SdfPanelMaterial>)>();
            let pairs: Vec<(usize, Handle<SdfPanelMaterial>)> = query
                .iter(world)
                .map(|(surface, material)| (surface.command_index, material.0.clone()))
                .collect();
            let materials = world.resource::<Assets<SdfPanelMaterial>>();
            pairs
                .into_iter()
                .map(|(command_index, handle)| {
                    (
                        command_index,
                        materials
                            .get(&handle)
                            .expect("quad material exists")
                            .clone(),
                    )
                })
                .collect()
        };
        quads.sort_by_key(|(command_index, _)| *command_index);
        assert_eq!(quads.len(), 2, "two overlapping backing quads expected");

        let (below_index, below) = &quads[0];
        let (above_index, above) = &quads[1];
        assert!(below_index < above_index);

        // Both ordering mechanisms must put the higher command index in
        // front: sorted bias rises and OIT offset rises (reverse-Z, positive =
        // closer).
        assert!(below.base.depth_bias < above.base.depth_bias);
        assert_eq!(below.base.depth_bias.to_bits(), 0.0_f32.to_bits());
        assert_eq!(
            above.base.depth_bias.to_bits(),
            constants::LAYER_DEPTH_BIAS.to_bits()
        );
        assert!(
            below.extension.uniforms.oit_depth_offset < above.extension.uniforms.oit_depth_offset
        );
        assert_eq!(
            below.extension.uniforms.oit_depth_offset.to_bits(),
            0.0_f32.to_bits()
        );
        assert_eq!(
            above.extension.uniforms.oit_depth_offset.to_bits(),
            constants::OIT_DEPTH_STEP.to_bits()
        );

        // The two quads are geometry- and color-identical, so the materials
        // must differ only in the two ordering fields: a command index must
        // not move shadow- or shading-relevant state.
        assert_eq!(below.base.alpha_mode, above.base.alpha_mode);
        assert_eq!(below.base.base_color, above.base.base_color);
        assert_eq!(below.base.unlit, above.base.unlit);
        assert_eq!(below.base.double_sided, above.base.double_sided);
        assert_eq!(below.base.cull_mode, above.base.cull_mode);
        let below_uniforms = &below.extension.uniforms;
        let above_uniforms = &above.extension.uniforms;
        assert_eq!(below_uniforms.half_size, above_uniforms.half_size);
        assert_eq!(below_uniforms.corner_radii, above_uniforms.corner_radii);
        assert_eq!(below_uniforms.border_widths, above_uniforms.border_widths);
        assert_eq!(
            below_uniforms.fill_alpha.to_bits(),
            above_uniforms.fill_alpha.to_bits()
        );
        assert_eq!(below_uniforms.clip_rect, above_uniforms.clip_rect);
    }
}
