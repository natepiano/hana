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

use crate::panel::SurfaceShadow;
use crate::render;
use crate::render::LAYER_DEPTH_BIAS;
use crate::render::OIT_DEPTH_STEP;
use crate::render::SDF_AA_PADDING;
use crate::render::SdfPanelMaterial;

/// Visual style for arrow end caps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrowStyle {
    /// Open chevron made from two line segments.
    Open,
    /// Solid triangular arrowhead with a sharp point.
    Solid,
}

/// Arrow-cap configuration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArrowCap {
    style:  ArrowStyle,
    length: Option<f32>,
    width:  Option<f32>,
    color:  Option<Color>,
}

impl ArrowCap {
    /// Creates a default arrow cap.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            style:  ArrowStyle::Open,
            length: None,
            width:  None,
            color:  None,
        }
    }

    /// Uses the open chevron arrow style.
    #[must_use]
    pub const fn open(mut self) -> Self {
        self.style = ArrowStyle::Open;
        self
    }

    /// Uses the solid triangular arrow style.
    #[must_use]
    pub const fn solid(mut self) -> Self {
        self.style = ArrowStyle::Solid;
        self
    }

    /// Sets the cap length along the line direction.
    #[must_use]
    pub const fn length(mut self, length: f32) -> Self {
        self.length = Some(length);
        self
    }

    /// Sets the cap width across the line direction.
    #[must_use]
    pub const fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Overrides the cap color. Defaults to the line color.
    #[must_use]
    pub const fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

/// Circle-cap configuration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CircleCap {
    radius: Option<f32>,
    color:  Option<Color>,
}

impl CircleCap {
    /// Creates a default circle cap.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            radius: None,
            color:  None,
        }
    }

    /// Sets the circle radius.
    #[must_use]
    pub const fn radius(mut self, radius: f32) -> Self {
        self.radius = Some(radius);
        self
    }

    /// Overrides the cap color. Defaults to the line color.
    #[must_use]
    pub const fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

/// Square-cap configuration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SquareCap {
    size:  Option<f32>,
    color: Option<Color>,
}

impl SquareCap {
    /// Creates a default square cap.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            size:  None,
            color: None,
        }
    }

    /// Sets the full square size.
    #[must_use]
    pub const fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    /// Overrides the cap color. Defaults to the line color.
    #[must_use]
    pub const fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

/// Diamond-cap configuration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiamondCap {
    width:  Option<f32>,
    height: Option<f32>,
    color:  Option<Color>,
}

impl DiamondCap {
    /// Creates a default diamond cap.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            width:  None,
            height: None,
            color:  None,
        }
    }

    /// Sets the full diamond width.
    #[must_use]
    pub const fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Sets the full diamond height.
    #[must_use]
    pub const fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Overrides the cap color. Defaults to the line color.
    #[must_use]
    pub const fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

/// Decoration that can appear at either end of a [`CalloutLine`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CalloutCap {
    /// No end cap.
    None,
    /// Arrow end cap with the given style.
    Arrow(ArrowCap),
    /// Filled circular cap.
    Circle(CircleCap),
    /// Filled square cap.
    Square(SquareCap),
    /// Filled diamond cap.
    Diamond(DiamondCap),
}

impl CalloutCap {
    /// Creates an arrow cap with default open styling.
    #[must_use]
    pub const fn arrow() -> Self { Self::Arrow(ArrowCap::new()) }

    /// Creates a circular cap.
    #[must_use]
    pub const fn circle() -> Self { Self::Circle(CircleCap::new()) }

    /// Creates a square cap.
    #[must_use]
    pub const fn square() -> Self { Self::Square(SquareCap::new()) }

    /// Creates a diamond cap.
    #[must_use]
    pub const fn diamond() -> Self { Self::Diamond(DiamondCap::new()) }

    /// Sets an arrow cap to the open chevron style.
    #[must_use]
    pub const fn open(self) -> Self {
        match self {
            Self::Arrow(cap) => Self::Arrow(cap.open()),
            other => other,
        }
    }

    /// Sets an arrow cap to the solid triangular style.
    #[must_use]
    pub const fn solid(self) -> Self {
        match self {
            Self::Arrow(cap) => Self::Arrow(cap.solid()),
            other => other,
        }
    }

    /// Sets the cap length along the line direction.
    #[must_use]
    pub const fn length(self, length: f32) -> Self {
        match self {
            Self::Arrow(cap) => Self::Arrow(cap.length(length)),
            other => other,
        }
    }

