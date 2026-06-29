//! Projects panel render commands into draw-order values.
//!
//! The projection maps each `(DrawZIndex, DrawSortTier, command_index)` key from
//! a panel command stream to a screen [`ScreenDepthBias`] and an OIT
//! [`OitDepthOffset`].
//!
//! Each authored z-index owns one screen-sort band. The shared SDF surface
//! batch uses [`SDF_SURFACE_BATCH_SORT_ANCHOR`], panel draw commands start at
//! [`FIRST_COMMAND_SORT_OFFSET`] and have [`COMMAND_SORT_OFFSET_CAPACITY`]
//! reserved positions, and text uses [`TEXT_BATCH_SORT_ANCHOR`].
//!
//! [`OitDepthOffset`] is a panel-global ordinal span added to `position.z` and
//! packed into 24-bit depth. [`OIT_DEPTH_STEP`] keeps adjacent layers about 17
//! quanta apart, and the panel-global command total is bounded by
//! `OIT_FOCUS_DEPTH / OIT_DEPTH_STEP`.
//!
//! The ceilings are independent: per-level band occupancy for screen ordering,
//! and panel-global command total for OIT ordering.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use bevy_kana::ToF32;
use bevy_kana::ToU32;

use super::constants::COMMAND_SORT_OFFSET_CAPACITY;
use super::constants::DRAW_Z_INDEX_BAND_WIDTH;
use super::constants::FIRST_COMMAND_SORT_OFFSET;
use super::constants::LAYER_DEPTH_BIAS;
use super::constants::OIT_DEPTH_STEP;
use super::constants::SDF_SURFACE_BATCH_SORT_ANCHOR;
use super::constants::TEXT_BATCH_SORT_ANCHOR;
use crate::layout::DrawSortTier;
use crate::layout::DrawZIndex;
use crate::layout::RenderCommand;

/// Zero-based index of a `RenderCommand` inside one panel's command stream.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct CommandIndex(
    /// Slot in the `LayoutResult::commands` vector.
    usize,
);

/// Zero-based index of an `Element` inside one panel's layout tree.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ElementIndex(
    /// Slot in the `LayoutTree` element vector.
    usize,
);

/// Zero-based ordinal of a panel-shape source inside one panel's command stream.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ShapeOrdinal(
    /// Source ordinal assigned while traversing panel-shape commands.
    u32,
);

/// Zero-based ordinal of a primitive inside one resolved panel-shape source.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct PrimitiveOrdinal(
    /// Primitive ordinal assigned while expanding one panel-shape source.
    u32,
);

/// Shared draw-order ordinal for a panel's coplanar children.
///
/// `HierarchicalDrawKey` projection assigns this dense rank once per
/// `RenderCommand` stream. [`DrawOrderProjection`] converts it into the two
/// material ordering fields so sorted and OIT views use the same order.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct PanelDrawCommandRank(i32);

/// Screen `Transparent3d` sort value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ScreenDepthBias(f32);

/// OIT per-fragment offset added to `position.z`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OitDepthOffset(f32);

/// Per-command material ordering values derived from one panel-local ordinal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DrawCommandDepth {
    panel_draw_command_rank: PanelDrawCommandRank,
    z_index:                 DrawZIndex,
    screen_depth_bias:       ScreenDepthBias,
    oit_depth_offset:        OitDepthOffset,
}

/// Index-aligned draw-order projection for one panel's command stream.
#[derive(Clone, Debug, Default)]
pub(crate) struct DrawOrderProjection {
    depths: Vec<Option<DrawCommandDepth>>,
}

/// Sort key for draw commands: `DrawZIndex`, then `DrawStep::ordinal`, then
/// `RenderCommand` stream index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HierarchicalDrawKey {
    z_index:        DrawZIndex,
    draw_sort_tier: DrawSortTier,
    command_index:  u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RankedDrawCommand {
    panel_draw_command_rank: PanelDrawCommandRank,
    z_index:                 DrawZIndex,
    command_rank_in_z_index: i32,
}

impl From<usize> for CommandIndex {
    fn from(value: usize) -> Self { Self(value) }
}

impl ElementIndex {
    /// Sentinel used for child-divider rectangles that have no source element.
    pub(crate) const CHILD_DIVIDER: Self = Self(usize::MAX);

    /// Returns the index into the panel's `LayoutTree`.
    #[must_use]
    pub(crate) const fn get(self) -> usize { self.0 }
}

