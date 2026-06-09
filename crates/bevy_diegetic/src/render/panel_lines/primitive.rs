//! CPU/GPU records for batched panel-line primitives.

use bevy::math::Mat4;
use bevy::math::Vec2;
use bevy::math::Vec3;
use bevy::math::Vec4;
use bevy::math::Vec4Swizzles;
use bevy::prelude::Entity;
use bevy::render::render_resource::ShaderType;

use crate::layout::PanelLinePrimitiveKey;
use crate::layout::PanelLinePrimitiveKind;

/// Stable cross-panel source identity for one line primitive record.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct PanelLineRenderKey {
    /// Panel entity that owns the primitive source.
    pub panel:  Entity,
    /// Stable primitive key inside the panel's resolved command stream.
    pub source: PanelLinePrimitiveKey,
}

/// Coarse primitive class used as a batch split. The shader can draw all
/// variants, but segment strips and cap forms are different paint families.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum LinePrimitiveClass {
    Segment,
    Form,
}

impl From<PanelLinePrimitiveKind> for LinePrimitiveClass {
    fn from(kind: PanelLinePrimitiveKind) -> Self {
        match kind {
            PanelLinePrimitiveKind::Segment => Self::Segment,
            PanelLinePrimitiveKind::Triangle
            | PanelLinePrimitiveKind::Circle
            | PanelLinePrimitiveKind::Square
            | PanelLinePrimitiveKind::Diamond => Self::Form,
        }
    }
}

/// One storage-buffer record consumed by `panel_line_batch.wgsl`.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub(super) struct PanelLineGpuRecord {
    /// Local primitive quad-to-world transform.
    pub transform:            Mat4,
    /// `xy` = mesh half-size, `z` = SDF kind, `w` = sorted depth nudge.
    pub mesh_half_kind_depth: Vec4,
    /// `xy` = shape half-size, `z` = OIT depth offset, `w` unused.
    pub shape_oit:            Vec4,
    /// Local clip rect `[left, bottom, right, top]`.
    pub clip_rect:            Vec4,
    /// Linear primitive color.
    pub color:                Vec4,
    /// Shape-specific SDF parameters.
    pub params:               Vec4,
}

impl Default for PanelLineGpuRecord {
    fn default() -> Self {
        Self {
            transform:            Mat4::ZERO,
            mesh_half_kind_depth: Vec4::ZERO,
            shape_oit:            Vec4::ZERO,
            clip_rect:            Vec4::ZERO,
            color:                Vec4::ZERO,
            params:               Vec4::ZERO,
        }
    }
}

impl PanelLineGpuRecord {
    /// Mesh half-size in local panel units.
    #[must_use]
    pub(super) fn mesh_half_size(self) -> Vec2 { self.mesh_half_kind_depth.xy() }

    /// World-space bounds over the transformed quad corners.
    #[must_use]
    pub(super) fn world_bounds(self) -> (Vec3, Vec3) {
        let half = self.mesh_half_size();
        let mut min = Vec3::MAX;
        let mut max = Vec3::MIN;
        for local in [
            Vec2::new(-half.x, -half.y),
            Vec2::new(half.x, -half.y),
            Vec2::new(half.x, half.y),
            Vec2::new(-half.x, half.y),
        ] {
            let world = self
                .transform
                .transform_point3(Vec3::new(local.x, local.y, 0.0));
            min = min.min(world);
            max = max.max(world);
        }
        (min, max)
    }
}
