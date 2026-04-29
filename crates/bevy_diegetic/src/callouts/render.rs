use bevy::camera::visibility::RenderLayers;
use bevy::color::Color;
use bevy::light::NotShadowCaster;
use bevy::math::Quat;
use bevy::math::Vec2;
use bevy::math::Vec3;
use bevy::math::Vec4;
use bevy::prelude::AlphaMode;
use bevy::prelude::Assets;
use bevy::prelude::Changed;
use bevy::prelude::Children;
use bevy::prelude::Commands;
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
use bevy::prelude::With;
use bevy_kana::ToF32;

use super::caps::ArrowCap;
use super::caps::ArrowStyle;
use super::caps::CalloutCap;
use super::caps::DiamondCap;
use super::constants::HIDDEN_HALF_HEIGHT_MULTIPLIER;
use super::line::CalloutLine;
use super::line::CalloutVisual;
use crate::panel::SurfaceShadow;
use crate::render;
use crate::render::LAYER_DEPTH_BIAS;
use crate::render::OIT_DEPTH_STEP;
use crate::render::SDF_AA_PADDING;
use crate::render::SdfPanelMaterial;

/// Shared rendering parameters that every callout spawn helper threads through.
/// Exists to keep helper argument lists under the "context struct when > 7
/// parameters" style threshold. Depth ordering lives on `order`, which each
/// leaf spawn advances so callers do not thread it by hand.
struct CalloutRender<'w, 's, 'a> {
    commands:       &'a mut Commands<'w, 's>,
    parent:         Entity,
    surface_shadow: SurfaceShadow,
    layer:          &'a RenderLayers,
    order:          u32,
    meshes:         &'a mut Assets<Mesh>,
    sdf_materials:  &'a mut Assets<SdfPanelMaterial>,
}

/// Visual stroke parameters shared across a callout cap and its adjoining
/// segments — cap footprint size, line thickness, and base color.
#[derive(Clone, Copy)]
struct CapStroke {
    cap_size:  f32,
    thickness: f32,
    color:     Color,
}

pub(super) fn update_callout_lines(
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
            for child in children {
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
        let start_tip = line.start + dir * line.start_inset;
        let end_tip = line.end - dir * line.end_inset;
        let shaft_start = start_tip + dir * line.start_cap.shaft_inset(line.cap_size);
        let shaft_end = end_tip - dir * line.end_cap.shaft_inset(line.cap_size);
        let layer = layers.cloned().unwrap_or(RenderLayers::layer(0));

        let mut ctx = CalloutRender {
            commands:       &mut commands,
            parent:         entity,
            surface_shadow: line.surface_shadow,
            layer:          &layer,
            order:          0,
            meshes:         &mut meshes,
            sdf_materials:  &mut sdf_materials,
        };

        if (shaft_end - shaft_start).length_squared() > f32::EPSILON {
            spawn_segment(&mut ctx, shaft_start, shaft_end, line.thickness, line.color);
        }

        let stroke = CapStroke {
            cap_size:  line.cap_size,
            thickness: line.thickness,
            color:     line.color,
        };
        spawn_cap(&mut ctx, start_tip, dir, line.start_cap, stroke);
        spawn_cap(&mut ctx, end_tip, -dir, line.end_cap, stroke);
    }
}

fn spawn_cap(
    ctx: &mut CalloutRender<'_, '_, '_>,
    tip: Vec3,
    dir: Vec3,
    cap: CalloutCap,
    stroke: CapStroke,
) {
    let color = cap.resolved_color(stroke.color);
    match cap {
        CalloutCap::Arrow(cap) if cap.style == ArrowStyle::Open => {
            let (length, width) = resolved_arrow_dimensions(cap, stroke.cap_size);
            spawn_open_arrow_cap(ctx, tip, dir, length, width, stroke.thickness, color);
        },
        CalloutCap::Arrow(cap) if cap.style == ArrowStyle::Solid => {
            let (length, width) = resolved_arrow_dimensions(cap, stroke.cap_size);
            spawn_cap_shape(ctx, tip, -dir, CapShape::Triangle, length, width, color);
        },
        CalloutCap::Circle(cap) => {
            let radius = cap.radius.unwrap_or(stroke.cap_size * 0.5);
            spawn_cap_shape(
                ctx,
                tip,
                dir,
                CapShape::Circle,
                radius * 2.0,
                radius * 2.0,
                color,
            );
        },
        CalloutCap::Square(cap) => {
            let size = cap.size.unwrap_or(stroke.cap_size);
            spawn_cap_shape(ctx, tip, dir, CapShape::Square, size, size, color);
        },
        CalloutCap::Diamond(cap) => {
            let (width, height) = resolved_diamond_dimensions(cap, stroke.cap_size);
            spawn_cap_shape(ctx, tip, dir, CapShape::Diamond, width, height, color);
        },
        CalloutCap::None | CalloutCap::Arrow(_) => {},
    }
}

fn resolved_arrow_dimensions(cap: ArrowCap, cap_size: f32) -> (f32, f32) {
    let length = cap.length.unwrap_or(cap_size);
    (length, cap.width.unwrap_or(length))
}

fn resolved_diamond_dimensions(cap: DiamondCap, cap_size: f32) -> (f32, f32) {
    let width = cap.width.unwrap_or(cap_size);
    (width, cap.height.unwrap_or(width))
}