impl From<usize> for ElementIndex {
    fn from(value: usize) -> Self { Self(value) }
}

impl From<usize> for ShapeOrdinal {
    fn from(value: usize) -> Self { Self(value.to_u32()) }
}

impl ShapeOrdinal {
    /// Returns the panel-shape source ordinal as the GPU-friendly row value.
    #[must_use]
    #[expect(
        dead_code,
        reason = "Phase 9 panel-shape records will write this ordinal into GPU records"
    )]
    pub(crate) const fn as_u32(self) -> u32 { self.0 }
}

impl From<usize> for PrimitiveOrdinal {
    fn from(value: usize) -> Self { Self(value.to_u32()) }
}

impl PrimitiveOrdinal {
    /// Returns the primitive ordinal as the GPU-friendly row value.
    #[must_use]
    #[expect(
        dead_code,
        reason = "Phase 9 panel-shape records will write this ordinal into GPU records"
    )]
    pub(crate) const fn as_u32(self) -> u32 { self.0 }
}

impl ScreenDepthBias {
    #[must_use]
    pub(crate) const fn get(self) -> f32 { self.0 }
}

impl OitDepthOffset {
    #[must_use]
    pub(crate) const fn get(self) -> f32 { self.0 }
}

impl PanelDrawCommandRank {
    /// `Transparent3d` sort bias. Bevy adds it to the item's view-space
    /// distance (`sort_distance = view_z + depth_bias`, ascending sort,
    /// drawn back-to-front), so a higher ordinal composites in front.
    pub(crate) fn depth_bias(self) -> ScreenDepthBias {
        ScreenDepthBias(self.0.to_f32() * LAYER_DEPTH_BIAS)
    }

    fn text_anchored_oit_depth_offset(self, text_anchor: Self) -> OitDepthOffset {
        OitDepthOffset((self.0 - text_anchor.0).to_f32() * OIT_DEPTH_STEP)
    }

    pub(crate) fn to_usize(self) -> usize { usize::try_from(self.0).unwrap_or(usize::MAX) }
}

impl DrawCommandDepth {
    fn new(
        ordinal: PanelDrawCommandRank,
        z_index: DrawZIndex,
        command_rank_in_z_index: i32,
        text_anchor: PanelDrawCommandRank,
    ) -> Self {
        Self {
            panel_draw_command_rank: ordinal,
            z_index,
            screen_depth_bias: command_depth_bias(z_index, command_rank_in_z_index),
            oit_depth_offset: ordinal.text_anchored_oit_depth_offset(text_anchor),
        }
    }

    /// Returns the dense panel-local command ordinal.
    #[cfg(test)]
    pub(crate) const fn panel_draw_command_rank(self) -> PanelDrawCommandRank {
        self.panel_draw_command_rank
    }

    /// Returns the ordinal as a nonnegative index.
    pub(crate) fn panel_draw_command_rank_index(self) -> usize {
        self.panel_draw_command_rank.to_usize()
    }

    /// Returns the command's authored z-index.
    pub(crate) const fn z_index(self) -> DrawZIndex { self.z_index }

    /// Returns the `Transparent3d` sort bias for this command.
    pub(crate) const fn depth_bias(self) -> ScreenDepthBias { self.screen_depth_bias }

    /// Returns the OIT `position.z` offset for this command.
    pub(crate) const fn oit_depth_offset(self) -> OitDepthOffset { self.oit_depth_offset }
}

impl DrawOrderProjection {
    /// Builds the command-indexed projection from a full panel command stream.
    pub(crate) fn from_commands(commands: &[RenderCommand]) -> Self {
        let enumerated = enumerate_draw_commands(commands);
        let text_anchor = commands
            .iter()
            .enumerate()
            .filter(|(_, command)| command.kind.draw_sort_tier() == Some(DrawSortTier::Text))
            .filter_map(|(index, _)| {
                enumerated[index].map(|command| command.panel_draw_command_rank)
            })
            .min()
            .unwrap_or_default();
        let depths = enumerated
            .into_iter()
            .map(|command| {
                command.map(|command| {
                    DrawCommandDepth::new(
                        command.panel_draw_command_rank,
                        command.z_index,
                        command.command_rank_in_z_index,
                        text_anchor,
                    )
                })
            })
            .collect();
        Self { depths }
    }

