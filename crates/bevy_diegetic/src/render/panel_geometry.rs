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

use super::CommandIndex;
use super::ElementIndex;
use super::PanelChildSystems;
use super::clip;
use super::constants;
use super::constants::SDF_STROKE_SHADER_HANDLE;
use super::draw_order::DrawCommandDepth;
use super::draw_order::DrawOrderProjection;
use super::material_table::MaterialTableAppendReady;
use super::sdf_material;
use super::sdf_material::LegacySdfExtendedMaterial;
use super::sdf_material::LegacySdfExtendedMaterialInput;
use crate::layout::BoundingBox;
use crate::layout::RectangleSource;
use crate::layout::RenderCommand;
use crate::layout::RenderCommandKind;
use crate::panel::ComputedDiegeticPanel;
use crate::panel::DiegeticPanel;
use crate::panel::DiegeticPerfStats;
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
    command_index: CommandIndex,
    /// Projected ordering values used by the quad's material.
    draw_depth:    DrawCommandDepth,
    /// Panel-local transform center (x, y).
    center:        [u32; 2],
    /// Panel-local mesh size (full width, height).
    mesh_size:     [u32; 2],
    /// Panel-local per-corner radii [TL, TR, BR, BL].
    corner_radii:  [u32; 4],
    /// Panel-local border widths [top, right, bottom, left].
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
        app.init_resource::<ResolvedSdfSurfaceRegistry>()
            .add_plugins(MaterialPlugin::<LegacySdfExtendedMaterial>::default());
        app.add_systems(
            PostUpdate,
            build_panel_geometry
                .in_set(PanelChildSystems::Build)
                .before(MaterialTableAppendReady),
        );
        app.add_systems(Last, update_panel_geometry_perf_stats);
    }
}

fn update_panel_geometry_perf_stats(
    sdf_quads: Query<(), With<PanelSdfMesh>>,
    mut perf: ResMut<DiegeticPerfStats>,
) {
    perf.panel_geometry.sdf_quads = sdf_quads.iter().count();
}

