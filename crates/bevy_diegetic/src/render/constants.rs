//! Rendering constants and material utilities for diegetic panels.

use std::cmp::Ordering;

use bevy::asset::uuid_handle;
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use bevy::render::render_resource::Face;
use bevy_kana::ToF32;

use crate::cascade::DEFAULT_DRAW_LAYER;
use crate::cascade::DrawLayer;
use crate::layout::DrawStep;
use crate::layout::RenderCommand;
use crate::layout::Sidedness;

// layer ordering
/// Per-command depth bias for Geometry mode sort ordering.
///
/// Bevy packs this through `i32` into `DepthBiasState.constant`.
/// Controls the `Transparent3d` sort key so back-to-front submission
/// order matches the painter's order. Also wins the depth test for
/// coplanar fragments.
pub(crate) const LAYER_DEPTH_BIAS: f32 = 1.0;
/// Per-command OIT depth offset for coplanar fragment ordering.
///
/// Added to `position.z` in the fragment shader before `oit_draw`
/// stores the fragment. Pipeline `depth_bias` does NOT affect
/// `in.position.z`, so we apply this offset manually.
/// Reverse-Z: positive = closer to camera = composited in front.
///
/// Calibration: `bevy_lagrange` syncs the perspective near plane to
/// `radius × 0.001`, so a fragment at the camera's focus distance has
/// `position.z = near / d ≈ 0.001`. The largest offset magnitude,
/// `DEFAULT_DRAW_LAYER` steps (slot 0), must stay well below
/// that or the offset drives `position.z` non-positive and
/// `pack_24bit_depth_8bit_alpha` in `oit_draw` saturates it to the
/// cleared-background depth, where bevy's resolve pass drops every
/// fragment whose alpha < 1.0. At `1e-6`, 64 steps total `6.4e-5`
/// (6.4% of the focus depth) and one step spans ~17 quanta of the
/// 24-bit OIT depth packing, so adjacent ordinals stay distinct.
///
/// Panels much farther than the camera focus shrink `position.z` below
/// the 64-step budget (z = near/d crosses `6.4e-5` at ~15.6× the orbit
/// radius). The `OIT_MIN_DEPTH` floor in `sdf_panel.wgsl`,
/// `analytic_path.wgsl`, and `panel_line_batch.wgsl` keeps those fragments
/// storable; past the bound their coplanar ordering collapses to OIT-list
/// insertion order instead of going invisible.
pub(crate) const OIT_DEPTH_STEP: f32 = 0.000_001;

/// Shared draw-order ordinal for a panel's coplanar children.
///
/// Backing/image/line draw slots (`RenderCommand::draw_slot`) and text draw
/// layers both convert into it; [`Self::depth_bias`] and
/// [`Self::oit_depth_offset`] are the only derivations of the two per-material
/// ordering fields, so any two ordinals composite in the same relative order
/// on sorted and OIT views.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct DrawOrdinal(i32);

/// Sort key for draw commands: `DrawLayer`, then `DrawStep::ordinal`, then
/// `RenderCommand` stream index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HierarchicalDrawKey {
    z_index:    Option<DrawLayer>,
    step:       DrawStep,
    tree_order: u32,
}

/// `Transparent3d` sort bias for batched panel-line entities.
///
/// Lines are vertex-pulled across many panels like text, so one batch render
/// item cannot use a per-command material depth without fragmenting batches.
/// The shader carries per-record command depth; this coarse lane keeps lines
/// above backing quads and below text in sorted, non-OIT views.
pub(crate) const BATCH_PANEL_LINE_DEPTH_BIAS: f32 = 63.0 * LAYER_DEPTH_BIAS;

impl DrawOrdinal {
    /// Converts a geometry draw slot, saturating at `i32::MAX`. A
    /// saturated ordinal sits above every text layer (`i8`), so it
    /// composites in front of all text on sorted views and clamps to the
    /// `0.0` OIT offset — unreachable in practice (a panel would need
    /// 2^31 geometry commands).
    pub(crate) fn from_draw_slot(draw_slot: usize) -> Self {
        Self(i32::try_from(draw_slot).unwrap_or(i32::MAX))
    }

