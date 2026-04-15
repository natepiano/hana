//! Low-level callout primitives.

use bevy::camera::visibility::RenderLayers;
use bevy::color::Color;
use bevy::light::NotShadowCaster;
use bevy::math::Quat;
use bevy::math::Vec3;
use bevy::math::Vec4;
use bevy::prelude::AlphaMode;
use bevy::prelude::Assets;
use bevy::prelude::Changed;
use bevy::prelude::Children;
use bevy::prelude::Commands;
use bevy::prelude::Component;
use bevy::prelude::Entity;
use bevy::prelude::GizmoAsset;
use bevy::prelude::Mesh;
use bevy::prelude::Mesh3d;
use bevy::prelude::MeshMaterial3d;
use bevy::prelude::Or;
use bevy::prelude::Query;
use bevy::prelude::Rectangle;
use bevy::prelude::ResMut;
use bevy::prelude::Transform;
use bevy::prelude::Visibility;
use bevy::prelude::With;
use bevy_kana::ToF32;
use bevy_kana::ToUsize;

use crate::plugin::SurfaceShadow;
use crate::render::LAYER_DEPTH_BIAS;
use crate::render::OIT_DEPTH_STEP;
use crate::render::SDF_AA_PADDING;
use crate::render::SdfPanelMaterial;
use crate::render::default_panel_material;
use crate::render::sdf_panel_material;

/// Visual style for arrow end caps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrowStyle {
    /// Open chevron made from two line segments.
    Open,
}

/// Decoration that can appear at either end of a [`CalloutLine`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalloutCap {
    /// No end cap.
    None,
    /// Arrow end cap with the given style.
    Arrow(ArrowStyle),
}

/// World-space/local-space callout line with configurable end caps.
///
/// The line is expressed in the entity's local space. If the entity is
/// parented or transformed, the rendered callout follows naturally.
#[derive(Component, Clone, Debug)]
pub struct CalloutLine {
    start:          Vec3,
    end:            Vec3,
    color:          Color,
    thickness:      f32,
    cap_size:       f32,
    start_inset:    f32,
    end_inset:      f32,
    start_cap:      CalloutCap,
    end_cap:        CalloutCap,
    surface_shadow: SurfaceShadow,
}

impl CalloutLine {
    /// Creates a new line from `start` to `end`.
    #[must_use]
    pub fn new(start: Vec3, end: Vec3) -> Self {
        Self {
            start,
            end,
            color: Color::WHITE,
            thickness: 0.002,
            cap_size: 0.008,
            start_inset: 0.0,
            end_inset: 0.0,
            start_cap: CalloutCap::None,
            end_cap: CalloutCap::None,
            surface_shadow: SurfaceShadow::Off,
        }
    }

    /// Sets the line color.
    #[must_use]
    pub const fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Sets the shaft thickness in world meters.
    #[must_use]
    pub const fn thickness(mut self, thickness: f32) -> Self {
        self.thickness = thickness;
        self
    }

    /// Sets the cap size in world meters.
    #[must_use]
    pub const fn cap_size(mut self, cap_size: f32) -> Self {
        self.cap_size = cap_size;
        self
    }

    /// Insets the start of the visible shaft inward from `start`.
    #[must_use]
    pub const fn start_inset(mut self, inset: f32) -> Self {
        self.start_inset = inset;
        self
    }

    /// Insets the end of the visible shaft inward from `end`.
    #[must_use]
    pub const fn end_inset(mut self, inset: f32) -> Self {
        self.end_inset = inset;
        self
    }

    /// Sets the cap at the start of the line.
    #[must_use]
    pub const fn start_cap(mut self, cap: CalloutCap) -> Self {
        self.start_cap = cap;
        self
    }

    /// Sets the cap at the end of the line.
    #[must_use]
    pub const fn end_cap(mut self, cap: CalloutCap) -> Self {
        self.end_cap = cap;
        self
    }