/// Gathered fill + border data for a single element.
pub(crate) struct ElementSurface {
    /// `Element` index in the layout tree.
    index:         ElementIndex,
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
    command_index: CommandIndex,
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
        &MeshMaterial3d<LegacySdfExtendedMaterial>,
    )>,
    old_interaction: Query<(Entity, &ChildOf, &PanelInteractionMesh)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut standard_materials: ResMut<Assets<StandardMaterial>>,
    mut sdf_materials: ResMut<Assets<LegacySdfExtendedMaterial>>,
    mut resolved_surfaces: ResMut<ResolvedSdfSurfaceRegistry>,
    mut commands: Commands,
) {
    for (panel_entity, panel, computed, panel_layers) in &changed_panels {
        let Some(result) = computed.result() else {
            resolved_surfaces.remove_panel(panel_entity);
            continue;
        };

        let (anchor_x, anchor_y) = panel.anchor_offsets();
        let context = PanelReconcileContext {
            panel_entity,
            panel,
            points_to_world: panel.points_to_world(),
            anchor_x,
            anchor_y,
            surface_shadow: panel.surface_shadow(),
            layer: panel_layers.cloned().unwrap_or(RenderLayers::layer(0)),
        };
        let gathered = gather_surfaces(context.panel, &result.commands, computed.draw_order());
        let desired = desired_surfaces(gathered);
        let mut resolved: Vec<ResolvedSdfSurface<'_>> = desired
            .iter()
            .map(|surface| resolve_sdf_surface(surface, &context))
            .collect();
        resolved.sort_by(|left, right| {
            left.draw_depth
                .ordinal_index()
                .cmp(&right.draw_depth.ordinal_index())
                .then(left.command_index.cmp(&right.command_index))
        });
        resolved_surfaces.upsert_panel(
            panel_entity,
            resolved
                .iter()
                .map(StoredResolvedSdfSurface::from_resolved)
                .collect(),
        );

        reconcile_sdf_quads(
            &context,
            &resolved,
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
pub(crate) struct PanelReconcileContext<'a> {
    /// Panel entity that owns the resolved SDF surfaces.
    pub(crate) panel_entity:    Entity,
    /// Source panel that provides layout scale, materials, and shadow policy.
    pub(crate) panel:           &'a DiegeticPanel,
    /// Conversion factor from layout points to panel-local world units.
    pub(crate) points_to_world: f32,
    /// Panel-local horizontal anchor offset.
    pub(crate) anchor_x:        f32,
    /// Panel-local vertical anchor offset.
    pub(crate) anchor_y:        f32,
    /// Surface shadow policy copied onto each resolved SDF surface.
    pub(crate) surface_shadow:  SurfaceShadow,
    /// Render layers copied onto each resolved SDF surface.
    pub(crate) layer:           RenderLayers,
}

/// Reconciles a panel's SDF quads against its render commands: identical
/// surfaces stay untouched, a color-only difference recolors the material in
/// place, and a geometry difference (or a new/removed surface) respawns just
/// that quad.
fn reconcile_sdf_quads(
    context: &PanelReconcileContext<'_>,
    desired: &[ResolvedSdfSurface<'_>],
    old_sdf: &Query<(
        Entity,
        &ChildOf,
        &PanelSdfSurface,
        &MeshMaterial3d<LegacySdfExtendedMaterial>,
    )>,
    meshes: &mut Assets<Mesh>,
    sdf_materials: &mut Assets<LegacySdfExtendedMaterial>,
    commands: &mut Commands,
) {
    let mut existing: HashMap<
        CommandIndex,
        (Entity, PanelSdfSurface, Handle<LegacySdfExtendedMaterial>),
    > = old_sdf
        .iter()
        .filter(|(_, child_of, ..)| child_of.parent() == context.panel_entity)
        .map(|(entity, _, signature, material)| {
            (
                signature.command_index,
                (entity, *signature, material.0.clone()),
            )
        })
        .collect();

    for surface in desired {
        let quad = build_sdf_quad_from_resolved(surface);
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
                spawn_sdf_quad(quad, meshes, sdf_materials, commands);
            },
            None => {
                spawn_sdf_quad(quad, meshes, sdf_materials, commands);
            },
        }
    }

    // Any quad whose surface disappeared between builds is now stale.
    for (_, (entity, ..)) in existing {
        commands.entity(entity).despawn();
    }
}

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
    surfaces: HashMap<ElementIndex, ElementSurface>,
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
    let mut surfaces: HashMap<ElementIndex, ElementSurface> = HashMap::new();
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
                let command_index = CommandIndex::from(cmd_index);
                if *source == RectangleSource::ChildDivider {
                    dividers.push(ElementSurface {
                        index: ElementIndex::CHILD_DIVIDER,
                        bounds: cmd.bounds,
                        fill_color: Some(*color),
                        border_widths: [0.0; 4],
                        border_color: None,
                        command_index,
                        draw_depth,
                        clip_rect: Some(active_clip),
                    });
                } else {
                    let element_index = ElementIndex::from(cmd.element_idx);
                    surfaces
                        .entry(element_index)
                        .or_insert_with(|| ElementSurface {
                            index: element_index,
                            bounds: cmd.bounds,
                            fill_color: None,
                            border_widths: [0.0; 4],
                            border_color: None,
                            command_index,
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
                let command_index = CommandIndex::from(cmd_index);
                let element_index = ElementIndex::from(cmd.element_idx);
                let surface = surfaces
                    .entry(element_index)
                    .or_insert_with(|| ElementSurface {
                        index: element_index,
                        bounds: cmd.bounds,
                        fill_color: None,
                        border_widths: [0.0; 4],
                        border_color: None,
                        command_index,
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

/// Borrowed `StandardMaterial` source plus the layout color override for one
/// SDF material slot.
pub(crate) struct ResolvedSdfMaterial<'a> {
    /// Whether the fill or border role was authored by the layout surface.
    pub(crate) authored:      bool,
    /// Element or panel material used as the `StandardMaterial` base; `None`
    /// connects this slot to `default_panel_material()`.
    pub(crate) base_material: Option<&'a StandardMaterial>,
    /// Layout color applied to `StandardMaterial::base_color` when the old
    /// adapter or SDF frame table builds the owned material.
    pub(crate) color:         Option<Color>,
}

impl ResolvedSdfMaterial<'_> {
    fn to_standard_material(&self) -> StandardMaterial {
        let mut material = self
            .base_material
            .cloned()
            .unwrap_or_else(super::default_panel_material);

        if let Some(color) = self.color {
            material.base_color = color;
        }

        material
    }
}

/// Render-neutral resolved SDF surface shared by the old quad adapter and the
/// fill batch builder.
pub(crate) struct ResolvedSdfSurface<'a> {
    /// Entity of the `DiegeticPanel` that owns this surface.
    pub(crate) panel_entity:    Entity,
    /// Command identity inside the panel's `LayoutResult::commands` stream.
    pub(crate) command_index:   CommandIndex,
    /// Full draw-depth projection for sorted and OIT ordering.
    pub(crate) draw_depth:      DrawCommandDepth,
    /// Fill material input consumed by `sdf_material::sdf_panel_material` and
    /// the SDF material table.
    pub(crate) fill_material:   ResolvedSdfMaterial<'a>,
    /// Border material input consumed by the SDF uniform builder and the SDF
    /// material table.
    pub(crate) border_material: ResolvedSdfMaterial<'a>,
    /// Panel-local center of the SDF mesh child.
    pub(crate) local_center:    Vec2,
    /// Panel-local transform inserted on the old `PanelSdfMesh` child.
    pub(crate) local_transform: Transform,
    /// Panel-local half size of the SDF form.
    pub(crate) sdf_half_size:   Vec2,
    /// Panel-local half size of the quad mesh, including `SDF_AA_PADDING`.
    pub(crate) mesh_half_size:  Vec2,
    /// Panel-local per-corner radii [TL, TR, BR, BL].
    pub(crate) corner_radii:    [f32; 4],
    /// Panel-local border widths [top, right, bottom, left].
    pub(crate) border_widths:   [f32; 4],
    /// Panel-local clip rect [left, bottom, right, top] after padding
    /// expansion.
    pub(crate) clip_rect:       Vec4,
    /// Render layers copied to the old child and future fill batch records.
    pub(crate) render_layers:   RenderLayers,
    /// Surface shadow policy copied to the old child and future fill batch
    /// records.
    pub(crate) surface_shadow:  SurfaceShadow,
    /// `SdfPanelUniform::sdf_kind` selector for this surface.
    pub(crate) sdf_kind:        u32,
    /// `SdfPanelUniform::sdf_params` payload for this surface.
    pub(crate) sdf_params:      Vec4,
}

/// Owned current-frame SDF material source retained for the private batch route.
#[derive(Clone)]
pub(crate) struct StoredResolvedSdfMaterial {
    /// Whether this fill or border role was authored by the layout surface.
    authored:      bool,
    /// Cloned material source used when appending the frame material table.
    base_material: Option<StandardMaterial>,
    /// Layout color applied to `StandardMaterial::base_color`.
    color:         Option<Color>,
}

impl StoredResolvedSdfMaterial {
    fn from_resolved(material: &ResolvedSdfMaterial<'_>) -> Self {
        Self {
            authored:      material.authored,
            base_material: material.base_material.cloned(),
            color:         material.color,
        }
    }

    const fn as_resolved(&self) -> ResolvedSdfMaterial<'_> {
        ResolvedSdfMaterial {
            authored:      self.authored,
            base_material: self.base_material.as_ref(),
            color:         self.color,
        }
    }
}

/// Owned resolved SDF surface retained while `ComputedDiegeticPanel` is current.
#[derive(Clone)]
pub(crate) struct StoredResolvedSdfSurface {
    /// Entity of the `DiegeticPanel` that owns this surface.
    panel_entity:    Entity,
    /// Command identity inside the panel's `LayoutResult::commands` stream.
    command_index:   CommandIndex,
    /// Full draw-depth projection for sorted and OIT ordering.
    draw_depth:      DrawCommandDepth,
    /// Fill material source for the SDF material table.
    fill_material:   StoredResolvedSdfMaterial,
    /// Border material source for the SDF material table.
    border_material: StoredResolvedSdfMaterial,
    /// Panel-local center of the SDF mesh.
    local_center:    Vec2,
    /// Panel-local transform for this surface.
    local_transform: Transform,
    /// Panel-local half size of the SDF form.
    sdf_half_size:   Vec2,
    /// Panel-local half size of the quad mesh, including `SDF_AA_PADDING`.
    mesh_half_size:  Vec2,
    /// Panel-local per-corner radii [TL, TR, BR, BL].
    corner_radii:    [f32; 4],
    /// Panel-local border widths [top, right, bottom, left].
    border_widths:   [f32; 4],
    /// Panel-local clip rect [left, bottom, right, top].
    clip_rect:       Vec4,
    /// `SdfPanelUniform::sdf_kind` selector for this surface.
    sdf_kind:        u32,
    /// `SdfPanelUniform::sdf_params` payload for this surface.
    sdf_params:      Vec4,
}

impl StoredResolvedSdfSurface {
    fn from_resolved(surface: &ResolvedSdfSurface<'_>) -> Self {
        Self {
            panel_entity:    surface.panel_entity,
            command_index:   surface.command_index,
            draw_depth:      surface.draw_depth,
            fill_material:   StoredResolvedSdfMaterial::from_resolved(&surface.fill_material),
            border_material: StoredResolvedSdfMaterial::from_resolved(&surface.border_material),
            local_center:    surface.local_center,
            local_transform: surface.local_transform,
            sdf_half_size:   surface.sdf_half_size,
            mesh_half_size:  surface.mesh_half_size,
            corner_radii:    surface.corner_radii,
            border_widths:   surface.border_widths,
            clip_rect:       surface.clip_rect,
            sdf_kind:        surface.sdf_kind,
            sdf_params:      surface.sdf_params,
        }
    }

    /// Returns the owning `DiegeticPanel` entity.
    pub(crate) const fn panel_entity(&self) -> Entity { self.panel_entity }

    /// Borrows the stored surface with current panel render state.
    pub(crate) const fn as_resolved(
        &self,
        render_layers: RenderLayers,
        surface_shadow: SurfaceShadow,
    ) -> ResolvedSdfSurface<'_> {
        ResolvedSdfSurface {
            panel_entity: self.panel_entity,
            command_index: self.command_index,
            draw_depth: self.draw_depth,
            fill_material: self.fill_material.as_resolved(),
            border_material: self.border_material.as_resolved(),
            local_center: self.local_center,
            local_transform: self.local_transform,
            sdf_half_size: self.sdf_half_size,
            mesh_half_size: self.mesh_half_size,
            corner_radii: self.corner_radii,
            border_widths: self.border_widths,
            clip_rect: self.clip_rect,
            render_layers,
            surface_shadow,
            sdf_kind: self.sdf_kind,
            sdf_params: self.sdf_params,
        }
    }
}

/// Current resolved SDF surfaces keyed by owning panel for the private batch route.
#[derive(Default, Resource)]
pub(crate) struct ResolvedSdfSurfaceRegistry {
    surfaces_by_panel: HashMap<Entity, Vec<StoredResolvedSdfSurface>>,
}

impl ResolvedSdfSurfaceRegistry {
    /// Replaces the retained SDF surfaces for one panel.
    pub(crate) fn upsert_panel(
        &mut self,
        panel_entity: Entity,
        surfaces: Vec<StoredResolvedSdfSurface>,
    ) {
        if surfaces.is_empty() {
            self.remove_panel(panel_entity);
        } else {
            self.surfaces_by_panel.insert(panel_entity, surfaces);
        }
    }

    /// Removes retained SDF surfaces for one panel.
    pub(crate) fn remove_panel(&mut self, panel_entity: Entity) {
        self.surfaces_by_panel.remove(&panel_entity);
    }

    /// Iterates retained SDF surfaces in panel-bucket order.
    pub(crate) fn surfaces(&self) -> impl Iterator<Item = &StoredResolvedSdfSurface> {
        self.surfaces_by_panel.values().flatten()
    }
}

/// An SDF quad ready to spawn or recolor: its material, mesh size, world
/// center, and the [`PanelSdfSurface`] signature describing what it was built
/// from.
struct BuiltSdfQuad {
    /// Entity of the `DiegeticPanel` that owns the old `PanelSdfMesh` child.
    panel_entity:    Entity,
    /// Legacy extended material inserted on the old `PanelSdfMesh` child.
    material:        LegacySdfExtendedMaterial,
    /// Panel-local mesh size inserted into `Rectangle`.
    mesh_size:       Vec2,
    /// Panel-local transform inserted on the old `PanelSdfMesh` child.
    local_transform: Transform,
    /// Render layers inserted on the old `PanelSdfMesh` child.
    render_layers:   RenderLayers,
    /// Shadow policy inserted on the old `PanelSdfMesh` child.
    surface_shadow:  SurfaceShadow,
    /// Signature used by `reconcile_sdf_quads` to diff old and new children.
    signature:       PanelSdfSurface,
}

/// Resolves an [`ElementSurface`] into render-neutral SDF data.
///
/// This performs only pre-propagation panel-local math. `StandardMaterial`
/// ownership and `StandardMaterial::depth_bias` assignment stay in
/// [`build_sdf_quad_from_resolved`].
pub(crate) fn resolve_sdf_surface<'a>(
    surface: &ElementSurface,
    context: &PanelReconcileContext<'a>,
) -> ResolvedSdfSurface<'a> {
    let element_mat = context.panel.tree().element_material(surface.index.get());
    let base_material = element_mat.or_else(|| context.panel.material());

    // Fill color from .background() or element .material() — never panel material.
    let effective_color = surface.fill_color.or_else(|| {
        if element_mat.is_some() {
            None
        } else {
            Some(Color::NONE)
        }
    });

    let local_width = surface.bounds.width * context.points_to_world;
    let local_height = surface.bounds.height * context.points_to_world;
    let corner_radii =
        panel_local_corner_radii(context.panel, surface.index, context.points_to_world);
    let border_widths = surface
        .border_widths
        .map(|width| width * context.points_to_world);

    let half_width = local_width * 0.5;
    let half_height = local_height * 0.5;

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
            let left = (clip_rect.x - cx) * context.points_to_world;
            let right = (clip_rect.x + clip_rect.width - cx) * context.points_to_world;
            // Layout Y-down → local Y-up.
            let top = -(clip_rect.y - cy) * context.points_to_world;
            let bottom = -(clip_rect.y + clip_rect.height - cy) * context.points_to_world;
            Vec4::new(
                left - pad,
                bottom.min(top) - pad,
                right + pad,
                bottom.max(top) + pad,
            )
        },
    );

    let local_center = bounds_to_panel_local_center(
        &surface.bounds,
        context.points_to_world,
        context.anchor_x,
        context.anchor_y,
    );

    ResolvedSdfSurface {
        panel_entity: context.panel_entity,
        command_index: surface.command_index,
        draw_depth: surface.draw_depth,
        fill_material: ResolvedSdfMaterial {
            authored: surface.fill_color.is_some() || element_mat.is_some(),
            base_material,
            color: effective_color,
        },
        border_material: ResolvedSdfMaterial {
            authored: surface.border_color.is_some(),
            base_material,
            color: surface.border_color,
        },
        local_center,
        local_transform: Transform::from_xyz(local_center.x, local_center.y, 0.0),
        sdf_half_size: Vec2::new(half_width, half_height),
        mesh_half_size: Vec2::new(mesh_half_width, mesh_half_height),
        corner_radii,
        border_widths,
        clip_rect,
        render_layers: context.layer.clone(),
        surface_shadow: context.surface_shadow,
        sdf_kind: constants::SDF_KIND_ROUNDED_RECT,
        sdf_params: constants::SDF_ROUNDED_RECT_PARAMS,
    }
}