    /// `Transparent3d` sort bias. Bevy adds it to the item's view-space
    /// distance (`sort_distance = view_z + depth_bias`, ascending sort,
    /// drawn back-to-front), so a higher ordinal composites in front.
    pub(crate) fn depth_bias(self) -> f32 { self.0.to_f32() * LAYER_DEPTH_BIAS }

    /// OIT fragment depth offset, added to `position.z` before `oit_draw`
    /// (reverse-Z: positive = closer). Clamped at `0.0`: ordinals below the
    /// default text layer move backward from zero so default-layer text
    /// composites over them, while ordinals at or above it stay at `0.0` so
    /// unrelated opaque geometry keeps real depth authority over panel
    /// content.
    pub(crate) fn oit_depth_offset(self) -> f32 {
        (self.0 - i32::from(DEFAULT_DRAW_LAYER)).min(0).to_f32() * OIT_DEPTH_STEP
    }
}

impl From<DrawLayer> for DrawOrdinal {
    fn from(draw_layer: DrawLayer) -> Self { Self(i32::from(draw_layer.0)) }
}

impl Ord for HierarchicalDrawKey {
    fn cmp(&self, other: &Self) -> Ordering {
        let z_level = |key: &Self| key.z_index.map_or(0_i8, |z_index| z_index.0);

        z_level(self)
            .cmp(&z_level(other))
            .then(self.step.ordinal().cmp(&other.step.ordinal()))
            .then(self.tree_order.cmp(&other.tree_order))
    }
}

impl PartialOrd for HierarchicalDrawKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}

/// Enumerates draw-participating commands into panel-local dense ranks.
///
/// The returned vector is index-aligned with `commands`; scissor commands map
/// to `None`. Each `DrawOrdinal` stores the dense rank. Callers deriving the
/// text-anchored OIT offset subtract the lowest `DrawStep::Text` rank, or `0`
/// for panels with no text commands.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "render reads this projection after the draw-order migration"
    )
)]
pub(crate) fn enumerate_ordinals(commands: &[RenderCommand]) -> Vec<Option<DrawOrdinal>> {
    let mut keyed_commands = commands
        .iter()
        .enumerate()
        .filter_map(|(index, command)| {
            command.kind.draw_step().map(|step| {
                (
                    HierarchicalDrawKey {
                        z_index: command.z_index,
                        step,
                        tree_order: u32::try_from(index).unwrap_or(u32::MAX),
                    },
                    index,
                )
            })
        })
        .collect::<Vec<_>>();

    keyed_commands.sort_by_key(|(key, _)| *key);

    let mut ordinals = vec![None; commands.len()];
    for (rank, (_, index)) in keyed_commands.into_iter().enumerate() {
        ordinals[index] = Some(DrawOrdinal(i32::try_from(rank).unwrap_or(i32::MAX)));
    }
    ordinals
}

// material defaults
/// Default metallic value for panel surfaces. Non-metallic (dielectric).
pub(super) const DEFAULT_METALLIC: f32 = 0.0;
/// Default reflectance for panel surfaces. Very low specular to avoid
/// washing out colors under bright lights.
pub(super) const DEFAULT_REFLECTANCE: f32 = 0.02;
/// Default roughness for panel surfaces. Matte paper-like appearance.
pub(super) const DEFAULT_ROUGHNESS: f32 = 0.95;

// sdf rendering
/// World-space padding added to each SDF quad mesh beyond the SDF boundary.
/// Gives the exterior anti-aliasing ramp room to render — without this, the
/// mesh edge coincides with the SDF boundary and the AA fade-out is clipped.
pub(crate) const SDF_AA_PADDING: f32 = 0.001;
/// Internal-asset handle for the `sdf_stroke.wgsl` shader.
pub(super) const SDF_STROKE_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("536f3741-5418-4d7a-a0b2-8cfb1d30e8a1");