    /// Sets the cap width across the line direction.
    #[must_use]
    pub const fn width(self, width: f32) -> Self {
        match self {
            Self::Arrow(cap) => Self::Arrow(cap.width(width)),
            Self::Diamond(cap) => Self::Diamond(cap.width(width)),
            other => other,
        }
    }

    /// Sets the cap height for shapes that support an explicit height.
    #[must_use]
    pub const fn height(self, height: f32) -> Self {
        match self {
            Self::Diamond(cap) => Self::Diamond(cap.height(height)),
            other => other,
        }
    }

    /// Sets the cap radius for circular caps.
    #[must_use]
    pub const fn radius(self, radius: f32) -> Self {
        match self {
            Self::Circle(cap) => Self::Circle(cap.radius(radius)),
            other => other,
        }
    }

    /// Sets the cap size for square caps.
    #[must_use]
    pub const fn size(self, size: f32) -> Self {
        match self {
            Self::Square(cap) => Self::Square(cap.size(size)),
            other => other,
        }
    }

    /// Overrides the cap color. Defaults to the callout line color.
    #[must_use]
    pub const fn color(self, color: Color) -> Self {
        match self {
            Self::Arrow(cap) => Self::Arrow(cap.color(color)),
            Self::Circle(cap) => Self::Circle(cap.color(color)),
            Self::Square(cap) => Self::Square(cap.color(color)),
            Self::Diamond(cap) => Self::Diamond(cap.color(color)),
            Self::None => Self::None,
        }
    }