/// Adapts a [`ResolvedSdfSurface`] to the old per-quad renderer.
fn build_sdf_quad_from_resolved(surface: &ResolvedSdfSurface<'_>) -> BuiltSdfQuad {
    let mut base = surface.fill_material.to_standard_material();
    base.depth_bias = surface.draw_depth.depth_bias().get();
    let fill_color = base.base_color;

    let material = sdf_material::sdf_panel_material(
        base,
        LegacySdfExtendedMaterialInput {
            half_size:        surface.sdf_half_size,
            mesh_half_size:   surface.mesh_half_size,
            corner_radii:     surface.corner_radii,
            border_widths:    surface.border_widths,
            border_color:     surface.border_material.color,
            sdf_kind:         surface.sdf_kind,
            sdf_params:       surface.sdf_params,
            clip_rect:        surface.clip_rect,
            oit_depth_offset: surface.draw_depth.oit_depth_offset().get(),
        },
    );

    let mesh_size = surface.mesh_half_size * 2.0;

    let signature = PanelSdfSurface {
        command_index: surface.command_index,
        draw_depth:    surface.draw_depth,
        center:        vec2_bits(surface.local_center),
        mesh_size:     vec2_bits(mesh_size),
        corner_radii:  array4_bits(surface.corner_radii),
        border_widths: array4_bits(surface.border_widths),
        clip_rect:     vec4_bits(surface.clip_rect),
        fill_color:    color_bits(fill_color),
        border_color:  surface.border_material.color.map_or([0; 4], color_bits),
    };

    BuiltSdfQuad {
        panel_entity: surface.panel_entity,
        material,
        mesh_size,
        local_transform: surface.local_transform,
        render_layers: surface.render_layers.clone(),
        surface_shadow: surface.surface_shadow,
        signature,
    }
}