    /// Returns this command's projected depth values, or `None` for scissor
    /// commands and out-of-range indices.
    pub(crate) fn depth_for(&self, command_index: usize) -> Option<DrawCommandDepth> {
        self.depths.get(command_index).copied().flatten()
    }

    /// Counts draw-participating commands at each projected z-index.
    pub(crate) fn command_counts_by_z_index(&self) -> Vec<(DrawZIndex, usize)> {
        let mut counts: BTreeMap<DrawZIndex, usize> = BTreeMap::new();

        for draw_depth in self.depths.iter().flatten() {
            *counts.entry(draw_depth.z_index()).or_default() += 1;
        }

        counts.into_iter().collect()
    }
}

impl Ord for HierarchicalDrawKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.z_index()
            .cmp(&other.z_index())
            .then(
                self.draw_sort_tier
                    .sort_order()
                    .cmp(&other.draw_sort_tier.sort_order()),
            )
            .then(self.command_index.cmp(&other.command_index))
    }
}

impl HierarchicalDrawKey {
    const fn z_index(self) -> DrawZIndex { self.z_index }
}

impl PartialOrd for HierarchicalDrawKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}

/// `Transparent3d` sort bias for one authored z-index's shared SDF surface batch.
pub(crate) fn sdf_surface_batch_depth_bias(z_index: impl Into<DrawZIndex>) -> ScreenDepthBias {
    let z_index = z_index.into();
    z_index_band_offset_depth_bias(z_index, SDF_SURFACE_BATCH_SORT_ANCHOR)
}

/// `Transparent3d` sort bias for one authored z-index's shared text batch.
pub(crate) fn text_batch_depth_bias(z_index: impl Into<DrawZIndex>) -> ScreenDepthBias {
    let z_index = z_index.into();
    z_index_band_offset_depth_bias(z_index, TEXT_BATCH_SORT_ANCHOR)
}

/// `Transparent3d` sort bias for one authored z-index's shared panel-shape batch.
pub(crate) fn panel_shape_batch_depth_bias(z_index: impl Into<DrawZIndex>) -> ScreenDepthBias {
    let z_index = z_index.into();
    z_index_band_offset_depth_bias(
        z_index,
        FIRST_COMMAND_SORT_OFFSET + COMMAND_SORT_OFFSET_CAPACITY - 1,
    )
}

fn command_depth_bias(
    z_index: impl Into<DrawZIndex>,
    command_rank_in_z_index: i32,
) -> ScreenDepthBias {
    let z_index = z_index.into();
    let depth_bias = z_index_band_offset_depth_bias(
        z_index,
        FIRST_COMMAND_SORT_OFFSET.saturating_add(command_rank_in_z_index),
    );
    debug_assert!(sdf_surface_batch_depth_bias(z_index).get() < depth_bias.get());
    depth_bias
}

fn z_index_band_offset_depth_bias(z_index: DrawZIndex, band_offset: i32) -> ScreenDepthBias {
    let screen_sort_position = i32::from(z_index)
        .saturating_mul(DRAW_Z_INDEX_BAND_WIDTH)
        .saturating_add(band_offset);
    PanelDrawCommandRank(screen_sort_position).depth_bias()
}

/// Enumerates draw-participating commands into panel-local dense ranks.
///
/// The returned vector is index-aligned with `commands`; scissor commands map
/// to `None`. Each `DrawOrdinal` stores the dense rank.
#[cfg(test)]
pub(crate) fn rank_draw_commands_for_test(
    commands: &[RenderCommand],
) -> Vec<Option<PanelDrawCommandRank>> {
    enumerate_draw_commands(commands)
        .into_iter()
        .map(|command| command.map(|command| command.panel_draw_command_rank))
        .collect()
}