// text rendering
/// Fixed panel-local Z for text and image meshes.
///
/// Layering is handled by `StandardMaterial::depth_bias`, so panel-local
/// geometry stays coplanar.
pub(super) const TEXT_Z_OFFSET: f32 = 0.0;
/// Default clip rect for unclipped text: effectively infinite panel-local
/// bounds so the shader clip test becomes a no-op.
pub(super) const UNCLIPPED_TEXT_CLIP_RECT: Vec4 = Vec4::new(-1e6, -1e6, 1e6, 1e6);

/// Configures a `StandardMaterial`'s `double_sided` and `cull_mode` fields from
/// a [`Sidedness`] choice. Shared by the panel-text and world-text glyph
/// material builders.
pub(crate) const fn apply_glyph_sidedness(base: &mut StandardMaterial, sidedness: Sidedness) {
    match sidedness {
        Sidedness::DoubleSided => {
            base.double_sided = true;
            base.cull_mode = None;
        },
        Sidedness::OneSided => {
            base.double_sided = false;
            base.cull_mode = Some(Face::Back);
        },
    }
}

/// Returns the library's default matte `StandardMaterial`.
#[must_use]
pub fn default_panel_material() -> StandardMaterial {
    StandardMaterial {
        perceptual_roughness: DEFAULT_ROUGHNESS,
        metallic: DEFAULT_METALLIC,
        reflectance: DEFAULT_REFLECTANCE,
        double_sided: true,
        cull_mode: None,
        ..default()
    }
}

/// Resolves a material from the element → panel → library default chain.
///
/// If `layout_color` is `Some`, the resolved material's `base_color` is
/// overridden with that color. If `layout_color` is `None`, the material's
/// own `base_color` is preserved.
#[must_use]
pub(super) fn resolve_material(
    element_material: Option<&StandardMaterial>,
    panel_material: Option<&StandardMaterial>,
    layout_color: Option<Color>,
) -> StandardMaterial {
    let mut material = element_material
        .or(panel_material)
        .cloned()
        .unwrap_or_else(default_panel_material);

    if let Some(color) = layout_color {
        material.base_color = color;
    }

    material
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests should panic on unexpected values"
)]
mod tests {
    use bevy::image::Image;
    use bevy_kana::ToI32;

    use super::*;
    use crate::layout::Border;
    use crate::layout::BoundingBox;
    use crate::layout::RectangleSource;
    use crate::layout::RenderCommandKind;
    use crate::layout::TextStyle;

    const LOWERED_TEXT_LEVEL: DrawLayer = DrawLayer(-1);
    /// Ordinal pairs `(low, high)` spanning below-default, default-crossing,
    /// and above-default ranges.
    const ORDERED_LAYER_PAIRS: [(i8, i8); 8] = [
        (i8::MIN, -1),
        (-1, 0),
        (0, 1),
        (1, 63),
        (63, DEFAULT_DRAW_LAYER),
        (0, DEFAULT_DRAW_LAYER),
        (DEFAULT_DRAW_LAYER, 65),
        (DEFAULT_DRAW_LAYER, i8::MAX),
    ];
    const RAISED_FILL_LEVEL: DrawLayer = DrawLayer(2);

    #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
    struct ShippedOracleKey {
        lane:         i32,
        stream_index: usize,
    }

    fn representative_streams() -> [Vec<RenderCommand>; 2] {
        [
            commands_from_kinds([
                (rectangle(), None),
                (text(), None),
                (image(), None),
                (lines(), None),
                (text(), None),
                (RenderCommandKind::ScissorStart, None),
                (RenderCommandKind::ScissorEnd, None),
            ]),
            commands_from_kinds([
                (text(), None),
                (lines(), None),
                (rectangle(), None),
                (RenderCommandKind::ScissorStart, None),
                (border(), None),
                (text(), None),
                (RenderCommandKind::ScissorEnd, None),
                (image(), None),
            ]),
        ]
    }