fn spawn_open_arrow_cap(
    ctx: &mut CalloutRender<'_, '_, '_>,
    tip: Vec3,
    shaft_dir: Vec3,
    length: f32,
    width: f32,
    thickness: f32,
    color: Color,
) {
    let perp = cap_perp(shaft_dir);
    for end in [
        tip + shaft_dir * length + perp * width,
        tip + shaft_dir * length - perp * width,
    ] {
        spawn_segment(ctx, tip, end, thickness, color);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CapShape {
    Triangle,
    Circle,
    Square,
    Diamond,
}

impl CapShape {
    const fn sdf_kind(self) -> u32 {
        match self {
            Self::Triangle => 1,
            Self::Circle => 2,
            Self::Square => 0,
            Self::Diamond => 3,
        }
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

fn spawn_segment(
    ctx: &mut CalloutRender<'_, '_, '_>,
    start: Vec3,
    end: Vec3,
    thickness: f32,
    color: Color,
) {
    let order = ctx.order;
    ctx.order += 1;

    let delta = end - start;
    let length = delta.length();
    if length < f32::EPSILON || thickness <= 0.0 {
        return;
    }

    let half_w = length * 0.5;
    let hidden_half_h = thickness * HIDDEN_HALF_HEIGHT_MULTIPLIER;
    let half_h = thickness.mul_add(0.5, hidden_half_h);
    let mesh_half_w = half_w + SDF_AA_PADDING;
    let mesh_half_h = half_h + SDF_AA_PADDING;

    let mut base = render::default_panel_material();
    base.base_color = Color::NONE;
    base.alpha_mode = AlphaMode::Blend;
    base.unlit = true;
    base.depth_bias = order.to_f32() * LAYER_DEPTH_BIAS;

    let material = render::sdf_panel_material(
        base,
        render::SdfPanelMaterialInput {
            half_size:        Vec2::new(half_w, half_h),
            mesh_half_size:   Vec2::new(mesh_half_w, mesh_half_h),
            corner_radii:     [0.0; 4],
            border_widths:    [0.0, 0.0, thickness, 0.0],
            border_color:     Some(color),
            clip_rect:        Vec4::new(-mesh_half_w, -mesh_half_h, mesh_half_w, mesh_half_h),
            oit_depth_offset: order.to_f32() * OIT_DEPTH_STEP,
        },
    );
    let mesh = ctx
        .meshes
        .add(Rectangle::new(mesh_half_w * 2.0, mesh_half_h * 2.0));
    let material = ctx.sdf_materials.add(material);

    let mid = (start + end) * 0.5;
    let rotation = Quat::from_rotation_arc(Vec3::X, delta / length);
    let line_center_offset = rotation * Vec3::Y * thickness.mul_add(-0.5, half_h);
    let common = (
        CalloutVisual,
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_translation(mid + line_center_offset).with_rotation(rotation),
        ctx.layer.clone(),
    );

    match ctx.surface_shadow {
        SurfaceShadow::Off => ctx
            .commands
            .entity(ctx.parent)
            .with_child((common, NotShadowCaster)),
        SurfaceShadow::On => ctx.commands.entity(ctx.parent).with_child(common),
    };
}

fn spawn_cap_shape(
    ctx: &mut CalloutRender<'_, '_, '_>,
    tip: Vec3,
    dir: Vec3,
    shape: CapShape,
    cap_width: f32,
    cap_height: f32,
    color: Color,
) {
    let order = ctx.order;
    ctx.order += 1;

    if cap_width <= 0.0 || cap_height <= 0.0 {
        return;
    }

    let (half_w, half_h) = match shape {
        CapShape::Triangle => (cap_width, cap_height),
        CapShape::Circle | CapShape::Square | CapShape::Diamond => {
            (cap_width * 0.5, cap_height * 0.5)
        },
    };
    let mesh_half_w = half_w + SDF_AA_PADDING;
    let mesh_half_h = half_h + SDF_AA_PADDING;

    let mut base = render::default_panel_material();
    base.base_color = color;
    base.alpha_mode = AlphaMode::Blend;
    base.unlit = true;
    base.depth_bias = order.to_f32() * LAYER_DEPTH_BIAS;

    let shape_params = match shape {
        CapShape::Triangle => Vec4::new(cap_width * 0.08, 0.6, 0.0, 0.0),
        _ => Vec4::ZERO,
    };

    let material = render::sdf_shape_material(
        base,
        render::SdfShapeMaterialInput {
            half_size: Vec2::new(half_w, half_h),
            mesh_half_size: Vec2::new(mesh_half_w, mesh_half_h),
            corner_radii: [0.0; 4],
            border_widths: [0.0; 4],
            border_color: None,
            shape_kind: shape.sdf_kind(),
            shape_params,
            clip_rect: Vec4::new(-mesh_half_w, -mesh_half_h, mesh_half_w, mesh_half_h),
            oit_depth_offset: order.to_f32() * OIT_DEPTH_STEP,
        },
    );
    let mesh = ctx
        .meshes
        .add(Rectangle::new(mesh_half_w * 2.0, mesh_half_h * 2.0));
    let material = ctx.sdf_materials.add(material);
    let rotation = Quat::from_rotation_arc(Vec3::X, dir);
    let center = tip - rotation * Vec3::X * half_w;

    let common = (
        CalloutVisual,
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_translation(center).with_rotation(rotation),
        ctx.layer.clone(),
    );

    match ctx.surface_shadow {
        SurfaceShadow::Off => ctx
            .commands
            .entity(ctx.parent)
            .with_child((common, NotShadowCaster)),
        SurfaceShadow::On => ctx.commands.entity(ctx.parent).with_child(common),
    };
}

/// Draws a double-headed dimension arrow into a gizmo asset.
pub(super) fn draw_dimension_arrow(
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