fn enumerate_draw_commands(commands: &[RenderCommand]) -> Vec<Option<RankedDrawCommand>> {
    let mut keyed_commands = commands
        .iter()
        .enumerate()
        .filter_map(|(index, command)| {
            command.kind.draw_sort_tier().map(|step| {
                (
                    HierarchicalDrawKey {
                        z_index:        command.z_index,
                        draw_sort_tier: step,
                        command_index:  u32::try_from(index).unwrap_or(u32::MAX),
                    },
                    index,
                )
            })
        })
        .collect::<Vec<_>>();

    keyed_commands.sort_by_key(|(key, _)| *key);

    let mut current_z_index = None;
    let mut command_rank_in_z_index = 0_i32;
    let mut enumerated = vec![None; commands.len()];
    for (rank, (key, index)) in keyed_commands.into_iter().enumerate() {
        let z_index = key.z_index();
        if current_z_index != Some(z_index) {
            current_z_index = Some(z_index);
            command_rank_in_z_index = 0;
        }
        enumerated[index] = Some(RankedDrawCommand {
            panel_draw_command_rank: PanelDrawCommandRank(i32::try_from(rank).unwrap_or(i32::MAX)),
            z_index,
            command_rank_in_z_index,
        });
        command_rank_in_z_index = command_rank_in_z_index.saturating_add(1);
    }
    enumerated
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use bevy::image::Image;
    use bevy::prelude::*;

    use super::*;
    use crate::layout::Border;
    use crate::layout::BoundingBox;
    use crate::layout::RectangleSource;
    use crate::layout::RenderCommandKind;
    use crate::layout::TextStyle;
    use crate::render::constants::COMMAND_SORT_OFFSET_CAPACITY;
    use crate::render::constants::FIRST_COMMAND_SORT_OFFSET;
    use crate::render::constants::SDF_SURFACE_BATCH_SORT_ANCHOR;

    const LOWERED_LEVEL: DrawZIndex = DrawZIndex(-1);
    /// Z-level pairs `(low, high)` spanning negative, default, positive, and
    /// saturated ranges.
    const ORDERED_Z_INDEX_PAIRS: [(i8, i8); 6] = [
        (i8::MIN, -1),
        (-1, 0),
        (0, 1),
        (1, 63),
        (63, 65),
        (65, i8::MAX),
    ];
    const RAISED_LEVEL: DrawZIndex = DrawZIndex(1);

    fn representative_streams() -> [Vec<RenderCommand>; 2] {
        [
            commands_from_kinds([
                (rectangle(), DrawZIndex::default()),
                (text(), DrawZIndex::default()),
                (image(), DrawZIndex::default()),
                (panel_shapes(), DrawZIndex::default()),
                (text(), DrawZIndex::default()),
                (RenderCommandKind::ScissorStart, DrawZIndex::default()),
                (RenderCommandKind::ScissorEnd, DrawZIndex::default()),
            ]),
            commands_from_kinds([
                (text(), DrawZIndex::default()),
                (panel_shapes(), DrawZIndex::default()),
                (rectangle(), DrawZIndex::default()),
                (RenderCommandKind::ScissorStart, DrawZIndex::default()),
                (border(), DrawZIndex::default()),
                (text(), DrawZIndex::default()),
                (RenderCommandKind::ScissorEnd, DrawZIndex::default()),
                (image(), DrawZIndex::default()),
            ]),
        ]
    }

    fn commands_from_kinds<const N: usize>(
        entries: [(RenderCommandKind, DrawZIndex); N],
    ) -> Vec<RenderCommand> {
        entries
            .into_iter()
            .enumerate()
            .map(|(element_idx, (kind, z_index))| RenderCommand {
                bounds: BoundingBox::default(),
                kind,
                element_idx,
                z_index,
            })
            .collect()
    }

    fn rectangle() -> RenderCommandKind {
        RenderCommandKind::Rectangle {
            color:  Color::WHITE,
            source: RectangleSource::Background,
        }
    }

    fn image() -> RenderCommandKind {
        RenderCommandKind::Image {
            handle: Handle::<Image>::default(),
            tint:   Color::WHITE,
        }
    }

    fn border() -> RenderCommandKind {
        RenderCommandKind::Border {
            border: Border::default(),
        }
    }

    fn panel_shapes() -> RenderCommandKind { RenderCommandKind::PanelShapes { shapes: Vec::new() } }

    fn text() -> RenderCommandKind {
        RenderCommandKind::Text {
            text:   String::new(),
            config: TextStyle::default(),
        }
    }

    fn drawing_indices(commands: &[RenderCommand]) -> Vec<usize> {
        commands
            .iter()
            .enumerate()
            .filter(|(_, command)| command.kind.draw_sort_tier().is_some())
            .map(|(index, _)| index)
            .collect()
    }

    fn panel_draw_command_rank_at(
        ranks_by_command_index: &[Option<PanelDrawCommandRank>],
        command_index: usize,
    ) -> PanelDrawCommandRank {
        ranks_by_command_index[command_index].expect("drawing commands receive ordinals")
    }

    fn draw_depth_at(projection: &DrawOrderProjection, index: usize) -> DrawCommandDepth {
        projection
            .depth_for(index)
            .expect("drawing command receives projected depth")
    }

    fn text_anchor_rank(
        commands: &[RenderCommand],
        ordinals: &[Option<PanelDrawCommandRank>],
    ) -> i32 {
        commands
            .iter()
            .enumerate()
            .filter(|(_, command)| command.kind.draw_sort_tier() == Some(DrawSortTier::Text))
            .map(|(index, _)| panel_draw_command_rank_at(ordinals, index).0)
            .min()
            .unwrap_or(0)
    }

    fn ranks_for_step(
        commands: &[RenderCommand],
        ordinals: &[Option<PanelDrawCommandRank>],
        step: DrawSortTier,
    ) -> Vec<i32> {
        commands
            .iter()
            .enumerate()
            .filter(|(_, command)| command.kind.draw_sort_tier() == Some(step))
            .map(|(index, _)| panel_draw_command_rank_at(ordinals, index).0)
            .collect()
    }

    fn ranks_for_z_index(
        commands: &[RenderCommand],
        ordinals: &[Option<PanelDrawCommandRank>],
        z_index: DrawZIndex,
    ) -> Vec<i32> {
        commands
            .iter()
            .enumerate()
            .filter(|(_, command)| command.z_index == z_index)
            .map(|(index, _)| panel_draw_command_rank_at(ordinals, index).0)
            .collect()
    }

    fn assert_scissors_excluded(commands: &[RenderCommand]) {
        let ordinals = rank_draw_commands_for_test(commands);
        for (index, command) in commands.iter().enumerate() {
            if command.kind.draw_sort_tier().is_none() {
                assert_eq!(
                    ordinals[index], None,
                    "scissor command {index} maps to None"
                );
            }
        }
        assert_eq!(
            ordinals.iter().flatten().count(),
            drawing_indices(commands).len(),
            "only drawing commands receive ordinals",
        );
    }

    fn assert_depth_bias_and_text_anchored_oit_agree(commands: &[RenderCommand]) {
        let ordinals = rank_draw_commands_for_test(commands);
        let projection = DrawOrderProjection::from_commands(commands);
        let text_anchor = text_anchor_rank(commands, &ordinals);
        let indices = drawing_indices(commands);

        for &left_index in &indices {
            for &right_index in &indices {
                let left_rank = panel_draw_command_rank_at(&ordinals, left_index).0;
                let right_rank = panel_draw_command_rank_at(&ordinals, right_index).0;
                let left_depth_bias = draw_depth_at(&projection, left_index).depth_bias();
                let right_depth_bias = draw_depth_at(&projection, right_index).depth_bias();
                let left_oit_depth_offset = (left_rank - text_anchor).to_f32() * OIT_DEPTH_STEP;
                let right_oit_depth_offset = (right_rank - text_anchor).to_f32() * OIT_DEPTH_STEP;

                assert_eq!(
                    left_depth_bias.get().total_cmp(&right_depth_bias.get()),
                    left_oit_depth_offset.total_cmp(&right_oit_depth_offset),
                    "sorted depth bias and text-anchored OIT offset must order indices \
                     {left_index} and {right_index} the same way",
                );
            }
        }
    }

    fn assert_no_override_projection_uses_geometry_sublanes(commands: &[RenderCommand]) {
        assert!(
            commands
                .iter()
                .all(|command| command.z_index == DrawZIndex::default()),
            "no-override streams use the default z-index level",
        );
        let ordinals = rank_draw_commands_for_test(commands);
        let projection = DrawOrderProjection::from_commands(commands);
        let text_anchor = text_anchor_rank(commands, &ordinals);

        for index in drawing_indices(commands) {
            let command = &commands[index];
            let ordinal = panel_draw_command_rank_at(&ordinals, index);
            let draw_command_depth = draw_depth_at(&projection, index);
            assert_eq!(draw_command_depth.z_index(), 0.into());
            assert_eq!(draw_command_depth.panel_draw_command_rank(), ordinal);
            assert_eq!(
                draw_command_depth.depth_bias().get().to_bits(),
                command_depth_bias(0, ordinal.0).get().to_bits(),
                "no-override command {index} uses the geometry screen sublane",
            );
            assert_eq!(
                draw_command_depth.oit_depth_offset().get().to_bits(),
                ((ordinal.0 - text_anchor).to_f32() * OIT_DEPTH_STEP).to_bits(),
                "no-override command {index} keeps its text-anchored OIT offset",
            );

            assert!(command.kind.draw_sort_tier().is_some());
        }
    }

    #[test]
    fn sorted_and_oit_orderings_agree_for_every_z_index_pair() {
        for (low, high) in ORDERED_Z_INDEX_PAIRS {
            let commands = commands_from_kinds([
                (rectangle(), DrawZIndex(low)),
                (rectangle(), DrawZIndex(high)),
            ]);
            let projection = DrawOrderProjection::from_commands(&commands);
            let low_depth = draw_depth_at(&projection, 0);
            let high_depth = draw_depth_at(&projection, 1);
            assert!(
                low_depth.depth_bias().get() < high_depth.depth_bias().get(),
                "sorted bias must rise from {low} to {high}",
            );
            assert!(
                low_depth.oit_depth_offset().get() < high_depth.oit_depth_offset().get(),
                "OIT offset must rise from {low} to {high}",
            );
        }
    }

    #[test]
    fn text_batch_depth_bias_uses_level_text_sublane() {
        assert_eq!(
            text_batch_depth_bias(0).get().to_bits(),
            z_index_band_offset_depth_bias(DrawZIndex(0), TEXT_BATCH_SORT_ANCHOR)
                .get()
                .to_bits()
        );
        assert!(
            z_index_band_offset_depth_bias(
                DrawZIndex(0),
                FIRST_COMMAND_SORT_OFFSET + COMMAND_SORT_OFFSET_CAPACITY - 1,
            )
            .get()
                < text_batch_depth_bias(0).get()
        );
        assert!(
            text_batch_depth_bias(0).get() < z_index_band_offset_depth_bias(DrawZIndex(1), 0).get()
        );
        assert!(
            text_batch_depth_bias(-1).get()
                < z_index_band_offset_depth_bias(DrawZIndex(0), 0).get()
        );
    }

    #[test]
    fn sdf_surface_batch_depth_bias_uses_level_fill_sublane() {
        assert_eq!(
            sdf_surface_batch_depth_bias(0).get().to_bits(),
            z_index_band_offset_depth_bias(DrawZIndex(0), SDF_SURFACE_BATCH_SORT_ANCHOR)
                .get()
                .to_bits()
        );
        assert!(sdf_surface_batch_depth_bias(0).get() < command_depth_bias(0, 0).get());
        assert!(sdf_surface_batch_depth_bias(0).get() < panel_shape_batch_depth_bias(0).get());
        assert!(sdf_surface_batch_depth_bias(0).get() < text_batch_depth_bias(0).get());
    }

    #[test]
    fn sdf_surface_batch_depth_bias_interleaves_between_z_indices() {
        assert_eq!(
            sdf_surface_batch_depth_bias(RAISED_LEVEL).get().to_bits(),
            z_index_band_offset_depth_bias(RAISED_LEVEL, SDF_SURFACE_BATCH_SORT_ANCHOR)
                .get()
                .to_bits()
        );
        assert!(
            text_batch_depth_bias(0).get() < sdf_surface_batch_depth_bias(RAISED_LEVEL).get(),
            "raised fill batch must draw in front of default text",
        );
        assert!(
            text_batch_depth_bias(LOWERED_LEVEL).get() < sdf_surface_batch_depth_bias(0).get(),
            "default fill batch must draw in front of lowered text",
        );
    }

    #[test]
    fn hierarchical_ordinals_order_steps_for_default_z_index() {
        for commands in representative_streams() {
            assert!(
                commands
                    .iter()
                    .all(|command| command.z_index == DrawZIndex::default()),
                "representative streams use the default z-index level",
            );
            let ordinals = rank_draw_commands_for_test(&commands);
            let fill_max = ranks_for_step(&commands, &ordinals, DrawSortTier::Surface)
                .into_iter()
                .max()
                .expect("representative streams include fill commands");
            let line_min = ranks_for_step(&commands, &ordinals, DrawSortTier::PanelShape)
                .into_iter()
                .min()
                .expect("representative streams include line commands");
            let text_min = ranks_for_step(&commands, &ordinals, DrawSortTier::Text)
                .into_iter()
                .min()
                .expect("representative streams include text commands");
            assert!(fill_max < line_min);
            assert!(line_min < text_min);
        }
    }

    #[test]
    fn panel_shape_batch_depth_bias_uses_level_line_sublane() {
        assert_eq!(
            panel_shape_batch_depth_bias(0).get().to_bits(),
            z_index_band_offset_depth_bias(
                DrawZIndex(0),
                FIRST_COMMAND_SORT_OFFSET + COMMAND_SORT_OFFSET_CAPACITY - 1,
            )
            .get()
            .to_bits()
        );
        assert!(sdf_surface_batch_depth_bias(0).get() < panel_shape_batch_depth_bias(0).get());
        assert!(panel_shape_batch_depth_bias(0).get() < text_batch_depth_bias(0).get());
        assert!(
            panel_shape_batch_depth_bias(0).get()
                < z_index_band_offset_depth_bias(DrawZIndex(1), 0).get()
        );
        assert!(
            panel_shape_batch_depth_bias(-1).get()
                < z_index_band_offset_depth_bias(DrawZIndex(0), 0).get()
        );
    }

    #[test]
    fn hierarchical_ordinals_exclude_scissors() {
        for commands in representative_streams() {
            assert_scissors_excluded(&commands);
        }
    }

    #[test]
    fn command_counts_by_z_index_counts_draw_commands() {
        let commands = commands_from_kinds([
            (text(), LOWERED_LEVEL),
            (rectangle(), DrawZIndex::default()),
            (RenderCommandKind::ScissorStart, DrawZIndex::default()),
            (panel_shapes(), DrawZIndex::default()),
            (RenderCommandKind::ScissorEnd, DrawZIndex::default()),
            (text(), DrawZIndex::default()),
            (image(), RAISED_LEVEL),
            (border(), RAISED_LEVEL),
        ]);
        let projection = DrawOrderProjection::from_commands(&commands);

        assert_eq!(
            projection.command_counts_by_z_index(),
            vec![
                (LOWERED_LEVEL, 1),
                (DrawZIndex::default(), 3),
                (RAISED_LEVEL, 2)
            ]
        );
    }

    #[test]
    fn text_anchor_keeps_lowest_text_oit_offset_at_zero() {
        let commands = representative_streams()
            .into_iter()
            .next()
            .expect("representative streams include a text stream");
        let ordinals = rank_draw_commands_for_test(&commands);
        let text_anchor = text_anchor_rank(&commands, &ordinals);
        let lowest_text_rank = ranks_for_step(&commands, &ordinals, DrawSortTier::Text)
            .into_iter()
            .min()
            .expect("representative stream includes text commands");
        let text_anchored_offset = (lowest_text_rank - text_anchor).to_f32() * OIT_DEPTH_STEP;

        assert_eq!(lowest_text_rank - text_anchor, 0);
        assert_eq!(text_anchored_offset.to_bits(), 0.0f32.to_bits());
    }

    #[test]
    fn hierarchical_depth_bias_and_oit_orderings_agree() {
        for commands in representative_streams() {
            assert_depth_bias_and_text_anchored_oit_agree(&commands);
        }
    }

    #[test]
    fn no_override_projection_uses_batch_lane_layout() {
        for commands in representative_streams() {
            assert_no_override_projection_uses_geometry_sublanes(&commands);
        }
    }

    #[test]
    fn draw_order_projection_uses_enumerated_rank_and_text_anchor() {
        for commands in representative_streams() {
            let ordinals = rank_draw_commands_for_test(&commands);
            let text_anchor = text_anchor_rank(&commands, &ordinals);
            let projection = DrawOrderProjection::from_commands(&commands);

            for index in drawing_indices(&commands) {
                let ordinal = panel_draw_command_rank_at(&ordinals, index);
                let draw_depth = projection
                    .depth_for(index)
                    .expect("drawing command receives projected depth");
                assert_eq!(draw_depth.panel_draw_command_rank(), ordinal);
                assert_eq!(
                    draw_depth.depth_bias().get().to_bits(),
                    command_depth_bias(draw_depth.z_index(), ordinal.0)
                        .get()
                        .to_bits(),
                );
                assert_eq!(
                    draw_depth.oit_depth_offset().get().to_bits(),
                    ((ordinal.0 - text_anchor).to_f32() * OIT_DEPTH_STEP).to_bits(),
                );
            }
        }
    }

    #[test]
    fn screen_depth_bias_orders_fills_lines_and_text_by_z_index() {
        let default_commands = commands_from_kinds([
            (rectangle(), DrawZIndex::default()),
            (rectangle(), DrawZIndex::default()),
            (rectangle(), DrawZIndex::default()),
            (panel_shapes(), DrawZIndex::default()),
            (text(), DrawZIndex::default()),
        ]);
        let default_projection = DrawOrderProjection::from_commands(&default_commands);
        let default_line_depth_bias = panel_shape_batch_depth_bias(0);
        let default_text_depth_bias = text_batch_depth_bias(0);
        let default_sdf_surface_batch_depth_bias = sdf_surface_batch_depth_bias(0);
        for (index, command) in default_commands.iter().enumerate() {
            if command.kind.draw_sort_tier() == Some(DrawSortTier::Surface) {
                let fill_depth_bias = draw_depth_at(&default_projection, index).depth_bias();
                assert!(default_sdf_surface_batch_depth_bias.get() < fill_depth_bias.get());
                assert!(fill_depth_bias.get() < default_line_depth_bias.get());
                assert!(fill_depth_bias.get() < default_text_depth_bias.get());
            }
        }

        let raised_fill_commands = commands_from_kinds([
            (text(), DrawZIndex::default()),
            (rectangle(), RAISED_LEVEL),
            (text(), DrawZIndex::default()),
        ]);
        let raised_fill_projection = DrawOrderProjection::from_commands(&raised_fill_commands);
        let raised_fill_depth_bias = raised_fill_commands
            .iter()
            .enumerate()
            .find(|(_, command)| command.z_index == RAISED_LEVEL)
            .map(|(index, _)| draw_depth_at(&raised_fill_projection, index).depth_bias())
            .expect("raised fill command receives projected depth");
        assert!(raised_fill_depth_bias.get() > default_text_depth_bias.get());
        assert!(panel_shape_batch_depth_bias(RAISED_LEVEL).get() > default_text_depth_bias.get());
        assert!(text_batch_depth_bias(RAISED_LEVEL).get() > default_text_depth_bias.get());

        let lowered_text_commands = commands_from_kinds([
            (text(), LOWERED_LEVEL),
            (panel_shapes(), LOWERED_LEVEL),
            (rectangle(), DrawZIndex::default()),
            (image(), DrawZIndex::default()),
        ]);
        let lowered_text_projection = DrawOrderProjection::from_commands(&lowered_text_commands);
        let lowered_line_depth_bias = panel_shape_batch_depth_bias(LOWERED_LEVEL);
        let lowered_text_depth_bias = text_batch_depth_bias(LOWERED_LEVEL);
        for (index, command) in lowered_text_commands.iter().enumerate() {
            if command.kind.draw_sort_tier() == Some(DrawSortTier::Surface) {
                let fill_depth_bias = draw_depth_at(&lowered_text_projection, index).depth_bias();
                assert!(lowered_line_depth_bias.get() < fill_depth_bias.get());
                assert!(lowered_text_depth_bias.get() < fill_depth_bias.get());
            }
        }
    }

    #[test]
    fn z_index_overrides_move_commands_between_step_groups() {
        let raised_fill_commands = commands_from_kinds([
            (text(), DrawZIndex::default()),
            (rectangle(), RAISED_LEVEL),
            (text(), DrawZIndex::default()),
        ]);
        let raised_fill_ordinals = rank_draw_commands_for_test(&raised_fill_commands);
        let raised_fill_rank =
            ranks_for_z_index(&raised_fill_commands, &raised_fill_ordinals, RAISED_LEVEL)
                .into_iter()
                .next()
                .expect("raised fill command receives an ordinal");
        for text_rank in ranks_for_step(
            &raised_fill_commands,
            &raised_fill_ordinals,
            DrawSortTier::Text,
        ) {
            assert!(
                raised_fill_rank > text_rank,
                "raised fill rank must sit above default text ranks",
            );
        }

        let lowered_text_commands = commands_from_kinds([
            (text(), LOWERED_LEVEL),
            (rectangle(), DrawZIndex::default()),
            (image(), DrawZIndex::default()),
        ]);
        let lowered_text_ordinals = rank_draw_commands_for_test(&lowered_text_commands);
        let lowered_text_rank = ranks_for_z_index(
            &lowered_text_commands,
            &lowered_text_ordinals,
            LOWERED_LEVEL,
        )
        .into_iter()
        .next()
        .expect("lowered text command receives an ordinal");
        for fill_rank in ranks_for_step(
            &lowered_text_commands,
            &lowered_text_ordinals,
            DrawSortTier::Surface,
        ) {
            assert!(
                lowered_text_rank < fill_rank,
                "lowered text rank must sit below default fill ranks",
            );
        }
    }
}