    /// Controls whether this callout contributes to shadows.
    #[must_use]
    pub const fn surface_shadow(mut self, mode: SurfaceShadow) -> Self {
        self.surface_shadow = mode;
        self
    }
}

/// Child marker for generated callout meshes.
#[derive(Component)]
pub(crate) struct CalloutVisual;

/// Spawns a callout-line entity under `parent`.
///
/// This is the simplest public entry point. The actual SDF mesh segments
/// are built by the callout rendering system.
pub fn spawn_callout_line(commands: &mut Commands, parent: Entity, line: &CalloutLine) {
    commands
        .entity(parent)
        .with_child((line.clone(), Transform::IDENTITY, Visibility::Inherited));
}

pub(crate) fn update_callout_lines(
    changed: Query<
        (
            Entity,
            &CalloutLine,
            Option<&RenderLayers>,
            Option<&Children>,
        ),
        Or<(Changed<CalloutLine>, Changed<RenderLayers>)>,
    >,
    old_visuals: Query<(), With<CalloutVisual>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut sdf_materials: ResMut<Assets<SdfPanelMaterial>>,
    mut commands: Commands,
) {
    for (entity, line, layers, children) in &changed {
        if let Some(children) = children {
            for child in children.iter() {
                if old_visuals.contains(*child) {
                    commands.entity(*child).despawn();
                }
            }
        }

        let delta = line.end - line.start;
        let len = delta.length();
        if len < f32::EPSILON {
            continue;
        }
        let dir = delta / len;
        let shaft_start = line.start + dir * line.start_inset;
        let shaft_end = line.end - dir * line.end_inset;
        let layer = layers.cloned().unwrap_or(RenderLayers::layer(0));
        let mut order = 0_u32;

        spawn_segment(
            &mut commands,
            entity,
            shaft_start,
            shaft_end,
            line.thickness,
            line.color,
            line.surface_shadow,
            &layer,
            order,
            &mut meshes,
            &mut sdf_materials,
        );
        order += 1;

        order = spawn_cap(
            &mut commands,
            entity,
            shaft_start,
            dir,
            line.start_cap,
            line.cap_size,
            line.thickness,
            line.color,
            line.surface_shadow,
            &layer,
            order,
            &mut meshes,
            &mut sdf_materials,
            true,
        );
        let _ = spawn_cap(
            &mut commands,
            entity,
            shaft_end,
            dir,
            line.end_cap,
            line.cap_size,
            line.thickness,
            line.color,
            line.surface_shadow,
            &layer,
            order,
            &mut meshes,
            &mut sdf_materials,
            false,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_cap(
    commands: &mut Commands,
    parent: Entity,
    tip: Vec3,
    dir: Vec3,
    cap: CalloutCap,
    cap_size: f32,
    thickness: f32,
    color: Color,
    shadow: SurfaceShadow,
    layer: &RenderLayers,
    mut order: u32,
    meshes: &mut Assets<Mesh>,
    sdf_materials: &mut Assets<SdfPanelMaterial>,
    is_start: bool,
) -> u32 {
    match cap {
        CalloutCap::None => order,
        CalloutCap::Arrow(ArrowStyle::Open) => {
            let shaft_dir = if is_start { dir } else { -dir };
            let perp = cap_perp(shaft_dir);
            for end in [
                tip + shaft_dir * cap_size + perp * cap_size,
                tip + shaft_dir * cap_size - perp * cap_size,
            ] {
                spawn_segment(
                    commands,
                    parent,
                    tip,
                    end,
                    thickness,
                    color,
                    shadow,
                    layer,
                    order,
                    meshes,
                    sdf_materials,
                );
                order += 1;
            }
            order
        },
    }
}

fn cap_perp(dir: Vec3) -> Vec3 {
    let reference = if dir.cross(Vec3::Z).length_squared() > 1e-6 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    dir.cross(reference).normalize()
}

#[allow(clippy::too_many_arguments)]
fn spawn_segment(
    commands: &mut Commands,
    parent: Entity,
    start: Vec3,
    end: Vec3,
    thickness: f32,
    color: Color,
    shadow: SurfaceShadow,
    layer: &RenderLayers,
    order: u32,
    meshes: &mut Assets<Mesh>,
    sdf_materials: &mut Assets<SdfPanelMaterial>,
) {
    let delta = end - start;
    let length = delta.length();
    if length < f32::EPSILON || thickness <= 0.0 {
        return;
    }

    let panel_height = line_panel_height(thickness);
    let half_w = length * 0.5;
    let half_h = panel_height * 0.5;
    let mesh_half_w = half_w + SDF_AA_PADDING;
    let mesh_half_h = half_h + SDF_AA_PADDING;

    let mut base = default_panel_material();
    base.base_color = Color::NONE;
    base.alpha_mode = AlphaMode::Blend;
    base.unlit = true;
    base.depth_bias = order.to_f32() * LAYER_DEPTH_BIAS;

    let material = sdf_panel_material(
        base,
        half_w,
        half_h,
        mesh_half_w,
        mesh_half_h,
        [0.0; 4],
        [thickness, 0.0, 0.0, 0.0],
        Some(color),
        Vec4::new(-mesh_half_w, -mesh_half_h, mesh_half_w, mesh_half_h),
        order.to_f32() * OIT_DEPTH_STEP,
    );
    let mesh = meshes.add(Rectangle::new(mesh_half_w * 2.0, mesh_half_h * 2.0));
    let material = sdf_materials.add(material);

    let mid = (start + end) * 0.5;
    let rotation = Quat::from_rotation_arc(Vec3::X, delta / length);
    let line_center_offset = rotation * Vec3::Y * -(panel_height * 0.5 - thickness * 0.5);
    let common = (
        CalloutVisual,
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_translation(mid + line_center_offset).with_rotation(rotation),
        layer.clone(),
    );

    match shadow {
        SurfaceShadow::Off => commands
            .entity(parent)
            .with_child((common, NotShadowCaster)),
        SurfaceShadow::On => commands.entity(parent).with_child(common),
    };
}

fn line_panel_height(thickness: f32) -> f32 { thickness * 8.0 }

/// Draws a double-headed dimension arrow into a gizmo asset.
pub(crate) fn draw_dimension_arrow(
    gizmo: &mut GizmoAsset,
    from: Vec3,
    to: Vec3,
    color: Color,
    head_size: f32,
    gap: f32,
) {
    let delta = to - from;
    let len = delta.length();
    if len < f32::EPSILON {
        return;
    }
    let dir = delta / len;
    let perp = Vec3::new(-dir.y, dir.x, 0.0);

    let tip_from = from + dir * gap;
    let tip_to = to - dir * gap;

    gizmo.line(tip_from, tip_to, color);
    gizmo.line(
        tip_from,
        tip_from + dir * head_size + perp * head_size,
        color,
    );
    gizmo.line(
        tip_from,
        tip_from + dir * head_size - perp * head_size,
        color,
    );
    gizmo.line(tip_to, tip_to - dir * head_size + perp * head_size, color);
    gizmo.line(tip_to, tip_to - dir * head_size - perp * head_size, color);
}

/// Draws a dashed line between two points. Dashes and gaps are
/// specified in world units along the line direction.
pub(crate) fn draw_dashed_line(
    gizmo: &mut GizmoAsset,
    start: Vec3,
    end: Vec3,
    dash_len: f32,
    gap_len: f32,
    color: Color,
) {
    let delta = end - start;
    let total_len = delta.length();
    if total_len < f32::EPSILON {
        return;
    }
    let dir = delta / total_len;
    let stride = dash_len + gap_len;
    let count = (total_len / stride).ceil().to_usize();
    for i in 0..count {
        let t = i.to_f32() * stride;
        let dash_end = (t + dash_len).min(total_len);
        gizmo.line(start + dir * t, start + dir * dash_end, color);
    }
}