    fn shaft_inset(self, default_size: f32) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Arrow(cap) => cap.length.unwrap_or(default_size),
            Self::Circle(cap) => cap.radius.unwrap_or(default_size * 0.5),
            Self::Square(cap) => cap.size.unwrap_or(default_size) * 0.5,
            Self::Diamond(cap) => cap.width.unwrap_or(default_size) * 0.5,
        }
    }

    fn resolved_color(self, fallback: Color) -> Color {
        match self {
            Self::None => fallback,
            Self::Arrow(cap) => cap.color.unwrap_or(fallback),
            Self::Circle(cap) => cap.color.unwrap_or(fallback),
            Self::Square(cap) => cap.color.unwrap_or(fallback),
            Self::Diamond(cap) => cap.color.unwrap_or(fallback),
        }
    }
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
    pub const fn new(start: Vec3, end: Vec3) -> Self {
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

pub fn update_callout_lines(
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
        let mut order = 0_u32;

        if (shaft_end - shaft_start).length_squared() > f32::EPSILON {
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
        }

        order = spawn_cap(
            &mut commands,
            entity,
            start_tip,
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
            end_tip,
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
    order: u32,
    meshes: &mut Assets<Mesh>,
    sdf_materials: &mut Assets<SdfPanelMaterial>,
    is_start: bool,
) -> u32 {
    let color = cap.resolved_color(color);
    let shape_dir = if is_start { dir } else { -dir };
    match cap {
        CalloutCap::Arrow(cap) if cap.style == ArrowStyle::Open => {
            let (length, width) = resolved_arrow_dimensions(cap, cap_size);
            spawn_open_arrow_cap(
                commands,
                parent,
                tip,
                shape_dir,
                length,
                width,
                thickness,
                color,
                shadow,
                layer,
                order,
                meshes,
                sdf_materials,
            )
        },
        CalloutCap::Arrow(cap) if cap.style == ArrowStyle::Solid => {
            let (length, width) = resolved_arrow_dimensions(cap, cap_size);
            spawn_single_shape_cap(
                commands,
                parent,
                tip,
                -shape_dir,
                CapShape::Triangle,
                length,
                width,
                thickness,
                color,
                shadow,
                layer,
                order,
                meshes,
                sdf_materials,
            )
        },
        CalloutCap::Circle(cap) => {
            let radius = cap.radius.unwrap_or(cap_size * 0.5);
            spawn_single_shape_cap(
                commands,
                parent,
                tip,
                shape_dir,
                CapShape::Circle,
                radius * 2.0,
                radius * 2.0,
                thickness,
                color,
                shadow,
                layer,
                order,
                meshes,
                sdf_materials,
            )
        },
        CalloutCap::Square(cap) => {
            let size = cap.size.unwrap_or(cap_size);
            spawn_single_shape_cap(
                commands,
                parent,
                tip,
                shape_dir,
                CapShape::Square,
                size,
                size,
                thickness,
                color,
                shadow,
                layer,
                order,
                meshes,
                sdf_materials,
            )
        },
        CalloutCap::Diamond(cap) => {
            let (width, height) = resolved_diamond_dimensions(cap, cap_size);
            spawn_single_shape_cap(
                commands,
                parent,
                tip,
                shape_dir,
                CapShape::Diamond,
                width,
                height,
                thickness,
                color,
                shadow,
                layer,
                order,
                meshes,
                sdf_materials,
            )
        },
        CalloutCap::None | CalloutCap::Arrow(_) => order,
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
    commands: &mut Commands,
    parent: Entity,
    tip: Vec3,
    shaft_dir: Vec3,
    length: f32,
    width: f32,
    thickness: f32,
    color: Color,
    shadow: SurfaceShadow,
    layer: &RenderLayers,
    mut order: u32,
    meshes: &mut Assets<Mesh>,
    sdf_materials: &mut Assets<SdfPanelMaterial>,
) -> u32 {
    let perp = cap_perp(shaft_dir);
    for end in [
        tip + shaft_dir * length + perp * width,
        tip + shaft_dir * length - perp * width,
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
}

fn spawn_single_shape_cap(
    commands: &mut Commands,
    parent: Entity,
    tip: Vec3,
    dir: Vec3,
    shape: CapShape,
    cap_width: f32,
    cap_height: f32,
    thickness: f32,
    color: Color,
    shadow: SurfaceShadow,
    layer: &RenderLayers,
    order: u32,
    meshes: &mut Assets<Mesh>,
    sdf_materials: &mut Assets<SdfPanelMaterial>,
) -> u32 {
    spawn_cap_shape(
        commands,
        parent,
        tip,
        dir,
        shape,
        cap_width,
        cap_height,
        thickness,
        color,
        shadow,
        layer,
        order,
        meshes,
        sdf_materials,
    );
    order + 1
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

    let half_w = length * 0.5;
    let hidden_half_h = thickness * 4.0;
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
        half_w,
        half_h,
        mesh_half_w,
        mesh_half_h,
        [0.0; 4],
        [0.0, 0.0, thickness, 0.0],
        Some(color),
        Vec4::new(-mesh_half_w, -mesh_half_h, mesh_half_w, mesh_half_h),
        order.to_f32() * OIT_DEPTH_STEP,
    );
    let mesh = meshes.add(Rectangle::new(mesh_half_w * 2.0, mesh_half_h * 2.0));
    let material = sdf_materials.add(material);

    let mid = (start + end) * 0.5;
    let rotation = Quat::from_rotation_arc(Vec3::X, delta / length);
    let line_center_offset = rotation * Vec3::Y * thickness.mul_add(-0.5, half_h);
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

fn spawn_cap_shape(
    commands: &mut Commands,
    parent: Entity,
    tip: Vec3,
    dir: Vec3,
    shape: CapShape,
    cap_width: f32,
    cap_height: f32,
    _thickness: f32,
    color: Color,
    shadow: SurfaceShadow,
    layer: &RenderLayers,
    order: u32,
    meshes: &mut Assets<Mesh>,
    sdf_materials: &mut Assets<SdfPanelMaterial>,
) {
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
        half_w,
        half_h,
        mesh_half_w,
        mesh_half_h,
        [0.0; 4],
        [0.0; 4],
        None,
        shape.sdf_kind(),
        shape_params,
        Vec4::new(-mesh_half_w, -mesh_half_h, mesh_half_w, mesh_half_h),
        order.to_f32() * OIT_DEPTH_STEP,
    );
    let mesh = meshes.add(Rectangle::new(mesh_half_w * 2.0, mesh_half_h * 2.0));
    let material = sdf_materials.add(material);
    let rotation = Quat::from_rotation_arc(Vec3::X, dir);
    let center = tip - rotation * Vec3::X * half_w;

    let common = (
        CalloutVisual,
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::from_translation(center).with_rotation(rotation),
        layer.clone(),
    );

    match shadow {
        SurfaceShadow::Off => commands
            .entity(parent)
            .with_child((common, NotShadowCaster)),
        SurfaceShadow::On => commands.entity(parent).with_child(common),
    };
}

/// Draws a double-headed dimension arrow into a gizmo asset.
pub fn draw_dimension_arrow(
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