    fn commands_from_kinds<const N: usize>(
        entries: [(RenderCommandKind, Option<DrawLayer>); N],
    ) -> Vec<RenderCommand> {
        let mut next_draw_slot = 0;
        entries
            .into_iter()
            .enumerate()
            .map(|(element_idx, (kind, z_index))| {
                let draw_slot = next_draw_slot;
                if kind.consumes_draw_slot() {
                    next_draw_slot += 1;
                }
                RenderCommand {
                    bounds: BoundingBox::default(),
                    kind,
                    element_idx,
                    z_index,
                    draw_slot,
                }
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

    fn lines() -> RenderCommandKind { RenderCommandKind::Lines { lines: Vec::new() } }

    fn text() -> RenderCommandKind {
        RenderCommandKind::Text {
            text:   String::new(),
            config: TextStyle::default(),
        }
    }

    fn shipped_oracle_key(index: usize, command: &RenderCommand) -> Option<ShippedOracleKey> {
        let step = command.kind.draw_step()?;
        let lane = match step {
            DrawStep::Fill => DrawOrdinal::from_draw_slot(command.draw_slot).0,
            DrawStep::Lines => line_batch_lane(),
            DrawStep::Text => i32::from(DEFAULT_DRAW_LAYER),
        };
        Some(ShippedOracleKey {
            lane,
            stream_index: index,
        })
    }

    fn line_batch_lane() -> i32 { (BATCH_PANEL_LINE_DEPTH_BIAS / LAYER_DEPTH_BIAS).to_i32() }

    fn drawing_indices(commands: &[RenderCommand]) -> Vec<usize> {
        commands
            .iter()
            .enumerate()
            .filter(|(_, command)| command.kind.draw_step().is_some())
            .map(|(index, _)| index)
            .collect()
    }

    fn ordinal_at(ordinals: &[Option<DrawOrdinal>], index: usize) -> DrawOrdinal {
        ordinals[index].expect("drawing commands receive ordinals")
    }

    fn text_anchor_rank(commands: &[RenderCommand], ordinals: &[Option<DrawOrdinal>]) -> i32 {
        commands
            .iter()
            .enumerate()
            .filter(|(_, command)| command.kind.draw_step() == Some(DrawStep::Text))
            .map(|(index, _)| ordinal_at(ordinals, index).0)
            .min()
            .unwrap_or(0)
    }

    fn ranks_for_step(
        commands: &[RenderCommand],
        ordinals: &[Option<DrawOrdinal>],
        step: DrawStep,
    ) -> Vec<i32> {
        commands
            .iter()
            .enumerate()
            .filter(|(_, command)| command.kind.draw_step() == Some(step))
            .map(|(index, _)| ordinal_at(ordinals, index).0)
            .collect()
    }

    fn ranks_for_z_index(
        commands: &[RenderCommand],
        ordinals: &[Option<DrawOrdinal>],
        z_index: DrawLayer,
    ) -> Vec<i32> {
        commands
            .iter()
            .enumerate()
            .filter(|(_, command)| command.z_index == Some(z_index))
            .map(|(index, _)| ordinal_at(ordinals, index).0)
            .collect()
    }

    fn assert_pairwise_order_agreement(commands: &[RenderCommand]) {
        // `shipped_oracle_key` and `enumerate_ordinals` agree when each
        // `DrawStep::Fill` has a `RenderCommand::draw_slot` strictly below
        // `line_batch_lane()` (`63`). At `draw_slot == 63`, the shipped lanes
        // assign `DrawStep::Fill` and `DrawStep::Lines` the same lane and
        // break the tie with `stream_index`; `HierarchicalDrawKey` sorts by
        // `DrawStep::ordinal` before `tree_order`, so `DrawStep::Fill` stays
        // below `DrawStep::Lines`.
        let ordinals = enumerate_ordinals(commands);
        let indices = drawing_indices(commands);

        for &left_index in &indices {
            for &right_index in &indices {
                let left_rank = ordinal_at(&ordinals, left_index).0;
                let right_rank = ordinal_at(&ordinals, right_index).0;
                let left_oracle = shipped_oracle_key(left_index, &commands[left_index])
                    .expect("drawing command has oracle key");
                let right_oracle = shipped_oracle_key(right_index, &commands[right_index])
                    .expect("drawing command has oracle key");

                assert_eq!(
                    left_rank.cmp(&right_rank),
                    left_oracle.cmp(&right_oracle),
                    "new rank order must match shipped order for indices {left_index} and \
                     {right_index}",
                );
            }
        }
    }

    fn assert_scissors_excluded(commands: &[RenderCommand]) {
        let ordinals = enumerate_ordinals(commands);
        for (index, command) in commands.iter().enumerate() {
            if command.kind.draw_step().is_none() {
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
        let ordinals = enumerate_ordinals(commands);
        let text_anchor = text_anchor_rank(commands, &ordinals);
        let indices = drawing_indices(commands);

        for &left_index in &indices {
            for &right_index in &indices {
                let left_rank = ordinal_at(&ordinals, left_index).0;
                let right_rank = ordinal_at(&ordinals, right_index).0;
                let left_depth_bias = DrawOrdinal(left_rank).depth_bias();
                let right_depth_bias = DrawOrdinal(right_rank).depth_bias();
                let left_oit_depth_offset = (left_rank - text_anchor).to_f32() * OIT_DEPTH_STEP;
                let right_oit_depth_offset = (right_rank - text_anchor).to_f32() * OIT_DEPTH_STEP;

                assert_eq!(
                    left_depth_bias.total_cmp(&right_depth_bias),
                    left_oit_depth_offset.total_cmp(&right_oit_depth_offset),
                    "sorted depth bias and text-anchored OIT offset must order indices \
                     {left_index} and {right_index} the same way",
                );
            }
        }
    }

    #[test]
    fn sorted_and_oit_orderings_agree_for_every_layer_pair() {
        for (low, high) in ORDERED_LAYER_PAIRS {
            let low_ordinal = DrawOrdinal::from(DrawLayer(low));
            let high_ordinal = DrawOrdinal::from(DrawLayer(high));
            assert!(
                low_ordinal.depth_bias() < high_ordinal.depth_bias(),
                "sorted bias must rise from {low} to {high}",
            );
            if low < DEFAULT_DRAW_LAYER {
                assert!(
                    low_ordinal.oit_depth_offset() < high_ordinal.oit_depth_offset(),
                    "OIT offset must rise from {low} to {high}",
                );
            } else {
                assert_eq!(
                    low_ordinal.oit_depth_offset().to_bits(),
                    high_ordinal.oit_depth_offset().to_bits(),
                    "OIT offsets at or above the default layer are clamped equal",
                );
            }
        }
    }

    #[test]
    fn default_layer_reproduces_previous_batch_material_values() {
        let text_ordinal = DrawOrdinal::from(DrawLayer(DEFAULT_DRAW_LAYER));
        // Pre-DrawOrdinal constants: BATCH_TEXT_DEPTH_BIAS = 64.0 ×
        // LAYER_DEPTH_BIAS and a hard-coded 0.0 OIT offset.
        assert_eq!(text_ordinal.depth_bias().to_bits(), 64.0f32.to_bits());
        assert_eq!(text_ordinal.oit_depth_offset().to_bits(), 0.0f32.to_bits());
    }

    #[test]
    fn backing_depth_bias_matches_previous_command_index_formula() {
        for draw_slot in [0_usize, 1, 5, 63] {
            let expected = draw_slot.to_f32() * LAYER_DEPTH_BIAS;
            let actual = DrawOrdinal::from_draw_slot(draw_slot).depth_bias();
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
    }

    #[test]
    fn backing_oit_offsets_stay_behind_default_text_and_rise_with_draw_slot() {
        let default_layer = usize::try_from(DEFAULT_DRAW_LAYER).expect("default layer is positive");
        let mut previous = f32::NEG_INFINITY;
        for draw_slot in 0..default_layer {
            let offset = DrawOrdinal::from_draw_slot(draw_slot).oit_depth_offset();
            assert!(offset < 0.0, "slot {draw_slot} must sit behind text");
            assert!(offset > previous, "offsets must rise with draw slot");
            previous = offset;
        }
    }

    #[test]
    fn from_draw_slot_saturates_at_i32_max() {
        assert_eq!(
            DrawOrdinal::from_draw_slot(usize::MAX),
            DrawOrdinal(i32::MAX),
        );
    }

    #[test]
    fn hierarchical_ordinals_match_shipped_order_for_unset_z_index() {
        for commands in representative_streams() {
            assert!(
                commands.iter().all(|command| command.z_index.is_none()),
                "representative streams omit z_index overrides",
            );
            assert_pairwise_order_agreement(&commands);
        }
    }

    #[test]
    fn level_zero_fill_stays_below_lines_at_lane_boundary() {
        let line_lane = usize::try_from(line_batch_lane()).expect("line lane is nonnegative");
        assert_eq!(line_lane, 63, "line lane stays at the documented boundary");

        let mut commands = commands_from_kinds([(lines(), None), (rectangle(), None)]);
        commands[1].draw_slot = line_lane;

        let ordinals = enumerate_ordinals(&commands);
        let line_rank = ranks_for_step(&commands, &ordinals, DrawStep::Lines)
            .into_iter()
            .next()
            .expect("lines command receives an ordinal");
        let fill_rank = ranks_for_step(&commands, &ordinals, DrawStep::Fill)
            .into_iter()
            .next()
            .expect("fill command receives an ordinal");

        assert!(
            fill_rank < line_rank,
            "level-zero fill at the line lane must stay below lines",
        );
    }

    #[test]
    fn hierarchical_ordinals_exclude_scissors() {
        for commands in representative_streams() {
            assert_scissors_excluded(&commands);
        }
    }

    #[test]
    fn text_anchor_keeps_lowest_text_oit_offset_at_zero() {
        let commands = representative_streams()
            .into_iter()
            .next()
            .expect("representative streams include a text stream");
        let ordinals = enumerate_ordinals(&commands);
        let text_anchor = text_anchor_rank(&commands, &ordinals);
        let lowest_text_rank = ranks_for_step(&commands, &ordinals, DrawStep::Text)
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
    fn z_index_overrides_move_commands_between_step_groups() {
        let raised_fill_commands = commands_from_kinds([
            (text(), None),
            (rectangle(), Some(RAISED_FILL_LEVEL)),
            (text(), None),
        ]);
        let raised_fill_ordinals = enumerate_ordinals(&raised_fill_commands);
        let raised_fill_rank = ranks_for_z_index(
            &raised_fill_commands,
            &raised_fill_ordinals,
            RAISED_FILL_LEVEL,
        )
        .into_iter()
        .next()
        .expect("raised fill command receives an ordinal");
        for text_rank in
            ranks_for_step(&raised_fill_commands, &raised_fill_ordinals, DrawStep::Text)
        {
            assert!(
                raised_fill_rank > text_rank,
                "raised fill rank must sit above default text ranks",
            );
        }

        let lowered_text_commands = commands_from_kinds([
            (text(), Some(LOWERED_TEXT_LEVEL)),
            (rectangle(), None),
            (image(), None),
        ]);
        let lowered_text_ordinals = enumerate_ordinals(&lowered_text_commands);
        let lowered_text_rank = ranks_for_z_index(
            &lowered_text_commands,
            &lowered_text_ordinals,
            LOWERED_TEXT_LEVEL,
        )
        .into_iter()
        .next()
        .expect("lowered text command receives an ordinal");
        for fill_rank in ranks_for_step(
            &lowered_text_commands,
            &lowered_text_ordinals,
            DrawStep::Fill,
        ) {
            assert!(
                lowered_text_rank < fill_rank,
                "lowered text rank must sit below default fill ranks",
            );
        }
    }
}