fn panel_local_corner_radii(
    panel: &DiegeticPanel,
    element_index: ElementIndex,
    points_to_world: f32,
) -> [f32; 4] {
    panel
        .tree()
        .element_corner_radius(element_index.get())
        .resolved(panel.layout_unit().to_points())
        .to_array()
        .map(|radius| radius * points_to_world)
}

/// Spawns one SDF quad as a child of `panel_entity`, tagged with its signature.
fn spawn_sdf_quad(
    quad: BuiltSdfQuad,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<LegacySdfExtendedMaterial>,
    commands: &mut Commands,
) {
    let mesh = meshes.add(Rectangle::new(quad.mesh_size.x, quad.mesh_size.y));
    let material = materials.add(quad.material);
    let base_components = (
        PanelSdfMesh,
        quad.signature,
        Mesh3d(mesh),
        MeshMaterial3d(material),
        quad.local_transform,
        quad.render_layers,
    );
    match quad.surface_shadow {
        SurfaceShadow::Off => {
            commands
                .entity(quad.panel_entity)
                .with_child((base_components, NotShadowCaster));
        },
        SurfaceShadow::On => {
            commands
                .entity(quad.panel_entity)
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

fn bounds_to_panel_local_center(
    bounds: &BoundingBox,
    points_to_world: f32,
    anchor_x: f32,
    anchor_y: f32,
) -> Vec2 {
    let width = bounds.width * points_to_world;
    let height = bounds.height * points_to_world;
    let left = bounds.x.mul_add(points_to_world, -anchor_x);
    let top = -(bounds.y.mul_add(points_to_world, -anchor_y));

    Vec2::new(width.mul_add(0.5, left), height.mul_add(-0.5, top))
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
    use crate::layout::CornerRadius;
    use crate::layout::El;
    use crate::layout::LayoutBuilder;
    use crate::layout::LayoutTree;
    use crate::layout::Sizing;
    use crate::layout::TextDimensions;
    use crate::layout::TextMeasure;
    use crate::layout::TextStyle;
    use crate::layout::Unit;
    use crate::panel::DiegeticPanelCommands;
    use crate::panel::HeadlessLayoutPlugin;
    use crate::text::DiegeticTextMeasurer;

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
        app.init_asset::<LegacySdfExtendedMaterial>();
        app.init_resource::<ResolvedSdfSurfaceRegistry>();
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
        let entries: Vec<(Entity, PanelSdfSurface, Handle<LegacySdfExtendedMaterial>)> = {
            let world = app.world_mut();
            let mut query = world.query::<(
                Entity,
                &PanelSdfSurface,
                &MeshMaterial3d<LegacySdfExtendedMaterial>,
            )>();
            query
                .iter(world)
                .map(|(entity, surface, material)| (entity, *surface, material.0.clone()))
                .collect()
        };
        assert_eq!(entries.len(), 1, "expected exactly one SDF quad");
        let (entity, surface, material) = entries.into_iter().next().expect("one SDF quad exists");
        let oit_depth_offset = app
            .world()
            .resource::<Assets<LegacySdfExtendedMaterial>>()
            .get(&material)
            .expect("quad material exists")
            .extension
            .uniforms
            .oit_depth_offset;
        (entity, surface, oit_depth_offset)
    }

    fn single_sdf_material(app: &mut App) -> LegacySdfExtendedMaterial {
        let materials: Vec<Handle<LegacySdfExtendedMaterial>> = {
            let world = app.world_mut();
            let mut query = world.query::<&MeshMaterial3d<LegacySdfExtendedMaterial>>();
            query
                .iter(world)
                .map(|material| material.0.clone())
                .collect()
        };
        assert_eq!(materials.len(), 1, "expected exactly one SDF material");
        app.world()
            .resource::<Assets<LegacySdfExtendedMaterial>>()
            .get(&materials[0])
            .expect("quad material exists")
            .clone()
    }

    fn rounded_root_tree() -> LayoutTree {
        let builder = LayoutBuilder::with_root(
            El::new()
                .width(Sizing::GROW)
                .height(Sizing::GROW)
                .corner_radius(CornerRadius::all(Mm(4.0)))
                .background(Color::WHITE),
        );
        builder.build()
    }

    #[test]
    fn sdf_corner_radii_resolve_authored_units_before_world_scaling() {
        let mut app = geometry_app();
        app.world_mut().spawn(
            DiegeticPanel::world()
                .size(Mm(100.0), Mm(50.0))
                .world_height(0.5)
                .with_tree(rounded_root_tree())
                .build()
                .expect("panel should build"),
        );
        app.update();
        app.update();

        let material = single_sdf_material(&mut app);
        let points_to_world = 0.5 / (50.0 * Unit::Millimeters.to_points());
        let expected_radius = 4.0 * Unit::Millimeters.to_points() * points_to_world;
        let actual_radius = material.extension.uniforms.corner_radii.x;

        assert!((actual_radius - expected_radius).abs() < 0.001);
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

        let mut quads: Vec<(CommandIndex, PanelSdfSurface, LegacySdfExtendedMaterial)> = {
            let world = app.world_mut();
            let mut query =
                world.query::<(&PanelSdfSurface, &MeshMaterial3d<LegacySdfExtendedMaterial>)>();
            let pairs: Vec<(
                CommandIndex,
                PanelSdfSurface,
                Handle<LegacySdfExtendedMaterial>,
            )> = query
                .iter(world)
                .map(|(surface, material)| (surface.command_index, *surface, material.0.clone()))
                .collect();
            let materials = world.resource::<Assets<LegacySdfExtendedMaterial>>();
            pairs
                .into_iter()
                .map(|(command_index, surface, handle)| {
                    (
                        command_index,
                        surface,
                        materials
                            .get(&handle)
                            .expect("quad material exists")
                            .clone(),
                    )
                })
                .collect()
        };
        quads.sort_by_key(|(command_index, ..)| *command_index);
        assert_eq!(quads.len(), 2, "two overlapping backing quads expected");

        let (below_index, below_surface, below) = &quads[0];
        let (above_index, above_surface, above) = &quads[1];
        assert!(below_index < above_index);

        // Both ordering mechanisms must put the higher command index in
        // front: sorted bias rises and OIT offset rises (reverse-Z, positive =
        // closer).
        assert!(below.base.depth_bias < above.base.depth_bias);
        assert_eq!(
            below.base.depth_bias.to_bits(),
            below_surface.draw_depth.depth_bias().get().to_bits()
        );
        assert_eq!(
            above.base.depth_bias.to_bits(),
            above_surface.draw_depth.depth_bias().get().to_bits()
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
