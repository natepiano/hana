//! Low-level callout primitives.

use bevy::color::Color;
use bevy::math::Quat;
use bevy::math::Vec3;
use bevy::pbr::StandardMaterial;
use bevy::prelude::AlphaMode;
use bevy::prelude::Commands;
use bevy::prelude::Entity;
use bevy::prelude::GizmoAsset;
use bevy::prelude::Transform;
use bevy_kana::ToF32;
use bevy_kana::ToUsize;

use crate::Anchor;
use crate::DiegeticPanel;
use crate::El;
use crate::LayoutBuilder;
use crate::default_panel_material;

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

/// World-space callout line with configurable end caps.
#[derive(Clone, Debug)]
pub struct CalloutLine {
    start:       Vec3,
    end:         Vec3,
    color:       Color,
    thickness:   f32,
    cap_size:    f32,
    start_inset: f32,
    end_inset:   f32,
    start_cap:   CalloutCap,
    end_cap:     CalloutCap,
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
}

/// Spawns a panel-backed callout line as children of `parent`.
pub fn spawn_callout_line(commands: &mut Commands, parent: Entity, line: &CalloutLine) {
    let delta = line.end - line.start;
    let len = delta.length();
    if len < f32::EPSILON {
        return;
    }
    let dir = delta / len;
    let shaft_start = line.start + dir * line.start_inset;
    let shaft_end = line.end - dir * line.end_inset;

    let material = callout_material();
    spawn_segment(
        commands,
        parent,
        shaft_start,
        shaft_end,
        line.thickness,
        line.color,
        &material,
    );

    spawn_cap(
        commands,
        parent,
        shaft_start,
        dir,
        line.start_cap,
        line.cap_size,
        line.thickness,
        line.color,
        &material,
        true,
    );
    spawn_cap(
        commands,
        parent,
        shaft_end,
        dir,
        line.end_cap,
        line.cap_size,
        line.thickness,
        line.color,
        &material,
        false,
    );
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
    material: &StandardMaterial,
    is_start: bool,
) {
    match cap {
        CalloutCap::None => {},
        CalloutCap::Arrow(ArrowStyle::Open) => {
            let shaft_dir = if is_start { dir } else { -dir };
            let perp = cap_perp(shaft_dir);
            spawn_segment(
                commands,
                parent,
                tip,
                tip + shaft_dir * cap_size + perp * cap_size,
                thickness,
                color,
                material,
            );
            spawn_segment(
                commands,
                parent,
                tip,
                tip + shaft_dir * cap_size - perp * cap_size,
                thickness,
                color,
                material,
            );
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

fn spawn_segment(
    commands: &mut Commands,
    parent: Entity,
    start: Vec3,
    end: Vec3,
    thickness: f32,
    color: Color,
    material: &StandardMaterial,
) {
    let delta = end - start;
    let length = delta.length();
    if length < f32::EPSILON || thickness <= 0.0 {
        return;
    }

    let panel_height = line_panel_height(thickness);
    let mid = (start + end) * 0.5;
    let rotation = Quat::from_rotation_arc(Vec3::X, delta / length);
    let line_center_offset = rotation * Vec3::Y * -(panel_height * 0.5 - thickness * 0.5);
    let tree = line_tree(length, panel_height, thickness, color);

    commands.entity(parent).with_child((
        DiegeticPanel::world()
            .size(length, panel_height)
            .anchor(Anchor::Center)
            .material(material.clone())
            .with_tree(tree)
            .build()
            .expect("callout line segment uses valid dimensions"),
        Transform::from_translation(mid + line_center_offset).with_rotation(rotation),
    ));
}

fn line_tree(length: f32, panel_height: f32, thickness: f32, color: Color) -> crate::LayoutTree {
    LayoutBuilder::with_root(
        El::new()
            .size(length, panel_height)
            .border(crate::Border::default().top(thickness).color(color)),
    )
    .build()
}

fn line_panel_height(thickness: f32) -> f32 { thickness * 8.0 }

fn callout_material() -> StandardMaterial {
    let mut material = default_panel_material();
    material.base_color = Color::NONE;
    material.alpha_mode = AlphaMode::Blend;
    material.unlit = true;
    material
}

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
